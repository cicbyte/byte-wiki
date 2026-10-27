//! Wiki 生成流水线（Qoder 式：大纲先行、逐页填充）
//!
//! 五阶段：索引 → **LLM 大纲设计**（outline.json：章节/页面/重要性/
//! 绑定文件/互链/逐页 brief）→ **逐页生成**（重要性排序，绑定文件带行号
//! 真实源码 + RAG 补充 + 强化 prompt）→ 总览页 → 组装落盘。
//!
//! - 大纲：仓库事实（文件树 + README + 模块依赖）喂给 LLM 决定「这个仓库
//!   该讲什么」，替代旧的目录镜像规划；brief 字段是大纲期写好的逐页指令
//! - 逐页：每页绑定 3-12 个真实文件，LLM 读到带行号的源码，产出带
//!   `Sources: [path:line-line]()` 引用的深度文档；后处理把引用转为
//!   `bw-file://` 内部跳转链接（文件浏览页定位行）
//! - 工程面：页级重试（2 次）+ 失败占位页（不阻塞整体）+ 重要性排序
//! - 增量更新：复用既有大纲，仅重生成绑定文件有变更的页面
//! - 进度：`on_event` 回调携带页面级状态（pages_done/total/当前页），
//!   桌面端转 Tauri 事件，大纲确定即推全量页面骨架

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::Serialize;

use crate::db::now_iso;
use crate::generator::outline::{
    normalize_outline, parse_outline, read_outline, OutlinePage, WikiOutline,
};
use crate::generator::pages::{page_messages, overview_messages};
use crate::generator::postprocess::{postprocess_page, safe_name, unique_name};
use crate::llm::{ChatMessage, ChatOptions, LlmClient};
use crate::llm_log::LlmLogEntry;
use crate::service::AppService;
use crate::{CoreError, CoreResult};

pub mod agent_outline;
pub mod outline;
pub mod pages;
pub mod postprocess;

/// 生成进度事件（推送给前端实时展示）
#[derive(Debug, Clone, Serialize)]
pub struct GenerateEvent {
    /// 阶段：indexing / outlining / generating / assembling / embedding / done / error
    pub phase: String,
    /// 当前序号（generating 阶段为已完成页数 + 1）
    pub current: usize,
    /// 总数（generating 阶段为页面总数）
    pub total: usize,
    /// 人类可读信息（页面标题/错误原因）
    pub message: String,
    /// 累计 prompt token
    pub prompt_tokens: u64,
    /// 累计 completion token
    pub completion_tokens: u64,
    /// 页面级状态增量（大纲确定事件携带全量骨架；此后每页携带单条变更）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pages: Option<Vec<PageProgress>>,
    /// 流式增量（携带时为终端事件，不参与进度展示）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<StreamChunk>,
    /// 归属项目（前端按项目过滤；本进程内事件必填）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_id: Option<i64>,
    /// 归属分支（任务启动时捕获；前端任务页/项目页按分支路由）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

/// 流式增量（右下角终端实时显示；高频小事件，与进度事件同回调、异通道）
#[derive(Debug, Clone, Serialize)]
pub struct StreamChunk {
    /// 调用标识（如「《核心库》撰写中」），前端按此分段
    pub call: String,
    /// meta（调用开始/结束标记）/ think / content
    pub channel: String,
    pub delta: String,
    /// 页面 id（正文流式按页累积，前端「点击实时查看」用）
    #[serde(default)]
    pub page_id: Option<String>,
    /// 归属项目
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_id: Option<i64>,
    /// 归属分支
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

/// 单页进度（前端按 id 合并出页面清单）
#[derive(Debug, Clone, Serialize)]
pub struct PageProgress {
    pub id: String,
    pub title: String,
    /// 章节链展示（"系统架构 / 后端"）
    pub section: String,
    /// high / medium / low
    pub importance: String,
    /// pending / generating / done / failed
    pub status: String,
    /// .bytewiki/ 内目标路径（前端文档树骨架定位 + 生成后无缝衔接磁盘文档）
    #[serde(default)]
    pub path: Option<String>,
}

/// 生成结果摘要
#[derive(Debug, Clone, Serialize)]
pub struct GenerateSummary {
    pub output_dir: String,
    pub pages_generated: usize,
    pub pages_failed: usize,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    /// 耗时（毫秒）
    pub elapsed_ms: u64,
}

/// 增量更新结果摘要
#[derive(Debug, Clone, Serialize)]
pub struct UpdateSummary {
    pub regenerated_pages: usize,
    pub changed_files: usize,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub elapsed_ms: u64,
    /// 无变更（什么都没做）
    pub unchanged: bool,
}

/// 页面重试次数（额外重试 2 次，共 3 次尝试）
const PAGE_RETRIES: usize = 2;

/// 供应商限流（429 / rate_limit / TPM·RPM 超限）：立即重试只会继续撞墙，
/// 退避等待是唯一有效手段
fn is_rate_limited(err: &crate::CoreError) -> bool {
    let s = err.to_string().to_lowercase();
    s.contains("429") || s.contains("rate_limit") || s.contains("too many requests") || s.contains("tpm") || s.contains("rpm")
}

/// 限流退避：第 n 次重试前等 20s * n，推送事件让进度面板可见
async fn backoff_on_rate_limit(
    err: &crate::CoreError,
    attempt: usize,
    title: &str,
    on_event: &(dyn Fn(GenerateEvent) + Send + Sync),
) {
    if !is_rate_limited(err) {
        return;
    }
    let wait = 20 * attempt as u64;
    on_event(ev("generating", 0, 0, format!("「{title}」触发限流（429），退避 {wait}s 后重试（页间大 prompt 打爆 TPM）")));
    tracing::warn!("限流退避 {wait}s：{title}");
    tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
}

/// 生成代次：prompt 结构发生实质变化时 bump，旧代页面不再沿用（整批重生成）
const GEN_VERSION: u32 = 1;

/// 生成元数据（断点续跑的兼容性判定：同供应商+同模型+同代次才沿用旧页面）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct GenMeta {
    provider: String,
    model: String,
    gen_version: u32,
}

fn gen_meta_path(root: &Path, branch: &str) -> std::path::PathBuf {
    crate::branch::wiki_root(root, branch).join(".gen-meta.json")
}

fn read_gen_meta(root: &Path, branch: &str) -> Option<GenMeta> {
    let text = std::fs::read_to_string(gen_meta_path(root, branch)).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_gen_meta(root: &Path, branch: &str, provider_id: &str, model: &str) {
    let meta = GenMeta {
        provider: provider_id.to_string(),
        model: model.to_string(),
        gen_version: GEN_VERSION,
    };
    let _ = std::fs::create_dir_all(crate::branch::wiki_root(root, branch));
    if let Ok(text) = serde_json::to_string_pretty(&meta) {
        let _ = std::fs::write(gen_meta_path(root, branch), text);
    }
}

// ============ 生成管线数据 ============

/// 模块上下文（统计 + 文件归属 + 依赖），大纲与页面 prompt 的公共输入
#[derive(Debug, Clone)]
struct ModuleCtx {
    name: String,
    langs: Vec<String>,
    file_count: usize,
    files: Vec<(String, String)>, // (path, lang)
    depends_on: Vec<String>,
}

/// 页面任务：生成或沿用既有内容
#[derive(Debug)]
enum PageAction {
    Generate,
    /// 增量更新：内容未变，原样写回
    Keep(String),
}

#[derive(Debug)]
struct PageJob {
    page: OutlinePage,
    /// .bytewiki/ 内相对路径（pages/…/x.md）
    rel_path: String,
    action: PageAction,
}

/// 页面产物（顺序与 outline.pages 一致）
#[derive(Debug, Clone)]
struct PageOutcome {
    rel_path: String,
    title: String,
    content: String,
    failed: bool,
}

impl AppService {
    // ============ 一键生成（全量） ============

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
        let branch = crate::branch::current_branch(root);

        // ---- 阶段 1：索引 ----
        on_event(ev("indexing", 0, 0, "建立文件与引用索引"));
        let summary = self.index_repo(&canon, &|n| {
            on_event(ev("indexing", n, 0, format!("已扫描 {n} 个文件")));
        })?;
        let modules = self.module_context(repo.id, &branch, &summary)?;
        let known_files: HashSet<String> =
            modules.iter().flat_map(|m| m.files.iter().map(|(p, _)| p.clone())).collect();
        let file_module: HashMap<String, String> = modules
            .iter()
            .flat_map(|m| m.files.iter().map(|(p, _)| (p.clone(), m.name.clone())))
            .collect();

        let provider = self.active_llm()?;
        let client = LlmClient::for_provider(&provider)?;

        // 后台任务注册 + 事件包裹：标注 repo_id（前端按项目过滤）并同步注册表
        let task_repo_id = repo.id;
        let task_branch = branch.clone();
        self.tasks.begin(task_repo_id, &task_branch, &repo.name, repo_path, "generate");
        let wrapped = |mut e: GenerateEvent| {
            e.repo_id = Some(task_repo_id);
            e.branch = Some(task_branch.clone());
            if let Some(chunk) = &mut e.stream {
                chunk.repo_id = Some(task_repo_id);
                chunk.branch = Some(task_branch.clone());
            }
            self.tasks.update(task_repo_id, &e);
            on_event(e);
        };
        let on_event: &(dyn Fn(GenerateEvent) + Send + Sync) = &wrapped;

        // ---- 阶段 2：LLM 大纲设计 ----
        on_event(ev("outlining", 0, 0, format!("设计 Wiki 大纲（{} 个文件）", known_files.len())));
        let outline = self
            .design_outline_auto(&client, repo.id, &repo.name, &branch, &modules, &known_files, repo_path, on_event)
            .await?;
        let total = outline.pages.len();
        // 路径分配先行：骨架事件即带目标路径（前端树骨架与生成后磁盘文档无缝衔接）
        let mut jobs = self.plan_jobs(&outline);
        // 断点续跑：上次中断已落盘且未过期的页面直接沿用（读旧元数据判定）
        self.resume_keep_pages(root, &branch, &mut jobs, &provider);
        let path_of: HashMap<String, String> =
            jobs.iter().map(|j| (j.page.id.clone(), j.rel_path.clone())).collect();
        on_event(GenerateEvent {
            phase: "outlining".into(),
            current: 0,
            total,
            message: format!("大纲确定：{} 章节 · {} 个页面", outline.sections.len(), total),
            prompt_tokens: 0,
            completion_tokens: 0,
            pages: Some(skeleton(&outline, &path_of)),
            stream: None,
            repo_id: None,
            branch: None,
        });
        persist_outline(root, &branch, &outline);
        write_gen_meta(root, &branch, &provider.id, &provider.chat_model);

        // ---- 阶段 3：总览页（仅依赖大纲与路径；提前生成——
        //      逐页可能耗时很久，避免「全部页面完成后还剩一次长调用」的最后一公里静默，
        //      且中途重启时总览与已完成页面均已进缓存，重跑零成本） ----
        on_event(ev("generating", 0, total, "生成总览页"));
        let rel_paths: Vec<String> = jobs.iter().map(|j| j.rel_path.clone()).collect();
        let (overview, out) = match self
            .generate_overview(&client, repo.id, &repo.name, &branch, &outline, &rel_paths, &modules, repo_path, on_event)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("总览页生成失败（用占位替代）：{e}");
                on_event(ev("generating", 0, total, format!("总览页生成失败（占位）：{e}")));
                ("# Wiki 总览\n\n> 总览页生成失败，可稍后重新生成。\n".to_string(), Default::default())
            }
        };
        let mut tokens = (out.prompt_tokens, out.completion_tokens);

        // ---- 阶段 4：逐页生成 ----
        let (results, failed) = self
            .run_page_jobs(&client, repo.id, &branch, &repo.name, repo_path, root, &outline, &jobs, &known_files, &file_module, &modules, on_event, &mut tokens)
            .await?;

        // ---- 阶段 5：组装落盘（全量成功后原子替换）----
        on_event(ev("assembling", 0, 0, "写入 .bytewiki/"));
        let output_dir =
            self.assemble_wiki(&canon, repo.id, &branch, &outline, &results, &overview, &modules, &provider.chat_model)?;

        let detail = format!(
            "pages={} failed={} prompt_tok={} completion_tok={}",
            total, failed, tokens.0, tokens.1
        );
        self.finish_pipeline(&canon, repo.id, &branch, "generate", &detail,
            &format!("生成完成：{} 个页面（{} 失败）", total - failed, failed), on_event)
        .await?;

        Ok(GenerateSummary {
            output_dir,
            pages_generated: total - failed,
            pages_failed: failed,
            prompt_tokens: tokens.0,
            completion_tokens: tokens.1,
            elapsed_ms: started.elapsed().as_millis() as u64,
        })
    }

    // ============ 增量更新（页级） ============

    /// 增量更新（FR-06）：复用既有大纲，仅重生成「绑定文件有变更」的页面；
    /// 无 outline.json（旧版 Wiki）时退化为全量重建。cards/ 与调用链产物不触碰。
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
        let branch = crate::branch::current_branch(root);

        // ---- 1）变更检测 ----
        on_event(ev("checking", 0, 0, "检测代码变更"));
        let report = self.check_staleness(&canon)?;
        if report.changes.is_empty() {
            return Ok(UpdateSummary {
                regenerated_pages: 0,
                changed_files: 0,
                prompt_tokens: 0,
                completion_tokens: 0,
                elapsed_ms: started.elapsed().as_millis() as u64,
                unchanged: true,
            });
        }
        let changed: HashSet<String> = report.changes.iter().map(|c| c.path.clone()).collect();
        let changed_files = changed.len();

        // ---- 2）重新索引 ----
        on_event(ev("indexing", 0, 0, format!("重扫索引（{} 个文件变更）", changed_files)));
        let summary = self.index_repo(&canon, &|n| {
            on_event(ev("indexing", n, 0, format!("已扫描 {n} 个文件")));
        })?;
        let modules = self.module_context(repo.id, &branch, &summary)?;
        let known_files: HashSet<String> =
            modules.iter().flat_map(|m| m.files.iter().map(|(p, _)| p.clone())).collect();
        let file_module: HashMap<String, String> = modules
            .iter()
            .flat_map(|m| m.files.iter().map(|(p, _)| (p.clone(), m.name.clone())))
            .collect();

        let provider = self.active_llm()?;
        let client = LlmClient::for_provider(&provider)?;
        let wiki_dir = crate::branch::wiki_root(root, &branch);

        // 后台任务注册 + 事件包裹：标注 repo_id（前端按项目过滤）并同步注册表
        let task_repo_id = repo.id;
        let task_branch = branch.clone();
        self.tasks.begin(task_repo_id, &task_branch, &repo.name, repo_path, "update");
        let wrapped = |mut e: GenerateEvent| {
            e.repo_id = Some(task_repo_id);
            e.branch = Some(task_branch.clone());
            if let Some(chunk) = &mut e.stream {
                chunk.repo_id = Some(task_repo_id);
                chunk.branch = Some(task_branch.clone());
            }
            self.tasks.update(task_repo_id, &e);
            on_event(e);
        };
        let on_event: &(dyn Fn(GenerateEvent) + Send + Sync) = &wrapped;

        // ---- 3）大纲：优先复用（保持文档结构稳定），无则全量重建 ----
        // outline_reused：大纲复用时总览也沿用（总览输入只有大纲+模块清单，
        // 不含页面内容——页面内容更新不影响总览语义，省一次 LLM）
        let mut outline_reused = false;
        let outline = match read_outline(&wiki_dir) {
            Some(o) if !o.pages.is_empty() => {
                outline_reused = true;
                o
            }
            _ => {
                on_event(ev("outlining", 0, 0, "未找到既有大纲，重新设计"));
                self.design_outline_auto(&client, repo.id, &repo.name, &branch, &modules, &known_files, repo_path, on_event)
                    .await?
            }
        };
        let total = outline.pages.len();
        // 路径分配先行：骨架事件即带目标路径
        let mut jobs = self.plan_jobs(&outline);
        let path_of: HashMap<String, String> =
            jobs.iter().map(|j| (j.page.id.clone(), j.rel_path.clone())).collect();
        on_event(GenerateEvent {
            phase: "outlining".into(),
            current: 0,
            total,
            message: format!("大纲就绪：{} 个页面", total),
            prompt_tokens: 0,
            completion_tokens: 0,
            pages: Some(skeleton(&outline, &path_of)),
            stream: None,
            repo_id: None,
            branch: None,
        });
        persist_outline(root, &branch, &outline);
        write_gen_meta(root, &branch, &provider.id, &provider.chat_model);

        // ---- 4）页任务：绑定文件 ∩ 变更 → 重生成；其余从磁盘沿用 ----
        for job in &mut jobs {
            let untouched = !job.page.files.iter().any(|f| changed.contains(f));
            if untouched {
                if let Ok(old) = std::fs::read_to_string(wiki_dir.join(&job.rel_path)) {
                    if !old.trim().is_empty() && !old.contains("本页生成失败") {
                        job.action = PageAction::Keep(old);
                    }
                }
            }
        }
        let regen = jobs.iter().filter(|j| matches!(j.action, PageAction::Generate)).count();

        // ---- 5）总览（提前：仅依赖大纲与路径）+ 逐页 + 组装 ----
        let rel_paths: Vec<String> = jobs.iter().map(|j| j.rel_path.clone()).collect();
        let (overview, out) = match std::fs::read_to_string(wiki_dir.join("overview.md")) {
            // 大纲复用 + 磁盘有既有总览 → 沿用（增量更新不再每次重生成总览）
            Ok(s) if outline_reused && !s.trim().is_empty() => {
                on_event(ev("generating", 0, total, "大纲未变，总览沿用既有内容"));
                (s, Default::default())
            }
            _ => {
                on_event(ev("generating", 0, total, "重新生成总览页"));
                match self
                    .generate_overview(&client, repo.id, &repo.name, &branch, &outline, &rel_paths, &modules, repo_path, on_event)
                    .await
                {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!("总览页生成失败（用占位替代）：{e}");
                        ("# Wiki 总览\n\n> 总览页生成失败，可稍后重新生成。\n".to_string(), Default::default())
                    }
                }
            }
        };
        let mut tokens = (out.prompt_tokens, out.completion_tokens);
        let (results, failed) = self
            .run_page_jobs(&client, repo.id, &branch, &repo.name, repo_path, root, &outline, &jobs, &known_files, &file_module, &modules, on_event, &mut tokens)
            .await?;

        on_event(ev("assembling", 0, 0, "写入 .bytewiki/"));
        self.assemble_wiki(&canon, repo.id, &branch, &outline, &results, &overview, &modules, &provider.chat_model)?;

        let detail = format!(
            "files={} pages_regenerated={} failed={} prompt_tok={} completion_tok={}",
            changed_files, regen, failed, tokens.0, tokens.1
        );
        let failed_note = if failed > 0 { format!("（{failed} 页失败占位）") } else { String::new() };
        self.finish_pipeline(&canon, repo.id, &branch, "update", &detail,
            &format!("更新完成：{regen} 个页面重生成{failed_note}（{changed_files} 个文件变更）"), on_event)
        .await?;

        Ok(UpdateSummary {
            regenerated_pages: regen,
            changed_files,
            prompt_tokens: tokens.0,
            completion_tokens: tokens.1,
            elapsed_ms: started.elapsed().as_millis() as u64,
            unchanged: false,
        })
    }

    // ============ 内部：阶段实现 ============

    /// LLM 大纲设计（流式输出 + 心跳事件；解析失败带错误重试一次）
    #[allow(clippy::too_many_arguments)]
    /// 大纲入口（auto）：偏好开启则先走 agent 多轮探查；agent 失败或解析失败
    /// 回退单发管线（新路径不能成为单点故障）
    #[allow(clippy::too_many_arguments)]
    async fn design_outline_auto(
        &self,
        client: &LlmClient,
        repo_id: i64,
        repo_name: &str,
        branch: &str,
        modules: &[ModuleCtx],
        known_files: &HashSet<String>,
        repo_path: &str,
        on_event: &(dyn Fn(GenerateEvent) + Send + Sync),
    ) -> CoreResult<WikiOutline> {
        let agent_on = self.outline_agent_enabled.load(std::sync::atomic::Ordering::Relaxed)
            && self.config().map(|c| c.preferences.outline_agent).unwrap_or(true);
        if agent_on {
            on_event(ev("outlining", 0, 0, "大纲 agent 探查仓库（多轮）"));
            let files: Vec<(String, String, u64)> = self.indexed_files(repo_path)?;
            let readme = read_readme(Path::new(repo_path));
            let modules_tri: Vec<(String, usize, Vec<String>)> = modules
                .iter()
                .map(|m| (m.name.clone(), m.file_count, m.langs.clone()))
                .collect();
            let edges: Vec<(String, String, i64)> = modules
                .iter()
                .flat_map(|m| m.depends_on.iter().map(|t| (m.name.clone(), t.clone(), 1i64)).collect::<Vec<_>>())
                .collect();
            let initial = outline::build_outline_prompt(repo_name, &files, &readme, &modules_tri, &edges);
            match crate::generator::agent_outline::run_agent_outline(
                self, client, repo_id, repo_name, repo_path, &initial, on_event,
            )
            .await
            {
                Ok(text) => match parse_outline(&text) {
                    Ok(o) => {
                        let normalized = normalize_outline(o, known_files);
                        if !normalized.pages.is_empty() {
                            return Ok(normalized);
                        }
                        tracing::warn!("agent 大纲归一化后无有效页面，回退单发");
                    }
                    Err(e) => tracing::warn!("agent 大纲解析失败，回退单发：{e}"),
                },
                Err(e) => tracing::warn!("agent 大纲失败，回退单发：{e}"),
            }
            on_event(ev("outlining", 0, 0, "agent 失败，回退单发大纲"));
        }
        self.design_outline(client, repo_id, repo_name, branch, modules, known_files, repo_path, on_event)
            .await
    }

    async fn design_outline(
        &self,
        client: &LlmClient,
        repo_id: i64,
        repo_name: &str,
        branch: &str,
        modules: &[ModuleCtx],
        known_files: &HashSet<String>,
        repo_path: &str,
        on_event: &(dyn Fn(GenerateEvent) + Send + Sync),
    ) -> CoreResult<WikiOutline> {
        let files: Vec<(String, String, u64)> = self.indexed_files(repo_path)?;
        let edges: Vec<(String, String, i64)> = modules
            .iter()
            .flat_map(|m| {
                m.depends_on.iter().map(|t| (m.name.clone(), t.clone(), 1i64)).collect::<Vec<_>>()
            })
            .collect();
        let modules_tri: Vec<(String, usize, Vec<String>)> = modules
            .iter()
            .map(|m| (m.name.clone(), m.file_count, m.langs.clone()))
            .collect();
        let readme = read_readme(Path::new(repo_path));

        let mut user = outline::build_outline_prompt(repo_name, &files, &readme, &modules_tri, &edges);
        let system = "你是资深软件架构师与技术主编，为代码仓库设计 Wiki 结构大纲。\
只输出要求的 XML，不要任何解释或 markdown 代码块围栏。";
        let mut last_err = String::new();
        for attempt in 0..=1 {
            if attempt == 1 {
                user.push_str(&format!(
                    "\n\n【重试】上一次输出解析失败：{last_err}。请严格按 XML 格式重新输出完整大纲。"
                ));
            }
            let messages = vec![ChatMessage::system(system), ChatMessage::user(user.clone())];
            let out = match self
                .llm_call_logged(client, repo_id, repo_name, branch, "outline", "Wiki 大纲", attempt + 1,
                    &messages, "outlining", 0, 0, "大纲生成中", None, on_event)
                .await
            {
                Ok(o) => o,
                Err(e) if is_rate_limited(&e) => {
                    backoff_on_rate_limit(&e, attempt as usize + 1, "Wiki 大纲", on_event).await;
                    last_err = e.to_string();
                    continue;
                }
                Err(e) => return Err(e),
            };
            match parse_outline(&out.text) {
                Ok(o) => {
                    let normalized = normalize_outline(o, known_files);
                    if normalized.pages.is_empty() {
                        last_err = "归一化后没有任何页面（文件引用全部无效）".into();
                        continue;
                    }
                    return Ok(normalized);
                }
                Err(e) => last_err = e,
            }
        }
        Err(CoreError::Other(anyhow::anyhow!("大纲解析失败：{last_err}")))
    }

    /// 页面任务规划：路径分配（全部待生成；增量场景由调用方按需翻转为 Keep）
    fn plan_jobs(&self, outline: &WikiOutline) -> Vec<PageJob> {
        let mut dir_taken: HashMap<String, HashSet<String>> = HashMap::new();
        let mut jobs = Vec::with_capacity(outline.pages.len());
        for page in &outline.pages {
            let rel = page_rel_path(outline, page, &mut dir_taken).unwrap_or_else(|| {
                // 兜底：根目录唯一名（理论上 page_rel_path 不会失败）
                let root_taken = dir_taken.entry("pages".into()).or_default();
                format!("pages/{}.md", unique_name(&safe_name(&page.title), root_taken))
            });
            jobs.push(PageJob { page: page.clone(), rel_path: rel, action: PageAction::Generate });
        }
        jobs
    }

    /// 执行页面任务：Generate 页走 LLM（重试 + 失败占位），Keep 页原样返回。
    /// 返回值顺序与 outline.pages 一致。
    #[allow(clippy::too_many_arguments)]
    async fn run_page_jobs(
        &self,
        client: &LlmClient,
        repo_id: i64,
        branch: &str,
        repo_name: &str,
        repo_path: &str,
        root: &Path,
        outline: &WikiOutline,
        jobs: &[PageJob],
        known_files: &HashSet<String>,
        file_module: &HashMap<String, String>,
        modules: &[ModuleCtx],
        on_event: &(dyn Fn(GenerateEvent) + Send + Sync),
        tokens: &mut (u64, u64),
    ) -> CoreResult<(Vec<PageOutcome>, usize)> {
        let total = jobs.len();
        let mut results: Vec<PageOutcome> = Vec::with_capacity(total);
        let mut failed = 0usize;
        let mut done = 0usize;
        let edges: Vec<(String, String, i64)> = modules
            .iter()
            .flat_map(|m| {
                m.depends_on.iter().map(|t| (m.name.clone(), t.clone(), 1i64)).collect::<Vec<_>>()
            })
            .collect();

        // 沿用页一次性批量置 done——进度只「走」真实重生成的页。此前逐页推
        // done 事件，每次更新进度条都是 0/N→N/N 全量走一遍，观感等同全量重新
        // 生成（批量事件同时解决「骨架里恒灰」：沿用页开局即 done）
        let kept_pages: Vec<PageProgress> = jobs
            .iter()
            .filter(|j| matches!(j.action, PageAction::Keep(_)))
            .map(|j| {
                let chain = outline.section_chain(&j.page).join(" / ");
                page_progress(&j.page, &chain, "done", Some(j.rel_path.clone()))
            })
            .collect();
        let kept = kept_pages.len();
        if kept > 0 {
            on_event(GenerateEvent {
                phase: "generating".into(),
                current: kept,
                total,
                message: format!("沿用 {kept} 个未变页面"),
                prompt_tokens: 0,
                completion_tokens: 0,
                pages: Some(kept_pages),
                stream: None,
                repo_id: None,
                branch: None,
            });
        }

        for job in jobs {
            let page = &job.page;
            let chain = outline.section_chain(page).join(" / ");
            if let PageAction::Keep(content) = &job.action {
                done += 1; // 批量事件已置 done，这里只收集结果不推事件
                results.push(PageOutcome {
                    rel_path: job.rel_path.clone(),
                    title: page.title.clone(),
                    content: content.clone(),
                    failed: false,
                });
                continue;
            }

            done += 1;
            on_event(GenerateEvent {
                phase: "generating".into(),
                current: done,
                total,
                message: format!("生成页面：{}", page.title),
                prompt_tokens: 0,
                completion_tokens: 0,
                pages: Some(vec![page_progress(page, &chain, "generating", Some(job.rel_path.clone()))]),
                stream: None,
                repo_id: None,
                branch: None,
            });

            let mut last_err: Option<String> = None;
            let mut content: Option<String> = None;
            for attempt in 0..=PAGE_RETRIES {
                if attempt > 0 {
                    tracing::warn!("页面「{}」第 {attempt} 次重试", page.title);
                }
                match self.generate_one_page(client, repo_id, repo_name, branch, repo_path, root, outline, page, known_files, file_module, modules, &edges, done, total, attempt as u32 + 1, on_event).await {
                    Ok((c, out)) => {
                        tokens.0 += out.prompt_tokens;
                        tokens.1 += out.completion_tokens;
                        content = Some(c);
                        break;
                    }
                    Err(e) => {
                        if attempt < PAGE_RETRIES {
                            backoff_on_rate_limit(&e, attempt + 1, &page.title, on_event).await;
                        }
                        last_err = Some(e.to_string());
                    }
                }
            }

            let (final_content, ok) = match content {
                Some(c) => (c, true),
                None => {
                    failed += 1;
                    (placeholder_page(page, last_err.as_deref().unwrap_or("未知错误")), false)
                }
            };
            // 实时落盘：页面完成即写入正式目录并登记——长任务（数十页 × 分钟级）
            // 中断时进度持久（重开即见，重跑未完成页即可）；最终组装以相同内容
            // 幂等替换，Keep 页内容本就来自磁盘不再重写
            let target = wiki_branch_root(root, &branch).join(&job.rel_path);
            if let Some(parent) = target.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::write(&target, &final_content) {
                tracing::warn!("页面实时落盘失败（最终组装时会重写）：{e}");
            } else {
                let _ = self.db.upsert_generated_doc(repo_id, branch, "page", &job.rel_path, &page.title);
            }
            results.push(PageOutcome {
                rel_path: job.rel_path.clone(),
                title: page.title.clone(),
                content: final_content,
                failed: !ok,
            });
            on_event(GenerateEvent {
                phase: "generating".into(),
                current: done,
                total,
                message: if ok {
                    format!("完成页面：{}", page.title)
                } else {
                    format!("页面失败（占位）：{}", page.title)
                },
                prompt_tokens: tokens.0,
                completion_tokens: tokens.1,
                pages: Some(vec![page_progress(page, &chain, if ok { "done" } else { "failed" }, Some(job.rel_path.clone()))]),
                stream: None,
                repo_id: None,
                branch: None,
            });
        }
        Ok((results, failed))
    }

    /// 单页生成：上下文（绑定文件 + RAG）→ prompt → LLM（流式 + 心跳）→ 后处理
    #[allow(clippy::too_many_arguments)]
    async fn generate_one_page(
        &self,
        client: &LlmClient,
        repo_id: i64,
        repo_name: &str,
        branch: &str,
        _repo_path: &str,
        root: &Path,
        outline: &WikiOutline,
        page: &OutlinePage,
        known_files: &HashSet<String>,
        file_module: &HashMap<String, String>,
        modules: &[ModuleCtx],
        edges: &[(String, String, i64)],
        current: usize,
        total: usize,
        attempt: u32,
        on_event: &(dyn Fn(GenerateEvent) + Send + Sync),
    ) -> CoreResult<(String, crate::llm::ChatOutcome)> {
        // 相关页（标题 + 定位）
        let related: Vec<String> = page
            .related
            .iter()
            .filter_map(|id| outline.pages.iter().find(|p| &p.id == id))
            .map(|p| {
                if p.description.is_empty() {
                    p.title.clone()
                } else {
                    format!("{}（{}）", p.title, p.description)
                }
            })
            .collect();

        // 模块上下文：页面绑定文件所属模块
        let mut mod_names: Vec<&str> = Vec::new();
        for f in &page.files {
            if let Some(m) = file_module.get(f) {
                if !mod_names.iter().any(|x| x == m) {
                    mod_names.push(m);
                }
            }
        }
        let mods: Vec<(String, usize, Vec<String>)> = mod_names
            .iter()
            .filter_map(|n| modules.iter().find(|m| m.name == *n))
            .map(|m| (m.name.clone(), m.file_count, m.langs.clone()))
            .collect();
        let module_ctx = outline::render_modules(&mods, edges);

        // 源文件档案（带行号）
        let (file_ctx, _full, _skipped) = crate::generator::pages::build_file_context(root, &page.files);

        // 注意：页面管线不拼 RAG 补充——chunks 是否存在会让 prompt 漂移，
        // 缓存键随之全变（曾导致重建向量索引后 38 页全部 miss 从 0 生成）。
        // 页上下文已含绑定文件全文，RAG 增量价值有限；确定性优先。
        let rag_ctx = String::new();

        let min_files = page.files.len().min(5);
        let messages = page_messages(
            &page.title, &page.brief, &page.description, &related, &module_ctx, &file_ctx, &rag_ctx, min_files,
        );
        let out = self
            .llm_call_logged(client, repo_id, repo_name, branch, "page", &page.title, attempt,
                &messages, "generating", current, total,
                &format!("《{}》撰写中", page.title), Some(&page.id), on_event)
            .await?;
        Ok((postprocess_page(&out.text, known_files), out))
    }

    /// 带日志的 LLM 调用：流式 + 心跳 + 全量 JSONL 留痕（失败也记一条）
    #[allow(clippy::too_many_arguments)]
    async fn llm_call_logged(
        &self,
        client: &LlmClient,
        repo_id: i64,
        repo_name: &str,
        branch: &str,
        stage: &str,
        title: &str,
        attempt: u32,
        messages: &[ChatMessage],
        phase: &str,
        current: usize,
        total: usize,
        label: &str,
        page_id: Option<&str>,
        on_event: &(dyn Fn(GenerateEvent) + Send + Sync),
    ) -> CoreResult<crate::llm::ChatOutcome> {
        let started = std::time::Instant::now();
        let system = messages.first().filter(|m| m.role == "system").map(|m| m.content.clone()).unwrap_or_default();
        let user = messages
            .iter()
            .filter(|m| m.role != "system")
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");

        // 缓存命中：直接回放文本（token 记 0——未真实消耗；日志带 cached 标记）
        let key = crate::llm_cache::cache_key(
            &client.provider_desc(),
            client.model_name(),
            &system,
            &user,
        );
        if let Some(hit) = self.cache_get(repo_id, &key) {
            self.append_llm_log(&LlmLogEntry {
                ts: crate::db::now_iso(),
                repo_id,
                repo_name: repo_name.to_string(),
                branch: branch.to_string(),
                stage: stage.to_string(),
                provider: client.provider_desc(),
                model: client.model_name().to_string(),
                title: title.to_string(),
                attempt,
                duration_ms: started.elapsed().as_millis() as u64,
                prompt_tokens: hit.prompt_tokens,
                completion_tokens: hit.completion_tokens,
                ok: true,
                cached: true,
                error: String::new(),
                system,
                user,
                response: hit.response.clone(),
            });
            tracing::debug!("LLM 缓存命中：{stage} / {title}");
            return Ok(crate::llm::ChatOutcome {
                text: hit.response,
                prompt_tokens: 0,
                completion_tokens: 0,
            });
        }

        on_event(GenerateEvent {
            phase: phase.to_string(),
            current,
            total,
            message: String::new(),
            prompt_tokens: 0,
            completion_tokens: 0,
            pages: None,
            repo_id: None,
            branch: None,
            stream: Some(StreamChunk {
                call: label.to_string(),
                channel: "meta".into(),
                delta: format!("
▶ [{stage}] {title}（第 {attempt} 次尝试）
"),
                page_id: page_id.map(str::to_string),
                repo_id: None,
                branch: None,
            }),
        });
        let result = stream_llm(client, messages, phase, current, total, label, page_id, on_event).await;
        on_event(GenerateEvent {
            phase: phase.to_string(),
            current,
            total,
            message: String::new(),
            prompt_tokens: 0,
            completion_tokens: 0,
            pages: None,
            repo_id: None,
            branch: None,
            stream: Some(StreamChunk {
                call: label.to_string(),
                channel: "meta".into(),
                delta: match &result {
                    Ok(o) => format!(
                        "
✔ [{stage}] 完成：{}ms · prompt {} / completion {} tokens
",
                        started.elapsed().as_millis(), o.prompt_tokens, o.completion_tokens
                    ),
                    Err(e) => format!("
✘ [{stage}] 失败：{e}
"),
                },
                page_id: page_id.map(str::to_string),
                repo_id: None,
                branch: None,
            }),
        });
        // 成功才缓存（失败结果不进缓存，重跑即重试）
        if let Ok(out) = &result {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            self.cache_set(
                repo_id,
                &crate::llm_cache::LlmCacheEntry {
                    prompt_hash: key,
                    response: out.text.clone(),
                    prompt_tokens: out.prompt_tokens,
                    completion_tokens: out.completion_tokens,
                    model: client.model_name().to_string(),
                    timestamp,
                },
            );
        }
        let entry = LlmLogEntry {
            ts: crate::db::now_iso(),
            repo_id,
            repo_name: repo_name.to_string(),
            branch: branch.to_string(),
            stage: stage.to_string(),
            provider: client.provider_desc(),
            model: client.model_name().to_string(),
            title: title.to_string(),
            attempt,
            duration_ms: started.elapsed().as_millis() as u64,
            prompt_tokens: result.as_ref().map(|o| o.prompt_tokens).unwrap_or(0),
            completion_tokens: result.as_ref().map(|o| o.completion_tokens).unwrap_or(0),
            ok: result.is_ok(),
            cached: false,
            error: result.as_ref().err().map(|e| e.to_string()).unwrap_or_default(),
            system,
            user,
            response: result.as_ref().map(|o| o.text.clone()).unwrap_or_default(),
        };
        self.append_llm_log(&entry);
        result
    }

    /// RAG 补充检索（best-effort：无索引/失败返回空）。
    /// 目前不接入页面管线（见 generate_one_page 注释）；预留给问答/未来
    /// 显式「RAG 增强生成」模式。
    #[allow(dead_code)]
    async fn rag_supplement(&self, repo_path: &str, query: &str) -> String {
        let Ok(canon) = crate::canonical_key(Path::new(repo_path)) else {
            return String::new();
        };
        let Ok(Some(repo)) = self.db.get_repo_by_path(&canon) else {
            return String::new();
        };
        if self.db.count_chunks(repo.id, &crate::branch::current_branch(Path::new(repo_path))).unwrap_or(0) == 0 {
            return String::new(); // 首次生成无向量索引，跳过
        }
        match self.search_similar(repo_path, None, query, 6).await {
            Ok(hits) if !hits.is_empty() => {
                let mut s = String::new();
                for h in &hits {
                    let piece: String = h.content.chars().take(600).collect();
                    s.push_str(&format!("--- {}（相似度 {:.2}）\n{}\n\n", h.doc_path, h.score, piece));
                    if s.len() > 4000 {
                        break;
                    }
                }
                s
            }
            _ => String::new(),
        }
    }

    /// 总览页：页面目录（真实相对链接）+ README + 依赖图（流式 + 心跳）。
    /// 只依赖大纲与页面路径（不依赖页面内容），可在逐页生成前调用。
    #[allow(clippy::too_many_arguments)]
    async fn generate_overview(
        &self,
        client: &LlmClient,
        repo_id: i64,
        repo_name: &str,
        branch: &str,
        outline: &WikiOutline,
        rel_paths: &[String],
        modules: &[ModuleCtx],
        repo_path: &str,
        on_event: &(dyn Fn(GenerateEvent) + Send + Sync),
    ) -> CoreResult<(String, crate::llm::ChatOutcome)> {
        // rel_paths 与 outline.pages 顺序一致（plan_jobs 保证），zip 得 id → rel
        let rel_of: HashMap<&str, &str> = outline
            .pages
            .iter()
            .map(|p| p.id.as_str())
            .zip(rel_paths.iter().map(|r| r.as_str()))
            .collect();

        // 目录：按章节组织；无章节页面归入「其他」
        let mut catalog = String::new();
        for section in &outline.sections {
            let pages: Vec<&OutlinePage> = outline
                .pages
                .iter()
                .filter(|p| p.section.as_deref() == Some(section.id.as_str()))
                .collect();
            if pages.is_empty() {
                continue;
            }
            catalog.push_str(&format!("### {}\n", section.title));
            for p in pages {
                let rel = rel_of.get(p.id.as_str()).copied().unwrap_or("");
                catalog.push_str(&format!("- [{}]({})：{}\n", p.title, rel, p.description));
            }
        }
        let rest: Vec<&OutlinePage> = outline.pages.iter().filter(|p| p.section.is_none()).collect();
        if !rest.is_empty() {
            catalog.push_str("### 其他\n");
            for p in rest {
                let rel = rel_of.get(p.id.as_str()).copied().unwrap_or("");
                catalog.push_str(&format!("- [{}]({})：{}\n", p.title, rel, p.description));
            }
        }

        let readme = read_readme(Path::new(repo_path));
        let readme_excerpt: String = readme.chars().take(6000).collect();
        let modules_tri: Vec<(String, usize, Vec<String>)> = modules
            .iter()
            .map(|m| (m.name.clone(), m.file_count, m.langs.clone()))
            .collect();
        let edges: Vec<(String, String, i64)> = modules
            .iter()
            .flat_map(|m| {
                m.depends_on.iter().map(|t| (m.name.clone(), t.clone(), 1i64)).collect::<Vec<_>>()
            })
            .collect();
        let dependency_mmd = build_dependency_graph(modules);
        let messages = overview_messages(
            repo_name,
            &outline.title,
            &outline.description,
            &catalog,
            &readme_excerpt,
            &outline::render_modules(&modules_tri, &edges),
            &dependency_mmd,
        );
        let out = self
            .llm_call_logged(client, repo_id, repo_name, branch, "overview", "Wiki 总览", 1,
                &messages, "generating", rel_paths.len(), rel_paths.len(), "总览页撰写中", None, on_event)
            .await?;
        Ok((postprocess::sanitize_md(&out.text), out))
    }

    /// 收尾：向量重建（best-effort）+ 变更日志 + done 事件
    async fn finish_pipeline(
        &self,
        canon: &str,
        repo_id: i64,
        branch: &str,
        kind: &str,
        detail: &str,
        done_msg: &str,
        on_event: &(dyn Fn(GenerateEvent) + Send + Sync),
    ) -> CoreResult<()> {
        on_event(ev("embedding", 0, 0, "重建向量索引"));
        match self.rebuild_embeddings(canon, &|m| on_event(ev("embedding", 0, 0, m))).await {
            Ok(s) => tracing::info!("向量索引完成：{} 块", s.embedded),
            Err(e) => {
                tracing::warn!("向量索引跳过（不影响 Wiki 产物）：{e}");
                on_event(ev("embedding", 0, 0, format!("向量索引跳过：{e}")));
            }
        }
        self.db.log_change(repo_id, branch, kind, detail)?;
        self.tasks.finish(repo_id, branch, &done_msg);
        on_event(ev("done", 0, 0, done_msg));
        Ok(())
    }

    // ============ 上下文构建 ============

    /// 索引文件清单 (path, lang, size)
    fn indexed_files(&self, repo_path: &str) -> CoreResult<Vec<(String, String, u64)>> {
        let canon = crate::canonical_key(Path::new(repo_path))?;
        let Some(repo) = self.db.get_repo_by_path(&canon)? else {
            return Err(CoreError::Other(anyhow::anyhow!("仓库未登记：{repo_path}")));
        };
        Ok(self
            .db
            .get_index_files_by_repo(repo.id, &crate::branch::current_branch(Path::new(repo_path)))?
            .into_iter()
            .map(|(path, lang, size, _module, _mtime)| (path, lang, size.max(0) as u64))
            .collect())
    }

    /// 模块上下文（按名排序稳定输出）
    fn module_context(&self, repo_id: i64, branch: &str, summary: &crate::indexer::IndexSummary) -> CoreResult<Vec<ModuleCtx>> {
        let files = self.db.get_index_files_by_repo(repo_id, branch)?;
        let edges = self.db.get_index_edges_by_repo(repo_id, branch)?;

        let mut by_module: HashMap<String, Vec<(String, String)>> = HashMap::new();
        for (path, lang, _size, module, _mtime) in &files {
            by_module.entry(module.clone()).or_default().push((path.clone(), lang.clone()));
        }
        for list in by_module.values_mut() {
            list.sort();
        }

        let mut modules: Vec<ModuleCtx> = summary
            .modules
            .iter()
            .map(|ms| ModuleCtx {
                name: ms.name.clone(),
                langs: ms.langs.clone(),
                file_count: ms.file_count,
                files: by_module.get(&ms.name).cloned().unwrap_or_default(),
                depends_on: edges.iter().filter(|(f, _, _)| *f == ms.name).map(|(_, t, _)| t.clone()).collect(),
            })
            .collect();
        modules.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(modules)
    }

    /// 断点续跑：上次运行已实时落盘且仍然有效的页面直接沿用（零 LLM 调用）。
    /// 有效性 = 元数据兼容（同供应商/模型/代次）且 非空、非失败占位、
    /// 绑定文件 mtime 均早于页面落盘（内容未过期）。
    fn resume_keep_pages(&self, root: &Path, branch: &str, jobs: &mut [PageJob], provider: &crate::model::LlmProvider) {
        let compatible = match read_gen_meta(root, branch) {
            Some(m) => {
                m.provider == provider.id && m.model == provider.chat_model && m.gen_version == GEN_VERSION
            }
            None => true, // 无元数据（旧版运行产物）：尽力沿用，本轮流写入元数据
        };
        if !compatible {
            tracing::info!("生成元数据不兼容（模型/代次变更），整批重新生成");
            return;
        }
        let mut kept = 0usize;
        for j in jobs.iter_mut() {
            let target = wiki_branch_root(root, &branch).join(&j.rel_path);
            let Ok(content) = std::fs::read_to_string(&target) else {
                continue; // 未落盘：待生成
            };
            if content.trim().is_empty() || content.contains("本页生成失败") {
                continue; // 空文件/失败占位：重做
            }
            let Ok(page_mtime) = std::fs::metadata(&target).and_then(|m| m.modified()) else {
                continue;
            };
            // 绑定源文件在页面落盘之后又改过 → 内容过期，重做
            let stale = j.page.files.iter().any(|f| {
                std::fs::metadata(root.join(f))
                    .and_then(|m| m.modified())
                    .map(|t| t > page_mtime)
                    .unwrap_or(false)
            });
            if stale {
                continue;
            }
            j.action = PageAction::Keep(content);
            kept += 1;
        }
        if kept > 0 {
            tracing::info!("断点续跑：沿用 {kept} 个已落盘页面");
        }
    }

    /// 指定查看分支的文档清单（只读视图：目标分支产物 + 共享 cards；
    /// view_branch=None 为当前分支即常规行为）
    pub fn list_wiki_docs_for(
        &self,
        repo_path: &str,
        view_branch: Option<&str>,
    ) -> CoreResult<Vec<(String, String, String)>> {
        let root = Path::new(repo_path);
        let branch = match view_branch {
            Some(b) if !b.is_empty() => b.to_string(),
            _ => crate::branch::current_branch(root),
        };
        // 分支目录名即 branch_dir 的转写（查看分支来自目录列表，天然一致）
        let wiki = crate::branch::wiki_root(root, &branch);
        let mut out: Vec<(String, String, String)> = Vec::new();
        for entry in walkdir::WalkDir::new(&wiki)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let Ok(rel) = entry.path().strip_prefix(&wiki) else { continue };
            let rel = rel.to_string_lossy().replace('\\', "/");
            let name = rel.rsplit('/').next().unwrap_or(&rel).to_string();
            if matches!(name.as_str(), "index.json" | "outline.json" | ".gen-meta.json") {
                continue;
            }
            let title = name.trim_end_matches(".md").trim_end_matches(".mmd").trim_end_matches(".json").to_string();
            // Wiki 视图只收文档类产物：graphs/（调用链/依赖图）有专门页面，不进树
            if rel.starts_with("graphs/") {
                continue;
            }
            let kind = if rel.starts_with("pages/") {
                "page"
            } else if rel.starts_with("modules/") {
                "module"
            } else {
                "architecture"
            };
            out.push((rel.clone(), kind.to_string(), title));
        }
        // 共享知识卡
        for entry in walkdir::WalkDir::new(crate::branch::cards_root(root))
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let Ok(rel) = entry.path().strip_prefix(crate::branch::cards_root(root)) else { continue };
            let rel = format!("cards/{}", rel.to_string_lossy().replace('\\', "/"));
            let title = rel.rsplit('/').next().unwrap_or(&rel).trim_end_matches(".md").to_string();
            out.push((rel, "card".to_string(), title));
        }
        out.sort();
        Ok(out)
    }

    /// 大纲既定顺序的页面路径表（文档树按大纲序渲染；无 outline.json 返回空）
    pub fn outline_page_paths(&self, repo_path: &str) -> CoreResult<Vec<(String, String)>> {
        let root = Path::new(repo_path);
        let Some(outline) = outline::read_outline(&crate::branch::wiki_root(root, &crate::branch::current_branch(root))) else {
            return Ok(vec![]);
        };
        let mut dir_taken: HashMap<String, HashSet<String>> = HashMap::new();
        let mut out = Vec::with_capacity(outline.pages.len());
        for page in &outline.pages {
            if let Some(rel) = page_rel_path(&outline, page, &mut dir_taken) {
                out.push((rel, page.title.clone()));
            }
        }
        Ok(out)
    }

    // ============ 既有 Wiki 加载（FR-08 / 克隆场景） ============

    /// 扫描仓库内 .bytewiki/ 目录，返回清单 (rel_path, kind, title) 并登记 wiki_docs。
    /// 只读文件系统，不触发任何 LLM 调用；目录不存在返回空表。
    pub fn scan_existing_wiki(&self, repo_root: &Path) -> CoreResult<Vec<(String, String, String)>> {
        // 当前分支产物 + 共享知识卡
        let branch = crate::branch::current_branch(repo_root);
        let out = self.scan_wiki_dir(&crate::branch::wiki_root(repo_root, &branch), "pages/")?;
        let cards = self.scan_wiki_dir(&crate::branch::cards_root(repo_root), "cards/")?;
        let mut out: Vec<(String, String, String)> = out.into_iter().map(|(p, k, t)| (p, k, t)).collect();
        out.extend(cards);
        let canon = crate::canonical_key(repo_root)?;
        if let Some(repo) = self.db.get_repo_by_path(&canon)? {
            if !out.is_empty() {
                self.db.replace_wiki_docs(repo.id, &branch, &out, &now_iso())?;
            }
        }
        out.sort();
        return Ok(out);
    }

    /// 扫描单个 wiki 目录（rel 前缀用于 kind 判定与 cards 路径归一）
    fn scan_wiki_dir(&self, wiki: &Path, _prefix: &str) -> CoreResult<Vec<(String, String, String)>> {
        if !wiki.is_dir() {
            return Ok(vec![]);
        }
        let mut out: Vec<(String, String, String)> = Vec::new();
        for entry in walkdir::WalkDir::new(&wiki)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let Ok(rel) = entry.path().strip_prefix(&wiki) else { continue };
            let rel = rel.to_string_lossy().replace('\\', "/");
            let name = rel.rsplit('/').next().unwrap_or(&rel).to_string();
            if name == "index.json" || name == "outline.json" || name == ".gen-meta.json" {
                continue; // 机器可读清单不进文档树
            }
            // 调用链 db 及其 WAL sidecar 是机器产物（#573），json 导出才是登记口径
            if name.starts_with("callgraph.db") {
                continue;
            }
            let title = name
                .trim_end_matches(".md")
                .trim_end_matches(".mmd")
                .trim_end_matches(".json")
                .to_string();
            let kind = if rel.starts_with("pages/") {
                "page"
            } else if rel.starts_with("modules/") {
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
        out.sort();
        Ok(out)
    }

    // ============ 组装 ============

    #[allow(clippy::too_many_arguments)]
    fn assemble_wiki(
        &self,
        repo_root: &str,
        repo_id: i64,
        branch: &str,
        outline: &WikiOutline,
        results: &[PageOutcome],
        overview: &str,
        modules: &[ModuleCtx],
        chat_model: &str,
    ) -> CoreResult<String> {
        let root = Path::new(repo_root);
        let wiki = crate::branch::wiki_root(root, branch);

        // 1) 全部产物写入暂存目录
        let staging = root.join(format!(".bytewiki.staging.{}", crate::branch::branch_dir(branch)));
        if staging.exists() {
            std::fs::remove_dir_all(&staging)?;
        }
        for r in results {
            let target = staging.join(&r.rel_path);
            std::fs::create_dir_all(target.parent().unwrap_or(&staging))?;
            std::fs::write(target, &r.content)?;
        }
        std::fs::create_dir_all(staging.join("graphs"))?;
        std::fs::write(staging.join("overview.md"), overview)?;
        std::fs::write(staging.join("outline.json"), serde_json::to_string_pretty(outline)?)?;

        // 依赖图谱：由索引的模块引用边生成（非 LLM 臆测）
        let dependency_mmd = build_dependency_graph(modules);
        std::fs::write(staging.join("graphs").join("dependency.mmd"), &dependency_mmd)?;
        let has_graph = dependency_mmd.lines().any(|l| l.contains("-->"));

        let mut files_json: Vec<serde_json::Value> =
            vec![serde_json::json!({ "path": "overview.md", "kind": "architecture" })];
        for r in results {
            files_json.push(serde_json::json!({
                "path": r.rel_path,
                "kind": "page",
                "title": r.title,
                "failed": r.failed,
            }));
        }
        if has_graph {
            files_json.push(serde_json::json!({ "path": "graphs/dependency.mmd", "kind": "graph" }));
        }
        let index_json = serde_json::json!({
            "version": 2,
            "generator": { "provider": "byte-wiki", "model": chat_model },
            "generated_at": now_iso(),
            "files": files_json,
        });
        std::fs::write(staging.join("index.json"), serde_json::to_string_pretty(&index_json)?)?;

        // 2) 替换正式目录（cards/ 与调用链产物保留）
        std::fs::create_dir_all(&wiki)?;
        swap_tree(&wiki, &staging, &["graphs/callchain"], &[
            "pages",
            "modules",
            "overview.md",
            "architecture.md",
            "outline.json",
            "index.json",
            "graphs/dependency.mmd",
        ])?;

        // 3) 登记 wiki_docs（生成区整体替换；cards 为人工来源不受影响）
        let mut doc_rows: Vec<(String, String, String)> = vec![(
            "overview.md".into(),
            "architecture".into(),
            "Wiki 总览".into(),
        )];
        for r in results {
            doc_rows.push((r.rel_path.clone(), "page".into(), r.title.clone()));
        }
        if has_graph {
            doc_rows.push(("graphs/dependency.mmd".into(), "graph".into(), "模块依赖图谱".into()));
        }
        if wiki.join(crate::callchain::CALLGRAPH_DOC_PATH).is_file() {
            doc_rows.push((
                crate::callchain::CALLGRAPH_DOC_PATH.into(),
                "graph".into(),
                "调用链图谱".into(),
            ));
        }
        self.db.replace_wiki_docs(repo_id, branch, &doc_rows, &now_iso())?;

        Ok(wiki.to_string_lossy().into_owned())
    }
}

// ============ 辅助函数 ============

/// 分支产物根（局部便捷别名）
fn wiki_branch_root(root: &Path, branch: &str) -> std::path::PathBuf {
    crate::branch::wiki_root(root, branch)
}

/// LLM 调用统一走流式：输出增量节流推「已输出 N 字」心跳事件，
/// 避免单次数分钟的静默期（前端看起来像卡死）
#[allow(clippy::too_many_arguments)]
async fn stream_llm(
    client: &LlmClient,
    messages: &[ChatMessage],
    phase: &str,
    current: usize,
    total: usize,
    label: &str,
    page_id: Option<&str>,
    on_event: &(dyn Fn(GenerateEvent) + Send + Sync),
) -> CoreResult<crate::llm::ChatOutcome> {
    // (上次心跳时间, 累计思考字数, 累计输出字数)
    // 思考型模型（reasoning_content）可能数分钟只出思考不出正文，
    // 心跳必须同时覆盖两个通道，否则长时间静默像卡死
    let state = std::sync::Arc::new(std::sync::Mutex::new((std::time::Instant::now(), 0usize, 0usize)));
    let st = std::sync::Arc::clone(&state);
    let phase_owned = phase.to_string();
    let label_owned = label.to_string();

    let heartbeat = |think: usize, out: usize| {
        let parts = match (think, out) {
            (0, 0) => "已发出请求，等待响应…".to_string(),
            (t, o) if t > 0 && o > 0 => format!("思考 {t} 字 · 已输出 {o} 字"),
            (t, 0) if t > 0 => format!("思考中 · {t} 字（正文未开始）"),
            (_, o) => format!("已输出 {o} 字"),
        };
        on_event(GenerateEvent {
            phase: phase_owned.clone(),
            current,
            total,
            message: format!("{label_owned} · {parts}"),
            prompt_tokens: 0,
            completion_tokens: 0,
            pages: None,
            stream: None,
            repo_id: None,
            branch: None,
        });
    };

    // 请求发出即报一次（网关整体缓冲时不至于完全静默）
    heartbeat(0, 0);

    let call_label = label.to_string();
    let stream_chunk = |channel: &str, delta: &str| {
        on_event(GenerateEvent {
            phase: phase.to_string(),
            current,
            total,
            message: String::new(),
            prompt_tokens: 0,
            completion_tokens: 0,
            pages: None,
            repo_id: None,
            branch: None,
            stream: Some(StreamChunk {
                call: call_label.clone(),
                channel: channel.to_string(),
                delta: delta.to_string(),
                page_id: page_id.map(str::to_string),
                repo_id: None,
                branch: None,
            }),
        });
    };

    let on_delta = move |delta: &str| {
        stream_chunk("content", delta);
        let mut g = st.lock().unwrap();
        g.2 += delta.chars().count();
        if g.0.elapsed() >= std::time::Duration::from_millis(1000) {
            let (t, o) = (g.1, g.2);
            *g = (std::time::Instant::now(), t, o);
            drop(g);
            heartbeat(t, o);
        }
    };
    let st2 = std::sync::Arc::clone(&state);
    let on_thinking = move |think: &str| {
        stream_chunk("think", think);
        let mut g = st2.lock().unwrap();
        g.1 += think.chars().count();
        if g.0.elapsed() >= std::time::Duration::from_millis(1000) {
            let (t, o) = (g.1, g.2);
            *g = (std::time::Instant::now(), t, o);
            drop(g);
            heartbeat(t, o);
        }
    };

    client
        .chat_stream_ext(messages, &ChatOptions::default(), &on_delta, &on_thinking, &|| false)
        .await
}


/// 大纲确定即持久化 outline.json：中断后重开应用，增量更新可按既有大纲续跑
fn persist_outline(root: &Path, branch: &str, outline: &WikiOutline) {
    let wiki = crate::branch::wiki_root(root, branch);
    let _ = std::fs::create_dir_all(&wiki);
    match serde_json::to_string_pretty(outline) {
        Ok(text) => {
            if let Err(e) = std::fs::write(wiki.join("outline.json"), text) {
                tracing::warn!("outline.json 提前落盘失败（组装时会重写）：{e}");
            }
        }
        Err(e) => tracing::warn!("outline 序列化失败：{e}"),
    }
}

fn ev(phase: &str, current: usize, total: usize, message: impl Into<String>) -> GenerateEvent {
    GenerateEvent {
        phase: phase.into(),
        current,
        total,
        message: message.into(),
        prompt_tokens: 0,
        completion_tokens: 0,
        pages: None,
        stream: None,
        repo_id: None,
        branch: None,
    }
}

/// 全量页面骨架（大纲确定事件，带目标路径）
fn skeleton(outline: &WikiOutline, path_of: &HashMap<String, String>) -> Vec<PageProgress> {
    outline
        .pages
        .iter()
        .map(|p| {
            page_progress(p, &outline.section_chain(p).join(" / "), "pending", path_of.get(&p.id).cloned())
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn page_progress(
    page: &OutlinePage,
    section: &str,
    status: &str,
    path: Option<String>,
) -> PageProgress {
    PageProgress {
        id: page.id.clone(),
        title: page.title.clone(),
        section: section.to_string(),
        importance: page.importance.clone(),
        status: status.into(),
        path,
    }
}

/// 页面 → .bytewiki/ 相对路径（pages/章节链/页名.md；同目录重名追加 -N）
fn page_rel_path(
    outline: &WikiOutline,
    page: &OutlinePage,
    dir_taken: &mut HashMap<String, HashSet<String>>,
) -> Option<String> {
    let mut parts = vec!["pages".to_string()];
    for t in outline.section_chain(page) {
        parts.push(safe_name(&t));
    }
    let dir = parts.join("/");
    let taken = dir_taken.entry(dir.clone()).or_default();
    let fname = unique_name(&safe_name(&page.title), taken);
    Some(format!("{dir}/{fname}.md"))
}

/// 失败占位页
fn placeholder_page(page: &OutlinePage, reason: &str) -> String {
    let mut s = format!("# {}\n\n> ⚠️ 本页生成失败：{reason}\n>\n> 可在总览页重新生成 Wiki 重试。\n\n", page.title);
    if !page.description.is_empty() {
        s.push_str(&format!("**大纲定位**：{}\n\n", page.description));
    }
    s.push_str("**绑定文件**\n");
    for f in &page.files {
        s.push_str(&format!("- {f}\n"));
    }
    s
}

/// 读 README（常见命名兜底）
fn read_readme(root: &Path) -> String {
    for name in ["README.md", "readme.md", "README.zh.md", "README"] {
        if let Ok(c) = std::fs::read_to_string(root.join(name)) {
            return c.chars().take(8000).collect();
        }
    }
    String::new()
}

/// 目录树替换：preserve 内的子树从旧目录搬到新目录，managed 内的路径由新目录接管。
/// （调用链产物 graphs/callchain 由独立管线拥有，不属于依赖图重生成范围）
fn swap_tree(
    wiki: &Path,
    staging: &Path,
    preserve: &[&str],
    managed: &[&str],
) -> CoreResult<()> {
    for rel in managed {
        let target = wiki.join(rel);
        if target.is_dir() {
            std::fs::remove_dir_all(&target)?;
        } else if target.exists() {
            std::fs::remove_file(&target)?;
        }
    }
    for rel in preserve {
        let from = wiki.join(rel);
        if from.exists() {
            let to = staging.join(rel);
            std::fs::create_dir_all(to.parent().unwrap_or(staging))?;
            move_dir(&from, &to)?;
        }
    }
    // staging 剩余内容整体搬入 wiki（保持子树结构）
    for entry in walkdir::WalkDir::new(staging)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path() != staging)
    {
        let rel = entry.path().strip_prefix(staging).unwrap();
        move_dir(entry.path(), &wiki.join(rel))?;
    }
    let _ = std::fs::remove_dir_all(staging);
    Ok(())
}

/// 由模块依赖生成 Mermaid flowchart（LR）。节点 = 全部模块（label 含文件数）；
/// 边 = 跨模块真实引用；孤立模块仅作节点。节点 id 经 safe_filename 清洗。
fn build_dependency_graph(modules: &[ModuleCtx]) -> String {
    let mut out = String::from("flowchart LR\n");
    for m in modules {
        let id = safe_filename(&m.name);
        out.push_str(&format!("  {id}[\"{} ({})\"]\n", m.name, m.file_count));
    }
    for m in modules {
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

/// 模块名 → 安全文件名（仅图谱节点 id 使用；中文模块名在 mermaid id 中
/// 可能引发兼容问题，统一转 ASCII 安全形态）
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

#[cfg(test)]
mod tests;
