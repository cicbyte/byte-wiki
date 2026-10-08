//! db 模块共享辅助：时间戳。
//!
//! `now_iso` 经 mod.rs `pub use` 重导出供 service 层使用。

/// 当前 ISO 8601 时间戳（UTC）
pub fn now_iso() -> String {
    use chrono::Utc;
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}
