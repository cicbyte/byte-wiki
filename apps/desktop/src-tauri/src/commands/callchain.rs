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

/// 走读页单个函数源码切片
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChainSource {
    pub id: String,
    pub code: String,
    /// 函数被截断（超行数上限）时为 true，前端提示
    pub truncated: bool,
}

/// 走读页数据源：批量取多个函数的源码切片。
/// items = (节点id, 相对路径, 起行, 止行)（1-based 含端）——按文件去重读取、
/// 服务端按行切片一次返回，避免前端逐函数 invoke。单函数上限 1200 行。
#[tauri::command]
pub fn get_chain_sources(
    repo_path: String,
    items: Vec<(String, String, usize, usize)>,
) -> Result<Vec<ChainSource>, String> {
    const MAX_FN_LINES: usize = 1200;
    const MAX_ITEMS: usize = 400;

    if items.len() > MAX_ITEMS {
        return Err(format!("走读链路过大（{} 个函数，上限 {}）", items.len(), MAX_ITEMS));
    }
    let root = std::path::Path::new(&repo_path)
        .canonicalize()
        .map_err(|e| e.to_string())?;

    // 按文件去重读取（路径防护同 read_repo_file）
    let mut file_cache: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let mut out = Vec::with_capacity(items.len());
    for (id, rel, start, end) in items {
        let entry = match file_cache.get(&rel) {
            Some(lines) => lines,
            None => {
                let full = root.join(&rel).canonicalize().map_err(|_| format!("文件不存在：{rel}"))?;
                if !full.starts_with(&root) || full.starts_with(root.join(".git")) {
                    return Err(format!("非法路径：{rel}"));
                }
                let len = std::fs::metadata(&full).map(|m| m.len()).map_err(|e| e.to_string())?;
                if len > 4 * 1024 * 1024 {
                    return Err(format!("文件过大：{rel}"));
                }
                let text = std::fs::read_to_string(&full).map_err(|e| e.to_string())?;
                file_cache.entry(rel.clone()).or_insert(text.split('\n').map(|s| s.to_string()).collect())
            }
        };
        let start = start.max(1);
        let end = end.min(entry.len());
        if start > end {
            out.push(ChainSource { id, code: String::new(), truncated: false });
            continue;
        }
        let want = end - start + 1;
        let truncated = want > MAX_FN_LINES;
        let slice: Vec<&str> = entry[start - 1..start - 1 + want.min(MAX_FN_LINES)]
            .iter()
            .map(|s| s.as_str())
            .collect();
        out.push(ChainSource { id, code: slice.join("\n"), truncated });
    }
    Ok(out)
}
