//! 壁纸命令：上传/列表/删除（data/wallpapers/）
//!
//! 含扩展名白名单 + 路径穿越防护 + 列表按修改时间倒序。

use crate::state::AppState;

/// 允许的壁纸扩展名（小写，不含点）
const WALLPAPER_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp", "avif"];

/// 壁纸目录（data/wallpapers/）
fn wallpapers_dir(state: &tauri::State<'_, AppState>) -> std::path::PathBuf {
    state.svc().data_dir().join("wallpapers")
}

/// 扩展名是否在壁纸白名单内（大小写不敏感）
fn is_wallpaper_ext(path: &std::path::Path) -> bool {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    WALLPAPER_EXTS.contains(&ext.as_str())
}

/// 上传壁纸：把选中图片复制到 data/wallpapers/，返回文件名
/// 扩展名白名单校验 + 时间戳前缀防同名覆盖
#[tauri::command]
pub fn upload_wallpaper(state: tauri::State<'_, AppState>, src_path: String) -> Result<String, String> {
    let src = std::path::Path::new(&src_path);
    if !is_wallpaper_ext(src) {
        return Err("不支持的图片格式".to_string());
    }
    let dir = wallpapers_dir(&state);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stem = src.file_name().and_then(|n| n.to_str()).unwrap_or("wallpaper");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let filename = format!("{ts}-{stem}");
    std::fs::copy(src, dir.join(&filename)).map_err(|e| e.to_string())?;
    Ok(filename)
}

/// 列出全部壁纸文件名（仅图片扩展名，按修改时间倒序）
#[tauri::command]
pub fn list_wallpapers(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    let dir = wallpapers_dir(&state);
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut entries: Vec<(String, std::time::SystemTime)> = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.is_file() || !is_wallpaper_ext(&path) {
            continue;
        }
        let modified = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .unwrap_or(std::time::UNIX_EPOCH);
        if let Some(name) = entry.file_name().to_str() {
            entries.push((name.to_string(), modified));
        }
    }
    entries.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(entries.into_iter().map(|(n, _)| n).collect())
}

/// 删除指定壁纸（防路径穿越：拒绝分隔符/..；必须是壁纸扩展名）
#[tauri::command]
pub fn delete_wallpaper(state: tauri::State<'_, AppState>, filename: String) -> Result<(), String> {
    if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
        return Err("非法文件名".to_string());
    }
    let path = wallpapers_dir(&state).join(&filename);
    if !is_wallpaper_ext(&path) {
        return Err("仅可删除壁纸文件".to_string());
    }
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}
