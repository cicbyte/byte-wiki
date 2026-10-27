//! git 命令组：AI/脚本了解项目差异（status / diff / log / show）。
//! 只读封装（core::git），供 agent 无需 MCP 即可查仓库状态与演进。

use clap::{Args, Subcommand};

use super::open_service;

#[derive(Args, Debug)]
pub struct GitArgs {
    /// 仓库路径（需已登记）
    pub repo: String,

    #[command(subcommand)]
    pub command: GitCommand,
}

#[derive(Subcommand, Debug)]
pub enum GitCommand {
    /// 工作区状态（暂存/修改/未跟踪）
    Status,
    /// 差异：--base 给 rev 看区间；缺省看工作区 vs HEAD；--path 过滤
    Diff {
        /// 基准 rev（如 HEAD~3、短 sha）；缺省 = 工作区 vs HEAD
        #[arg(long)]
        base: Option<String>,
        /// 只看某路径
        #[arg(long)]
        path: Option<String>,
    },
    /// 提交历史（sha/作者/日期/主题）
    Log {
        /// 条数（缺省 20）
        #[arg(short, long, default_value_t = 20)]
        n: usize,
        /// 只看某路径的历史
        #[arg(long)]
        path: Option<String>,
    },
    /// 某次提交的明细（stat + 补丁）
    Show {
        /// 提交 rev（sha / HEAD^ 等）
        rev: String,
        /// 只看某路径
        #[arg(long)]
        path: Option<String>,
    },
}

pub async fn run_git(args: GitArgs, json: bool) -> anyhow::Result<()> {
    let _svc = open_service(json)?; // 仓库登记校验口径与其他命令一致
    let root = std::path::PathBuf::from(&args.repo).canonicalize()
        .map_err(|e| anyhow::anyhow!("仓库路径无效：{}（{e}）", args.repo))?;
    match &args.command {
        GitCommand::Status => {
            let st = byte_wiki_core::git::status(&root)?;
            if json {
                println!("{}", serde_json::to_string(&st)?);
            } else if st.is_empty() {
                println!("工作区干净");
            } else {
                for s in st {
                    println!("[{}{}] {}", s.x, s.y, s.path);
                }
            }
        }
        GitCommand::Diff { base, path } => {
            print!("{}", byte_wiki_core::git::diff(&root, base.as_deref(), path.as_deref())?);
        }
        GitCommand::Log { n, path } => {
            let logs = byte_wiki_core::git::log(&root, *n, path.as_deref())?;
            if json {
                println!("{}", serde_json::to_string(&logs)?);
            } else {
                for l in logs {
                    println!("{} {} {} {}", l.sha, l.date, l.author, l.subject);
                }
            }
        }
        GitCommand::Show { rev, path } => {
            print!("{}", byte_wiki_core::git::show(&root, rev, path.as_deref())?);
        }
    }
    Ok(())
}
