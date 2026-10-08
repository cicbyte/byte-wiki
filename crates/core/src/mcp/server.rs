//! MCP server：rmcp 工具定义 + 双 transport 启动
//!
//! 骨架阶段仅暴露 `status` 工具；Wiki 检索/问答工具随后续功能补充。

use std::sync::Arc;

use anyhow::Result;
use rmcp::{
    handler::server::router::tool::ToolRouter,
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
        .with_server_info(Implementation::from_build_env())
        .with_protocol_version(ProtocolVersion::V_2024_11_05)
        .with_instructions(
            "byte-wiki — 本地优先的仓库级知识引擎（Repo Wiki）。\n\
             当前工具：status() — 服务状态。Wiki 检索/问答工具随后续版本提供。"
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
