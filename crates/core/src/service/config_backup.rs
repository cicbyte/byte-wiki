//! 备份 / 恢复、config.toml 读写。
use super::*;
use crate::model::{AppConfig, LlmConfig};

impl super::AppService {
    // ============ 备份 / 恢复 / 配置 ============

    /// 读取用户配置（config.toml）
    pub fn config(&self) -> CoreResult<AppConfig> {
        read_config(&self.data_root)
    }

    /// 读取模型 Provider 配置
    pub fn llm_config(&self) -> CoreResult<LlmConfig> {
        Ok(self.config()?.llm)
    }

    /// 保存模型 Provider 配置（即时生效，生成/问答读取时现读）
    pub fn set_llm_config(&self, cfg: LlmConfig) -> CoreResult<()> {
        let mut full = read_config(&self.data_root).unwrap_or_default();
        full.llm = cfg;
        write_config(&self.data_root, &full)
    }

    /// 导出备份：DB checkpoint + 整个 data/ → zip 或目录快照
    ///
    /// - `dest`：目标路径。压缩模式为 .zip 文件路径；非压缩模式为目录路径。
    /// - `compress`：true=zip 文件，false=目录快照（直接复制 data/）。
    /// 返回最终生成的备份路径。
    pub fn backup(&self, dest: &Path, compress: bool) -> CoreResult<PathBuf> {
        // 1. WAL checkpoint 确保一致性（把 -wal 刷入主库）
        self.db.wal_checkpoint()?;

        let data_dir = self.data_dir();
        let dest = dest.to_path_buf();

        if compress {
            // 打包 data/ 为 zip
            let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
            let zip_path = if dest.extension().and_then(|e| e.to_str()) == Some("zip") {
                dest
            } else {
                dest.join(format!("{timestamp}-bytewiki-backup.zip"))
            };
            if let Some(parent) = zip_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let file = std::fs::File::create(&zip_path)?;
            let mut writer = zip::ZipWriter::new(file);
            let opts =
                zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            // 遍历 data/ 下所有文件加入 zip
            for entry in walkdir::WalkDir::new(data_dir).into_iter().filter_map(|e| e.ok()) {
                if !entry.file_type().is_file() {
                    continue;
                }
                let rel = entry
                    .path()
                    .strip_prefix(data_dir)
                    .map_err(|e| CoreError::Other(anyhow::anyhow!("路径剥离失败：{e}")))?;
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                writer
                    .start_file(&rel_str, opts)
                    .map_err(|e| CoreError::Other(anyhow::anyhow!("zip 写入失败：{e}")))?;
                let mut f = std::fs::File::open(entry.path())?;
                std::io::copy(&mut f, &mut writer)?;
            }
            writer
                .finish()
                .map_err(|e| CoreError::Other(anyhow::anyhow!("zip 完成失败：{e}")))?;
            tracing::info!("备份已导出：{}", zip_path.display());
            Ok(zip_path)
        } else {
            // 目录快照：递归复制 data/ → dest
            std::fs::create_dir_all(&dest)?;
            copy_dir_all(data_dir, &dest)?;
            tracing::info!("备份已导出（目录快照）：{}", dest.display());
            Ok(dest)
        }
    }

    /// 从备份恢复（覆盖当前 data/）
    ///
    /// `src` 可以是 .zip 文件或目录（快照）。恢复前会校验含 db/bytewiki.db。
    ///
    /// **注意**：此方法会覆盖 data/ 目录，但当前 service 仍持有旧 db 连接。
    /// CLI 场景下进程随即退出无影响；桌面端场景恢复后必须重启 app。
    pub fn restore(&self, src: &Path) -> CoreResult<()> {
        restore_files(src, self.data_dir())
    }
}

/// 恢复文件到指定 data 目录（不打开 db，避免文件锁）
///
/// 独立函数，CLI restore 可直接调用，无需先 open service（避免 db 文件锁）。
pub fn restore_files(src: &Path, data_dir: &Path) -> CoreResult<()> {
    let src = src.to_path_buf();
    let data_dir = data_dir.to_path_buf();

    if src.is_file() && src.extension().and_then(|e| e.to_str()) == Some("zip") {
        // 解压 zip 到临时目录，校验后覆盖 data/
        let tmp = data_dir
            .parent()
            .unwrap_or(Path::new("."))
            .join(".restore-tmp");
        if tmp.exists() {
            std::fs::remove_dir_all(&tmp)?;
        }
        std::fs::create_dir_all(&tmp)?;
        let archive = std::fs::File::open(&src)?;
        let mut zip = zip::ZipArchive::new(archive)
            .map_err(|e| CoreError::Other(anyhow::anyhow!("打开 zip 失败：{e}")))?;
        for i in 0..zip.len() {
            let mut entry = zip
                .by_index(i)
                .map_err(|e| CoreError::Other(anyhow::anyhow!("读取 zip 条目失败：{e}")))?;
            let outpath = match entry.enclosed_name() {
                Some(p) => tmp.join(p),
                None => continue,
            };
            if entry.is_dir() {
                std::fs::create_dir_all(&outpath)?;
            } else {
                if let Some(p) = outpath.parent() {
                    std::fs::create_dir_all(p)?;
                }
                let mut outfile = std::fs::File::create(&outpath)?;
                std::io::copy(&mut entry, &mut outfile)?;
            }
        }
        // 校验
        if !tmp.join("db").join("bytewiki.db").exists() {
            std::fs::remove_dir_all(&tmp).ok();
            return Err(CoreError::Other(anyhow::anyhow!(
                "备份文件无效：缺少 db/bytewiki.db"
            )));
        }
        // 覆盖 data/
        replace_dir(&tmp, &data_dir)?;
        std::fs::remove_dir_all(&tmp).ok();
    } else if src.is_dir() {
        // 目录快照
        if !src.join("db").join("bytewiki.db").exists() {
            return Err(CoreError::Other(anyhow::anyhow!(
                "备份目录无效：缺少 db/bytewiki.db"
            )));
        }
        replace_dir(&src, &data_dir)?;
    } else {
        return Err(CoreError::Other(anyhow::anyhow!(
            "恢复源既不是 .zip 文件也不是目录：{}",
            src.display()
        )));
    }
    tracing::info!("已从备份恢复：{}", src.display());
    Ok(())
}

/// config.toml 路径：{root}/config/config.toml
fn config_path(root: &Path) -> PathBuf {
    root.join("config").join("config.toml")
}

/// 确保 config.toml 存在（不存在则写默认配置）
pub(super) fn ensure_config(root: &Path) -> CoreResult<()> {
    let path = config_path(root);
    if path.exists() {
        return Ok(());
    }
    // 兼容：旧版可能把 config.toml 放在根目录，迁移进来
    let old = root.join("config.toml");
    if old.exists() {
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::rename(&old, &path)?;
        return Ok(());
    }
    // 生成默认配置（含随机 API Key）
    let mut cfg = AppConfig::default();
    cfg.api.key = generate_api_key();
    cfg.api.mcp_key = generate_api_key();
    write_config(root, &cfg)
}

/// 生成 32 字符随机 API Key
fn generate_api_key() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    (0..32)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// 读取 config.toml，解析失败时返回默认值
pub(super) fn read_config(root: &Path) -> CoreResult<AppConfig> {
    let path = config_path(root);
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let content = std::fs::read_to_string(&path)?;
    match toml::from_str::<AppConfig>(&content) {
        Ok(c) => Ok(c),
        Err(e) => {
            tracing::warn!("config.toml 解析失败，使用默认值：{e}");
            Ok(AppConfig::default())
        }
    }
}

/// 写 config.toml
pub(super) fn write_config(root: &Path, cfg: &AppConfig) -> CoreResult<()> {
    let path = config_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(cfg)
        .map_err(|e| CoreError::Other(anyhow::anyhow!("序列化 config.toml 失败：{e}")))?;
    std::fs::write(&path, content)?;
    Ok(())
}

/// 递归复制目录
fn copy_dir_all(src: &Path, dst: &Path) -> CoreResult<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// 用 src 内容替换 dst 目录（先清空 dst 再复制 src）
///
/// Windows 上文件锁可能延迟释放，remove 失败时重试几次。
fn replace_dir(src: &Path, dst: &Path) -> CoreResult<()> {
    if dst.exists() {
        // 重试删除（Windows 文件锁延迟释放）
        let mut last_err = None;
        for attempt in 0..5 {
            if std::fs::remove_dir_all(dst).is_ok() {
                last_err = None;
                break;
            }
            last_err = Some(format!("删除 {} 失败（第 {} 次尝试）", dst.display(), attempt + 1));
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        if let Some(msg) = last_err {
            return Err(CoreError::Other(anyhow::anyhow!(
                "{msg}，文件可能被占用。请关闭正在运行的 byte-wiki 后重试。"
            )));
        }
    }
    copy_dir_all(src, dst)?;
    Ok(())
}
