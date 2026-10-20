//! byte-wiki CLI 入口
//!
//! 子命令式：`bs <command>`。当前提供备份/恢复与 MCP/REST 服务管理；
//! Repo Wiki 的仓库打开、索引、生成命令随后续功能填充。

mod cmd;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "bs",
    version,
    about = "byte-wiki — 本地优先的仓库级知识引擎（Repo Wiki）",
    long_about = "local-first 的桌面知识引擎：为本地 Git 仓库生成结构化 Wiki（架构 / 模块 / 图谱），\n支持 RAG 问答、变更增量更新与团队共享。本 CLI 提供备份与服务管理。",
    propagate_version = true
)]
struct Cli {
    /// 输出 JSON（脚本/AI 友好）而非彩色人类可读输出
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// 导出备份（zip 或目录快照）
    Backup(cmd::BackupArgs),
    /// 从备份恢复（覆盖当前数据）
    Restore(cmd::RestoreArgs),
    /// 生成函数级调用链（tree-sitter 多语言，产物随仓库共享）
    /// 登记（打开）仓库：后续 callchain/wiki 的前置
    Open(cmd::OpenArgs),
    /// 列出最近登记的仓库
    Repos(cmd::ReposArgs),
    Callchain(cmd::CallchainArgs),
    /// 查询既有调用链产物（节点/上下游/入口；测试用）
    CallchainQuery(cmd::CallchainQueryArgs),
    /// 运行 MCP 服务器（供 Claude Desktop / Cursor 等 AI 工具检索）
    #[command(name = "mcp")]
    Mcp(cmd::McpArgs),
    /// 运行 REST API 服务器（供第三方应用 HTTP 对接）
    #[command(subcommand)]
    Api(cmd::ApiAction),
}

fn main() -> anyhow::Result<()> {
    init_tracing();
    let cli = Cli::parse();

    let Some(command) = cli.command else {
        println!("byte-wiki {}\n", env!("CARGO_PKG_VERSION"));
        println!("用 `bs --help` 查看可用命令。");
        return Ok(());
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        match command {
            Commands::Backup(a) => cmd::run_backup(a, cli.json).await,
            Commands::Restore(a) => cmd::run_restore(a, cli.json).await,
            Commands::Open(a) => cmd::run_open(a, cli.json).await,
            Commands::Repos(a) => cmd::run_repos(a, cli.json).await,
            Commands::Callchain(a) => cmd::run_callchain(a, cli.json).await,
            Commands::CallchainQuery(a) => cmd::run_callchain_query(a, cli.json).await,
            Commands::Mcp(a) => cmd::run_mcp(a).await,
            Commands::Api(a) => {
                cmd::run_api(cmd::ApiArgs { action: a }).await
            }
        }
    })?;
    Ok(())
}

/// 初始化日志：默认 INFO 级别，写到 stderr（绝不污染 stdout，MCP stdio 模式依赖此约定）。
fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
}
