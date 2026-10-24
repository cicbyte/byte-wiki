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
    if version < 3 {
        v3_branch_dimension(conn)?;
        version = 3;
    }
    if version < 4 {
        v4_repo_exclude_patterns(conn)?;
        version = 4;
    }
    if version < 5 {
        v5_repo_origin_url(conn)?;
        version = 5;
    }
    if version < 6 {
        v6_repo_callchain_rules(conn)?;
        version = 6;
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

/// v3：子表加 branch 维度——文档/索引/向量/日志按分支独立，项目身份不变。
///
/// 既有行 branch=''（归入当时的分支归属，由首次打开时的归属迁移处理：
/// 首次以分支 B 操作时把 branch='' 的行认领为 B）。cards 例外：wiki_docs
/// 中 kind='card' 的行使用共享哨兵 '__shared__'（知识卡跨分支共享）。
fn v3_branch_dimension(conn: &Connection) -> CoreResult<()> {
    conn.execute_batch(
        r#"
        ALTER TABLE index_files ADD COLUMN branch TEXT NOT NULL DEFAULT '';
        ALTER TABLE index_edges ADD COLUMN branch TEXT NOT NULL DEFAULT '';
        ALTER TABLE wiki_docs   ADD COLUMN branch TEXT NOT NULL DEFAULT '';
        ALTER TABLE chunks      ADD COLUMN branch TEXT NOT NULL DEFAULT '';
        ALTER TABLE change_log  ADD COLUMN branch TEXT NOT NULL DEFAULT '';

        UPDATE wiki_docs SET branch = '__shared__' WHERE kind = 'card';

        DROP INDEX IF EXISTS idx_chunks_repo;
        CREATE INDEX IF NOT EXISTS idx_chunks_repo_branch ON chunks(repo_id, branch);
        CREATE INDEX IF NOT EXISTS idx_wiki_docs_repo_branch ON wiki_docs(repo_id, branch);
        CREATE INDEX IF NOT EXISTS idx_index_files_repo_branch ON index_files(repo_id, branch);
        CREATE INDEX IF NOT EXISTS idx_change_log_repo_branch ON change_log(repo_id, branch);
        "#,
    )?;
    // 旧 UNIQUE(repo_id, path) 与 branch 维度冲突（不同分支同路径行）——重建为含 branch
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS wiki_docs_v3 (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            repo_id    INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
            kind       TEXT NOT NULL,
            path       TEXT NOT NULL,
            title      TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            source     TEXT NOT NULL DEFAULT 'generated',
            branch     TEXT NOT NULL DEFAULT '',
            UNIQUE(repo_id, branch, path)
        );
        INSERT INTO wiki_docs_v3 SELECT id, repo_id, kind, path, title, updated_at, source, branch FROM wiki_docs;
        DROP TABLE wiki_docs;
        ALTER TABLE wiki_docs_v3 RENAME TO wiki_docs;

        CREATE TABLE IF NOT EXISTS index_files_v3 (
            id       INTEGER PRIMARY KEY AUTOINCREMENT,
            repo_id  INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
            path     TEXT NOT NULL,
            lang     TEXT NOT NULL,
            size     INTEGER NOT NULL,
            module   TEXT NOT NULL,
            mtime_ms INTEGER NOT NULL DEFAULT 0,
            branch   TEXT NOT NULL DEFAULT '',
            UNIQUE(repo_id, branch, path)
        );
        INSERT INTO index_files_v3 SELECT id, repo_id, path, lang, size, module, mtime_ms, branch FROM index_files;
        DROP TABLE index_files;
        ALTER TABLE index_files_v3 RENAME TO index_files;

        CREATE TABLE IF NOT EXISTS index_edges_v3 (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            repo_id     INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
            from_module TEXT NOT NULL,
            to_module   TEXT NOT NULL,
            refs        INTEGER NOT NULL DEFAULT 1,
            branch      TEXT NOT NULL DEFAULT '',
            UNIQUE(repo_id, branch, from_module, to_module)
        );
        INSERT INTO index_edges_v3 SELECT id, repo_id, from_module, to_module, refs, branch FROM index_edges;
        DROP TABLE index_edges;
        ALTER TABLE index_edges_v3 RENAME TO index_edges;
        "#,
    )?;
    conn.pragma_update(None, "user_version", 3)?;
    Ok(())
}

/// v4：repos 加 exclude_patterns（JSON 字符串数组，gitignore 风格 glob）——
/// 用户自定义的索引排除规则（叠加在 .gitignore 与内置排除之上）
fn v4_repo_exclude_patterns(conn: &Connection) -> CoreResult<()> {
    conn.execute_batch(
        "ALTER TABLE repos ADD COLUMN exclude_patterns TEXT NOT NULL DEFAULT '[]';",
    )?;
    conn.pragma_update(None, "user_version", 4)?;
    Ok(())
}

/// v6：repos 加 callchain_rules（JSON 对象，可空）——调用链规则按项目管理
/// （文件排除/入口排除/入口正则是对具体仓库的语义决策，全局共享必然串台）。
/// NULL = 未自定义，读取时回落全局 preferences.callchain（平滑兼容旧配置）
fn v6_repo_callchain_rules(conn: &Connection) -> CoreResult<()> {
    conn.execute_batch("ALTER TABLE repos ADD COLUMN callchain_rules TEXT;")?;
    conn.pragma_update(None, "user_version", 6)?;
    Ok(())
}

/// v5：repos 加 origin_url——远程克隆的来源地址（本地打开为空）
fn v5_repo_origin_url(conn: &Connection) -> CoreResult<()> {
    conn.execute_batch(
        "ALTER TABLE repos ADD COLUMN origin_url TEXT NOT NULL DEFAULT '';",
    )?;
    conn.pragma_update(None, "user_version", 5)?;
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
        assert_eq!(v, 6); // 随最新迁移版本更新（当前 v6：repos.callchain_rules）
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
