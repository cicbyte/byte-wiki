//! 系统级命令：健康检查、数据目录、打开文件夹/URL

use crate::state::AppState;

/// 健康检查（前端验证 invoke 链路）
#[tauri::command]
pub fn ping() -> String {
    "pong".into()
}

/// 数据目录路径（实际存放 db/项目文件/片段/笔记的目录，~/.cicbyte/apps/byte-wiki/data/ 或 config.toml 中自定义的 data_dir）
#[tauri::command]
pub fn data_dir(state: tauri::State<'_, AppState>) -> String {
    state.svc().data_dir().to_string_lossy().into_owned()
}

/// 在系统文件管理器中打开指定目录
///
/// 跨平台实现：
/// - Windows: `explorer.exe /select,<path>`（若 path 是文件则定位到该文件并选中，目录则打开）
/// - macOS:   `open <path>`（Finder 打开）
/// - Linux:   `xdg-open <path>`（默认文件管理器打开）
///
/// 路径不存在时返回错误，由前端提示。
#[tauri::command]
pub fn open_in_folder(path: String) -> Result<(), String> {
    use std::process::Command;
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(format!("路径不存在：{path}"));
    }
    let result = if cfg!(target_os = "windows") {
        // explorer /select 需要文件路径用反斜杠；对目录则直接打开
        if p.is_dir() {
            Command::new("explorer.exe").arg(p).spawn()
        } else {
            Command::new("explorer.exe")
                .arg(format!("/select,{}", p.display()))
                .spawn()
        }
    } else if cfg!(target_os = "macos") {
        // 若是文件，用 -R 在 Finder 中定位；目录则直接 open
        if p.is_file() {
            Command::new("open").arg("-R").arg(p).spawn()
        } else {
            Command::new("open").arg(p).spawn()
        }
    } else {
        // Linux / 其他 Unix：xdg-open（仅打开目录或所在目录）
        Command::new("xdg-open").arg(p).spawn()
    };
    match result {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("打开失败：{e}")),
    }
}

/// 用系统默认浏览器打开 URL
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    // Windows: start, macOS: open, Linux: xdg-open
    #[cfg(target_os = "windows")]
    let cmd = "cmd";
    #[cfg(target_os = "windows")]
    let args = ["/C", "start", "", &url];
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(target_os = "macos")]
    let args = [&url];
    #[cfg(target_os = "linux")]
    let cmd = "xdg-open";
    #[cfg(target_os = "linux")]
    let args = [&url];

    std::process::Command::new(cmd)
        .args(&args)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
