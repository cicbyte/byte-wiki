//! 变更检测命令（FR-06）

use byte_wiki_core::watch::StaleReport;

use crate::state::AppState;

/// 检测仓库代码变更（与索引快照对比），输出滞后报告
#[tauri::command]
pub fn check_staleness(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<StaleReport, String> {
    state.svc().check_staleness(&path).map_err(|e| e.to_string())
}
