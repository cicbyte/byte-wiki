//! MCP 服务器 / REST API 服务器命令

use clap::{Args, Subcommand, ValueEnum};

use byte_wiki_core::mcp;

use super::open_service;

#[derive(Args, Debug)]
pub struct McpArgs {
    #[command(subcommand)]
    pub action: McpAction,
}

#[derive(Subcommand, Debug)]
pub enum McpAction {
    /// 启动 MCP 服务器
    Serve {
        /// 传输方式：stdio（默认，供 Claude Desktop/Cursor）| http（本地 HTTP，多客户端/调试）
        #[arg(long, value_enum, default_value_t = Transport::Stdio)]
        transport: Transport,
        /// HTTP 模式监听端口
        #[arg(long, default_value = "8217")]
        port: u16,
        /// HTTP 模式绑定地址
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
    },
}

#[derive(ValueEnum, Clone, Debug)]
pub enum Transport {
    /// 标准输入输出（Claude Desktop / Cursor 等）
    Stdio,
    /// 本地 HTTP（StreamableHTTP，多客户端/调试）
    Http,
}

#[derive(Args, Debug)]
pub struct ApiArgs {
    #[command(subcommand)]
    pub action: ApiAction,
}

#[derive(Subcommand, Debug)]
pub enum ApiAction {
    /// 启动 REST API 服务器
    Serve {
        /// 监听端口
        #[arg(long, default_value = "8318")]
        port: u16,
        /// 绑定地址
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
    },
}

pub async fn run_mcp(args: McpArgs) -> anyhow::Result<()> {
    match args.action {
        McpAction::Serve {
            transport,
            port,
            host,
        } => match transport {
            Transport::Stdio => mcp::serve_stdio().await,
            Transport::Http => {
                let svc = open_service(false)?;
                let cfg = svc.config()?;
                let mcp_key = cfg.api.mcp_key;
                if mcp_key.is_empty() {
                    eprintln!("⚠️  MCP HTTP 无 API Key 认证保护。可在 config.toml 设置 [api] mcp_key");
                } else {
                    eprintln!("MCP API Key: {mcp_key}");
                }
                mcp::serve_http(&host, port, &mcp_key).await
            }
        },
    }
}

pub async fn run_api(args: ApiArgs) -> anyhow::Result<()> {
    use byte_wiki_core::mcp::rest;
    use std::sync::Arc;

    match args.action {
        ApiAction::Serve { port, host } => {
            let svc = open_service(false)?;
            let cfg = svc.config()?;
            let api_key = cfg.api.key.clone();
            if api_key.is_empty() {
                eprintln!("⚠️  API Key 未配置，REST API 将无认证保护。请在 config.toml 设置 [api] key");
            } else {
                eprintln!("API Key: {api_key}");
            }
            let svc_arc = Arc::new(svc);
            let cancel = tokio_util::sync::CancellationToken::new();
            let ct_clone = cancel.clone();
            let handle = rest::serve_api_http_managed(
                svc_arc, &host, port, api_key, cfg.api.allow_local_no_key, cancel,
            )?;
            eprintln!("REST API 服务已启动: http://{host}:{port}/api/v1");
            eprintln!("按 Ctrl+C 停止");
            tokio::spawn(async move {
                tokio::signal::ctrl_c().await.ok();
                ct_clone.cancel();
            });
            let _ = handle.await;
            eprintln!("REST API 服务已停止");
            Ok(())
        }
    }
}
