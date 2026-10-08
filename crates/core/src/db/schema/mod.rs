//! Schema 迁移
//!
//! byte-wiki（Repo Wiki）全新表结构，从 v1 起步（PRD §7 数据模型）：
//! - `repos`        已打开的 Git 仓库（路径唯一）
//! - `wiki_docs`    `.bytewiki/` 内文档的元数据（kind/path/来源）
//! - `chunks`       RAG 分块与向量（f32 小端序列化存 BLOB）
//! - `change_log`   变更检测与增量更新记录
//!
//! 迁移机制：`PRAGMA user_version` 记录当前版本，逐版本执行。

use rusqlite::Connection;

use crate::CoreResult;

/// 运行全部未应用的迁移
pub fn run_migrations(conn: &Connection) -> CoreResult<()> {
    let mut version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 1 {
        v1_initial(conn)?;
        version = 1;
    }
    // 后续版本在此追加：if version < 2 { v2_xxx(conn)?; version = 2; }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap();
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 1);
    }
}
