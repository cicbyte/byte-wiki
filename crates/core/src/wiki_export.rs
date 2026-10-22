//! Wiki HTML 导出：VitePress 形态的静态站（#716）
//!
//! 纯 Rust 生成、零 Node 依赖：pulldown-cmark 渲染 + syntect 代码高亮（内联样式，
//! 无需外部 CSS）+ mermaid.min.js 内嵌（include_bytes，导出目录自包含、离线可开）。
//!
//! 形态：index.html（总览）+ 逐页 HTML + 共享侧边栏（按 pages/ 目录分组，VitePress 式）；
//! `.md` 文档间链接重写为相对 .html 链接；`bw-file://` 引用导出后无 IDE 可跳，
//! href 中性化为 `#`（链接文本本身即 `路径:行`，出处信息完整保留）。
//! @media print 样式内置——浏览器/WebView 打印即得 PDF（v1 路径）。

use std::collections::BTreeMap;
use std::path::Path;

use std::sync::OnceLock;

use percent_encoding::utf8_percent_encode;
use percent_encoding::AsciiSet;
use pulldown_cmark::CodeBlockKind;
use pulldown_cmark::CowStr;
use pulldown_cmark::Event;
use pulldown_cmark::Options;
use pulldown_cmark::Parser;
use pulldown_cmark::Tag;
use pulldown_cmark::TagEnd;

use crate::service::AppService;
use crate::CoreResult;

/// mermaid 渲染器（与桌面端/扩展同源 vendored IIFE 构建）
const MERMAID_JS: &[u8] = include_bytes!("../assets_mermaid.min.js");

/// 导出结果摘要
#[derive(Debug, Clone, serde::Serialize)]
pub struct WikiHtmlExport {
    pub out_dir: String,
    pub pages: usize,
}

/// 站内一页（源 .md → 输出 .html 的映射与导航信息）
struct PageSpec {
    /// 源文件相对分支根（/ 分隔）
    src: String,
    /// 输出 html 相对导出根（/ 分隔）
    out: String,
    title: String,
    /// 侧边栏分组（None = 根级文档，排在最前）
    group: Option<String>,
}

// ============ syntect 高亮（进程级单例，首次使用加载） ============

struct Hl {
    ss: syntect::parsing::SyntaxSet,
    theme: syntect::highlighting::Theme,
}

fn hl() -> &'static Hl {
    static HL: OnceLock<Hl> = OnceLock::new();
    HL.get_or_init(|| Hl {
        ss: syntect::parsing::SyntaxSet::load_defaults_newlines(),
        theme: syntect::highlighting::ThemeSet::load_defaults()
            .themes
            .remove("InspiredGitHub")
            .expect("syntect 内置 InspiredGitHub 主题"),
    })
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('\"', "&quot;")
}

/// 代码块 → syntect 高亮 HTML（内联样式；未知语言纯转义）
fn highlight_code(lang: &str, code: &str) -> String {
    let h = hl();
    let syntax = h
        .ss
        .find_syntax_by_token(lang)
        .unwrap_or_else(|| h.ss.find_syntax_plain_text());
    match syntect::html::highlighted_html_for_string(code, &h.ss, syntax, &h.theme) {
        Ok(html) => format!("<pre class=\"code\"><code data-lang=\"{lang}\">{html}</code></pre>"),
        Err(_) => format!("<pre class=\"code\"><code>{}</code></pre>", escape_html(code)),
    }
}

// ============ 链接编码与相对路径 ============

const HREF_SET: &AsciiSet = &AsciiSet::EMPTY
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'\\')
    .add(b'^')
    .add(b'|');

fn encode_href(path: &str) -> String {
    utf8_percent_encode(path, HREF_SET).to_string()
}

/// 输出页之间的相对链接（from/to 均为 / 分隔的站内路径）
fn rel_href(from: &str, to: &str) -> String {
    let up = from.matches('/').count();
    let mut p = String::new();
    for _ in 0..up {
        p.push_str("../");
    }
    p.push_str(to);
    encode_href(&p)
}

// ============ markdown 渲染（事件流改写） ============

/// 惰性态：普通代码块收集到文本后高亮；mermaid 块改为 div 容器
fn render_markdown(
    md: &str,
    specs: &BTreeMap<String, String>, // src 路径 → out 路径（.md 链接重写）
    cur_src: &str,
) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(md, opts);

    let cur_dir: Vec<&str> = cur_src.split('/').collect::<Vec<_>>()[..cur_src.matches('/').count()]
        .to_vec();

    let mut out_events: Vec<Event> = Vec::new();
    let mut code_buf = String::new();
    let mut code_lang = String::new();
    let mut in_code = false;       // 普通代码块（收集后高亮）
    let mut in_mermaid = false;    // mermaid 块（透传文本进 div）

    for ev in parser {
        match ev {
            Event::Start(Tag::CodeBlock(kind)) => {
                let info = match &kind {
                    CodeBlockKind::Fenced(i) => i.trim().to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                if info == "mermaid" {
                    in_mermaid = true;
                    out_events.push(Event::Html("<div class=\"mermaid\">".into()));
                } else {
                    in_code = true;
                    code_buf.clear();
                    code_lang = info.split_whitespace().next().unwrap_or("").to_string();
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if in_mermaid {
                    in_mermaid = false;
                    out_events.push(Event::Html("</div>".into()));
                } else if in_code {
                    in_code = false;
                    out_events.push(Event::Html(highlight_code(&code_lang, &code_buf).into()));
                }
            }
            Event::Text(t) if in_code => code_buf.push_str(&t),
            Event::Start(Tag::Link { dest_url, title, link_type, id }) => {
                let href = dest_url.to_string();
                if href.starts_with("bw-file://") {
                    // 引用链接：导出后无 IDE 可跳——中性化 href，文本（路径:行）保留出处
                    out_events.push(Event::Start(Tag::Link {
                        dest_url: CowStr::from("#"),
                        title: CowStr::from("源码引用（导出态不可跳转）"),
                        link_type,
                        id,
                    }));
                } else if !href.contains("://") && href.split('#').next().unwrap_or("").ends_with(".md") {
                    let file = href.split('#').next().unwrap_or(&href);
                    let file = file.trim_start_matches("./");
                    // 相对当前文档目录解析（与桌面端 WikiViewer 同语义）
                    let mut parts = cur_dir.clone();
                    for seg in file.split('/') {
                        if seg == ".." {
                            parts.pop();
                        } else if !seg.is_empty() && seg != "." {
                            parts.push(seg);
                        }
                    }
                    let target_src = parts.join("/");
                    if let Some(out) = specs.get(&target_src) {
                        let cur_out = specs.get(cur_src).cloned().unwrap_or_default();
                        out_events.push(Event::Start(Tag::Link {
                            dest_url: CowStr::from(rel_href(&cur_out, out)),
                            title,
                            link_type,
                            id,
                        }));
                    } else {
                        out_events.push(Event::Start(Tag::Link { dest_url: CowStr::from("#"), title, link_type, id }));
                    }
                } else {
                    out_events.push(Event::Start(Tag::Link { dest_url, title, link_type, id }));
                }
            }
            other => out_events.push(other),
        }
    }

    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, out_events.into_iter());
    html
}

// ============ 页面收集与模板 ============

fn strip_frontmatter(raw: &str) -> &str {
    let re = raw.strip_prefix("---\n");
    if let Some(rest) = re {
        if let Some(end) = rest.find("\n---\n") {
            return rest[end + 5..].trim_start_matches('\n');
        }
    }
    raw
}

fn collect_pages(broot: &Path) -> Vec<PageSpec> {
    let mut specs = Vec::new();
    let push = |specs: &mut Vec<PageSpec>, src: &str, out: &str, title: String, group: Option<String>| {
        specs.push(PageSpec { src: src.to_string(), out: out.to_string(), title, group });
    };
    if broot.join("overview.md").is_file() {
        push(&mut specs, "overview.md", "index.html", "总览".into(), None);
    }
    if broot.join("architecture.md").is_file() {
        push(&mut specs, "architecture.md", "architecture.html", "架构".into(), None);
    }
    // pages/**：按首层目录分组（章节），文件名即页名
    let pages_dir = broot.join("pages");
    let mut files: Vec<String> = Vec::new();
    collect_md(&pages_dir, "", &mut files);
    files.sort();
    for rel in files {
        let out = rel.trim_end_matches(".md").to_string() + ".html";
        let group = rel.split_once('/').map(|(g, _)| g.to_string());
        let title = rel.trim_end_matches(".md").rsplit('/').next().unwrap_or(&rel).to_string();
        push(&mut specs, &format!("pages/{rel}"), &out, title, group);
    }
    // 知识卡（跨分支共享区，.bytewiki/cards/）
    let cards_dir = broot
        .parent()
        .and_then(|p| p.parent())
        .map(|w| w.join("cards"));
    if let Some(cd) = cards_dir {
        let mut cards: Vec<String> = Vec::new();
        collect_md(&cd, "", &mut cards);
        cards.sort();
        for rel in cards {
            let out = format!("cards/{}.html", rel.trim_end_matches(".md"));
            let title = rel.trim_end_matches(".md").to_string();
            push(&mut specs, &format!("cards/{rel}"), &out, title, Some("知识卡".into()));
        }
    }
    // 模块依赖图谱（mermaid 文档）
    let dep = broot.join("graphs/dependency.mmd");
    if dep.is_file() {
        push(&mut specs, "graphs/dependency.mmd", "graphs/dependency.html", "模块依赖图谱".into(), Some("图谱".into()));
    }
    specs
}

fn collect_md(dir: &Path, prefix: &str, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let rel = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            collect_md(&e.path(), &rel, out);
        } else if rel.ends_with(".md") {
            out.push(rel);
        }
    }
}

fn sidebar_html(specs: &[PageSpec], cur_out: &str) -> String {
    // 分组序：根级（总览/架构）→ 章节字典序 → 知识卡 → 图谱
    let mut groups: BTreeMap<String, Vec<&PageSpec>> = BTreeMap::new();
    let mut roots: Vec<&PageSpec> = Vec::new();
    for s in specs {
        match &s.group {
            Some(g) if g == "知识卡" => groups.entry("知识卡".into()).or_default().push(s),
            Some(g) if g == "图谱" => groups.entry("图谱".into()).or_default().push(s),
            Some(g) => groups.entry(g.clone()).or_default().push(s),
            None => roots.push(s),
        }
    }
    let mut h = String::from("<nav class=\"side\"><div class=\"side-logo\">Byte Wiki</div>");
    let href = |out: &str| rel_href(cur_out, out);
    for s in roots {
        let active = if s.out == cur_out { " class=\"active\"" } else { "" };
        h += &format!("<a href=\"{}\"{active}>{}</a>", href(&s.out), escape_html(&s.title));
    }
    for (g, items) in &groups {
        h += &format!("<div class=\"side-group\">{}</div>", escape_html(g));
        for s in items {
            let active = if s.out == cur_out { " class=\"active\"" } else { "" };
            h += &format!("<a href=\"{}\"{active}>{}</a>", href(&s.out), escape_html(&s.title));
        }
    }
    h + "</nav>"
}

fn page_html(title: &str, site: &str, sidebar: &str, content: &str, cur_out: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} · {site}</title>
<link rel="stylesheet" href="{css}">
</head>
<body>
{sidebar}
<main class="content">
<article>{content}</article>
<footer class="page-foot">由 byte-wiki 导出 · <a href="{home}">返回总览</a></footer>
</main>
<script src="{mermaid}"></script>
<script>mermaid.initialize({{ startOnLoad: true, theme: "default", securityLevel: "strict" }});</script>
</body>
</html>
"#,
        title = escape_html(title),
        site = escape_html(site),
        css = rel_href(cur_out, "assets/style.css"),
        sidebar = sidebar,
        content = content,
        home = rel_href(cur_out, "index.html"),
        mermaid = rel_href(cur_out, "assets/mermaid.min.js"),
    )
}

const STYLE_CSS: &str = r#"
:root { --tx: #213547; --tx2: #6b7785; --bd: #e3e8ee; --bg: #fff; --bg2: #f6f8fa; --pri: #345dce; }
* { box-sizing: border-box; }
body { margin: 0; font-family: -apple-system, "Segoe UI", "Microsoft YaHei", "PingFang SC", sans-serif; color: var(--tx); background: var(--bg); font-size: 15.5px; line-height: 1.75; }
a { color: var(--pri); text-decoration: none; }
a:hover { text-decoration: underline; }
.side { position: fixed; top: 0; left: 0; bottom: 0; width: 268px; padding: 20px 16px; border-right: 1px solid var(--bd); background: var(--bg2); overflow-y: auto; }
.side-logo { font-weight: 700; font-size: 16px; margin-bottom: 14px; color: var(--tx); }
.side-group { font-size: 12px; color: var(--tx2); text-transform: uppercase; letter-spacing: .06em; margin: 14px 0 4px; padding-left: 8px; }
.side a { display: block; padding: 4px 8px  4px 14px; border-radius: 6px; color: var(--tx); font-size: 13.5px; line-height: 1.5; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.side a:hover { background: #e9edf3; text-decoration: none; }
.side a.active { color: var(--pri); font-weight: 600; background: #e3e9f5; }
.content { margin-left: 268px; padding: 36px 48px 80px; }
article { max-width: 820px; }
article h1 { font-size: 1.75em; border-bottom: 1px solid var(--bd); padding-bottom: .3em; margin-top: .5em; }
article h2 { font-size: 1.35em; border-bottom: 1px solid var(--bd); padding-bottom: .25em; margin-top: 1.6em; }
article h3 { font-size: 1.12em; margin-top: 1.4em; }
article img { max-width: 100%; }
article table { border-collapse: collapse; margin: 1em 0; display: block; overflow-x: auto; }
article th, article td { border: 1px solid var(--bd); padding: 6px 13px; }
article th { background: var(--bg2); }
article blockquote { margin: 1em 0; padding: .1em 1em; border-left: 4px solid var(--bd); color: var(--tx2); background: var(--bg2); border-radius: 0 6px 6px 0; }
article code { font-family: Consolas, "JetBrains Mono", monospace; font-size: .9em; background: #eceff3; border-radius: 4px; padding: .15em .4em; }
pre.code { background: #f6f8fa; border: 1px solid var(--bd); border-radius: 8px; padding: 14px 16px; overflow-x: auto; line-height: 1.5; }
pre.code code { background: none; padding: 0; font-size: 13px; }
.mermaid { margin: 1.2em auto; text-align: center; overflow-x: auto; }
.mermaid svg { max-width: 100%; height: auto; }
.page-foot { margin-top: 48px; padding-top: 16px; border-top: 1px solid var(--bd); color: var(--tx2); font-size: 12.5px; }

/* 打印/导出 PDF：隐侧栏、内容铺满、避免图/代码块跨页断切 */
@media print {
  .side { display: none; }
  .content { margin: 0; padding: 0; }
  article { max-width: none; }
  .page-foot { display: none; }
  .mermaid, pre.code, table { break-inside: avoid; page-break-inside: avoid; }
  body { font-size: 12pt; }
}
"#;

impl AppService {
    /// 导出当前（或指定）分支的 Wiki 为自包含静态 HTML 站
    pub fn export_wiki_html(
        &self,
        repo_path: &str,
        out_dir: &Path,
        branch: Option<&str>,
    ) -> CoreResult<WikiHtmlExport> {
        let root = Path::new(repo_path);
        let branch = branch
            .map(str::to_string)
            .unwrap_or_else(|| crate::branch::current_branch(root));
        let broot = crate::branch::wiki_root(root, &branch);
        if !broot.join("overview.md").is_file() && !broot.join("pages").is_dir() {
            return Err(crate::CoreError::Other(anyhow::anyhow!(
                "分支「{branch}」没有 Wiki 产物（先在桌面端或 bw 生成）"
            )));
        }

        let specs = collect_pages(&broot);
        let src_to_out: BTreeMap<String, String> =
            specs.iter().map(|s| (s.src.clone(), s.out.clone())).collect();

        // 站点标题/描述：outline.json 优先，回退仓库名
        let (site, _desc) = std::fs::read_to_string(broot.join("outline.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .map(|j| {
                (
                    j["title"].as_str().unwrap_or("Wiki").to_string(),
                    j["description"].as_str().unwrap_or_default().to_string(),
                )
            })
            .unwrap_or_else(|| {
                (
                    root.file_name().and_then(|n| n.to_str()).unwrap_or("Wiki").to_string(),
                    String::new(),
                )
            });

        std::fs::create_dir_all(out_dir.join("assets"))?;
        std::fs::write(out_dir.join("assets/style.css"), STYLE_CSS)?;
        std::fs::write(out_dir.join("assets/mermaid.min.js"), MERMAID_JS)?;

        let mut written = 0usize;
        for s in &specs {
            // 知识卡在共享目录 .bytewiki/cards/，其余在分支目录下
            let src_path = if s.src.starts_with("cards/") {
                broot.parent().and_then(|p| p.parent()).map(|w| w.join(&s.src)).unwrap_or_else(|| broot.join(&s.src))
            } else {
                broot.join(&s.src)
            };
            let raw = std::fs::read_to_string(&src_path)
                .map_err(|e| crate::CoreError::Other(anyhow::anyhow!("读取页面失败 {}：{e}", s.src)))?;
            let md = if s.src.ends_with(".mmd") {
                format!("```mermaid\n{raw}\n```")
            } else if s.src.starts_with("cards/") {
                strip_frontmatter(&raw).to_string()
            } else {
                strip_frontmatter(&raw).to_string()
            };
            let content = render_markdown(&md, &src_to_out, &s.src);
            let sidebar = sidebar_html(&specs, &s.out);
            let html = page_html(&s.title, &site, &sidebar, &content, &s.out);
            let out_path = out_dir.join(s.out.replace('/', std::path::MAIN_SEPARATOR_STR));
            if let Some(p) = out_path.parent() {
                std::fs::create_dir_all(p)
                    .map_err(|e| crate::CoreError::Other(anyhow::anyhow!("创建目录失败 {}：{e}", p.display())))?;
            }
            std::fs::write(&out_path, html)
                .map_err(|e| crate::CoreError::Other(anyhow::anyhow!("写出页面失败 {}：{e}", out_path.display())))?;
            written += 1;
        }

        Ok(WikiHtmlExport {
            out_dir: out_dir.to_string_lossy().into_owned(),
            pages: written,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(dir: &std::path::Path) -> std::path::PathBuf {
        let repo = dir.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        let b = crate::branch::wiki_root(&repo, "main");
        std::fs::create_dir_all(b.join("pages/核心功能")).unwrap();
        std::fs::create_dir_all(b.join("graphs")).unwrap();
        std::fs::create_dir_all(b.parent().unwrap().parent().unwrap().join("cards")).unwrap();
        std::fs::write(
            b.join("overview.md"),
            "# 总览\n\n参见 [快速入门](pages/核心功能/入门.md)。\n\n```mermaid\ngraph TB\n  A --> B\n```\n",
        )
        .unwrap();
        std::fs::write(
            b.join("pages/核心功能/入门.md"),
            "# 入门\n\n源码引用：[src/main.rs:12-40]()\n\n```rust\nfn main() {}\n```\n",
        )
        .unwrap();
        std::fs::write(b.join("graphs/dependency.mmd"), "graph LR\n  A --> B\n").unwrap();
        std::fs::write(
            b.parent().unwrap().parent().unwrap().join("cards/note.md"),
            "---\ntitle: 卡\n---\n# 卡内容\n",
        )
        .unwrap();
        std::fs::write(
            b.join("outline.json"),
            r#"{"version":2,"title":"测试站","description":"d","sections":[],"pages":[]}"#,
        )
        .unwrap();
        let svc = AppService::open_at(dir.join("app"), dir.join("data")).unwrap();
        svc.open_repo(repo.to_str().unwrap()).unwrap();
        repo
    }

    #[test]
    fn exports_self_contained_site() {
        let dir = tempfile::tempdir().unwrap();
        let repo = setup(dir.path());
        let svc = AppService::open_at(dir.path().join("app"), dir.path().join("data")).unwrap();
        let out = dir.path().join("site");
        let sum = svc.export_wiki_html(repo.to_str().unwrap(), &out, None).unwrap();
        assert_eq!(sum.pages, 4, "总览+1页+图谱+1卡");

        for e in walkdir::WalkDir::new(&out).into_iter().flatten() { eprintln!("[debug] {}", e.path().display()); }
        let index = std::fs::read_to_string(out.join("index.html")).unwrap();
        assert!(index.contains("测试站"), "站点标题取自 outline");
        // .md 互链 → 相对 .html（含中文路径编码）
        assert!(index.contains("href=\"%E6%A0%B8%E5%BF%83%E5%8A%9F%E8%83%BD/%E5%85%A5%E9%97%A8.html\""), "中文路径链接已编码：{index}");
        assert!(index.contains("<div class=\"mermaid\">"), "mermaid 块容器化：{index}");
        // 资产自包含
        assert!(out.join("assets/mermaid.min.js").is_file());
        assert!(out.join("assets/style.css").is_file());
        assert!(index.contains("@media print") || std::fs::read_to_string(out.join("assets/style.css")).unwrap().contains("@media print"));

        let page = std::fs::read_to_string(out.join("核心功能/入门.html")).unwrap();
        assert!(page.contains("<code data-lang=\"rust\">"), "rust 代码块高亮");
        assert!(page.contains("class=\"active\""), "当前页侧边栏高亮");
        // 知识卡 frontmatter 剥离
        let card = std::fs::read_to_string(out.join("cards/note.html")).unwrap();
        assert!(card.contains("卡内容"));
        assert!(!card.contains("title: 卡"));

        // 指定不存在分支 → 明确报错
        assert!(svc.export_wiki_html(repo.to_str().unwrap(), &out, Some("nope")).is_err());
    }
}
