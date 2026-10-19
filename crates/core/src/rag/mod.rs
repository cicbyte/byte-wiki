//! RAG 向量索引（FR-10 / FR-05 前置）
//!
//! - 分块：`.bytewiki/` 的 md 按 `##` 二级标题切段（带文档路径头）；
//!   仓库源码文件按 CHUNK_LINES 行切段（带路径头）
//! - 向量化：LLM Provider 的 embedding 模型（批量 16），f32 小端存 BLOB
//! - 检索：全量余弦相似度（M1 规模下数千块暴力扫足够，sqlite-vec 为后续优化项）
//! - 重建：`rebuild_embeddings` 替换式（生成完成后调用；克隆加载场景同样调用）
//!   Provider 不可用时返回错误，由调用方决定降级（生成流程只 warn 不中断）

use std::path::Path;

use serde::Serialize;

use crate::llm::LlmClient;
use crate::service::AppService;
use crate::{CoreError, CoreResult};

/// 检索命中
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub doc_path: String,
    pub content: String,
    /// 余弦相似度（0~1）
    pub score: f32,
}

/// 重建摘要
#[derive(Debug, Clone, Serialize)]
pub struct EmbedSummary {
    pub chunked: usize,
    pub embedded: usize,
}

/// 源码分块行数
const CHUNK_LINES: usize = 80;
/// 单块字符上限（防超长段落）
const CHUNK_CHARS: usize = 4000;
/// embedding 批大小
const EMBED_BATCH: usize = 16;
/// 检索默认 top-k
pub const DEFAULT_TOP_K: usize = 6;

impl AppService {
    /// 替换式重建向量索引：wiki md + 仓库源码 → 分块 → embed → 入库
    pub async fn rebuild_embeddings(
        &self,
        repo_path: &str,
        on_progress: &(dyn Fn(&str) + Send + Sync),
    ) -> CoreResult<EmbedSummary> {
        let root = Path::new(repo_path);
        let canon = crate::canonical_key(root)?;
        let Some(repo) = self.db.get_repo_by_path(&canon)? else {
            return Err(CoreError::Other(anyhow::anyhow!("仓库未登记：{repo_path}")));
        };

        let branch = crate::branch::current_branch(root);
        // 1) 分块（分支 wiki 产物 + 共享知识卡）
        on_progress("收集文档与代码分块");
        let mut chunks: Vec<(String, String)> = Vec::new(); // (doc_path, content)
        chunk_wiki_dir(&crate::branch::wiki_root(root, &branch), &mut chunks);
        chunk_wiki_dir(&crate::branch::cards_root(root), &mut chunks);
        for (path, _lang, size, _module, _mtime) in self.db.get_index_files_by_repo(repo.id, &branch)? {
            if !is_embeddable(&path) || size > 512 * 1024 {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(root.join(&path)) {
                chunk_code(&path, &content, &mut chunks);
            }
        }
        if chunks.is_empty() {
            self.db.replace_chunks(repo.id, &branch, &[])?;
            return Ok(EmbedSummary { chunked: 0, embedded: 0 });
        }
        let chunked = chunks.len();

        // 2) 批量 embed
        // embedding 走独立供应商（未配置则跟随当前）
        let cfg = self.llm_config()?;
        let provider = match cfg.embed_provider_id.as_deref() {
            Some(id) if !id.is_empty() => cfg
                .providers
                .iter()
                .find(|p| p.id == id)
                .cloned()
                .map(Result::Ok)
                .unwrap_or_else(|| Err(CoreError::Other(anyhow::anyhow!("Embedding 供应商不存在（设置 → 模型）")))),
            _ => self.active_llm(),
        }?;
        if provider.embed_model.trim().is_empty() {
            return Err(CoreError::Other(anyhow::anyhow!(
                "未配置 embedding 模型（设置 → 模型）"
            )));
        }
        let client = LlmClient::for_provider(&provider)?;
        let mut embedded = 0usize;
        let mut rows: Vec<(String, String, Vec<u8>, i64)> = Vec::with_capacity(chunks.len());
        for batch in chunks.chunks(EMBED_BATCH) {
            on_progress(&format!("向量化 {}/{}", rows.len(), chunked));
            let texts: Vec<String> = batch.iter().map(|(_, c)| c.clone()).collect();
            let vectors = client.embed(&texts).await?;
            if vectors.len() != batch.len() {
                return Err(CoreError::Other(anyhow::anyhow!(
                    "embedding 返回数量不匹配：{} != {}",
                    vectors.len(),
                    batch.len()
                )));
            }
            for ((doc_path, content), vec) in batch.iter().zip(vectors) {
                if vec.is_empty() {
                    continue; // 个别空向量跳过
                }
                rows.push((
                    doc_path.clone(),
                    content.clone(),
                    vec_to_blob(&vec),
                    vec.len() as i64,
                ));
                embedded += 1;
            }
        }

        // 3) 替换入库
        self.db.replace_chunks(repo.id, &branch, &rows)?;
        Ok(EmbedSummary { chunked, embedded })
    }

    /// 相似检索：query 向量化后与该仓库全部块算余弦，取 top-k
    pub async fn search_similar(&self, repo_path: &str, query: &str, top_k: usize) -> CoreResult<Vec<SearchHit>> {
        let root = Path::new(repo_path);
        let canon = crate::canonical_key(root)?;
        let Some(repo) = self.db.get_repo_by_path(&canon)? else {
            return Err(CoreError::Other(anyhow::anyhow!("仓库未登记：{repo_path}")));
        };
        // query 向量化必须与建库同一 embed 供应商（独立选择，回退当前）
        let client = LlmClient::for_embed(&self.llm_config()?)?;
        let q_vec = client
            .embed(&[query.to_string()])
            .await?
            .into_iter()
            .next()
            .unwrap_or_default();
        if q_vec.is_empty() {
            return Ok(vec![]);
        }

        let mut hits: Vec<SearchHit> = self
            .db
            .list_chunks(repo.id, &crate::branch::current_branch(Path::new(repo_path)))?
            .into_iter()
            .filter_map(|(doc_path, content, blob, dim)| {
                let vec = blob_to_vec(&blob, dim)?;
                let score = cosine(&q_vec, &vec);
                Some(SearchHit { doc_path, content, score })
            })
            .collect();
        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        hits.truncate(top_k.max(1));
        Ok(hits)
    }

    /// 仓库问答（FR-05）：检索注入 + 流式回答；返回全文与出处
    pub async fn chat_with_repo(
        &self,
        repo_path: &str,
        question: &str,
        history: &[(String, String)], // (role, content)，多轮上下文
        on_delta: &(dyn Fn(&str) + Send + Sync),
    ) -> CoreResult<ChatAnswer> {
        self.chat_with_repo_ext(repo_path, question, history, None, on_delta, &|_| {}, &|| false)
            .await
    }

    /// 仓库问答（扩展）：思考过程通道 + 取消（取消经 __CHAT_CANCELLED__ 哨兵向上传播）；
    /// provider_id 为 AI 对话栏的模型选择覆盖（None = 全局默认供应商）
    #[allow(clippy::too_many_arguments)]
    pub async fn chat_with_repo_ext(
        &self,
        repo_path: &str,
        question: &str,
        history: &[(String, String)],
        provider_id: Option<&str>,
        on_delta: &(dyn Fn(&str) + Send + Sync),
        on_thinking: &(dyn Fn(&str) + Send + Sync),
        is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    ) -> CoreResult<ChatAnswer> {
        let hits = self.search_similar(repo_path, question, DEFAULT_TOP_K).await?;
        let canon = crate::canonical_key(Path::new(repo_path))?;
        let Some(repo) = self.db.get_repo_by_path(&canon)? else {
            return Err(CoreError::Other(anyhow::anyhow!("仓库未登记：{repo_path}")));
        };
        if hits.is_empty() {
            return Err(CoreError::Other(anyhow::anyhow!(
                "该仓库尚无向量索引（先「一键生成」或在浏览模式点「重建索引」）"
            )));
        }

        // 上下文拼接（截断到 ~6000 字符，优先高分块）
        let mut ctx = String::new();
        for h in &hits {
            let piece = format!("--- 来源：{}（相似度 {:.2}）\n{}\n\n", h.doc_path, h.score, h.content);
            if ctx.len() + piece.len() > 6000 {
                break;
            }
            ctx.push_str(&piece);
        }

        let client = LlmClient::for_chat(&self.llm_config()?, provider_id)?;
        let mut messages: Vec<crate::llm::ChatMessage> =
            vec![crate::llm::ChatMessage::system(CHAT_SYSTEM_PROMPT)];
        for (role, content) in history.iter().take(8) {
            messages.push(crate::llm::ChatMessage {
                role: role.clone(),
                content: content.clone(),
            });
        }
        messages.push(crate::llm::ChatMessage::user(format!(
            "仓库检索上下文：\n{ctx}\n问题：{question}"
        )));

        let started = std::time::Instant::now();
        let outcome = client
            .chat_stream_ext(&messages, &Default::default(), on_delta, on_thinking, is_cancelled)
            .await;
        // 模型日志：完整输入输出留痕（取消哨兵不记）
        let cancelled = outcome
            .as_ref()
            .err()
            .map(|e| e.to_string().contains("__CHAT_CANCELLED__"))
            .unwrap_or(false);
        if !cancelled {
            let title: String = question.chars().take(60).collect();
            self.append_llm_log(&crate::llm_log::LlmLogEntry {
                ts: crate::db::now_iso(),
                repo_id: repo.id,
                repo_name: repo.name.clone(),
                // 对话读的是工作区（检索走当前分支索引），归属 = 提问时 git HEAD
                branch: crate::branch::current_branch(Path::new(repo_path)),
                stage: "chat".into(),
                provider: client.provider_desc(),
                model: client.model_name().to_string(),
                title,
                attempt: 1,
                duration_ms: started.elapsed().as_millis() as u64,
                prompt_tokens: outcome.as_ref().map(|o| o.prompt_tokens).unwrap_or(0),
                completion_tokens: outcome.as_ref().map(|o| o.completion_tokens).unwrap_or(0),
                ok: outcome.is_ok(),
                cached: false,
                error: outcome.as_ref().err().map(|e| e.to_string()).unwrap_or_default(),
                system: messages.first().map(|m| m.content.clone()).unwrap_or_default(),
                user: question.to_string(),
                response: outcome.as_ref().map(|o| o.text.clone()).unwrap_or_default(),
            });
        }
        let outcome = outcome?;
        let sources: Vec<String> = {
            let mut seen = std::collections::BTreeSet::new();
            hits.iter()
                .filter(|h| h.score > 0.3)
                .map(|h| h.doc_path.clone())
                .filter(|p| seen.insert(p.clone()))
                .collect()
        };
        Ok(ChatAnswer {
            text: outcome.text,
            sources,
            prompt_tokens: outcome.prompt_tokens,
            completion_tokens: outcome.completion_tokens,
        })
    }
}

/// 仓库问答结果
#[derive(Debug, Clone, Serialize)]
pub struct ChatAnswer {
    pub text: String,
    /// 引用出处（去重 doc_path）
    pub sources: Vec<String>,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

const CHAT_SYSTEM_PROMPT: &str = "你是仓库知识库问答助手。仅依据给定的检索上下文回答问题，\
引用来源时注明文件路径；上下文不足以回答时明确说明，不要编造。使用中文，简洁准确。";

// ============ 分块 ============

fn is_embeddable(path: &str) -> bool {
    matches!(
        path.rsplit('.').next().unwrap_or(""),
        "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "md" | "toml" | "json" | "vue"
    ) && !path.contains(".bytewiki/")
}

/// wiki md 按 ## 标题切段；标题前缀带上文档路径头
fn chunk_wiki_dir(wiki: &Path, out: &mut Vec<(String, String)>) {
    for entry in walkdir::WalkDir::new(wiki)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let ext = entry.path().extension().and_then(|e| e.to_str()).unwrap_or("");
        let rel = match entry.path().strip_prefix(wiki) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        let rel = format!(".bytewiki/{rel}");
        let Ok(content) = std::fs::read_to_string(entry.path()) else { continue };
        if ext == "mmd" {
            out.push((rel.clone(), format!("[{rel}]\n{content}")));
            continue;
        }
        chunk_markdown(&rel, &content, out);
    }
}

/// markdown：按 ## 切段；无标题则整体一块（超长再按字符切）
fn chunk_markdown(doc_path: &str, content: &str, out: &mut Vec<(String, String)>) {
    let mut current_title = String::new();
    let mut buf = String::new();
    let mut flush = |title: &str, buf: &mut String| {
        let text = buf.trim();
        if !text.is_empty() {
            for piece in split_by_chars(&format!("[{doc_path}] {title}\n{text}"), CHUNK_CHARS) {
                out.push((doc_path.to_string(), piece));
            }
        }
        buf.clear();
    };
    for line in content.lines() {
        if line.starts_with("## ") {
            flush(&current_title, &mut buf);
            current_title = line.trim_start_matches("## ").to_string();
        }
        buf.push_str(line);
        buf.push('\n');
    }
    flush(&current_title, &mut buf);
}

/// 源码：按行数切段，每块带路径头（行号为 1-based 闭区间）
fn chunk_code(path: &str, content: &str, out: &mut Vec<(String, String)>) {
    let mut chunk = String::new();
    let mut lines_in_chunk = 0usize;
    let mut start_line = 1usize;
    let mut current = 1usize; // 下一个待处理行号
    for line in content.lines() {
        if lines_in_chunk >= CHUNK_LINES {
            out.push((
                path.to_string(),
                format!("[{path}:{start_line}-{}]\n{chunk}", current - 1),
            ));
            chunk.clear();
            lines_in_chunk = 0;
            start_line = current;
        }
        chunk.push_str(line);
        chunk.push('\n');
        lines_in_chunk += 1;
        current += 1;
    }
    if !chunk.trim().is_empty() {
        out.push((
            path.to_string(),
            format!("[{path}:{start_line}-{}]\n{chunk}", current - 1),
        ));
    }
}

fn split_by_chars(text: &str, max: usize) -> Vec<String> {
    if text.chars().count() <= max {
        return vec![text.to_string()];
    }
    let mut parts = Vec::new();
    let mut buf = String::new();
    let mut n = 0usize;
    for ch in text.chars() {
        if n >= max {
            parts.push(std::mem::take(&mut buf));
            n = 0;
        }
        buf.push(ch);
        n += 1;
    }
    if !buf.is_empty() {
        parts.push(buf);
    }
    parts
}

// ============ 向量序列化与相似度 ============

fn vec_to_blob(v: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(v.len() * 4);
    for f in v {
        bytes.extend_from_slice(&f.to_le_bytes());
    }
    bytes
}

fn blob_to_vec(blob: &[u8], dim: i64) -> Option<Vec<f32>> {
    if dim <= 0 || blob.len() != dim as usize * 4 {
        return None;
    }
    Some((0..dim as usize).map(|i| {
        f32::from_le_bytes([blob[i * 4], blob[i * 4 + 1], blob[i * 4 + 2], blob[i * 4 + 3]])
    }).collect())
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0f32;
    let mut na = 0f32;
    let mut nb = 0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

#[cfg(test)]
mod tests;
