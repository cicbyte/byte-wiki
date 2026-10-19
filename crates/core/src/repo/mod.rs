//! 仓库域业务：打开（校验 Git 仓库）、最近列表
//!
//! Git 识别采用轻量探测（`.git` 存在性 + HEAD 解析），满足 M1 的
//! 校验/分支名需求；M2 变更增量（diff/log）再引入完整 Git 库。

use std::path::Path;

use crate::db::RepoInfo;
use crate::{CoreError, CoreResult};
use crate::service::AppService;

/// 快速文件计数上限（超出记 10000；精确计数由索引器负责）
const FILE_COUNT_CAP: i64 = 10_000;

/// 读取仓库 HEAD 分支名（普通仓库解析 .git/HEAD 的 ref: 行）
fn head_branch(repo_root: &Path) -> Option<String> {
    let head = std::fs::read_to_string(repo_root.join(".git").join("HEAD")).ok()?;
    head.strip_prefix("ref: refs/heads/")
        .map(|s| s.trim().to_string())
}

impl AppService {
    /// 打开仓库：校验为 Git 仓库 → 快速统计 → 登记入库（刷新 last_opened_at）
    pub fn open_repo(&self, path: &str) -> CoreResult<RepoInfo> {
        let root = Path::new(path);
        if !root.is_dir() {
            return Err(CoreError::Other(anyhow::anyhow!(
                "路径不存在或不是目录：{path}"
            )));
        }
        if !root.join(".git").exists() {
            return Err(CoreError::Other(anyhow::anyhow!(
                "不是 Git 仓库（未找到 .git）：{path}。请选择一个已 git init / clone 的仓库。"
            )));
        }
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string());
        let branch = head_branch(root);
        // 旧版单副本布局 → 分支目录（幂等；.bytewiki/cards 保持共享位）
        crate::branch::migrate_legacy_wiki(root, branch.as_deref().unwrap_or(""))?;
        let file_count = quick_file_count(root);
        self.db.upsert_repo(
            &crate::canonical_key(root)?,
            &name,
            branch.as_deref(),
            file_count,
        )
    }

    /// 最近打开的仓库列表
    pub fn recent_repos(&self, limit: i64) -> CoreResult<Vec<RepoInfo>> {
        self.db.list_recent_repos(limit)
    }
}

/// 快速统计仓库文件数（遵守 .gitignore，排除 .git），封顶 FILE_COUNT_CAP
fn quick_file_count(root: &Path) -> i64 {
    use ignore::WalkBuilder;
    let mut count: i64 = 0;
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .filter_entry(|e| e.file_name() != ".git" && crate::indexer::keep_entry(e))
        .build();
    for entry in walker.flatten() {
        if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            count += 1;
            if count >= FILE_COUNT_CAP {
                return FILE_COUNT_CAP;
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svc_in_temp() -> (AppService, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let app_root = dir.path().join("app");
        let data_dir = dir.path().join("data");
        let svc = AppService::open_at(&app_root, &data_dir).unwrap();
        (svc, dir)
    }

    /// 造一个最小 git 仓库（仅 .git/HEAD，足够探测）
    fn make_git_repo(parent: &Path, name: &str) -> std::path::PathBuf {
        let repo = parent.join(name);
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::write(repo.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(repo.join("a.txt"), "hello").unwrap();
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join("src").join("b.rs"), "fn main() {}").unwrap();
        repo
    }

    #[test]
    fn open_repo_registers_and_rejects_non_git() {
        let (svc, dir) = svc_in_temp();
        let repo = make_git_repo(dir.path(), "demo");

        let info = svc.open_repo(repo.to_str().unwrap()).unwrap();
        assert_eq!(info.name, "demo");
        assert_eq!(info.branch.as_deref(), Some("main"));
        // a.txt + src/b.rs（.git 被 filter_entry 排除）
        assert_eq!(info.file_count, 2);

        // 非 git 目录被拒
        let plain = dir.path().join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        let err = svc.open_repo(plain.to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("不是 Git 仓库"));

        // 不存在的路径被拒
        let err = svc.open_repo(dir.path().join("nope").to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("路径不存在"));

        // 最近列表可取回
        let recent = svc.recent_repos(5).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].name, "demo");
    }

    #[test]
    fn gitignore_respected_in_count() {
        let (svc, dir) = svc_in_temp();
        let repo = make_git_repo(dir.path(), "demo2");
        std::fs::write(repo.join(".gitignore"), "ignored.txt\n").unwrap();
        std::fs::write(repo.join("ignored.txt"), "x").unwrap();
        let info = svc.open_repo(repo.to_str().unwrap()).unwrap();
        // a.txt + src/b.rs；ignored.txt 被 gitignore 排除，.gitignore 属隐藏文件不计
        assert_eq!(info.file_count, 2);
    }
}
