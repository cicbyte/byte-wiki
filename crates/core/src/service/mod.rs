//! 业务服务层
//!
//! [`AppService`] 是对外的统一 API：持有 [`Database`] 与数据目录布局，
//! 提供 config.toml 配置读写与备份/恢复。CLI 与桌面端均复用本服务。
//!
//! Repo Wiki 的索引、生成、RAG 问答等业务域随对应功能以 `impl AppService`
//! 扩展方法的方式补充。

use std::path::{Path, PathBuf};

use crate::db::Database;
use crate::{CoreError, CoreResult};

mod config_backup;

// open_at 与顶层配置函数调用的 config_backup 内部 helper（对 service 可见）
use config_backup::{ensure_config, read_config, write_config};
// CLI restore 直接调用（crate::service::restore_files）
pub use config_backup::restore_files;

/// 确定 app 根目录（固定位置，config.toml 在此之下）
///
/// 优先级：`BYTE_WIKI_HOME` 环境变量 > 默认 `~/.cicbyte/apps/byte-wiki/`。
/// 此路径固定不变，config/ 和 data/ 的默认位置都基于它。
pub fn resolve_app_root() -> CoreResult<PathBuf> {
    if let Ok(p) = std::env::var("BYTE_WIKI_HOME") {
        return Ok(PathBuf::from(p));
    }
    let home = dirs_or_home()?;
    Ok(home.join(".cicbyte").join("apps").join("byte-wiki"))
}

/// 确定实际数据目录（可由用户在设置页自定义）
///
/// 优先级：config.toml 的 data_dir 字段 > 默认 app_root/data/
/// config.toml 永远在 app_root/config/ 下（固定锚点），data 可移动到任意位置。
pub fn resolve_data_dir() -> CoreResult<PathBuf> {
    let app_root = resolve_app_root()?;
    let cfg = read_config(&app_root).unwrap_or_default();
    if let Some(d) = &cfg.data_dir {
        if !d.is_empty() {
            let path = PathBuf::from(d);
            if path.is_dir() {
                return Ok(path);
            }
            tracing::warn!("config.toml 的 data_dir 指向的目录不存在，回退到默认：{}", path.display());
        }
    }
    Ok(app_root.join("data"))
}

/// 设置自定义数据目录（写入 config.toml 的 data_dir 字段，重启后生效）
pub fn set_data_dir(dir: &Path) -> CoreResult<()> {
    let app_root = resolve_app_root()?;
    let mut cfg = read_config(&app_root).unwrap_or_default();
    cfg.data_dir = Some(dir.to_string_lossy().into_owned());
    write_config(&app_root, &cfg)?;
    tracing::info!("数据目录已设置为：{}", dir.display());
    Ok(())
}

/// 清除自定义数据目录（恢复默认 app_root/data/，重启后生效）
pub fn clear_data_dir() -> CoreResult<()> {
    let app_root = resolve_app_root()?;
    let mut cfg = read_config(&app_root).unwrap_or_default();
    cfg.data_dir = None;
    write_config(&app_root, &cfg)?;
    tracing::info!("数据目录已恢复默认");
    Ok(())
}

/// 设置索引时排除的目录名列表（写入 config.toml 的 [preferences].exclude_dirs）
pub fn set_exclude_dirs(dirs: Vec<String>) -> CoreResult<()> {
    let app_root = resolve_app_root()?;
    let mut cfg = read_config(&app_root).unwrap_or_default();
    cfg.preferences.exclude_dirs = dirs;
    write_config(&app_root, &cfg)?;
    tracing::info!("排除目录偏好已更新");
    Ok(())
}

pub(crate) fn dirs_or_home() -> CoreResult<PathBuf> {
    if let Some(h) = home_dir_via_env() {
        return Ok(h);
    }
    Err(CoreError::Other(anyhow::anyhow!(
        "无法定位用户家目录（HOME / USERPROFILE 均未设置）"
    )))
}

fn home_dir_via_env() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// 核心业务服务
pub struct AppService {
    /// 加密/本地元数据库（repos / wiki_docs / chunks / change_log）
    pub db: Database,
    /// app 根目录（config/ 在此之下）
    data_root: PathBuf,
    /// 数据目录（db/ 在此之下）
    data_dir: PathBuf,
    /// LLM 响应缓存开关（默认开；测试与强制重生成时关闭）
    pub llm_cache_enabled: std::sync::atomic::AtomicBool,
    /// 大纲 agent 开关（默认开；单测走单发大纲，避免对 mock 的额外轮次请求）
    pub outline_agent_enabled: std::sync::atomic::AtomicBool,
    /// 生成任务注册表（后台任务页/页面重入水合）
    pub tasks: crate::tasks::TaskManager,
}

impl AppService {
    /// 仓库自定义索引排除规则（gitignore 风格 glob）
    pub fn repo_exclude_patterns(&self, repo_path: &str) -> CoreResult<Vec<String>> {
        let canon = crate::canonical_key(Path::new(repo_path))?;
        match self.db.get_repo_by_path(&canon)? {
            Some(r) => Ok(r.exclude_patterns),
            None => Ok(vec![]),
        }
    }

    /// 保存仓库自定义索引排除规则（重建索引后生效）
    pub fn set_repo_exclude_patterns(&self, repo_path: &str, patterns: &[String]) -> CoreResult<()> {
        let canon = crate::canonical_key(Path::new(repo_path))?;
        self.db.set_repo_exclude_patterns(&canon, patterns)
    }

    /// 保存调用链规则（config preferences.callchain）
    pub fn set_callchain_rules(&self, rules: crate::model::CallchainRules) -> CoreResult<()> {
        let mut cfg = self.config()?;
        cfg.preferences.callchain = rules;
        write_config(&self.data_root, &cfg)
    }

    /// 大纲 agent 开关（默认开；测试关闭走单发）
    pub fn set_outline_agent(&self, on: bool) {
        self.outline_agent_enabled.store(on, std::sync::atomic::Ordering::Relaxed);
    }

    /// 在默认位置打开服务（app_root 固定，data_dir 由 config.toml 决定）
    pub fn open_default() -> CoreResult<Self> {
        let app_root = resolve_app_root()?;
        let data_dir = resolve_data_dir()?;
        Self::open_at(&app_root, &data_dir)
    }

    /// 打开服务（app_root 放 config，data_dir 放数据）
    pub fn open_at<P: AsRef<Path>, Q: AsRef<Path>>(app_root: P, data_dir: Q) -> CoreResult<Self> {
        let app_root = app_root.as_ref().to_path_buf();
        let data_dir = data_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&app_root)?;
        std::fs::create_dir_all(&data_dir)?;
        let db_path = data_dir.join("db").join("bytewiki.db");
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let db = Database::open(&db_path)?;
        // 首次启动写默认 config.toml
        ensure_config(&app_root)?;
        Ok(Self {
            db,
            data_root: app_root,
            data_dir,
            llm_cache_enabled: std::sync::atomic::AtomicBool::new(true),
            outline_agent_enabled: std::sync::atomic::AtomicBool::new(true),
            tasks: crate::tasks::TaskManager::default(),
        })
    }

    /// app 根目录（config/ 在此之下）
    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    /// 数据目录（db/ 在此之下）
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// 开关 LLM 响应缓存（关闭后查询永远未命中）
    pub fn set_llm_cache_enabled(&self, enabled: bool) {
        self.llm_cache_enabled
            .store(enabled, std::sync::atomic::Ordering::Relaxed);
    }
}
