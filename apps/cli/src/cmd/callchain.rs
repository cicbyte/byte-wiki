//! 调用链命令：为已登记仓库生成函数级调用图（产物落 .bytewiki/graphs/callchain/）

use clap::Args;

use super::open_service;

#[derive(Args, Debug)]
pub struct CallchainArgs {
    /// 仓库路径（需已在 bs/桌面端打开登记）
    pub repo: String,
}

pub async fn run_callchain(args: CallchainArgs, json: bool) -> anyhow::Result<()> {
    let svc = open_service(json)?;
    let summary = svc.generate_callchain(&args.repo, &|n| {
        tracing::info!("调用链：已解析 {n} 个源文件");
    })?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "out_path": summary.out_path,
                "nodes": summary.nodes,
                "edges": summary.edges,
                "entries": summary.entries,
                "modules": summary.modules,
                "langs": summary.langs,
                "duration_ms": summary.duration_ms,
            })
        );
    } else {
        let langs = summary
            .langs
            .iter()
            .map(|(k, v)| format!("{k}×{v}"))
            .collect::<Vec<_>>()
            .join(" ");
        println!(
            "✅ 调用链已生成：函数 {} · 调用边 {} · 入口 {} · 模块 {} · 语言 {}（{}ms）",
            summary.nodes, summary.edges, summary.entries, summary.modules, langs, summary.duration_ms
        );
        println!("   产物：{}", summary.out_path);
    }
    Ok(())
}
