//! MCP HTTP 服务 / REST API 服务管理命令

use crate::state::{AppState, McpServerState, ApiServerState};

/// 启动 MCP HTTP 服务（后台 axum，可选 API Key 认证）
#[tauri::command]
pub async fn start_mcp_server(
    state: tauri::State<'_, AppState>,
    host: Option<String>,
    port: u16,
    use_auth: Option<bool>,
) -> Result<String, String> {
    use tokio_util::sync::CancellationToken;

    let host = host.unwrap_or_else(|| "0.0.0.0".into());

    // 检查是否已在运行
    {
        let mcp_state = state.mcp.lock().map_err(|e| e.to_string())?;
        if mcp_state.is_some() {
            return Err("MCP 服务已在运行".into());
        }
    }

    // 可选认证：use_auth=true 时从 config 读 MCP 专用 Key
    let mcp_key = if use_auth.unwrap_or(false) {
        let svc = state.svc();
        let cfg = svc.config().map_err(|e| e.to_string())?;
        cfg.api.mcp_key
    } else {
        String::new()
    };

    let ct = CancellationToken::new();

    byte_wiki_core::mcp::server::serve_http_managed(&host, port, mcp_key, ct.clone())
        .map_err(|e| e.to_string())?;

    // 存状态
    {
        let mut mcp_state = state.mcp.lock().map_err(|e| e.to_string())?;
        *mcp_state = Some(McpServerState { cancel: ct, port });
    }

    let url = format!("http://localhost:{port}/mcp");
    Ok(url)
}

/// 停止 MCP HTTP 服务
#[tauri::command]
pub fn stop_mcp_server(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut mcp_state = state.mcp.lock().map_err(|e| e.to_string())?;
    if let Some(s) = mcp_state.take() {
        s.cancel.cancel();
        eprintln!("MCP HTTP server stopped (port {})", s.port);
    }
    Ok(())
}

/// 查询 MCP 服务状态 + MCP Key
#[tauri::command]
pub fn mcp_server_status(state: tauri::State<'_, AppState>) -> std::result::Result<McpStatus, String> {
    let svc = state.svc();
    let cfg = svc.config().map_err(|e| e.to_string())?;
    let mcp_state = state.mcp.lock().map_err(|e| e.to_string())?;
    if let Some(s) = mcp_state.as_ref() {
        Ok(McpStatus {
            running: true,
            port: s.port,
            url: format!("http://localhost:{}/mcp", s.port),
            mcp_key: cfg.api.mcp_key.clone(),
        })
    } else {
        Ok(McpStatus { running: false, port: 0, url: String::new(), mcp_key: cfg.api.mcp_key.clone() })
    }
}

#[derive(serde::Serialize)]
pub struct McpStatus {
    running: bool,
    port: u16,
    url: String,
    mcp_key: String,
}

/// 启动 REST API 服务
#[tauri::command]
pub async fn start_api_server(
    state: tauri::State<'_, AppState>,
    host: Option<String>,
    port: u16,
    custom_key: Option<String>,
) -> Result<ApiStatus, String> {
    use byte_wiki_core::mcp::rest;

    let host = host.unwrap_or_else(|| "0.0.0.0".into());

    // 检查是否已在运行
    {
        let api_state = state.api.lock().map_err(|e| e.to_string())?;
        if api_state.is_some() {
            return Err("REST API 服务已在运行".into());
        }
    }

    // 读 config（API Key 可被前端覆盖）
    let (api_key, allow_local) = {
        let svc = state.svc();
        let cfg = svc.config().map_err(|e| e.to_string())?;
        let key = custom_key.unwrap_or(cfg.api.key);
        (key, cfg.api.allow_local_no_key)
    };

    let ct = tokio_util::sync::CancellationToken::new();
    let svc_arc = std::sync::Arc::clone(&state.svc);

    rest::serve_api_http_managed(svc_arc, &host, port, api_key.clone(), allow_local, ct.clone())
        .map_err(|e| e.to_string())?;

    {
        let mut api_state = state.api.lock().map_err(|e| e.to_string())?;
        *api_state = Some(ApiServerState { cancel: ct, port });
    }

    Ok(ApiStatus {
        running: true,
        port,
        url: format!("http://localhost:{port}/api/v1"),
        api_key: Some(api_key),
    })
}

/// 停止 REST API 服务
#[tauri::command]
pub fn stop_api_server(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut api_state = state.api.lock().map_err(|e| e.to_string())?;
    if let Some(s) = api_state.take() {
        s.cancel.cancel();
        eprintln!("REST API server stopped (port {})", s.port);
    }
    Ok(())
}

/// 查询 REST API 服务状态 + API Key
#[tauri::command]
pub fn api_server_status(state: tauri::State<'_, AppState>) -> Result<ApiStatus, String> {
    let api_state = state.api.lock().map_err(|e| e.to_string())?;
    let svc = state.svc();
    let cfg = svc.config().map_err(|e| e.to_string())?;
    if let Some(s) = api_state.as_ref() {
        Ok(ApiStatus {
            running: true,
            port: s.port,
            url: format!("http://localhost:{}/api/v1", s.port),
            api_key: Some(cfg.api.key.clone()),
        })
    } else {
        Ok(ApiStatus {
            running: false,
            port: 0,
            url: String::new(),
            api_key: Some(cfg.api.key.clone()),
        })
    }
}

#[derive(serde::Serialize)]
pub struct ApiStatus {
    running: bool,
    port: u16,
    url: String,
    api_key: Option<String>,
}
