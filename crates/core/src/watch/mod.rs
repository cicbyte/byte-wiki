//! 变更检测（FR-06 前半）
//!
//! 轻量快照对比：当前文件系统（同索引排除规则）与 index_files 快照的
//! (path, size, mtime) 差集 → 变更清单与受影响模块。
//! 「文档滞后」= 存在变更且该仓库生成过 Wiki。
//! 文件级监听（notify 常驻 watcher）不引入——按 PRD 定案为手动触发 +
//! 滞后提示，打开仓库 / 点更新时现算即可。

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use crate::indexer::module_of;
use crate::service::AppService;
use crate::CoreResult;

/// 单个变更
#[derive(Debug, Clone, Serialize)]
pub struct FileChange {
    pub path: String,
    /// added / removed / modified
    pub kind: String,
    /// 所属模块
    pub module: String,
}

/// 滞后报告
#[derive(Debug, Clone, Serialize)]
pub struct StaleReport {
    /// 是否滞后（有变更且生成过 Wiki）
    pub stale: bool,
    pub changes: Vec<FileChange>,
    /// 受影响模块（去重）
    pub affected_modules: Vec<String>,
    pub last_generated_at: Option<String>,
}

impl AppService {
    /// 对比当前仓库与索引快照，输出变更与受影响模块
    pub fn check_staleness(&self, repo_path: &str) -> CoreResult<StaleReport> {
        let root = Path::new(repo_path);
        let canon = crate::canonical_key(root)?;
        let Some(repo) = self.db.get_repo_by_path(&canon)? else {
            return Err(crate::CoreError::Other(anyhow::anyhow!("仓库未登记：{repo_path}")));
        };
        let branch = crate::branch::current_branch(root);

        // 当前快照（与 indexer 相同的遍历规则，但不解析内容）
        let mut excludes: std::collections::HashSet<String> = crate::indexer::EXTRA_EXCLUDES
            .iter()
            .map(|s| s.to_string())
            .collect();
        if let Ok(cfg) = self.config() {
            excludes.extend(cfg.preferences.exclude_dirs);
        }
        let mut current: HashMap<String, (u64, i64)> = HashMap::new();
        let repo_excludes = crate::canonical_key(root)
            .ok()
            .and_then(|c| self.db.get_repo_by_path(&c).ok().flatten())
            .map(|r| r.exclude_patterns)
            .unwrap_or_default();
        let mut wb = ignore::WalkBuilder::new(root);
        wb.hidden(false)
            .filter_entry(move |e| {
                if !crate::indexer::keep_entry(e) {
                    return false;
                }
                e.file_name()
                    .to_str()
                    .map(|n| !excludes.contains(n))
                    .unwrap_or(true)
            });
        if let Some(ovr) = crate::indexer::exclude_overrides(root, &repo_excludes) {
            wb.overrides(ovr);
        }
        let walker = wb.build();
        for entry in walker.flatten() {
            if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                continue;
            }
            let Ok(rel) = entry.path().strip_prefix(root) else { continue };
            let (size, mtime) = entry
                .metadata()
                .map(|m| {
                    let mt = m
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0);
                    (m.len(), mt)
                })
                .unwrap_or((0, 0));
            current.insert(rel.to_string_lossy().replace('\\', "/"), (size, mtime));
        }

        // 索引快照
        let indexed: HashMap<String, (u64, i64)> = self
            .db
            .get_index_files_by_repo(repo.id, &branch)?
            .into_iter()
            .map(|(p, _lang, size, _module, mtime)| (p, (size as u64, mtime)))
            .collect();

        let mut changes: Vec<FileChange> = Vec::new();
        // 新增 / 修改
        for (path, (size, mtime)) in &current {
            match indexed.get(path) {
                None => changes.push(FileChange {
                    path: path.clone(),
                    kind: "added".into(),
                    module: module_of(path),
                }),
                Some((old_size, old_mtime)) => {
                    if old_size != size || old_mtime != mtime {
                        changes.push(FileChange {
                            path: path.clone(),
                            kind: "modified".into(),
                            module: module_of(path),
                        });
                    }
                }
            }
        }
        // 删除（模块从旧索引记录取）
        for path in indexed.keys() {
            if !current.contains_key(path) {
                changes.push(FileChange {
                    path: path.clone(),
                    kind: "removed".into(),
                    module: module_of(path),
                });
            }
        }
        changes.sort_by(|a, b| a.path.cmp(&b.path));

        let mut affected: Vec<String> = changes.iter().map(|c| c.module.clone()).collect();
        affected.sort();
        affected.dedup();

        let last_generated = repo.last_generated_at.clone();
        let stale = !changes.is_empty() && last_generated.is_some();
        Ok(StaleReport {
            stale,
            changes,
            affected_modules: affected,
            last_generated_at: last_generated,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::AppService;

    fn setup(dir: &Path) -> (AppService, std::path::PathBuf) {
        let repo = dir.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::create_dir_all(repo.join("apps").join("cli")).unwrap();
        std::fs::write(repo.join("apps/cli/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(repo.join("README.md"), "# r\n").unwrap();
        let svc = AppService::open_at(dir.join("app"), dir.join("data")).unwrap();
        svc.open_repo(repo.to_str().unwrap()).unwrap();
        svc.index_repo(repo.to_str().unwrap(), &|_| {}).unwrap();
        (svc, repo)
    }

    fn touch(path: &std::path::PathBuf, content: &str) {
        // 两次写入保证 mtime 变化（同一毫秒内 size 变化也可检出）
        std::fs::write(path, content).unwrap();
        std::fs::write(path, &(content.to_string() + "// x")).unwrap();
    }

    #[test]
    fn no_change_not_stale() {
        let dir = tempfile::tempdir().unwrap();
        let (svc, _repo) = setup(dir.path());
        let r = svc.check_staleness(dir.path().join("repo").to_str().unwrap()).unwrap();
        assert!(!r.stale);
        assert!(r.changes.is_empty());
    }

    #[test]
    fn modified_added_removed_detected_with_modules() {
        let dir = tempfile::tempdir().unwrap();
        let (svc, repo) = setup(dir.path());
        let repo_str = repo.to_str().unwrap().to_string();

        // 未生成过 → 即便有变更也不算「文档滞后」
        touch(&repo.join("apps/cli/main.rs"), "fn main() { changed }\n");
        let r = svc.check_staleness(&repo_str).unwrap();
        assert!(!r.stale, "未生成过 Wiki 不报滞后");
        assert_eq!(r.changes.len(), 1);
        assert_eq!(r.changes[0].kind, "modified");
        assert_eq!(r.changes[0].module, "apps");

        // 模拟已生成：恢复索引基线后，用 scan_existing_wiki 置 last_generated_at
        svc.index_repo(&repo_str, &|_| {}).unwrap();
        std::fs::create_dir_all(repo.join(".bytewiki/branches/main")).unwrap();
        std::fs::write(repo.join(".bytewiki/branches/main/architecture.md"), "# arch\n").unwrap();
        svc.scan_existing_wiki(&repo).unwrap();

        // 新增 + 删除 + 修改
        std::fs::create_dir_all(repo.join("crates/core")).unwrap();
        std::fs::write(repo.join("crates/core/lib.rs"), "pub struct A;\n").unwrap();
        std::fs::remove_file(repo.join("README.md")).unwrap();
        touch(&repo.join("apps/cli/main.rs"), "fn main() { changed2 }\n");

        let r = svc.check_staleness(&repo_str).unwrap();
        assert!(r.stale);
        let kinds: Vec<&str> = r.changes.iter().map(|c| c.kind.as_str()).collect();
        assert!(kinds.contains(&"added") && kinds.contains(&"removed") && kinds.contains(&"modified"));
        assert!(r.affected_modules.iter().any(|m| m == "apps"));
        assert!(r.affected_modules.iter().any(|m| m == "crates"));
        assert!(r.affected_modules.iter().any(|m| m == "."));
    }
}
