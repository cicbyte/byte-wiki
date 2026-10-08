//! 应用状态：持有 core 的 AppService 单例 + MCP/API 服务运行状态
//!
//! AppService 内部（SQLite 连接）自带锁、字段只读，天然 Sync，
//! 故用 Arc 共享而非外层 Mutex——async 命令可跨 await 持有引用。

use std::sync::{Arc, Mutex};
use byte_wiki_core::AppService;

/// MCP HTTP 服务的运行状态
pub struct McpServerState {
    pub cancel: tokio_util::sync::CancellationToken,
    pub port: u16,
}

/// REST API 服务的运行状态
pub struct ApiServerState {
    pub cancel: tokio_util::sync::CancellationToken,
    pub port: u16,
}

/// 应用状态：持有 core 的 AppService 单例 + MCP/API 服务状态
pub struct AppState {
    pub svc: Arc<AppService>,
    pub mcp: Mutex<Option<McpServerState>>,
    pub api: Mutex<Option<ApiServerState>>,
}

impl AppState {
    pub fn svc(&self) -> &AppService {
        &self.svc
    }
}
