//! MCP server：rmcp 工具定义 + 双 transport 启动
//!
//! 工具面（9 个，全部只读）：status / list_repos / wiki_outline / read_wiki_doc /
//! ask_repo / callgraph_overview / callgraph_focus / callgraph_chain / symbol_search
//! —— Wiki 阅读、RAG 问答、调用链三查询一站暴露给 AI agent；写入类工具随 M3 开放。
//! rmcp 2.x 参数用 `Parameters<T>` 聚合（字段 doc 注释进 JSON Schema 描述）。

use std::sync::Arc;

use anyhow::Result;
use rmcp::{
    handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::*,
    tool, tool_handler, tool_router,
    ErrorData as McpError, ServerHandler, ServiceExt,
};

use crate::AppService;

/// MCP 服务器：每个会话持有独立的 AppService（本地 SQLite 短查询，开销可忽略）
#[derive(Clone)]
pub struct ByteWikiMcpServer {
    svc: Arc<AppService>,
    // tool_router 字段由 #[tool_handler] 宏在生成的 ServerHandler 实现里读取，
    // 编译器静态分析看不到该引用，故显式抑制 dead_code。
    #[allow(dead_code)]
    tool_router: ToolRouter<ByteWikiMcpServer>,
}

impl ByteWikiMcpServer {
    pub fn new() -> Result<Self> {
        Ok(Self {
            svc: Arc::new(AppService::open_default()?),
            tool_router: Self::tool_router(),
        })
    }
}

// ============ 工具参数（字段 doc 注释 → JSON Schema 参数描述） ============

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct RepoBranchParams {
    /// 仓库绝对路径（list_repos 可查）
    repo: String,
    /// 分支（缺省 = 仓库当前分支）
    #[serde(default)]
    branch: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ReadDocParams {
    /// 仓库绝对路径
    repo: String,
    /// 文档路径（wiki_outline 可查；cards/ 前缀读人工知识卡）
    doc_path: String,
    /// 分支（缺省 = 仓库当前分支）
    #[serde(default)]
    branch: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct AskRepoParams {
    /// 仓库绝对路径
    repo: String,
    /// 问题（基于该仓库 Wiki/知识卡的向量检索回答）
    question: String,
    /// 分支（缺省 = 仓库当前分支）
    #[serde(default)]
    branch: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct FocusParams {
    /// 仓库绝对路径
    repo: String,
    /// 种子节点 ID（symbol_search 可查）
    seed: String,
    /// 跳数（1-6，缺省 2）
    #[serde(default)]
    depth: Option<usize>,
    /// 节点数上限（1-1000，缺省 300）
    #[serde(default)]
    cap: Option<usize>,
    /// 分支（缺省 = 仓库当前分支）
    #[serde(default)]
    branch: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ChainParams {
    /// 仓库绝对路径
    repo: String,
    /// 起点节点 ID（symbol_search 可查）
    node: String,
    /// 块数上限（1-400，缺省 200）
    #[serde(default)]
    cap: Option<usize>,
    /// 分支（缺省 = 仓库当前分支）
    #[serde(default)]
    branch: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct SymbolSearchParams {
    /// 仓库绝对路径
    repo: String,
    /// 模糊模式（qualname/file 包含即命中，≥2 字符）
    pattern: String,
    /// 条数上限（1-100，缺省 20）
    #[serde(default)]
    limit: Option<usize>,
    /// 分支（缺省 = 仓库当前分支）
    #[serde(default)]
    branch: Option<String>,
}

// ============ 辅助 ============

/// CoreError/anyhow → MCP 错误（INTERNAL_ERROR：agent 侧能读到中文语义）
fn err(e: impl std::fmt::Display) -> McpError {
    McpError::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
}

/// 解析仓库路径 → 已登记行（repo 参数统一收仓库绝对路径，list_repos 可查）
fn resolve_repo(svc: &AppService, repo: &str) -> Result<crate::db::RepoInfo, McpError> {
    let canon = crate::canonical_key(std::path::Path::new(repo)).map_err(err)?;
    svc.db
        .get_repo_by_path(&canon)
        .map_err(err)?
        .ok_or_else(|| err(format!("仓库未登记：{repo}（用 list_repos 查看已登记仓库）")))
}

fn json_result(v: impl serde::Serialize) -> CallToolResult {
    text_result(serde_json::to_string_pretty(&v).unwrap_or_default())
}

#[tool_router]
impl ByteWikiMcpServer {
    /// 服务状态
    #[tool(description = "返回 byte-wiki 服务状态：版本、数据目录、已登记仓库数。")]
    async fn status(&self) -> Result<CallToolResult, McpError> {
        let repos = self.svc.db.repo_count().unwrap_or(-1);
        Ok(text_result(format!(
            "byte-wiki {}\n数据目录：{}\n已登记仓库：{}",
            env!("CARGO_PKG_VERSION"),
            self.svc.data_dir().display(),
            repos
        )))
    }

    /// 已登记仓库清单
    #[tool(description = "列出已登记的仓库：名称、路径（后续工具的 repo 参数）、分支、文件数、最近生成时间。")]
    async fn list_repos(&self) -> Result<CallToolResult, McpError> {
        let repos = self.svc.db.list_recent_repos(100).map_err(err)?;
        if repos.is_empty() {
            return Ok(text_result("尚无已登记仓库（在桌面端打开一个 Git 仓库即可登记）".into()));
        }
        let mut out = String::from("已登记仓库：\n");
        for r in repos {
            out.push_str(&format!(
                "- {}  path={}  branch={}  files={}  generated={}\n",
                r.name,
                r.path,
                r.branch.as_deref().unwrap_or("-"),
                r.file_count,
                r.last_generated_at.as_deref().unwrap_or("-"),
            ));
        }
        Ok(text_result(out))
    }

    /// Wiki 文档清单
    #[tool(description = "仓库 Wiki 文档清单（路径/类型/标题），doc_path 供 read_wiki_doc 使用。branch 缺省=当前分支。")]
    async fn wiki_outline(
        &self,
        Parameters(RepoBranchParams { repo, branch }): Parameters<RepoBranchParams>,
    ) -> Result<CallToolResult, McpError> {
        let info = resolve_repo(&self.svc, &repo)?;
        let branch =
            branch.unwrap_or_else(|| info.branch.clone().filter(|b| !b.is_empty()).unwrap_or_default());
        let docs = self.svc.db.list_wiki_docs(info.id, &branch).map_err(err)?;
        if docs.is_empty() {
            return Ok(text_result(format!(
                "该分支（{branch}）尚无 Wiki 文档——先在桌面端「一键生成 Wiki」"
            )));
        }
        let mut out = format!("Wiki 文档（{} 篇，branch={branch}）：\n", docs.len());
        for (path, kind, title) in docs {
            out.push_str(&format!("- [{kind}] {path} — {title}\n"));
        }
        Ok(text_result(out))
    }

    /// 读文档原文
    #[tool(description = "读取 Wiki 文档原文（Markdown）。doc_path 来自 wiki_outline；cards/ 前缀读人工知识卡（跨分支共享）。")]
    async fn read_wiki_doc(
        &self,
        Parameters(ReadDocParams { repo, doc_path, branch }): Parameters<ReadDocParams>,
    ) -> Result<CallToolResult, McpError> {
        let root = std::path::Path::new(&repo)
            .canonicalize()
            .map_err(|e| err(format!("仓库路径无效：{repo}（{e}）")))?;
        // 与桌面端 read_wiki_doc 同口径：cards 跨分支共享，其余产物按分支
        let branch = branch.unwrap_or_else(|| crate::branch::current_branch(&root));
        let (wiki, rel) = if let Some(card) = doc_path.strip_prefix("cards/") {
            (crate::branch::cards_root(&root), card)
        } else {
            (crate::branch::wiki_root(&root, &branch), doc_path.as_str())
        };
        let full = wiki
            .join(rel)
            .canonicalize()
            .map_err(|_| err(format!("文档不存在：{doc_path}（用 wiki_outline 查有效路径）")))?;
        if !full.starts_with(&wiki) {
            return Err(err(format!("非法路径：{doc_path}")));
        }
        let meta = std::fs::metadata(&full).map_err(err)?;
        if meta.len() > 2 * 1024 * 1024 {
            return Err(err(format!("文档过大（{}KB，上限 2MB）", meta.len() / 1024)));
        }
        let text = std::fs::read_to_string(&full).map_err(err)?;
        Ok(text_result(text))
    }

    /// RAG 问答
    #[tool(description = "仓库 RAG 问答：基于 Wiki/知识卡的向量检索回答，返回答案与出处清单。需先建向量索引（生成 Wiki 时自动）。")]
    async fn ask_repo(
        &self,
        Parameters(AskRepoParams { repo, question, branch }): Parameters<AskRepoParams>,
    ) -> Result<CallToolResult, McpError> {
        let ans = self
            .svc
            .chat_with_repo_ext(
                &repo,
                &question,
                &[],
                None,
                branch.as_deref(),
                &|_| {},
                &|_| {},
                &|| false,
            )
            .await
            .map_err(err)?;
        let mut out = ans.text;
        if !ans.sources.is_empty() {
            out.push_str("\n\n出处：\n");
            for s in &ans.sources {
                out.push_str(&format!("- {s}\n"));
            }
        }
        Ok(text_result(out))
    }

    /// 调用图概览
    #[tool(description = "调用图概览（JSON）：函数/边/入口统计、入口清单（零入口库回退公共 API 面）、模块列表与模块聚合边、滞后状态。")]
    async fn callgraph_overview(
        &self,
        Parameters(RepoBranchParams { repo, branch }): Parameters<RepoBranchParams>,
    ) -> Result<CallToolResult, McpError> {
        let ov = self
            .svc
            .callgraph_overview(&repo, branch.as_deref())
            .map_err(err)?
            .ok_or_else(|| err("尚未生成调用链（先 bw callchain <repo>）"))?;
        Ok(json_result(ov))
    }

    /// 邻域子图
    #[tool(description = "种子节点的 N 跳双向邻域子图（JSON：nodes/edges）。seed 为节点 ID，可用 symbol_search 查。")]
    async fn callgraph_focus(
        &self,
        Parameters(FocusParams { repo, seed, depth, cap, branch }): Parameters<FocusParams>,
    ) -> Result<CallToolResult, McpError> {
        let sub = self
            .svc
            .callgraph_focus(
                &repo,
                branch.as_deref(),
                &seed,
                depth.unwrap_or(2).clamp(1, 6),
                cap.unwrap_or(300).clamp(1, 1000),
            )
            .map_err(err)?
            .ok_or_else(|| err("尚未生成调用链（先 bw callchain <repo>）"))?;
        Ok(json_result(sub))
    }

    /// 走读链
    #[tool(description = "走读链（JSON）：从函数 DFS 先序展开整条执行流，含主路径与断点边界报告（断在哪一行、候选目标）。node 为节点 ID。")]
    async fn callgraph_chain(
        &self,
        Parameters(ChainParams { repo, node, cap, branch }): Parameters<ChainParams>,
    ) -> Result<CallToolResult, McpError> {
        let ch = self
            .svc
            .callgraph_chain(&repo, branch.as_deref(), &node, cap.unwrap_or(200).clamp(1, 400))
            .map_err(err)?
            .ok_or_else(|| err("尚未生成调用链（先 bw callchain <repo>）"))?;
        Ok(json_result(ch))
    }

    /// 符号搜索
    #[tool(description = "符号搜索：按 qualname/file 模糊匹配函数节点，返回 id/file/module/is_entry（定位 focus/chain 的种子节点用）。")]
    async fn symbol_search(
        &self,
        Parameters(SymbolSearchParams { repo, pattern, limit, branch }): Parameters<SymbolSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        let hits = self
            .svc
            .callgraph_search(&repo, branch.as_deref(), &pattern, limit.unwrap_or(20).clamp(1, 100))
            .map_err(err)?;
        if hits.is_empty() {
            return Ok(text_result("无命中（模式至少 2 个字符；也可先确认已生成调用链）".into()));
        }
        Ok(json_result(hits))
    }
}

/// ServerHandler：声明能力并挂上工具路由
#[tool_handler]
impl ServerHandler for ByteWikiMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
        // from_build_env 拿到的是 rmcp 自己的名字——客户端工具列表会显示它，显式纠正
        .with_server_info(Implementation::new("byte-wiki", env!("CARGO_PKG_VERSION")))
        .with_protocol_version(ProtocolVersion::V_2024_11_05)
        .with_instructions(
            "byte-wiki — 本地优先的仓库级知识引擎。\n\
             工具：status（服务状态）/ list_repos（仓库清单）→ wiki_outline + read_wiki_doc（读 Wiki）\
             / ask_repo（RAG 问答）/ callgraph_overview + symbol_search + callgraph_focus + \
             callgraph_chain（调用链查询）。repo 参数为仓库绝对路径（list_repos 可查）；\
             branch 缺省 = 当前分支。"
                .to_string(),
        )
    }
}

fn text_result(text: String) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(text)])
}

// ============ transport ============

/// stdio 模式
pub async fn serve_stdio() -> Result<()> {
    let server = ByteWikiMcpServer::new()?;
    tracing::info!("MCP stdio server starting");
    let service = server
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|e| anyhow::anyhow!("stdio serve 失败：{e:?}"))?;
    service.waiting().await?;
    Ok(())
}

/// http 模式（StreamableHTTP via axum）。api_key 非空则启用认证。
pub async fn serve_http(host: &str, port: u16, api_key: &str) -> Result<()> {
    let ct = tokio_util::sync::CancellationToken::new();
    let handle = serve_http_managed(host, port, api_key.to_string(), ct.clone())?;
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        ct.cancel();
    });
    handle.await?;
    Ok(())
}

/// 启动 HTTP MCP 服务，返回 JoinHandle。
///
/// `api_key` 非空时启用认证中间件（X-API-Key / Bearer token）。
/// 桌面端用此函数：spawn 到后台，存 CancellationToken 用于外部停止。
pub fn serve_http_managed(
    host: &str,
    port: u16,
    api_key: String,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<tokio::task::JoinHandle<()>> {
    use rmcp::transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    };

    let service = StreamableHttpService::new(
        || {
            ByteWikiMcpServer::new()
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default().with_cancellation_token(cancel),
    );

    let mut app = axum::Router::new().nest_service("/mcp", service);

    // 可选 API Key 认证
    if !api_key.is_empty() {
        app = app.layer(axum::middleware::from_fn_with_state(
            api_key,
            mcp_auth_middleware,
        ));
        tracing::info!("MCP HTTP server enabled API Key auth");
    }

    let addr = format!("{host}:{port}");
    let rt = tokio::runtime::Handle::current();
    let handle = rt.spawn(async move {
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(listener) => {
                tracing::info!("MCP HTTP server listening on http://{}/mcp", addr);
                let _ = axum::serve(listener, app).await;
            }
            Err(e) => tracing::error!("MCP HTTP bind {addr} 失败：{e}"),
        }
    });
    Ok(handle)
}

/// MCP HTTP API Key 认证中间件（可选启用）
async fn mcp_auth_middleware(
    axum::extract::State(expected_key): axum::extract::State<String>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    // 检查 X-API-Key 头
    let provided = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if provided == expected_key {
        return next.run(req).await;
    }
    // 检查 Bearer token
    let auth = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if let Some(token) = auth.strip_prefix("Bearer ") {
        if token == expected_key {
            return next.run(req).await;
        }
    }
    use axum::response::IntoResponse;
    (
        axum::http::StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({"error": "Invalid or missing API key"})),
    )
        .into_response()
}
