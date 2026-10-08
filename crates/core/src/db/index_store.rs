//! 索引产物存取：index_files / index_edges（按仓库整体替换）

use rusqlite::params;

use crate::db::Database;
use crate::CoreResult;

impl Database {
    /// 替换式写入索引（单事务：清旧 → 插新 → 刷新 last_indexed_at）
    #[allow(clippy::type_complexity)]
    pub fn replace_index(
        &self,
        repo_id: i64,
        files: &[(String, String, i64, String, i64)],   // (path, lang, size, module, mtime_ms)
        edges: &[(String, String, i64)],           // (from_module, to_module, refs)
        indexed_at: &str,
    ) -> CoreResult<()> {
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM index_files WHERE repo_id = ?1", params![repo_id])?;
        tx.execute("DELETE FROM index_edges WHERE repo_id = ?1", params![repo_id])?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO index_files(repo_id, path, lang, size, module, mtime_ms)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for (path, lang, size, module, mtime_ms) in files {
                stmt.execute(params![repo_id, path, lang, size, module, mtime_ms])?;
            }
        }
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO index_edges(repo_id, from_module, to_module, refs)
                 VALUES(?1, ?2, ?3, ?4)",
            )?;
            for (from, to, refs) in edges {
                stmt.execute(params![repo_id, from, to, refs])?;
            }
        }
        tx.execute(
            "UPDATE repos SET last_indexed_at = ?2 WHERE id = ?1",
            params![repo_id, indexed_at],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 读取文件清单 (path, lang, size, module, mtime_ms)
    pub fn get_index_files_by_repo(
        &self,
        repo_id: i64,
    ) -> CoreResult<Vec<(String, String, i64, String, i64)>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT path, lang, size, module, mtime_ms FROM index_files WHERE repo_id = ?1 ORDER BY path",
        )?;
        let rows = stmt
            .query_map(params![repo_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// 读取引用边 (from_module, to_module, refs)
    pub fn get_index_edges_by_repo(
        &self,
        repo_id: i64,
    ) -> CoreResult<Vec<(String, String, i64)>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT from_module, to_module, refs FROM index_edges
             WHERE repo_id = ?1 ORDER BY refs DESC",
        )?;
        let rows = stmt
            .query_map(params![repo_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// 记录变更日志
    pub fn log_change(&self, repo_id: i64, kind: &str, detail: &str) -> CoreResult<()> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO change_log(repo_id, kind, detail, created_at) VALUES(?1, ?2, ?3, ?4)",
            params![repo_id, kind, detail, crate::db::now_iso()],
        )?;
        Ok(())
    }

    /// 替换式登记生成的 wiki 文档（kind: architecture / module / graph / card）
    pub fn replace_wiki_docs(
        &self,
        repo_id: i64,
        docs: &[(String, String, String)], // (path, kind, title)
        updated_at: &str,
    ) -> CoreResult<()> {
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        // 保留人工来源（cards 等），生成区整体替换
        tx.execute(
            "DELETE FROM wiki_docs WHERE repo_id = ?1 AND source = 'generated'",
            params![repo_id],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO wiki_docs(repo_id, kind, path, title, updated_at, source)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for (path, kind, title) in docs {
                // 知识卡为人工来源：生成区替换永不触碰
                let source = if kind == "card" { "human" } else { "generated" };
                stmt.execute(params![repo_id, kind, path, title, updated_at, source])?;
            }
        }
        tx.execute(
            "UPDATE repos SET last_generated_at = ?2 WHERE id = ?1",
            params![repo_id, updated_at],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 读取 wiki 文档清单 (path, kind, title)
    pub fn list_wiki_docs(&self, repo_id: i64) -> CoreResult<Vec<(String, String, String)>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT path, kind, title FROM wiki_docs WHERE repo_id = ?1 ORDER BY path",
        )?;
        let rows = stmt
            .query_map(params![repo_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// 替换式写入向量分块 (doc_path, content, embedding BLOB, dim)
    pub fn replace_chunks(
        &self,
        repo_id: i64,
        rows: &[(String, String, Vec<u8>, i64)],
    ) -> CoreResult<()> {
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM chunks WHERE repo_id = ?1", params![repo_id])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO chunks(repo_id, doc_path, content, embedding, dim, created_at)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            let now = crate::db::now_iso();
            for (doc_path, content, blob, dim) in rows {
                stmt.execute(params![repo_id, doc_path, content, blob, dim, now])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// 登记或刷新人工文档（source=human；生成区替换不影响）
    pub fn upsert_human_doc(&self, repo_id: i64, kind: &str, path: &str, title: &str) -> CoreResult<()> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO wiki_docs(repo_id, kind, path, title, updated_at, source)
             VALUES(?1, ?2, ?3, ?4, ?5, 'human')
             ON CONFLICT(repo_id, path) DO UPDATE SET
                title = excluded.title,
                updated_at = excluded.updated_at",
            params![repo_id, kind, path, title, crate::db::now_iso()],
        )?;
        Ok(())
    }

    /// 删除单条文档登记
    pub fn delete_wiki_doc(&self, repo_id: i64, path: &str) -> CoreResult<()> {
        let conn = self.lock()?;
        conn.execute(
            "DELETE FROM wiki_docs WHERE repo_id = ?1 AND path = ?2",
            params![repo_id, path],
        )?;
        Ok(())
    }

    /// 读取向量分块 (doc_path, content, embedding, dim)
    pub fn list_chunks(&self, repo_id: i64) -> CoreResult<Vec<(String, String, Vec<u8>, i64)>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT doc_path, content, embedding, dim FROM chunks
             WHERE repo_id = ?1 AND embedding IS NOT NULL",
        )?;
        let rows = stmt
            .query_map(params![repo_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }
}
