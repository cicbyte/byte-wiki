//! 配置与备份命令：备份/恢复、数据目录、排除目录、API Keys

use crate::state::AppState;

#[tauri::command]
pub fn backup(state: tauri::State<'_, AppState>, output: Option<String>, no_compress: bool) -> Result<String, String> {
    let svc = state.svc();
    let cfg = svc.config().map_err(|e| e.to_string())?;
    let compress = if no_compress { false } else { cfg.backup.compress };
    let dest = if let Some(o) = &output {
        std::path::PathBuf::from(o)
    } else if let Some(dir) = &cfg.backup.output_dir {
        if !dir.is_empty() {
            std::path::PathBuf::from(dir)
        } else {
            svc.data_root().join("backups")
        }
    } else {
        svc.data_root().join("backups")
    };
    let result = svc.backup(&dest, compress).map_err(|e| e.to_string())?;
    Ok(result.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn restore(state: tauri::State<'_, AppState>, path: String) -> Result<(), String> {
    state.svc().restore(std::path::Path::new(&path)).map_err(|e| e.to_string())?;
    Ok(())
}

/// 设置自定义数据目录（写入 config.toml 的 data_dir 字段，重启后生效）
#[tauri::command]
pub fn set_data_dir(dir: String) -> Result<(), String> {
    byte_wiki_core::set_data_dir(std::path::Path::new(&dir)).map_err(|e| e.to_string())
}

/// 清除自定义数据目录（恢复默认 app_root/data/，重启后生效）
#[tauri::command]
pub fn clear_data_dir() -> Result<(), String> {
    byte_wiki_core::clear_data_dir().map_err(|e| e.to_string())
}

/// 读取导入时排除的目录名列表（现读 config，确保最新）
#[tauri::command]
pub fn get_exclude_dirs(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    let svc = state.svc();
    svc.config()
        .map(|c| c.preferences.exclude_dirs)
        .map_err(|e| e.to_string())
}

/// 设置导入时排除的目录名列表（写入 config.toml，即时生效）
#[tauri::command]
pub fn set_exclude_dirs(dirs: Vec<String>) -> Result<(), String> {
    byte_wiki_core::set_exclude_dirs(dirs).map_err(|e| e.to_string())
}

/// 保存 API Keys 到 config.toml
#[tauri::command]
pub fn set_api_keys(state: tauri::State<'_, AppState>, rest_key: Option<String>, mcp_key: Option<String>) -> Result<(), String> {
    use byte_wiki_core::model::AppConfig;
    let svc = state.svc();
    let app_root = svc.data_root().to_path_buf();
    // 读当前 config
    let path = app_root.join("config").join("config.toml");
    let mut cfg: AppConfig = if path.exists() {
        let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        toml::from_str(&content).unwrap_or_default()
    } else {
        AppConfig::default()
    };
    if let Some(k) = rest_key {
        cfg.api.key = k;
    }
    if let Some(k) = mcp_key {
        cfg.api.mcp_key = k;
    }
    let content = toml::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(())
}
