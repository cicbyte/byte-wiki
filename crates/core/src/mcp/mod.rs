//! MCP 服务器（rmcp v2.2）
//!
//! 当前为骨架：暴露 `status` 工具供 AI 客户端探测服务状态。
//! Wiki 查询/问答工具随后续功能补充（PRD：MCP 逐步改造为 Wiki 服务）。
//!
//! 双 transport：stdio（Claude Desktop / Cursor）+ http（多客户端/调试）

pub mod server;
pub mod rest;

use anyhow::Result;

/// stdio 模式启动
pub async fn serve_stdio() -> Result<()> {
    server::serve_stdio().await
}

/// http 模式启动。api_key 非空则启用认证。
pub async fn serve_http(host: &str, port: u16, api_key: &str) -> Result<()> {
    server::serve_http(host, port, api_key).await
}

/// 启动 HTTP MCP 服务（managed，api_key 非空则启用认证）
pub fn serve_http_managed(
    host: &str,
    port: u16,
    api_key: String,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<tokio::task::JoinHandle<()>> {
    server::serve_http_managed(host, port, api_key, cancel)
}
