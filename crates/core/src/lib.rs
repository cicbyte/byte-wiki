//! byte-wiki 核心库
//!
//! Repo Wiki 业务逻辑 + 数据层的单一真相源。
//! 不依赖任何具体 transport（stdio/http）或 UI 框架，
//! CLI（apps/cli）和桌面端（apps/desktop/src-tauri）各自实现 transport/UI 层，
//! 共同复用本 crate 暴露的 [`service::AppService`]。

pub mod cards;
pub mod branch;
pub mod callchain;
pub mod callchain_db;
pub mod db;
pub mod error;
pub mod generator;
pub mod git_read;
pub mod git;
pub mod indexer;
pub mod lang_ext;
pub mod llm;
pub mod lsp;
pub mod llm_cache;
pub mod llm_log;
pub mod model;
pub mod overview;
pub mod rag;
pub mod repo;
pub mod service;
pub mod shortid;
pub mod tasks;
pub mod watch;
pub mod wiki_export;

#[cfg(feature = "mcp")]
pub mod mcp;

pub use error::{CoreError, CoreResult};
pub use db::{now_iso, RepoInfo};
pub use model::*;
pub use service::{
    AppService, clear_data_dir, resolve_app_root, resolve_data_dir, restore_files,
    set_data_dir, set_exclude_dirs,
};

/// canonicalize 并剥掉 Windows verbatim 前缀（`\\?\`、`\\?\UNC\`）。
///
/// `std::fs::canonicalize` 在 Windows 返回 `\\?\C:\...` 形式，作为 repos.path
/// 存储键会原样透传到 UI 显示；剥离后两种形式指向同一目录，fs 操作均有效。
/// 存储与查询必须统一走本函数，避免键不一致。
pub fn canonical_key(root: &std::path::Path) -> std::io::Result<String> {
    let s = root.canonicalize()?.to_string_lossy().into_owned();
    Ok(strip_verbatim(&s))
}

/// 剥离 Windows verbatim 路径前缀（幂等，非 verbatim 原样返回）
pub fn strip_verbatim(path: &str) -> String {
    if let Some(t) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{t}")
    } else if let Some(t) = path.strip_prefix(r"\\?\") {
        t.to_string()
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::strip_verbatim;

    #[test]
    fn strip_verbatim_prefixes() {
        assert_eq!(strip_verbatim(r"\\?\C:\dev\repo"), r"C:\dev\repo");
        assert_eq!(strip_verbatim(r"\\?\UNC\server\share\x"), r"\\server\share\x");
        assert_eq!(strip_verbatim(r"C:\dev\repo"), r"C:\dev\repo");
        assert_eq!(strip_verbatim("/home/u/repo"), "/home/u/repo");
    }
}
