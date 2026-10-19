//! 模型调用日志：每次 LLM 调用完整输入输出落 JSONL（按项目一文件）
//!
//! 路径：`{data_dir}/llm-logs/{repo_id}.jsonl`；单文件超 50MB 轮转为
//! `.old`（保留一代）。日志是本机调试数据，不进 `.bytewiki/`（产物目录
//! 会被分享/克隆）。读取只取尾部 N 条（最新在后），坏行跳过。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::service::AppService;
use crate::{CoreError, CoreResult};

/// 单文件轮转阈值
const ROTATE_BYTES: u64 = 50 * 1024 * 1024;
/// 列表条数上限（防御异常庞大的文件；条目按时间序取最新一段）
pub const MAX_LIST: usize = 5000;
/// 列表项 error 字段截断（完整错误在详情里）
const LIST_ERROR_CHARS: usize = 400;

/// 一次 LLM 调用的完整记录（JSONL 一行）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmLogEntry {
    /// ISO 时间
    pub ts: String,
    pub repo_id: i64,
    pub repo_name: String,
    /// 归属分支（chat = 提问时 git HEAD；分支维度引入前的旧行为空）
    #[serde(default)]
    pub branch: String,
    /// outline / page / page-retry / overview / chat
    pub stage: String,
    /// 供应商（id · 名称）
    pub provider: String,
    pub model: String,
    /// 页面标题 / 问题摘要
    pub title: String,
    /// 第几次尝试（1 起；>1 表示重试）
    pub attempt: u32,
    /// 耗时（毫秒）
    pub duration_ms: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub ok: bool,
    /// 响应来自缓存回放（未真实请求）
    #[serde(default)]
    pub cached: bool,
    #[serde(default)]
    pub error: String,
    /// 完整 system prompt
    pub system: String,
    /// 完整 user prompt（多轮取拼接）
    pub user: String,
    /// 完整响应文本
    pub response: String,
}

fn logs_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("llm-logs")
}

fn log_path(data_dir: &Path, repo_id: i64) -> PathBuf {
    logs_dir(data_dir).join(format!("{repo_id}.jsonl"))
}

/// 追加一条；文件超阈值时先轮转（.jsonl → .jsonl.old，旧的 .old 覆盖）
pub fn append(data_dir: &Path, entry: &LlmLogEntry) -> CoreResult<()> {
    let dir = logs_dir(data_dir);
    std::fs::create_dir_all(&dir)?;
    let path = log_path(data_dir, entry.repo_id);
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > ROTATE_BYTES {
            let old = path.with_extension("jsonl.old");
            let _ = std::fs::remove_file(&old);
            let _ = std::fs::rename(&path, &old);
        }
    }
    let mut line = serde_json::to_string(entry)
        .map_err(|e| CoreError::Other(anyhow::anyhow!("日志序列化失败：{e}")))?;
    line.push('\n');
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
    f.write_all(line.as_bytes())
        .map_err(|e| CoreError::Other(anyhow::anyhow!("日志写入失败：{e}")))?;
    Ok(())
}

/// 列表项：瘦身视图（不含 prompt/响应全文）。
/// 全量列表可能有几千条，若每条都带全文（页面级 user prompt 数万字符），
/// 一次 IPC 就是几十 MB——列表只给元数据与字数，全文按 `line` 懒加载。
#[derive(Debug, Clone, Serialize)]
pub struct LlmLogListItem {
    /// 详情定位（当前文件内 0 起行号；轮转后失效，前端刷新列表即可）
    pub line: usize,
    pub ts: String,
    pub branch: String,
    pub stage: String,
    pub provider: String,
    pub model: String,
    pub title: String,
    pub attempt: u32,
    pub duration_ms: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub ok: bool,
    pub cached: bool,
    /// 截断后的错误预览（完整错误走详情）
    pub error: String,
    pub system_chars: usize,
    pub user_chars: usize,
    pub response_chars: usize,
}

fn to_list_item(line: usize, e: &LlmLogEntry) -> LlmLogListItem {
    LlmLogListItem {
        line,
        ts: e.ts.clone(),
        branch: e.branch.clone(),
        stage: e.stage.clone(),
        provider: e.provider.clone(),
        model: e.model.clone(),
        title: e.title.clone(),
        attempt: e.attempt,
        duration_ms: e.duration_ms,
        prompt_tokens: e.prompt_tokens,
        completion_tokens: e.completion_tokens,
        ok: e.ok,
        cached: e.cached,
        error: e.error.chars().take(LIST_ERROR_CHARS).collect(),
        system_chars: e.system.chars().count(),
        user_chars: e.user.chars().count(),
        response_chars: e.response.chars().count(),
    }
}

/// 全量列表（流式逐行读，坏行跳过；旧→新；超上限保留最新一段）
pub fn read_list(data_dir: &Path, repo_id: i64) -> CoreResult<Vec<LlmLogListItem>> {
    let path = log_path(data_dir, repo_id);
    let Ok(f) = std::fs::File::open(&path) else {
        return Ok(vec![]);
    };
    let mut items = Vec::new();
    use std::io::BufRead;
    for (idx, line) in std::io::BufReader::new(f).lines().enumerate() {
        let Ok(text) = line else { continue };
        if text.trim().is_empty() {
            continue;
        }
        // 逐行反序列化完整结构后立即瘦身丢弃——单行峰值内存 = 一条日志，
        // 不把整份文件（上限 50MB）一次性驻留
        if let Ok(e) = serde_json::from_str::<LlmLogEntry>(&text) {
            items.push(to_list_item(idx, &e));
        }
    }
    if items.len() > MAX_LIST {
        let drop = items.len() - MAX_LIST;
        items.drain(0..drop);
    }
    Ok(items)
}

/// 按行号读单条完整日志（详情懒加载）
pub fn read_detail(data_dir: &Path, repo_id: i64, line: usize) -> CoreResult<LlmLogEntry> {
    let path = log_path(data_dir, repo_id);
    let f = std::fs::File::open(&path)
        .map_err(|_| CoreError::Other(anyhow::anyhow!("日志文件不存在，请刷新列表")))?;
    use std::io::BufRead;
    for (idx, text) in std::io::BufReader::new(f).lines().enumerate() {
        if idx != line {
            continue;
        }
        let text = text.map_err(|e| CoreError::Other(anyhow::anyhow!("日志读取失败：{e}")))?;
        return serde_json::from_str::<LlmLogEntry>(&text).map_err(|_| {
            CoreError::Other(anyhow::anyhow!("日志行解析失败（可能已轮转），请刷新列表"))
        });
    }
    Err(CoreError::Other(anyhow::anyhow!("日志行不存在（可能已轮转），请刷新列表")))
}

/// 清空（删除当前与轮转文件）
pub fn clear(data_dir: &Path, repo_id: i64) -> CoreResult<()> {
    let path = log_path(data_dir, repo_id);
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    let old = path.with_extension("jsonl.old");
    if old.exists() {
        let _ = std::fs::remove_file(&old);
    }
    Ok(())
}

/// 日志文件体积（字节；不存在为 0）
pub fn log_size(data_dir: &Path, repo_id: i64) -> u64 {
    std::fs::metadata(log_path(data_dir, repo_id)).map(|m| m.len()).unwrap_or(0)
}

impl AppService {
    /// 全量列表（瘦身项，旧→新）+ 当前文件体积
    pub fn list_llm_logs(&self, repo_path: &str) -> CoreResult<(Vec<LlmLogListItem>, u64)> {
        let canon = crate::canonical_key(std::path::Path::new(repo_path))?;
        let Some(repo) = self.db.get_repo_by_path(&canon)? else {
            return Ok((vec![], 0));
        };
        Ok((read_list(self.data_dir(), repo.id)?, log_size(self.data_dir(), repo.id)))
    }

    /// 单条完整日志（行号定位；轮转后报错由前端刷新）
    pub fn llm_log_detail(&self, repo_path: &str, line: usize) -> CoreResult<LlmLogEntry> {
        let canon = crate::canonical_key(std::path::Path::new(repo_path))?;
        let Some(repo) = self.db.get_repo_by_path(&canon)? else {
            return Err(CoreError::Other(anyhow::anyhow!("仓库未登记")));
        };
        read_detail(self.data_dir(), repo.id, line)
    }

    /// 清空项目日志
    pub fn clear_llm_logs(&self, repo_path: &str) -> CoreResult<()> {
        let canon = crate::canonical_key(std::path::Path::new(repo_path))?;
        let Some(repo) = self.db.get_repo_by_path(&canon)? else {
            return Ok(());
        };
        clear(self.data_dir(), repo.id)
    }

    /// 追加（仅 crate 内管线使用；失败只告警不影响主流程）
    pub(crate) fn append_llm_log(&self, entry: &LlmLogEntry) {
        if let Err(e) = append(self.data_dir(), entry) {
            tracing::warn!("模型日志写入失败（不影响主流程）：{e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(n: &str, title: &str) -> LlmLogEntry {
        LlmLogEntry {
            ts: format!("2026-09-15T10:00:{n}"),
            repo_id: 7,
            repo_name: "demo".into(),
            branch: "main".into(),
            stage: "page".into(),
            provider: "p1 · 测试".into(),
            model: "mock".into(),
            title: title.into(),
            attempt: 1,
            duration_ms: 1234,
            prompt_tokens: 100,
            completion_tokens: 20,
            ok: true,
            cached: false,
            error: String::new(),
            system: "SYS".into(),
            user: format!("USER-{n}"),
            response: format!("RESP-{n}"),
        }
    }

    #[test]
    fn list_all_items_with_line_index_and_detail_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        for n in ["00", "01", "02", "03"] {
            append(dir.path(), &entry(n, "页")).unwrap();
        }
        let items = read_list(dir.path(), 7).unwrap();
        assert_eq!(items.len(), 4);
        assert_eq!(items[0].line, 0); // 行号 = 文件内序（旧→新）
        assert_eq!(items[3].line, 3);
        assert_eq!(items[0].branch, "main");
        assert_eq!(items[0].user_chars, "USER-00".chars().count());
        // 全文不进列表
        let joined = format!("{:?}", items);
        assert!(!joined.contains("USER-00"));
        // 详情按行号取回完整条目
        let d = read_detail(dir.path(), 7, 2).unwrap();
        assert_eq!(d.user, "USER-02");
        assert_eq!(d.response, "RESP-02");
    }

    #[test]
    fn old_lines_without_branch_default_empty() {
        let dir = tempfile::tempdir().unwrap();
        append(dir.path(), &entry("00", "页")).unwrap();
        let path = log_path(dir.path(), 7);
        let old = r#"{"ts":"2026-09-01T00:00:00","repo_id":7,"repo_name":"demo","stage":"chat","provider":"p","model":"m","title":"t","attempt":1,"duration_ms":1,"prompt_tokens":1,"completion_tokens":1,"ok":true,"error":"","system":"s","user":"u","response":"r"}"#;
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(f, "{old}").unwrap();
        let items = read_list(dir.path(), 7).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].branch, ""); // 旧行缺字段 → 空串（前端显示 —）
    }

    #[test]
    fn list_cap_keeps_newest_tail() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..(MAX_LIST + 10) {
            let e = LlmLogEntry {
                ts: format!("2026-09-15T10:00:{i:04}"),
                user: format!("USER-{i}"),
                ..entry("00", "页")
            };
            append(dir.path(), &e).unwrap();
        }
        let items = read_list(dir.path(), 7).unwrap();
        assert_eq!(items.len(), MAX_LIST);
        let d = read_detail(dir.path(), 7, items[0].line).unwrap();
        assert_eq!(d.user, format!("USER-{}", 10)); // 丢弃最旧 10 条
    }

    #[test]
    fn bad_lines_skipped_and_clear_works() {
        let dir = tempfile::tempdir().unwrap();
        append(dir.path(), &entry("00", "页")).unwrap();
        let path = log_path(dir.path(), 7);
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(f, "not-json").unwrap();
        append(dir.path(), &entry("01", "页")).unwrap();
        let all = read_list(dir.path(), 7).unwrap();
        assert_eq!(all.len(), 2); // 坏行跳过但占行号（01 的 line = 2）
        assert_eq!(all[1].line, 2);
        assert!(log_size(dir.path(), 7) > 0);
        clear(dir.path(), 7).unwrap();
        assert!(read_list(dir.path(), 7).unwrap().is_empty());
        assert!(read_detail(dir.path(), 7, 0).is_err());
        assert_eq!(log_size(dir.path(), 7), 0);
    }
}
