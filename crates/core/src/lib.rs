//! byte-wiki 核心库
//!
//! Repo Wiki 业务逻辑 + 数据层的单一真相源。
//! 不依赖任何具体 transport（stdio/http）或 UI 框架，
//! CLI（apps/cli）和桌面端（apps/desktop/src-tauri）各自实现 transport/UI 层，
//! 共同复用本 crate 暴露的 [`service::AppService`]。

pub mod cards;
pub mod callchain;
pub mod db;
pub mod error;
pub mod generator;
pub mod indexer;
pub mod lang_ext;
pub mod llm;
pub mod model;
pub mod rag;
pub mod repo;
pub mod service;
pub mod shortid;
pub mod watch;

#[cfg(feature = "mcp")]
pub mod mcp;

pub use error::{CoreError, CoreResult};
pub use db::{now_iso, RepoInfo};
pub use model::*;
pub use service::{
    AppService, clear_data_dir, resolve_app_root, resolve_data_dir, restore_files,
    set_data_dir, set_exclude_dirs,
};
