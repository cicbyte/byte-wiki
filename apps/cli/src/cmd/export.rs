//! Wiki 导出命令：导出为自包含静态 HTML 站（VitePress 形态，纯 Rust 生成）

use clap::Args;

use super::open_service;

#[derive(Args, Debug)]
pub struct ExportArgs {
    /// 仓库路径
    pub repo: String,

    /// 输出目录（默认 <repo>/.bytewiki/export/html）
    #[arg(long)]
    pub out: Option<String>,

    /// 指定分支（默认当前 git 分支）
    #[arg(long)]
    pub branch: Option<String>,
}

pub async fn run_export(args: ExportArgs, json: bool) -> anyhow::Result<()> {
    let svc = open_service(json)?;
    let out = args.out.clone().unwrap_or_else(|| {
        format!("{}/.bytewiki/export/html", args.repo.trim_end_matches(['/', '\\']))
    });
    let sum = svc.export_wiki_html(
        &args.repo,
        std::path::Path::new(&out),
        args.branch.as_deref(),
    )?;
    if json {
        println!("{}", serde_json::json!({ "out_dir": sum.out_dir, "pages": sum.pages }));
    } else {
        println!("✅ 已导出 {} 个页面 → {}", sum.pages, sum.out_dir);
        println!("   打开 index.html 即可离线浏览（mermaid 已内嵌，含打印样式可另存 PDF）");
    }
    Ok(())
}
