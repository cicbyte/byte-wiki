//! 模型 Provider 配置命令（设置页「模型」分区）

use byte_wiki_core::model::LlmConfig;

use crate::state::AppState;

/// 读取模型 Provider 配置
#[tauri::command]
pub fn get_llm_config(state: tauri::State<'_, AppState>) -> Result<LlmConfig, String> {
    state.svc().llm_config().map_err(|e| e.to_string())
}

/// 保存模型 Provider 配置（即时生效）
#[tauri::command]
pub fn set_llm_config(
    state: tauri::State<'_, AppState>,
    config: LlmConfig,
) -> Result<(), String> {
    state.svc().set_llm_config(config).map_err(|e| e.to_string())
}

/// 测试连接结果
#[derive(serde::Serialize)]
pub struct LlmTestResult {
    pub ok: bool,
    /// 可用模型列表（成功时）
    pub models: Vec<String>,
    /// 失败原因
    pub error: String,
}

/// 测试 Provider 连接（列出可用模型）
#[tauri::command]
pub async fn test_llm_connection(
    state: tauri::State<'_, AppState>,
) -> Result<LlmTestResult, String> {
    let cfg = state.svc().llm_config().map_err(|e| e.to_string())?;
    match byte_wiki_core::llm::probe(&cfg).await {
        Ok(r) => Ok(LlmTestResult {
            ok: true,
            models: r.models,
            error: String::new(),
        }),
        Err(e) => Ok(LlmTestResult {
            ok: false,
            models: vec![],
            error: e.to_string(),
        }),
    }
}
