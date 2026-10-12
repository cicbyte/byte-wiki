//! 仓库命令：打开（校验+登记）、最近列表、索引

use byte_wiki_core::indexer::IndexSummary;
use byte_wiki_core::RepoInfo;

use crate::state::AppState;

/// 打开本地 Git 仓库（校验 + 快速统计 + 登记），失败返回可读错误
#[tauri::command]
pub fn open_repo(state: tauri::State<'_, AppState>, path: String) -> Result<RepoInfo, String> {
    state.svc().open_repo(&path).map_err(|e| e.to_string())
}

/// 最近打开的仓库列表（默认 10 条）
#[tauri::command]
pub fn recent_repos(
    state: tauri::State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<RepoInfo>, String> {
    state
        .svc()
        .recent_repos(limit.unwrap_or(10))
        .map_err(|e| e.to_string())
}

/// 全量索引仓库（文件清单 + 模块 + 引用关系入库）
#[tauri::command]
pub fn index_repo(state: tauri::State<'_, AppState>, path: String) -> Result<IndexSummary, String> {
    state
        .svc()
        .index_repo(&path, &|_| {})
        .map_err(|e| e.to_string())
}


// ===== 文件浏览页（只读工作区浏览） =====

/// 目录条目（浏览页左树懒加载）
#[derive(Debug, Clone, serde::Serialize)]
pub struct DirEntryInfo {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
}

/// 列出仓库内指定子目录（一层）：目录在前、字母序，不区分大小写。
/// 只排 .git（浏览是全量视角，不套索引排除偏好）；路径防护同 read_repo_file。
#[tauri::command]
pub fn list_repo_dir(repo_path: String, sub_path: String) -> Result<Vec<DirEntryInfo>, String> {
    let root = std::path::Path::new(&repo_path)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let dir = if sub_path.is_empty() {
        root.clone()
    } else {
        let full = root.join(&sub_path).canonicalize().map_err(|_| format!("目录不存在：{sub_path}"))?;
        if !full.starts_with(&root) {
            return Err(format!("非法路径：{sub_path}"));
        }
        full
    };
    let mut out: Vec<DirEntryInfo> = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".git" {
            continue;
        }
        // fs::metadata 跟随符号链接：链接到目录的条目正确展开（file_type() 不跟随）
        let meta = std::fs::metadata(entry.path()).map_err(|e| e.to_string())?;
        let rel = entry
            .path()
            .strip_prefix(&root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| name.clone());
        out.push(DirEntryInfo {
            name,
            path: rel,
            is_dir: meta.is_dir(),
            size: if meta.is_dir() { 0 } else { meta.len() },
        });
    }
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

/// 按文件名搜索仓库（浏览页搜索框）：ignore walker 遵循 .gitignore，上限 200 条
#[tauri::command]
pub fn search_repo_files(repo_path: String, query: String) -> Result<Vec<String>, String> {
    let root = std::path::Path::new(&repo_path)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let q = query.to_lowercase();
    let mut hits: Vec<String> = Vec::new();
    for entry in ignore::WalkBuilder::new(&root)
        .hidden(false)
        .filter_entry(|e| e.file_name().to_str().map(|n| n != ".git").unwrap_or(true))
        .build()
        .flatten()
    {
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let rel = match entry.path().strip_prefix(&root) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        let name = rel.rsplit('/').next().unwrap_or(&rel);
        if q.is_empty() || name.to_lowercase().contains(&q) {
            hits.push(rel);
            if hits.len() >= 200 {
                break;
            }
        }
    }
    Ok(hits)
}
