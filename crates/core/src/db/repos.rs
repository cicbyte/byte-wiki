//! repos 表数据访问：仓库登记与最近打开列表

use rusqlite::{params, OptionalExtension};

use crate::db::{now_iso, Database};
use crate::CoreResult;

/// 仓库登记信息（前端总览页展示）
#[derive(Debug, Clone, serde::Serialize)]
pub struct RepoInfo {
    pub id: i64,
    /// 仓库绝对路径（唯一键）
    pub path: String,
    pub name: String,
    pub branch: Option<String>,
    /// 快速扫描的文件数（上限 1 万，超出记 10000）
    pub file_count: i64,
    pub last_opened_at: Option<String>,
    pub last_indexed_at: Option<String>,
    pub last_generated_at: Option<String>,
    /// 用户自定义索引排除规则（gitignore 风格 glob；叠加在内置排除与 .gitignore 之上）
    pub exclude_patterns: Vec<String>,
}

impl Database {
    /// 登记或刷新仓库（按 path UPSERT），返回仓库信息
    pub fn upsert_repo(&self, path: &str, name: &str, branch: Option<&str>, file_count: i64) -> CoreResult<RepoInfo> {
        let now = now_iso();
        {
            let conn = self.lock()?;
            conn.execute(
                "INSERT INTO repos(path, name, branch, file_count, last_opened_at, created_at)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?5)
                 ON CONFLICT(path) DO UPDATE SET
                    name = excluded.name,
                    branch = excluded.branch,
                    file_count = excluded.file_count,
                    last_opened_at = excluded.last_opened_at",
                params![path, name, branch, file_count, now],
            )?;
        }
        self.get_repo_by_path(path)?
            .ok_or_else(|| crate::CoreError::Other(anyhow::anyhow!("upsert_repo 后读取失败：{path}")))
    }

    /// 按路径取仓库
    pub fn get_repo_by_path(&self, path: &str) -> CoreResult<Option<RepoInfo>> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT id, path, name, branch, file_count, last_opened_at, last_indexed_at, last_generated_at, exclude_patterns
             FROM repos WHERE path = ?1",
            params![path],
            row_to_repo,
        )
        .optional()
        .map_err(Into::into)
    }

    /// 最近打开的仓库（按 last_opened_at 倒序）
    pub fn list_recent_repos(&self, limit: i64) -> CoreResult<Vec<RepoInfo>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, path, name, branch, file_count, last_opened_at, last_indexed_at, last_generated_at, exclude_patterns
             FROM repos ORDER BY last_opened_at DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit], row_to_repo)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }
}

fn row_to_repo(r: &rusqlite::Row<'_>) -> rusqlite::Result<RepoInfo> {
    Ok(RepoInfo {
        id: r.get(0)?,
        path: r.get(1)?,
        name: r.get(2)?,
        branch: r.get(3)?,
        file_count: r.get(4)?,
        last_opened_at: r.get(5)?,
        last_indexed_at: r.get(6)?,
        last_generated_at: r.get(7)?,
        exclude_patterns: r
            .get::<_, String>(8)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default(),
    })
}

impl Database {
    /// 记录远程克隆来源（本地打开的仓库为空串）
    pub fn set_repo_origin_url(&self, path: &str, url: &str) -> CoreResult<()> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE repos SET origin_url = ?1 WHERE path = ?2",
            params![url, path],
        )?;
        Ok(())
    }

    /// 更新仓库的自定义排除规则（gitignore 风格 glob；空数组 = 清空）
    pub fn set_repo_exclude_patterns(&self, path: &str, patterns: &[String]) -> CoreResult<()> {
        let json = serde_json::to_string(patterns)
            .map_err(|e| crate::CoreError::Other(anyhow::anyhow!("序列化排除规则失败：{e}")))?;
        let conn = self.lock()?;
        conn.execute(
            "UPDATE repos SET exclude_patterns = ?1 WHERE path = ?2",
            params![json, path],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_and_recent_roundtrip() {
        let db = Database::open_in_memory_for_test().unwrap();
        let a = db.upsert_repo("/tmp/a", "a", Some("main"), 10).unwrap();
        assert_eq!(a.name, "a");
        assert_eq!(a.branch.as_deref(), Some("main"));

        // 再登记 b 与重复 a（a 刷新 last_opened_at，排序应在 b 前）
        db.upsert_repo("/tmp/b", "b", None, 5).unwrap();
        db.upsert_repo("/tmp/a", "a", Some("main"), 12).unwrap();
        let recent = db.list_recent_repos(10).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].path, "/tmp/a");
        assert_eq!(recent[0].file_count, 12);
        assert_eq!(recent[1].path, "/tmp/b");
        assert_eq!(recent[1].branch, None);
    }
}
