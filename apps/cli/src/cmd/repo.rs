//! 仓库命令：登记/列出仓库（CLI 侧管理；调用链/Wiki 生成的前置）

use clap::Args;

use super::open_service;

#[derive(Args, Debug)]
pub struct OpenArgs {
    /// 仓库路径（含 .git 的目录）
    pub repo: String,
}

pub async fn run_open(args: OpenArgs, json: bool) -> anyhow::Result<()> {
    let svc = open_service(json)?;
    let info = svc.open_repo(&args.repo)?;
    if json {
        println!(
            "{}",
            serde_json::json!({"id": info.id, "path": info.path, "name": info.name, "branch": info.branch, "file_count": info.file_count})
        );
    } else {
        println!("✅ 已登记：{}（#{}) · 分支 {} · {} 个文件", info.name, info.id, info.branch.unwrap_or_default(), info.file_count);
    }
    Ok(())
}

#[derive(Args, Debug)]
pub struct ReposArgs {
    /// 显示条数
    #[arg(long, default_value_t = 20)]
    pub limit: i64,
}

pub async fn run_repos(args: ReposArgs, json: bool) -> anyhow::Result<()> {
    let svc = open_service(json)?;
    let list = svc.db.list_recent_repos(args.limit)?;
    if json {
        println!("{}", serde_json::json!({"repos": list}));
    } else {
        for r in list {
            println!("#{}	{}	{}", r.id, r.name, r.path);
        }
    }
    Ok(())
}
