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


/// 调用链查询（读现有产物，不重新生成；测试/核对用）
#[derive(Args, Debug)]
pub struct CallchainQueryArgs {
    /// 仓库路径
    pub repo: String,

    /// 模糊匹配模式：qualname/name/file 包含即命中（如 "Query"、"cmd/query.rs"）
    #[arg(short, long)]
    pub grep: Option<String>,

    /// 查询目标节点 ID（精确）——给出时打印其上下游（调用了谁/被谁调用）
    #[arg(short, long)]
    pub id: Option<String>,

    /// 只列入口节点（is_entry）
    #[arg(long)]
    pub entries: bool,

    /// 限制输出条数（默认 50；id 查询不受限）
    #[arg(long, default_value_t = 50)]
    pub limit: usize,
}

pub async fn run_callchain_query(args: CallchainQueryArgs, json: bool) -> anyhow::Result<()> {
    let svc = open_service(json)?;
    let Some(g) = svc.read_callgraph(&args.repo)? else {
        anyhow::bail!("该仓库尚未生成调用链（先运行：bs callchain <repo>）");
    };
    let nodes = g["nodes"].as_array().cloned().unwrap_or_default();
    let edges = g["edges"].as_array().cloned().unwrap_or_default();

    // id 精确查询：节点档案 + 上下游
    if let Some(id) = &args.id {
        let node = nodes.iter().find(|n| n["id"] == id.as_str())
            .ok_or_else(|| anyhow::anyhow!("节点不存在：{id}"))?;
        let callees: Vec<serde_json::Value> = edges.iter()
            .filter(|e| e["src"] == id.as_str())
            .map(|e| serde_json::json!({"dst": e["dst"], "line": e["line"], "kind": e["kind"]}))
            .collect();
        let callers: Vec<serde_json::Value> = edges.iter()
            .filter(|e| e["dst"] == id.as_str())
            .map(|e| serde_json::json!({"src": e["src"], "line": e["line"], "kind": e["kind"]}))
            .collect();
        let out = serde_json::json!({
            "node": node,
            "callees": callees,
            "callers": callers,
        });
        if json {
            println!("{out}");
        } else {
            println!("📍 {}", node["id"]);
            println!("   {}:{}-{} · entry={} · routes={}",
                node["file"], node["lineno"], node["end_lineno"], node["is_entry"], node["routes"]);
            println!("   调用 {} 个：", callees.len());
            for c in &callees { println!("     → {} [{}] L{}", c["dst"], c["kind"], c["line"]); }
            println!("   被调 {} 次：", callers.len());
            for c in &callers { println!("     ← {} [{}] L{}", c["src"], c["kind"], c["line"]); }
        }
        return Ok(());
    }

    // 列表查询：grep / entries
    let pat = args.grep.unwrap_or_default().to_lowercase();
    let mut hits: Vec<&serde_json::Value> = nodes.iter()
        .filter(|n| {
            (args.entries && n["is_entry"] == true)
                || (!pat.is_empty() && (
                    n["id"].as_str().unwrap_or("").to_lowercase().contains(&pat)
                    || n["file"].as_str().unwrap_or("").to_lowercase().contains(&pat)))
        })
        .collect();
    if !args.entries && pat.is_empty() {
        anyhow::bail!("请指定 --grep <模式> 或 --entries 或 --id <节点ID>");
    }
    hits.sort_by_key(|n| n["id"].as_str().unwrap_or("").to_string());
    let total = hits.len();
    hits.truncate(args.limit);
    if json {
        println!("{}", serde_json::json!({"total": total, "shown": hits.len(), "nodes": hits}));
    } else {
        println!("命中 {total} 个（显示 {}）：", hits.len());
        for n in &hits {
            println!("  {} [{}] entry={} {}", n["id"], n["file"], n["is_entry"], n["routes"]);
        }
    }
    Ok(())
}
