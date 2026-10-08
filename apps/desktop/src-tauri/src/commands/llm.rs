//! 模型供应商配置命令（设置页「模型」分区：多供应商两栏布局）

use byte_wiki_core::model::LlmConfig;

use crate::state::AppState;

/// 读取模型供应商配置（providers + active_id）
#[tauri::command]
pub fn get_llm_config(state: tauri::State<'_, AppState>) -> Result<LlmConfig, String> {
    state.svc().llm_config().map_err(|e| e.to_string())
}

/// 保存模型供应商配置（即时生效）
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

/// 测试指定供应商连通性（列出可用模型）
#[tauri::command]
pub async fn test_llm_connection(
    state: tauri::State<'_, AppState>,
    provider_id: String,
) -> Result<LlmTestResult, String> {
    let cfg = state.svc().llm_config().map_err(|e| e.to_string())?;
    let Some(p) = cfg.providers.iter().find(|p| p.id == provider_id) else {
        return Err(format!("供应商不存在：{provider_id}"));
    };
    match byte_wiki_core::llm::probe_provider(p).await {
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
