//! Schema 迁移
//!
//! byte-wiki（Repo Wiki）全新表结构，从 v1 起步（PRD §7 数据模型）：
//! - `repos`        已打开的 Git 仓库（路径唯一）
//! - `wiki_docs`    `.bytewiki/` 内文档的元数据（kind/path/来源）
//! - `chunks`       RAG 分块与向量（f32 小端序列化存 BLOB）
//! - `change_log`   变更检测与增量更新记录
//!
//! 迁移机制：`PRAGMA user_version` 记录当前版本，逐版本执行。

use rusqlite::{Connection, OptionalExtension};

use crate::CoreResult;

/// 运行全部未应用的迁移
pub fn run_migrations(conn: &Connection) -> CoreResult<()> {
    let mut version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 1 {
        v1_initial(conn)?;
        version = 1;
    }
    if version < 2 {
        v2_strip_verbatim_path(conn)?;
        version = 2;
    }
    let _ = version;
    Ok(())
}

/// v1：初始表结构
fn v1_initial(conn: &Connection) -> CoreResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS repos (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,
            path              TEXT NOT NULL UNIQUE,
            name              TEXT NOT NULL,
            branch            TEXT,
            file_count        INTEGER NOT NULL DEFAULT 0,
            last_opened_at    TEXT,
            last_indexed_at   TEXT,
            last_generated_at TEXT,
            created_at        TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS wiki_docs (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            repo_id    INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
            kind       TEXT NOT NULL,          -- architecture / module / graph / card / memory
            path       TEXT NOT NULL,          -- .bytewiki/ 内相对路径
            title      TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            source     TEXT NOT NULL DEFAULT 'generated',  -- generated / human
            UNIQUE(repo_id, path)
        );

        CREATE TABLE IF NOT EXISTS chunks (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            repo_id    INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
            doc_path   TEXT NOT NULL,          -- 来源相对仓库路径（wiki md 或代码文件）
            content    TEXT NOT NULL,
            embedding  BLOB,                   -- f32 小端序列化；NULL = 尚未 embed
            dim        INTEGER,
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_chunks_repo ON chunks(repo_id);

        CREATE TABLE IF NOT EXISTS change_log (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            repo_id    INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
            kind       TEXT NOT NULL,          -- index / generate / embed / stale ...
            detail     TEXT,
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_change_log_repo ON change_log(repo_id);

        -- 索引产物（每次全量索引按仓库替换）
        CREATE TABLE IF NOT EXISTS index_files (
            id       INTEGER PRIMARY KEY AUTOINCREMENT,
            repo_id  INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
            path     TEXT NOT NULL,              -- 相对仓库根路径（/ 分隔）
            lang     TEXT NOT NULL,              -- rust / ts / py / go / ... / other
            size     INTEGER NOT NULL,
            module   TEXT NOT NULL,              -- 一级目录（根文件为 "."）
            mtime_ms INTEGER NOT NULL DEFAULT 0, -- 修改时间（变更检测用）
            UNIQUE(repo_id, path)
        );
        CREATE INDEX IF NOT EXISTS idx_index_files_repo ON index_files(repo_id);

        CREATE TABLE IF NOT EXISTS index_edges (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            repo_id     INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
            from_module TEXT NOT NULL,
            to_module   TEXT NOT NULL,
            refs        INTEGER NOT NULL DEFAULT 1,  -- 去重后的引用计数
            UNIQUE(repo_id, from_module, to_module)
        );
        CREATE INDEX IF NOT EXISTS idx_index_edges_repo ON index_edges(repo_id);
        "#,
    )?;
    conn.pragma_update(None, "user_version", 1)?;
    Ok(())
}

/// v2：剥离 repos.path 的 Windows verbatim 前缀（`\\?\`），与 canonical_key 口径统一。
///
/// 新旧二进制交替期间可能同时存在「带前缀行」与「干净行」（同一路径两个键），
/// 直接 UPDATE 会撞 repos.path 的 UNIQUE 约束——冲突时把带前缀行的子表数据
/// （wiki_docs / chunks / change_log / index_files / index_edges）搬到干净行，
/// 再删除带前缀行；无冲突则原行剥前缀。
fn v2_strip_verbatim_path(conn: &Connection) -> CoreResult<()> {
    const CHILD_TABLES: [&str; 5] = ["wiki_docs", "chunks", "change_log", "index_files", "index_edges"];
    let prefixed: Vec<(i64, String)> = {
        let mut stmt = conn.prepare("SELECT id, path FROM repos WHERE path LIKE '\\\\?\\%'")?;
        let rows: Vec<(i64, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .flatten()
            .collect();
        rows
    };
    for (id, path) in prefixed {
        let clean = crate::strip_verbatim(&path);
        let dup = conn
            .query_row(
                "SELECT id FROM repos WHERE path = ?1 AND id != ?2",
                rusqlite::params![clean, id],
                |r| r.get::<_, i64>(0),
            )
            .optional()?;
        match dup {
            Some(keep) => {
                // 干净行已存在：子表数据改挂过去（同键子记录冲突则忽略，残留旧行数据随后清除）
                for t in CHILD_TABLES {
                    conn.execute(
                        &format!("UPDATE OR IGNORE {t} SET repo_id = ?1 WHERE repo_id = ?2"),
                        rusqlite::params![keep, id],
                    )?;
                    conn.execute(
                        &format!("DELETE FROM {t} WHERE repo_id = ?1"),
                        rusqlite::params![id],
                    )?;
                }
                conn.execute("DELETE FROM repos WHERE id = ?1", rusqlite::params![id])?;
            }
            None => {
                conn.execute(
                    "UPDATE repos SET path = ?1 WHERE id = ?2",
                    rusqlite::params![clean, id],
                )?;
            }
        }
    }
    conn.pragma_update(None, "user_version", 2)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap();
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 2);
    }

    #[test]
    fn v2_strips_verbatim_prefix() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn.execute(
            "INSERT INTO repos (path, name, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![r"\\?\C:\dev\repo", "repo", "2026-01-01"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO repos (path, name, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![r"C:\dev\plain", "plain", "2026-01-01"],
        )
        .unwrap();
        v2_strip_verbatim_path(&conn).unwrap();
        let paths: Vec<String> = {
            let mut stmt = conn.prepare("SELECT path FROM repos ORDER BY path").unwrap();
            stmt.query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .flatten()
                .collect()
        };
        assert_eq!(paths, vec![r"C:\dev\plain".to_string(), r"C:\dev\repo".to_string()]);
    }

    #[test]
    fn v2_merges_children_when_clean_row_exists() {
        // 新旧二进制交替：带前缀行（挂全部子表数据）+ 同路径干净行并存——
        // 迁移应把子表搬到干净行、删带前缀行，而不是撞 UNIQUE 失败
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        let old_id: i64 = conn
            .query_row(
                "INSERT INTO repos (path, name, created_at) VALUES (?1, ?2, ?3) RETURNING id",
                rusqlite::params![r"\\?\C:\dev\repo", "repo", "2026-01-01"],
                |r| r.get(0),
            )
            .unwrap();
        let clean_id: i64 = conn
            .query_row(
                "INSERT INTO repos (path, name, created_at) VALUES (?1, ?2, ?3) RETURNING id",
                rusqlite::params![r"C:\dev\repo", "repo2", "2026-01-02"],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT INTO wiki_docs (repo_id, kind, path, title, updated_at) VALUES (?1, 'card', 'cards/x.md', 'X', '2026-01-01')",
            rusqlite::params![old_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chunks (repo_id, doc_path, content, created_at) VALUES (?1, 'a.md', 'hello', '2026-01-01')",
            rusqlite::params![old_id],
        )
        .unwrap();
        v2_strip_verbatim_path(&conn).unwrap();
        let paths: Vec<String> = conn
            .prepare("SELECT path FROM repos ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(paths, vec![r"C:\dev\repo".to_string()]);
        let doc_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM wiki_docs WHERE repo_id = ?1", rusqlite::params![clean_id], |r| r.get(0))
            .unwrap();
        let chunk_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM chunks WHERE repo_id = ?1", rusqlite::params![clean_id], |r| r.get(0))
            .unwrap();
        assert_eq!((doc_count, chunk_count), (1, 1));
    }
}
