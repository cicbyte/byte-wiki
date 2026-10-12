//! byte-wiki 桌面端 Tauri 后端
//!
//! 作为 byte-wiki-core 的薄封装层：每个 `#[tauri::command]` 调用 core 的 AppService，
//! 桌面端复用与 CLI 完全一致的业务逻辑。
//!
//! 命令按业务域拆分到 [`commands`] 各子模块，应用状态见 [`state`]。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod state;

use std::sync::Arc;
use byte_wiki_core::AppService;

use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let svc = Arc::new(AppService::open_default().expect("无法初始化 byte-wiki 数据目录"));
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            svc,
            mcp: std::sync::Mutex::new(None),
            api: std::sync::Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            // 系统
            commands::system::ping,
            commands::system::data_dir,
            commands::system::open_in_folder,
            commands::system::open_url,
            // 备份 / 数据目录 / 配置
            commands::config::backup,
            commands::config::restore,
            commands::config::set_data_dir,
            commands::config::clear_data_dir,
            commands::config::get_exclude_dirs,
            commands::config::set_exclude_dirs,
            commands::config::set_api_keys,
            // 模型 Provider（LLM）
            commands::llm::get_llm_config,
            commands::llm::set_llm_config,
            commands::llm::test_llm_connection,
            // 仓库（Repo Wiki）
            commands::repo::open_repo,
            commands::repo::recent_repos,
            commands::repo::index_repo,
            commands::repo::list_repo_dir,
            commands::repo::search_repo_files,
            commands::generate::generate_wiki,
            commands::generate::update_wiki,
            commands::generate::list_wiki_docs,
            commands::generate::read_wiki_doc,
            commands::generate::repo_chat,
            commands::generate::chat_cancel,
            commands::generate::rebuild_embeddings,
            // 调用链（函数级视图）
            commands::callchain::generate_callchain,
            commands::callchain::read_repo_file,
            commands::callchain::get_chain_sources,
            // 变更检测 + 知识卡（M2）
            commands::watch::check_staleness,
            commands::cards::create_card,
            commands::cards::delete_card,
            // 壁纸
            commands::wallpaper::upload_wallpaper,
            commands::wallpaper::list_wallpapers,
            commands::wallpaper::delete_wallpaper,
            // MCP HTTP 服务管理
            commands::servers::start_mcp_server,
            commands::servers::stop_mcp_server,
            commands::servers::mcp_server_status,
            // REST API 服务管理
            commands::servers::start_api_server,
            commands::servers::stop_api_server,
            commands::servers::api_server_status,
        ])
        .run(tauri::generate_context!())
        .expect("运行 byte-wiki 桌面端时出错");
}
