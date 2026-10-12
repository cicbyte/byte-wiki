//! Wiki 生成流水线（FR-01 / FR-02）
//!
//! 四阶段：索引 → 规划 → 逐模块生成（拓扑序，被依赖者先出）→ 组装落盘。
//!
//! - 规划：本地结构化规划（模块清单 + 固定文档大纲），不耗 LLM token；
//!   大纲由索引数据（文件/语言/依赖边）填充，逐模块生成时交给 LLM 扩写
//! - 组装：全部成功后原子替换 `.bytewiki/`（modules/ + architecture.md +
//!   index.json），任一模块失败则保留旧 Wiki，不产生半成品 index.json
//! - 进度：`on_event` 回调（阶段/序号/累计 token），桌面端转 Tauri 事件

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use crate::db::now_iso;
use crate::llm::{ChatMessage, ChatOptions, LlmClient};
use crate::service::AppService;
use crate::{CoreError, CoreResult};

/// 生成进度事件（推送给前端实时展示）
#[derive(Debug, Clone, Serialize)]
pub struct GenerateEvent {
    /// 阶段：indexing / planning / generating / assembling / done / error
    pub phase: String,
    /// 当前序号（generating 阶段为第几个模块，从 1 起）
    pub current: usize,
    /// 总数（generating 阶段为模块总数）
    pub total: usize,
    /// 人类可读信息（模块名/错误原因）
    pub message: String,
    /// 累计 prompt token
    pub prompt_tokens: u64,
    /// 累计 completion token
    pub completion_tokens: u64,
}

/// 生成结果摘要
#[derive(Debug, Clone, Serialize)]
pub struct GenerateSummary {
    pub output_dir: String,
    pub modules_generated: usize,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    /// 耗时（毫秒）
    pub elapsed_ms: u64,
}

/// 增量更新结果摘要
#[derive(Debug, Clone, Serialize)]
pub struct UpdateSummary {
    pub regenerated_modules: usize,
    pub changed_files: usize,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub elapsed_ms: u64,
    /// 无变更（什么都没做）
    pub unchanged: bool,
}

/// 单模块上下文预算（文件路径条数上限）
const MAX_FILES_IN_PROMPT: usize = 300;
/// 单模块代表源码片段：最多文件数 × 每文件行数
const MAX_SNIPPET_FILES: usize = 3;
const MAX_SNIPPET_LINES: usize = 120;
/// 长行截断
const MAX_LINE_LEN: usize = 500;

impl AppService {
    /// 增量更新 Wiki（FR-06：手动触发，仅重生成受影响模块 + 反向依赖闭包）。
    ///
    /// 未变模块的文档从现有 `.bytewiki/modules/` 读入作为基底原样写回
    /// （内容字节不变）；`cards/` 与人工内容不触碰；架构文档与图谱重生成。
    pub async fn update_wiki(
        &self,
        repo_path: &str,
        on_event: &(dyn Fn(GenerateEvent) + Send + Sync),
    ) -> CoreResult<UpdateSummary> {
        let started = std::time::Instant::now();
        let root = Path::new(repo_path);
        let canon = crate::canonical_key(root)?;
        let Some(repo) = self.db.get_repo_by_path(&canon)? else {
            return Err(CoreError::Other(anyhow::anyhow!("仓库未登记：{repo_path}")));
        };

        // ---- 1）变更检测（基于旧索引快照）----
        on_event(ev("checking", 0, 0, "检测代码变更"));
        let report = self.check_staleness(&canon)?;
        if report.changes.is_empty() {
            return Ok(UpdateSummary {
                regenerated_modules: 0,
                changed_files: 0,
                prompt_tokens: 0,
                completion_tokens: 0,
                elapsed_ms: started.elapsed().as_millis() as u64,
                unchanged: true,
            });
        }

        // ---- 2）受影响模块 = 直接模块 ∪ 反向依赖闭包（依赖它们的一方也更新）----
        let edges = self.db.get_index_edges_by_repo(repo.id)?;
        let mut affected: std::collections::HashSet<String> =
            report.affected_modules.iter().cloned().collect();
        loop {
            let add: Vec<String> = edges
                .iter()
                .filter(|(_, to, _)| affected.contains(to))
                .map(|(from, _, _)| from.clone())
                .filter(|f| !affected.contains(f))
                .collect();
            if add.is_empty() {
                break;
            }
            affected.extend(add);
        }
        let changed_files = report.changes.len();

        // ---- 3）重新索引（图谱与规划基于新状态）----
        on_event(ev("indexing", 0, 0, format!("重扫索引（{} 个文件变更）", changed_files)));
        let summary = self.index_repo(&canon, &|n| {
            on_event(ev("indexing", n, 0, format!("已扫描 {n} 个文件")));
        })?;
        let plan = self.build_plan(repo.id, &summary)?;

        // ---- 4）基底：未变模块沿用现有文档 ----
        let wiki_dir = root.join(".bytewiki").join("modules");
        let mut docs: HashMap<String, String> = HashMap::new();
        for m in &plan.modules {
            if !affected.contains(&m.name) {
                if let Ok(old) = std::fs::read_to_string(wiki_dir.join(format!("{}.md", safe_filename(&m.name)))) {
                    docs.insert(m.name.clone(), old);
                }
            }
        }

        // ---- 5）仅重生成受影响模块 ----
        let provider = self.active_llm()?;
        let client = LlmClient::for_provider(&provider)?;
        let mut tokens = (0u64, 0u64);
        let to_regenerate: Vec<&ModulePlan> = plan
            .modules
            .iter()
            .filter(|m| affected.contains(&m.name))
            .collect();
        let total = to_regenerate.len();
        for (i, module) in to_regenerate.iter().enumerate() {
            on_event(ev(
                "generating",
                i + 1,
                total,
                format!("更新模块文档：{}", module.name),
            ));
            let (content, out) = self.generate_module_doc(&client, root, module, &provider.chat_model).await?;
            tokens.0 += out.prompt_tokens;
            tokens.1 += out.completion_tokens;
            docs.insert(module.name.clone(), content);
        }

        // ---- 6）架构重生成（一次调用，依赖全模块摘要）----
        on_event(ev("generating", total, total, "重新生成顶层架构文档"));
        let (arch, out) = self.generate_architecture_doc(&client, &plan, &docs).await?;
        tokens.0 += out.prompt_tokens;
        tokens.1 += out.completion_tokens;

        // ---- 7）组装 + 向量重建 ----
        on_event(ev("assembling", 0, 0, "写入 .bytewiki/"));
        let output_dir = self.assemble_wiki(&canon, repo.id, &plan, &docs, &arch, &provider.chat_model)?;
        match self.rebuild_embeddings(&canon, &|m| on_event(ev("embedding", 0, 0, m))).await {
            Ok(s) => tracing::info!("增量更新后向量重建：{} 块", s.embedded),
            Err(e) => {
                tracing::warn!("向量索引跳过（不影响 Wiki 产物）：{e}");
                on_event(ev("embedding", 0, 0, format!("向量索引跳过：{e}")));
            }
        }

        self.db.log_change(
            repo.id,
            "update",
            &format!(
                "files={} modules={:?} prompt_tok={} completion_tok={}",
                changed_files,
                to_regenerate.iter().map(|m| m.name.clone()).collect::<Vec<_>>(),
                tokens.0,
                tokens.1
            ),
        )?;

        let _ = output_dir;
        Ok(UpdateSummary {
            regenerated_modules: total,
            changed_files,
            prompt_tokens: tokens.0,
            completion_tokens: tokens.1,
            elapsed_ms: started.elapsed().as_millis() as u64,
            unchanged: false,
        })
    }

    /// 一键生成 Wiki（自动补索引）。失败保留旧 `.bytewiki/`。
    pub async fn generate_wiki(
        &self,
        repo_path: &str,
        on_event: &(dyn Fn(GenerateEvent) + Send + Sync),
    ) -> CoreResult<GenerateSummary> {
        let started = std::time::Instant::now();
        let root = Path::new(repo_path);
        let canon = crate::canonical_key(root)?;
        let Some(repo) = self.db.get_repo_by_path(&canon)? else {
            return Err(CoreError::Other(anyhow::anyhow!("仓库未登记，请先打开：{repo_path}")));
        };

        // ---- 阶段 1：索引（复用已有索引则跳过重扫由调用方决定；这里始终刷新）----
        on_event(ev("indexing", 0, 0, "建立文件与引用索引"));
        let summary = self.index_repo(&canon, &|n| {
            on_event(ev("indexing", n, 0, format!("已扫描 {n} 个文件")));
        })?;

        // ---- 阶段 2：规划（本地结构化，不耗 token）----
        on_event(ev("planning", 0, 0, format!("规划文档结构：{} 个模块", summary.modules.len())));
        let plan = self.build_plan(repo.id, &summary)?;

        // LLM 客户端（当前使用的供应商）
        let provider = self.active_llm()?;
        let client = LlmClient::for_provider(&provider)?;

        // ---- 阶段 3：逐模块生成（拓扑序：被依赖者先生成）----
        let mut tokens = (0u64, 0u64);
        let total = plan.modules.len();
        let mut docs: HashMap<String, String> = HashMap::new();
        for (i, module) in plan.modules.iter().enumerate() {
            on_event(ev(
                "generating",
                i + 1,
                total,
                format!("生成模块文档：{}", module.name),
            ));
            let (content, out) = self
                .generate_module_doc(&client, root, &module, &provider.chat_model)
                .await?;
            tokens.0 += out.prompt_tokens;
            tokens.1 += out.completion_tokens;
            docs.insert(module.name.clone(), content);
        }

        // ---- 阶段 3.5：顶层架构（带各模块摘要）----
        on_event(ev("generating", total, total, "生成顶层架构文档"));
        let (arch, out) = self.generate_architecture_doc(&client, &plan, &docs).await?;
        tokens.0 += out.prompt_tokens;
        tokens.1 += out.completion_tokens;

        // ---- 阶段 4：组装落盘（全量成功后原子替换）----
        on_event(ev("assembling", 0, 0, "写入 .bytewiki/"));
        let output_dir = self.assemble_wiki(&canon, repo.id, &plan, &docs, &arch, &provider.chat_model)?;

        // ---- 阶段 5：向量索引（best-effort：Provider 不可用时仅告警，不阻塞生成）----
        on_event(ev("embedding", 0, 0, "重建向量索引"));
        match self.rebuild_embeddings(&canon, &|m| {
            on_event(ev("embedding", 0, 0, m));
        }).await {
            Ok(s) => {
                tracing::info!("向量索引完成：{} 块", s.embedded);
            }
            Err(e) => {
                tracing::warn!("向量索引跳过（不影响 Wiki 产物）：{e}");
                on_event(ev("embedding", 0, 0, format!("向量索引跳过：{e}")));
            }
        }

        self.db.log_change(
            repo.id,
            "generate",
            &format!("modules={} prompt_tok={} completion_tok={}", total, tokens.0, tokens.1),
        )?;

        let summary = GenerateSummary {
            output_dir,
            modules_generated: total,
            prompt_tokens: tokens.0,
            completion_tokens: tokens.1,
            elapsed_ms: started.elapsed().as_millis() as u64,
        };
        on_event(ev("done", total, total, format!("生成完成：{}", summary.output_dir)));
        Ok(summary)
    }

    // ============ 规划 ============

    fn build_plan(&self, repo_id: i64, summary: &crate::indexer::IndexSummary) -> CoreResult<GeneratePlan> {
        let files = self.db.get_index_files_by_repo(repo_id)?;
        let edges = self.db.get_index_edges_by_repo(repo_id)?;

        let mut by_module: HashMap<String, Vec<(String, String)>> = HashMap::new();
        for (path, lang, _size, module, _mtime) in &files {
            by_module
                .entry(module.clone())
                .or_default()
                .push((path.clone(), lang.clone()));
        }
        for list in by_module.values_mut() {
            list.sort();
        }

        // 模块按依赖拓扑排序（被依赖者在前；环/无关节点按字母序）
        let ordered = topo_order(&summary.modules.iter().map(|m| m.name.clone()).collect::<Vec<_>>(), &edges);

        let modules = ordered
            .into_iter()
            .filter_map(|name| {
                let ms = summary.modules.iter().find(|m| m.name == name)?;
                let module_files = by_module.get(&name).cloned().unwrap_or_default();
                Some(ModulePlan {
                    name: name.clone(),
                    langs: ms.langs.clone(),
                    file_count: ms.file_count,
                    files: module_files,
                    depends_on: edges.iter().filter(|(f, _, _)| *f == name).map(|(_, t, _)| t.clone()).collect(),
                    depended_by: edges.iter().filter(|(_, t, _)| *t == name).map(|(f, _, _)| f.clone()).collect(),
                })
            })
            .collect();

        Ok(GeneratePlan { modules })
    }

    // ============ 逐模块生成 ============

    async fn generate_module_doc(
        &self,
        client: &LlmClient,
        repo_root: &Path,
        module: &ModulePlan,
        _model: &str,
    ) -> CoreResult<(String, crate::llm::ChatOutcome)> {
        let mut ctx = String::new();
        ctx.push_str(&format!("## 模块：{}\n", module.name));
        ctx.push_str(&format!("语言：{}；文件数：{}\n", module.langs.join("/"), module.file_count));
        if !module.depends_on.is_empty() {
            ctx.push_str(&format!("依赖的模块：{}\n", module.depends_on.join("、")));
        }
        if !module.depended_by.is_empty() {
            ctx.push_str(&format!("被依赖：{}\n", module.depended_by.join("、")));
        }
        ctx.push_str("\n文件清单（相对仓库根）：\n");
        for (path, lang) in module.files.iter().take(MAX_FILES_IN_PROMPT) {
            ctx.push_str(&format!("- {path} ({lang})\n"));
        }
        if module.files.len() > MAX_FILES_IN_PROMPT {
            ctx.push_str(&format!("…另有 {} 个文件未列出\n", module.files.len() - MAX_FILES_IN_PROMPT));
        }

        // 代表源码片段：模块主语言优先，最多 MAX_SNIPPET_FILES 个
        let mut picked = 0;
        for (path, lang) in &module.files {
            if picked >= MAX_SNIPPET_FILES {
                break;
            }
            if !module.langs.iter().any(|l| l == lang) || path.ends_with(".d.ts") {
                continue;
            }
            if let Some(snippet) = read_snippet(&repo_root.join(path)) {
                ctx.push_str(&format!("\n### 代表片段：{path}\n```{lang}\n{snippet}```\n"));
                picked += 1;
            }
        }

        let messages = vec![
            ChatMessage::system(MODULE_SYSTEM_PROMPT),
            ChatMessage::user(format!(
                "根据以下仓库索引信息，为模块「{name}」撰写一份中文 Markdown 技术文档。\n\
                 只输出 Markdown 正文（不要代码块包裹整篇），结构遵循：\n\
                 # {name}\n## 职责定位\n## 目录结构\n## 关键文件\n## 对外接口与依赖\n## 注意事项\n\n{ctx}",
                name = module.name,
            )),
        ];
        let out = client.chat_detailed(&messages, &ChatOptions::default()).await?;
        let content = sanitize_md(&out.text);
        Ok((content, out))
    }

    async fn generate_architecture_doc(
        &self,
        client: &LlmClient,
        plan: &GeneratePlan,
        docs: &HashMap<String, String>,
    ) -> CoreResult<(String, crate::llm::ChatOutcome)> {
        let mut ctx = String::new();
        for m in &plan.modules {
            ctx.push_str(&format!("### 模块 {}（{} 文件，{}）\n", m.name, m.file_count, m.langs.join("/")));
            if !m.depends_on.is_empty() {
                ctx.push_str(&format!("依赖：{}\n", m.depends_on.join("、")));
            }
            // 模块文档的标题行摘要（控制 token）
            let headings: Vec<&str> = docs
                .get(&m.name)
                .map(|d| d.lines().filter(|l| l.starts_with("## ")).take(6).collect())
                .unwrap_or_default();
            for h in headings {
                ctx.push_str(&format!("  {h}\n"));
            }
        }
        let messages = vec![
            ChatMessage::system(ARCH_SYSTEM_PROMPT),
            ChatMessage::user(format!(
                "根据以下模块规划与模块文档摘要，撰写仓库顶层架构文档（中文 Markdown）。\n\
                 只输出 Markdown 正文，结构遵循：\n\
                 # 架构总览\n## 技术栈\n## 模块划分与职责\n## 模块依赖关系\n## 构建与运行\n\n{ctx}"
            )),
        ];
        let out = client.chat_detailed(&messages, &ChatOptions::default()).await?;
        Ok((sanitize_md(&out.text), out))
    }

    // ============ 既有 Wiki 加载（FR-08 / 克隆场景） ============

    /// 扫描仓库内 .bytewiki/ 目录，返回清单 (rel_path, kind, title) 并登记 wiki_docs。
    /// 只读文件系统，不触发任何 LLM 调用；目录不存在返回空表。
    pub fn scan_existing_wiki(&self, repo_root: &Path) -> CoreResult<Vec<(String, String, String)>> {
        let wiki = repo_root.join(".bytewiki");
        if !wiki.is_dir() {
            return Ok(vec![]);
        }
        let canon = crate::canonical_key(repo_root)?;
        let mut out: Vec<(String, String, String)> = Vec::new();
        for entry in walkdir::WalkDir::new(&wiki)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let Ok(rel) = entry.path().strip_prefix(&wiki) else { continue };
            let rel = rel.to_string_lossy().replace('\\', "/");
            let name = rel.rsplit('/').next().unwrap_or(&rel).to_string();
            let title = name
                .trim_end_matches(".md")
                .trim_end_matches(".mmd")
                .trim_end_matches(".json")
                .to_string();
            let kind = if rel.starts_with("modules/") {
                "module"
            } else if rel.starts_with("graphs/") {
                "graph"
            } else if rel.starts_with("cards/") {
                "card"
            } else {
                "architecture"
            };
            out.push((rel, kind.to_string(), title));
        }
        if let Some(repo) = self.db.get_repo_by_path(&canon)? {
            if !out.is_empty() {
                self.db.replace_wiki_docs(repo.id, &out, &now_iso())?;
            }
        }
        out.sort();
        Ok(out)
    }

    // ============ 组装 ============

    #[allow(clippy::too_many_arguments)]
    fn assemble_wiki(
        &self,
        repo_root: &str,
        repo_id: i64,
        plan: &GeneratePlan,
        docs: &HashMap<String, String>,
        architecture: &str,
        chat_model: &str,
    ) -> CoreResult<String> {
        let root = Path::new(repo_root);
        let wiki = root.join(".bytewiki");

        // 1) 全部产物写入暂存目录
        let staging = root.join(".bytewiki.staging");
        if staging.exists() {
            std::fs::remove_dir_all(&staging)?;
        }
        std::fs::create_dir_all(staging.join("modules"))?;
        std::fs::create_dir_all(staging.join("graphs"))?;
        std::fs::write(staging.join("architecture.md"), architecture)?;
        for m in &plan.modules {
            if let Some(content) = docs.get(&m.name) {
                std::fs::write(staging.join("modules").join(format!("{}.md", safe_filename(&m.name))), content)?;
            }
        }
        // 依赖图谱：由索引的模块引用边生成（非 LLM 臆测），孤立模块仅作节点不出边
        let dependency_mmd = build_dependency_graph(plan);
        std::fs::write(staging.join("graphs").join("dependency.mmd"), &dependency_mmd)?;
        let has_graph = dependency_mmd.lines().any(|l| l.contains("-->"));

        let mut files_json: Vec<serde_json::Value> =
            vec![serde_json::json!({ "path": "architecture.md", "kind": "architecture" })];
        for m in &plan.modules {
            files_json.push(serde_json::json!({
                "path": format!("modules/{}.md", safe_filename(&m.name)),
                "kind": "module",
                "module": m.name,
            }));
        }
        if has_graph {
            files_json.push(serde_json::json!({ "path": "graphs/dependency.mmd", "kind": "graph" }));
        }
        let index_json = serde_json::json!({
            "version": 1,
            "generator": { "provider": "byte-wiki", "model": chat_model },
            "generated_at": now_iso(),
            "files": files_json,
        });
        std::fs::write(staging.join("index.json"), serde_json::to_string_pretty(&index_json)?)?;

        // 2) 替换正式目录（modules/ 与 architecture.md；cards/ 若存在则保留）
        std::fs::create_dir_all(&wiki)?;
        let old_modules = wiki.join("modules");
        if old_modules.exists() {
            std::fs::remove_dir_all(&old_modules)?;
        }
        move_dir(&staging.join("modules"), &wiki.join("modules"))?;
        let old_graphs = wiki.join("graphs");
        if old_graphs.exists() {
            // 保留调用链产物：graphs/callchain/ 由调用链管线独立拥有，不属于依赖图重生成范围
            let cc = old_graphs.join("callchain");
            if cc.is_dir() {
                std::fs::rename(&cc, staging.join("graphs").join("callchain"))?;
            }
            std::fs::remove_dir_all(&old_graphs)?;
        }
        move_dir(&staging.join("graphs"), &wiki.join("graphs"))?;
        std::fs::write(wiki.join("architecture.md"), std::fs::read(staging.join("architecture.md"))?)?;
        std::fs::write(wiki.join("index.json"), std::fs::read(staging.join("index.json"))?)?;
        let _ = std::fs::remove_dir_all(&staging);

        // 3) 登记 wiki_docs（生成区整体替换）
        let mut doc_rows: Vec<(String, String, String)> = vec![(
            "architecture.md".into(),
            "architecture".into(),
            "架构总览".into(),
        )];
        if has_graph {
            doc_rows.push((
                "graphs/dependency.mmd".into(),
                "graph".into(),
                "模块依赖图谱".into(),
            ));
        }
        // 调用链产物在整体替换后仍存在（上方已保留），重新补登避免登记丢失
        if wiki.join(&crate::callchain::CALLGRAPH_DOC_PATH).is_file() {
            doc_rows.push((
                crate::callchain::CALLGRAPH_DOC_PATH.into(),
                "graph".into(),
                "调用链图谱".into(),
            ));
        }
        doc_rows.extend(plan.modules.iter().map(|m| {
            (
                format!("modules/{}.md", safe_filename(&m.name)),
                "module".into(),
                m.name.clone(),
            )
        }));
        self.db.replace_wiki_docs(repo_id, &doc_rows, &now_iso())?;

        Ok(wiki.to_string_lossy().into_owned())
    }
}

// ============ 规划数据结构 ============

#[derive(Debug, Clone)]
struct GeneratePlan {
    modules: Vec<ModulePlan>,
}

#[derive(Debug, Clone)]
struct ModulePlan {
    name: String,
    langs: Vec<String>,
    file_count: usize,
    files: Vec<(String, String)>,
    depends_on: Vec<String>,
    depended_by: Vec<String>,
}

// ============ 辅助 ============

fn ev(phase: &str, current: usize, total: usize, message: impl Into<String>) -> GenerateEvent {
    GenerateEvent {
        phase: phase.into(),
        current,
        total,
        message: message.into(),
        prompt_tokens: 0,
        completion_tokens: 0,
    }
}

/// 模块拓扑排序：被依赖者在前；无依赖/环按字母序稳定输出
fn topo_order(names: &[String], edges: &[(String, String, i64)]) -> Vec<String> {
    use std::collections::{BTreeSet, VecDeque};
    let mut indegree: HashMap<String, usize> = names.iter().map(|n| (n.clone(), 0)).collect();
    let mut dependents: HashMap<String, Vec<String>> = HashMap::new();
    for (from, to, _) in edges {
        if indegree.contains_key(from) && indegree.contains_key(to) {
            *indegree.entry(from.clone()).or_insert(0) += 1; // from 依赖 to → from 的入度
            dependents.entry(to.clone()).or_default().push(from.clone());
        }
    }
    let mut ready: BTreeSet<String> = indegree
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(n, _)| n.clone())
        .collect();
    let mut ordered = Vec::with_capacity(names.len());
    while let Some(first) = ready.iter().next().cloned() {
        ready.remove(&first);
        ordered.push(first.clone());
        if let Some(deps) = dependents.get(&first) {
            for d in deps.clone() {
                if let Some(v) = indegree.get_mut(&d) {
                    *v -= 1;
                    if *v == 0 {
                        ready.insert(d);
                    }
                }
            }
        }
    }
    // 环上剩余节点按字母序补齐
    for n in names {
        if !ordered.contains(n) {
            ordered.push(n.clone());
        }
    }
    let _ = VecDeque::<String>::new();
    ordered
}

/// 读取代码片段（截断行数与行宽）
fn read_snippet(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut out = String::new();
    for line in content.lines().take(MAX_SNIPPET_LINES) {
        let l: String = line.chars().take(MAX_LINE_LEN).collect();
        out.push_str(&l);
        out.push('\n');
    }
    Some(out)
}

/// 清理 LLM 输出：去掉整篇代码块包裹与首尾空白
fn sanitize_md(text: &str) -> String {
    let t = text.trim();
    let t = t.strip_prefix("```markdown").or_else(|| t.strip_prefix("```md")).unwrap_or(t);
    let t = t.strip_suffix("```").unwrap_or(t);
    t.trim().to_string()
}

/// 由计划中的模块依赖生成 Mermaid flowchart（LR）。
///
/// 节点 = 全部模块（label 含文件数）；边 = 跨模块真实引用（带计数）；
/// 孤立模块仅作为节点出现，不产生边。节点 id 经 safe_filename 清洗。
fn build_dependency_graph(plan: &GeneratePlan) -> String {
    let mut out = String::from("flowchart LR\n");
    for m in &plan.modules {
        let id = safe_filename(&m.name);
        out.push_str(&format!("  {id}[\"{} ({})\"]\n", m.name, m.file_count));
    }
    for m in &plan.modules {
        // depends_on 已是去重模块列表；这里带边计数（同目标聚合）
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for d in &m.depends_on {
            *counts.entry(d.as_str()).or_insert(0) += 1;
        }
        for (to, n) in counts {
            out.push_str(&format!(
                "  {} -->|{}| {}\n",
                safe_filename(&m.name),
                n,
                safe_filename(to)
            ));
        }
    }
    out
}

/// 模块名 → 安全文件名（路径分隔符等替换为下划线）
fn safe_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => c,
            _ => '_',
        })
        .collect()
}

fn move_dir(src: &Path, dst: &Path) -> CoreResult<()> {
    std::fs::create_dir_all(dst.parent().unwrap_or(Path::new(".")))?;
    if std::fs::rename(src, dst).is_err() {
        // 跨盘回退：递归复制
        std::fs::create_dir_all(dst)?;
        for entry in walkdir::WalkDir::new(src) {
            let entry = entry.map_err(|e| CoreError::Other(anyhow::anyhow!("{e}")))?;
            let rel = entry
                .path()
                .strip_prefix(src)
                .map_err(|e| CoreError::Other(anyhow::anyhow!("{e}")))?;
            let to = dst.join(rel);
            if entry.file_type().is_dir() {
                std::fs::create_dir_all(&to)?;
            } else {
                std::fs::copy(entry.path(), &to)?;
            }
        }
        let _ = std::fs::remove_dir_all(src);
    }
    Ok(())
}

const MODULE_SYSTEM_PROMPT: &str = "你是资深软件架构师，为代码仓库撰写结构化技术文档。\
输出中文 Markdown，基于给定索引信息客观描述，不臆造不存在的文件或接口；\
文件路径原样引用。禁止输出与文档无关的寒暄。";

const ARCH_SYSTEM_PROMPT: &str = "你是资深软件架构师，基于模块规划与模块文档摘要撰写仓库顶层架构文档。\
输出中文 Markdown，模块名与依赖关系必须与给定信息一致，不臆造。";

#[cfg(test)]
mod tests;
