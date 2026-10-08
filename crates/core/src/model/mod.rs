//! 数据模型
//!
//! byte-wiki（Repo Wiki）的模型层：目前仅 config.toml 配置类型。
//! Wiki 索引/文档模型在索引器与生成流水线功能中按需补充。

pub mod config;

pub use config::{ApiConfig, AppConfig, BackupConfig, LlmConfig, PreferencesConfig};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_config_default_has_empty_keys() {
        let cfg = AppConfig::default();
        assert!(cfg.data_dir.is_none());
        assert!(cfg.backup.compress);
        assert!(cfg.api.key.is_empty());
        assert!(cfg.api.mcp_key.is_empty());
        assert!(cfg.api.allow_local_no_key);
    }
}
