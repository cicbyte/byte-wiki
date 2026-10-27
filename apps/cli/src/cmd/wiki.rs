//! wiki 命令组：AI/脚本读写项目知识（outline / read / edit / card）。
//! 与 MCP 工具同一套 core 服务实现——行为天然一致。

use clap::{Args, Subcommand};

use super::open_service;

#[derive(Args, Debug)]
pub struct WikiArgs {
    /// 仓库路径（需已登记）
    pub repo: String,

    #[command(subcommand)]
    pub command: WikiCommand,
}

#[derive(Subcommand, Debug)]
pub enum WikiCommand {
    /// 文档清单（路径/类型/标题）
    Outline {
        /// 分支（缺省 = 当前分支）
        #[arg(long)]
        branch: Option<String>,
    },
    /// 读文档原文（cards/ 前缀读知识卡）
    Read {
        /// 文档路径（outline 可查）
        doc: String,
        /// 分支（缺省 = 当前分支）
        #[arg(long)]
        branch: Option<String>,
    },
    /// 定向编辑：search/replace 块全有或全无（--replacements JSON 文件：[{"search":"…","replace":"…"}]）
    Edit {
        /// 文档路径
        doc: String,
        /// 替换块 JSON 文件路径（UTF-8）
        #[arg(long)]
        replacements: String,
    },
    /// 创建知识卡（人工知识区，生成永不覆盖、可被问答检索）
    Card {
        /// 卡片标题
        #[arg(long)]
        title: String,
        /// 正文 Markdown 文件路径（UTF-8）
        #[arg(long)]
        file: String,
    },
}

pub async fn run_wiki(args: WikiArgs, json: bool) -> anyhow::Result<()> {
    let svc = open_service(json)?;
    match &args.command {
        WikiCommand::Outline { branch } => {
            let canon = byte_wiki_core::canonical_key(std::path::Path::new(&args.repo))?;
            let repo = svc
                .db
                .get_repo_by_path(&canon)?
                .ok_or_else(|| anyhow::anyhow!("仓库未登记：{}", args.repo))?;
            let b = branch
                .clone()
                .or_else(|| repo.branch.filter(|b| !b.is_empty()))
                .unwrap_or_default();
            let docs = svc.db.list_wiki_docs(repo.id, &b)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string(&docs.into_iter().map(|(p, k, t)| serde_json::json!({"path": p, "kind": k, "title": t})).collect::<Vec<_>>())?
                );
            } else {
                for (p, k, t) in docs {
                    println!("[{k}] {p} — {t}");
                }
            }
        }
        WikiCommand::Read { doc, branch } => {
            print!("{}", svc.read_wiki_doc(&args.repo, doc, branch.as_deref())?);
        }
        WikiCommand::Edit { doc, replacements } => {
            let raw = std::fs::read_to_string(replacements)
                .map_err(|e| anyhow::anyhow!("读取替换文件失败 {replacements}：{e}"))?;
            let reps: Vec<(String, String)> = serde_json::from_str::<Vec<serde_json::Value>>(&raw)?
                .into_iter()
                .map(|v| {
                    (
                        v["search"].as_str().unwrap_or_default().to_string(),
                        v["replace"].as_str().unwrap_or_default().to_string(),
                    )
                })
                .collect();
            if reps.is_empty() {
                anyhow::bail!("替换清单为空：{replacements}");
            }
            svc.edit_wiki_doc(&args.repo, doc, &reps)?;
            println!("✅ 已编辑 {doc}（{} 块替换；RAG 检索需重建向量索引后刷新）", reps.len());
        }
        WikiCommand::Card { title, file } => {
            let content = std::fs::read_to_string(file)
                .map_err(|e| anyhow::anyhow!("读取正文失败 {file}：{e}"))?;
            let rel = svc.create_card(&args.repo, title, &content)?;
            println!("✅ 已创建知识卡 {rel}（检索需重建向量索引后生效）");
        }
    }
    Ok(())
}
