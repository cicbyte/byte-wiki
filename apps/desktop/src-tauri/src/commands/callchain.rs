//! 调用链命令：生成（进度事件推送）+ 源码文件读取（Monaco 定位打开）

use std::sync::Arc;

use byte_wiki_core::callchain::CallchainSummary;
use tauri::Emitter;

use crate::state::AppState;

/// 生成进度事件通道名（前端 listen，负载 = 已解析文件数）
pub const CALLCHAIN_EVENT: &str = "wiki://callchain";

/// 生成函数级调用图（tree-sitter CPU 密集，走 spawn_blocking 不阻塞主线程），
/// 产物落 .bytewiki/graphs/callchain/callgraph.json（不带 source，详情面板 Monaco 读实时文件）
#[tauri::command]
pub async fn generate_callchain(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<CallchainSummary, String> {
    let svc = Arc::clone(&state.svc);
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        svc.generate_callchain(&path, &move |n: usize| {
            let _ = app_handle.emit(CALLCHAIN_EVENT, n);
        })
    })
    .await
    .map_err(|e| format!("调用链任务执行失败：{e}"))?
    .map_err(|e| e.to_string())
}

/// 读取仓库内源码文件（详情面板 Monaco 定位打开用）。
/// 路径防护：canonicalize 后必须在仓库根内且不在 .git；体积上限 4MB。
#[tauri::command]
pub fn read_repo_file(repo_path: String, rel_path: String) -> Result<String, String> {
    let root = std::path::Path::new(&repo_path)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let full = root
        .join(&rel_path)
        .canonicalize()
        .map_err(|_| format!("文件不存在：{rel_path}"))?;
    if !full.starts_with(&root) || full.starts_with(root.join(".git")) {
        return Err(format!("非法路径：{rel_path}"));
    }
    let len = std::fs::metadata(&full).map(|m| m.len()).map_err(|e| e.to_string())?;
    if len > 4 * 1024 * 1024 {
        return Err("文件过大（源码查看上限 4MB）".into());
    }
    std::fs::read_to_string(&full).map_err(|e| e.to_string())
}
