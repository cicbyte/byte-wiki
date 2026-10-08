//! 知识卡命令（FR-07）：cards/ 人工知识创建与删除

use crate::state::AppState;

/// 创建知识卡（写 .bytewiki/cards/ 并登记 human 来源），返回卡片相对路径
#[tauri::command]
pub fn create_card(
    state: tauri::State<'_, AppState>,
    path: String,
    title: String,
    content: String,
) -> Result<String, String> {
    state
        .svc()
        .create_card(&path, &title, &content)
        .map_err(|e| e.to_string())
}

/// 删除知识卡（文件 + 登记）
#[tauri::command]
pub fn delete_card(
    state: tauri::State<'_, AppState>,
    path: String,
    card_rel: String,
) -> Result<(), String> {
    state
        .svc()
        .delete_card(&path, &card_rel)
        .map_err(|e| e.to_string())
}
