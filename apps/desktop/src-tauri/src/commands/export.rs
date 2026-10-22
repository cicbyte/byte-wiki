//! Wiki 导出命令（#716）：静态 HTML 站导出（dialog 选目录）

use crate::state::AppState;

/// 导出当前（或指定）分支 Wiki 为自包含 HTML 站；返回导出目录。
/// 目录选择走系统对话框（取消返回 null）。
#[tauri::command]
pub async fn export_wiki_html(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    path: String,
    branch: Option<String>,
) -> Result<Option<byte_wiki_core::wiki_export::WikiHtmlExport>, String> {
    use tauri_plugin_dialog::DialogExt;
    // 选目录（异步插件的 blocking 封装跑在 tauri 命令线程池，不阻塞主线程）
    let out = app
        .dialog()
        .file()
        .set_title("选择 Wiki HTML 导出目录")
        .blocking_pick_folder();
    let Some(out) = out else {
        return Ok(None);
    };
    let out = out
        .as_path()
        .ok_or_else(|| "无效目录".to_string())?
        .to_path_buf();
    let svc = std::sync::Arc::clone(&state.svc);
    let sum = tauri::async_runtime::spawn_blocking(move || {
        svc.export_wiki_html(&path, &out, branch.as_deref())
    })
    .await
    .map_err(|e| format!("导出任务失败：{e}"))?
    .map_err(|e| e.to_string())?;
    Ok(Some(sum))
}
