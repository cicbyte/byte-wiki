//! 模型日志：查看（尾部 N 条 + 文件体积）与清空（调试支撑）

use std::sync::Arc;

use byte_wiki_core::llm_log::{LlmLogEntry, DEFAULT_TAIL};
use byte_wiki_core::AppService;

use crate::state::AppState;

/// 尾部日志 + 当前文件体积（字节）
#[tauri::command]
pub fn list_llm_logs(
    state: tauri::State<'_, AppState>,
    path: String,
    limit: Option<usize>,
) -> Result<(Vec<LlmLogEntry>, u64), String> {
    let svc: Arc<AppService> = Arc::clone(&state.svc);
    svc.list_llm_logs(&path, limit.unwrap_or(DEFAULT_TAIL))
        .map_err(|e| e.to_string())
}

/// 清空项目模型日志
#[tauri::command]
pub fn clear_llm_logs(state: tauri::State<'_, AppState>, path: String) -> Result<(), String> {
    let svc: Arc<AppService> = Arc::clone(&state.svc);
    svc.clear_llm_logs(&path).map_err(|e| e.to_string())
}

/// LLM 响应缓存统计（条数， 总字节）
#[tauri::command]
pub fn llm_cache_stats(state: tauri::State<'_, AppState>, path: String) -> Result<(usize, u64), String> {
    let svc: Arc<AppService> = Arc::clone(&state.svc);
    svc.llm_cache_stats(&path).map_err(|e| e.to_string())
}

/// 清空项目 LLM 响应缓存（下次生成将全部真实请求）
#[tauri::command]
pub fn clear_llm_cache(state: tauri::State<'_, AppState>, path: String) -> Result<(), String> {
    let svc: Arc<AppService> = Arc::clone(&state.svc);
    svc.clear_llm_cache(&path).map_err(|e| e.to_string())
}
