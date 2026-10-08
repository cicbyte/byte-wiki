//! 备份 / 恢复命令

use clap::Args;

use super::open_service;

#[derive(Args, Debug)]
pub struct BackupArgs {
    /// 导出路径：zip 文件路径或目录路径。省略则用 config.toml 配置的 output_dir 或 data_root/backups/
    #[arg(long)]
    pub output: Option<String>,
    /// 不压缩，导出为目录快照而非 zip
    #[arg(long)]
    pub no_compress: bool,
}

#[derive(Args, Debug)]
pub struct RestoreArgs {
    /// 备份文件（.zip）或目录路径
    pub path: String,
    /// 跳过确认
    #[arg(long, short = 'y')]
    pub yes: bool,
}

pub async fn run_backup(args: BackupArgs, json: bool) -> anyhow::Result<()> {
    let svc = open_service(json)?;
    let cfg = svc.config()?;
    let compress = if args.no_compress { false } else { cfg.backup.compress };

    // 确定输出路径
    let dest = if let Some(o) = &args.output {
        std::path::PathBuf::from(o)
    } else if let Some(dir) = &cfg.backup.output_dir {
        if !dir.is_empty() {
            std::path::PathBuf::from(dir)
        } else {
            svc.data_root().join("backups")
        }
    } else {
        svc.data_root().join("backups")
    };

    let result = svc.backup(&dest, compress)?;
    if json {
        println!("{}", serde_json::json!({ "backup": result.to_string_lossy() }));
    } else {
        println!("✅ 备份已导出：{}", result.display());
    }
    Ok(())
}

pub async fn run_restore(args: RestoreArgs, json: bool) -> anyhow::Result<()> {
    if !args.yes && !json {
        print!("恢复将覆盖当前所有数据，确认？[y/N] ");
        std::io::Write::flush(&mut std::io::stdout())?;
        let mut buf = String::new();
        std::io::stdin().read_line(&mut buf)?;
        if !buf.trim().eq_ignore_ascii_case("y") {
            println!("已取消");
            return Ok(());
        }
    }
    // 不 open service，避免 db 文件被锁。直接操作文件系统。
    let data_root = byte_wiki_core::resolve_data_dir()?;
    let data_dir = data_root.join("data");
    byte_wiki_core::restore_files(std::path::Path::new(&args.path), &data_dir)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    if json {
        println!("{{\"restored\":\"{}\"}}", args.path);
    } else {
        println!("✅ 已从备份恢复：{}\n⚠️  如果桌面端正在运行，请重启以加载新数据。", args.path);
    }
    Ok(())
}
