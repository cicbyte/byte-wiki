//! 仓库命令：打开（校验+登记）、最近列表、索引

use byte_wiki_core::indexer::IndexSummary;
use byte_wiki_core::RepoInfo;

use crate::state::AppState;

/// 打开本地 Git 仓库（校验 + 快速统计 + 登记），失败返回可读错误
#[tauri::command]
pub fn open_repo(state: tauri::State<'_, AppState>, path: String) -> Result<RepoInfo, String> {
    state.svc().open_repo(&path).map_err(|e| e.to_string())
}

/// 最近打开的仓库列表（默认 10 条）
#[tauri::command]
pub fn recent_repos(
    state: tauri::State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<RepoInfo>, String> {
    state
        .svc()
        .recent_repos(limit.unwrap_or(10))
        .map_err(|e| e.to_string())
}

/// 全量索引仓库（文件清单 + 模块 + 引用关系入库）
#[tauri::command]
pub fn index_repo(state: tauri::State<'_, AppState>, path: String) -> Result<IndexSummary, String> {
    state
        .svc()
        .index_repo(&path, &|_| {})
        .map_err(|e| e.to_string())
}
