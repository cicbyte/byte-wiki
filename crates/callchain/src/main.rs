//! byte-callchain CLI：多语言（Python/JS/TS/Go/Java/Rust）AST 调用链提取 + 交互式 HTML 渲染。
//! standalone 入口：服务无 app 的流水线场景，任意目录可跑（不要求 .git）。

use anyhow::Result;
use clap::Parser as ClapParser;
use std::path::PathBuf;

#[derive(ClapParser, Debug)]
#[command(
    name = "byte-callchain",
    version,
    about = "多语言调用链提取（tree-sitter）：函数级调用图 + 交互式 HTML"
)]
struct Args {
    /// 源码目录（默认当前目录）
    src: Option<PathBuf>,
    /// 输出目录（默认 ./callchain-out）
    #[arg(short, long, default_value = "callchain-out")]
    out: PathBuf,
    /// 页面标题（默认取源码目录名）
    #[arg(long)]
    title: Option<String>,
    /// 额外入口的 qualname 正则（路由装饰器自动识别）
    #[arg(long, default_value = "")]
    entry: String,
    /// 额外排除目录名（可多次；内置表已含 node_modules/.git/.bytewiki 等）
    #[arg(long = "exclude")]
    excludes: Vec<String>,
    /// 不内嵌函数源码（默认内嵌；HTML 详情面板依赖它）
    #[arg(long)]
    no_source: bool,
    /// 生成后不自动打开浏览器
    #[arg(long)]
    no_open: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let root: PathBuf = args.src.unwrap_or_else(|| ".".into());
    let root = root.canonicalize().unwrap_or(root);
    if !root.is_dir() {
        anyhow::bail!("目录不存在: {}", root.display());
    }
    let title = args
        .title
        .clone()
        .unwrap_or_else(|| {
            root.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "callchain".into())
        });

    let entry_re = if args.entry.is_empty() {
        None
    } else {
        Some(regex::Regex::new(&args.entry)?)
    };
    let opts = byte_wiki_callchain::ExtractOptions {
        entry_regex: entry_re,
        excludes: args.excludes,
        embed_source: !args.no_source,
    };

    let t0 = std::time::Instant::now();
    let mut graph = byte_wiki_callchain::extract(&root, &opts, &|_| {})?;
    graph["title"] = serde_json::json!(title);

    let out_dir = args.out;
    byte_wiki_callchain::render(&graph, &title, &out_dir)?;

    // 统计输出
    let n_nodes = graph["nodes"].as_array().map(|a| a.len()).unwrap_or(0);
    let n_edges = graph["edges"].as_array().map(|a| a.len()).unwrap_or(0);
    let n_entry = graph["nodes"]
        .as_array()
        .map(|a| a.iter().filter(|n| n["is_entry"].as_bool().unwrap_or(false)).count())
        .unwrap_or(0);
    let langs: Vec<String> = graph["langs"]
        .as_object()
        .map(|m| {
            let mut v: Vec<(String, u64)> = m
                .iter()
                .map(|(k, val)| (k.clone(), val.as_u64().unwrap_or(0)))
                .collect();
            v.sort();
            v.into_iter().map(|(k, c)| format!("{k}×{c}")).collect()
        })
        .unwrap_or_default();
    println!(
        "[OK] 函数 {n_nodes} · 调用边 {n_edges} · 入口 {n_entry} · 语言 {} · 耗时 {:?}",
        langs.join(" "),
        t0.elapsed()
    );
    let json_path = out_dir.join("callgraph.json");
    std::fs::write(&json_path, serde_json::to_string_pretty(&graph)?)?;
    println!("[OK] 输出: {} / callchain.html", json_path.display());

    if !args.no_open {
        let url = out_dir.join("callchain.html").canonicalize()?;
        let _ = webbrowser::open(url.as_os_str().to_string_lossy().as_ref());
    }
    Ok(())
}
