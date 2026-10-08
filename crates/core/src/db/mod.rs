//! 数据访问层
//!
//! 持有一个 [`rusqlite::Connection`]，封装全部 SQL 操作。
//! 本模块是 SQL 的唯一接触面：service 层只调用 db 方法，不直接写 SQL。
//!
//! byte-wiki（Repo Wiki）的表结构见 [`schema`]（repos / wiki_docs / chunks / change_log）。
//! Wiki 域的具体查询方法随索引器与生成流水线功能补充。

pub mod schema;

mod common;
mod index_store;
mod repos;

pub use common::now_iso;
pub use repos::RepoInfo;

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

use crate::{CoreError, CoreResult};

/// 数据库句柄（线程安全的单连接，CLI/桌面端本地短查询足够）
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// 打开（或创建）指定路径的 SQLite 库，并运行全部迁移
    pub fn open<P: AsRef<Path>>(path: P) -> CoreResult<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        schema::run_migrations(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// 内存库（仅供测试）
    #[cfg(test)]
    pub fn open_in_memory_for_test() -> CoreResult<Self> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        schema::run_migrations(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub(crate) fn lock(
        &self,
    ) -> CoreResult<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|e| CoreError::Other(anyhow::anyhow!("db mutex poisoned: {e}")))
    }

    /// WAL checkpoint：把 -wal 刷入主库（备份前调用保证一致性）
    pub fn wal_checkpoint(&self) -> CoreResult<()> {
        let conn = self.lock()?;
        conn.pragma_update(None, "wal_checkpoint", "TRUNCATE")?;
        Ok(())
    }

    /// 已登记的仓库数量
    pub fn repo_count(&self) -> CoreResult<i64> {
        let conn = self.lock()?;
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM repos", [], |r| r.get(0))?;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory_creates_schema() {
        let db = Database::open_in_memory_for_test().unwrap();
        assert_eq!(db.repo_count().unwrap(), 0);
    }
}
