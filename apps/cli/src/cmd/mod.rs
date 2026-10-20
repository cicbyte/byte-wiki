//! CLI 子命令定义与实现，按业务域拆分到各子模块
//!
//! 通过 `pub use` 把各域的 Args 结构 / `run_*` 函数重导出到 `cmd::` 名字空间，
//! 使 `main.rs` 的引用路径（`cmd::BackupArgs`、`cmd::run_backup` 等）保持不变。

mod backup;
mod callchain;
mod server;

// ---- 备份 / 恢复 ----
pub use backup::{BackupArgs, RestoreArgs, run_backup, run_restore};

// ---- 调用链 ----
pub use callchain::{CallchainArgs, CallchainQueryArgs, run_callchain, run_callchain_query};

// ---- MCP / REST API 服务 ----
// McpAction / Transport 是 McpArgs 的内部字段类型（server 模块内已用），main 不直接引用，故不重导出
pub use server::{ApiAction, ApiArgs, McpArgs, run_api, run_mcp};

// ===== 公共辅助（供各域 run_* 复用）=====

/// 打开 service；失败时返回 anyhow 错误（json 参数保留签名兼容，当前两分支等价）
pub(crate) fn open_service(json: bool) -> anyhow::Result<byte_wiki_core::AppService> {
    byte_wiki_core::AppService::open_default().map_err(|e| {
        if json {
            // JSON 模式下错误也要可解析——但这里直接转 anyhow，main 会打印
            anyhow::anyhow!("初始化数据目录失败：{e}")
        } else {
            anyhow::anyhow!("初始化数据目录失败：{e}")
        }
    })
}
