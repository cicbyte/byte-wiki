//! 大纲阶段（Qoder 式管线第一步）：LLM 设计 Wiki 结构
//!
//! 输入 = 文件清单 + README + 模块统计（依赖边），产出 [`WikiOutline`]：
//! 扁平章节（parent 引用支持任意层级）+ 页面（标题/定位/重要性/brief/
//! 绑定文件/互链）。brief 是大纲阶段写好的逐页生成指令（Qoder catalog.prompt
//! 思路），逐页生成直接消费，避免二次理解。
//!
//! 解析容错：剥 markdown 围栏、正则抽取 XML 块（page/section 标签不自嵌套，
//! 无需递归解析）、路径归一化、未知文件剔除、空页丢弃。

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

// ============ 类型 ============

/// Wiki 大纲（outline.json 的 schema）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiOutline {
    /// 结构版本（当前 2：章节 + 页面 + brief）
    #[serde(default = "outline_version")]
    pub version: u32,
    pub title: String,
    pub description: String,
    /// 扁平章节表；层级由 parent 引用表达（None = 顶层）
    pub sections: Vec<OutlineSection>,
    pub pages: Vec<OutlinePage>,
}

fn outline_version() -> u32 {
    2
}

/// 章节（仅组织结构，不承载内容）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlineSection {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

/// 页面：wiki 的内容单元
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlinePage {
    pub id: String,
    pub title: String,
    /// 一句话定位（大纲阶段给出）
    pub description: String,
    /// high / medium / low
    pub importance: String,
    /// 逐页生成指令：覆盖什么、强调什么、配什么图（Qoder catalog.prompt 思路）
    pub brief: String,
    /// 绑定文件（仓库相对路径，/ 分隔）
    pub files: Vec<String>,
    /// 互链页面 id
    pub related: Vec<String>,
    /// 所属章节 id（None = 不挂章节）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
}

impl WikiOutline {
    /// 重要性排序（high → medium → low，稳定），逐页生成按此序（重要页先出）
    pub fn sorted_by_importance(&self) -> Vec<&OutlinePage> {
        let rank = |p: &OutlinePage| match p.importance.as_str() {
            "high" => 0,
            "medium" => 1,
            _ => 2,
        };
        let mut pages: Vec<&OutlinePage> = self.pages.iter().collect();
        pages.sort_by_key(|p| rank(p));
        pages
    }

    /// 页面的章节链标题（根在前）；无章节返回空
    pub fn section_chain(&self, page: &OutlinePage) -> Vec<String> {
        let mut chain = Vec::new();
        let mut cur = page.section.as_deref();
        let mut guard = 0;
        while let Some(id) = cur {
            guard += 1;
            if guard > 8 {
                break; // 环保护
            }
            match self.sections.iter().find(|s| s.id == id) {
                Some(s) => {
                    chain.push(s.title.clone());
                    cur = s.parent.as_deref();
                }
                None => break,
            }
        }
        chain.reverse();
        chain
    }
}

/// 从磁盘读取 outline.json（增量更新复用旧大纲）
pub fn read_outline(wiki_dir: &Path) -> Option<WikiOutline> {
    let text = std::fs::read_to_string(wiki_dir.join("outline.json")).ok()?;
    serde_json::from_str(&text).ok()
}

// ============ 大纲 prompt ============

/// 按仓库规模给出页数目标（deepwiki-open 的 8-12 对中型仓太保守，
/// Qoder 同规模铺 151 页；取中间档：文件数/4，夹在 [10, 40]）
fn target_pages(file_count: usize) -> (usize, usize) {
    let target = (file_count / 4).clamp(10, 40);
    let quarter = (target / 4).max(2);
    (target.saturating_sub(quarter), target + quarter)
}

/// 文件清单渲染（大纲输入）：排序平铺 + 语言/体积标注，超限截断
/// 大纲清单不值得占位的扩展（图片/字体/压缩包/锁文件/压缩产物）——
/// 留在索引里但不出现在给 LLM 的文件清单（省行数与注意力）
fn listable_in_outline(path: &str) -> bool {
    const JUNK: &[&str] = &[
        ".png", ".jpg", ".jpeg", ".gif", ".ico", ".svg", ".webp", ".bmp",
        ".woff", ".woff2", ".ttf", ".eot", ".otf",
        ".zip", ".gz", ".tar", ".rar", ".7z", ".pdf",
        ".lock", ".min.js", ".min.css", ".map",
    ];
    let lower = path.to_lowercase();
    if JUNK.iter().any(|e| lower.ends_with(e)) {
        return false;
    }
    !matches!(
        path.rsplit('/').next().unwrap_or(path),
        "Cargo.lock" | "package-lock.json" | "pnpm-lock.yaml" | "yarn.lock" | "uv.lock" | "poetry.lock"
    )
}

fn render_file_tree(files: &[(String, String, u64)], max_lines: usize) -> String {
    let mut sorted: Vec<&(String, String, u64)> = files
        .iter()
        .filter(|(p, _, _)| listable_in_outline(p))
        .collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let total = files.len();
    let mut out = String::new();
    if total > sorted.len() {
        out.push_str(&format!("（另有 {} 个二进制/锁文件不在清单内）
", total - sorted.len()));
    }
    for (i, (path, lang, size)) in sorted.iter().enumerate() {
        if i >= max_lines {
            out.push_str(&format!("…（另有 {} 个文件未列出）\n", sorted.len() - max_lines));
            break;
        }
        out.push_str(&format!("- {path} [{lang}, {}B]\n", size));
    }
    out
}

/// 模块统计渲染：模块名（语言/文件数）+ 依赖方向
pub(crate) fn render_modules(
    modules: &[(String, usize, Vec<String>)], // (name, file_count, langs)
    edges: &[(String, String, i64)],          // (from 依赖 to, refs)
) -> String {
    let mut out = String::new();
    for (name, count, langs) in modules {
        let deps: Vec<&str> = edges
            .iter()
            .filter(|(f, _, _)| f == name)
            .map(|(_, t, _)| t.as_str())
            .collect();
        let dep_str = if deps.is_empty() {
            String::new()
        } else {
            format!("；依赖：{}", deps.join("、"))
        };
        out.push_str(&format!("- {name}（{} 文件，{}）{dep_str}\n", count, langs.join("/")));
    }
    out
}

/// 组装大纲设计 prompt（输出为固定 XML 结构，含逐页 brief）
pub fn build_outline_prompt(
    repo_name: &str,
    files: &[(String, String, u64)], // (path, lang, size)
    readme: &str,
    modules: &[(String, usize, Vec<String>)],
    edges: &[(String, String, i64)],
) -> String {
    let (min_pages, max_pages) = target_pages(files.len());
    format!(
        r#"【Wiki 大纲设计】请分析仓库 {repo_name}，为其设计一份技术 Wiki 的结构大纲。你只能依据下面给定的仓库事实，不得臆造不存在的文件或模块。

<文件清单>
{file_tree}</文件清单>

<README>
{readme}</README>

<模块统计与依赖（from 依赖 to）>
{modules_ctx}</模块统计与依赖>

设计要求：
1. 页数目标：{min_pages}-{max_pages} 页。每页聚焦一个可独立阅读的主题（子系统、核心机制、数据流、接口层、部署形态……），不要按目录机械镜像，也不要一页塞整个仓库。
2. 章节用于组织页面：4-9 个顶层章节，必要时可嵌套子章节（用 parent 指向上级章节 id）。参考章节维度：项目总览 / 系统架构 / 核心功能 / 数据管理 / 接口层 / 前端 / 部署运维 / 开发指南（按仓库实际取舍，不合适的不硬凑）。
3. 每页必须给 brief：2-4 句「这一页写什么、重点覆盖哪些能力、建议配什么图（架构图/时序图/流程图）」——它是后续逐页生成的直接指令，务必具体。
4. 每页绑定 3-12 个真实存在的文件（relevant files，逐页生成的唯一事实来源）；文件少的主题可少于 3 个。
5. importance：核心链路 high、重要子系统 medium、边缘/参考 low。
6. related 给出主题相关的其他页面 id，形成互链网络。

严格按以下 XML 格式输出（章节标签自闭合；只输出 XML，不要 markdown 代码块，不要任何解释文字）：

<wiki_structure>
  <title>仓库 Wiki 总标题</title>
  <description>仓库一句话描述</description>
  <sections>
    <section id="s1" title="章节标题" parent=""/>
    <section id="s2" title="子章节标题" parent="s1"/>
  </sections>
  <pages>
    <page id="p1">
      <title>页面标题</title>
      <description>一句话定位</description>
      <importance>high|medium|low</importance>
      <brief>逐页生成指令：写什么、覆盖哪些能力、建议配图</brief>
      <section>s1</section>
      <files>
        <file>真实相对路径</file>
      </files>
      <related>
        <page_ref>p2</page_ref>
      </related>
    </page>
  </pages>
</wiki_structure>"#,
        file_tree = render_file_tree(files, 1500),
        readme = if readme.trim().is_empty() { "（无 README）" } else { readme },
        modules_ctx = render_modules(modules, edges),
    )
}

// ============ 解析与归一化 ============

/// XML 实体还原
fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

fn extract_tag(block: &str, tag: &str) -> Option<String> {
    let re = regex::Regex::new(&format!(r"<{tag}>([\s\S]*?)</{tag}>")).ok()?;
    re.captures(block)
        .map(|c| xml_unescape(c.get(1).map(|m| m.as_str()).unwrap_or("")).trim().to_string())
}

/// 解析 LLM 输出为大纲（失败返回可读错误，供重试 prompt 拼接）
pub fn parse_outline(raw: &str) -> Result<WikiOutline, String> {
    // 剥 ```xml / ``` 围栏与前后杂讯
    let mut text = raw.trim().to_string();
    if let Some(start) = text.find("<wiki_structure>") {
        if let Some(end_rel) = text.rfind("</wiki_structure>") {
            let end = (end_rel + "</wiki_structure>".len()).min(text.len());
            text = text[start..end].to_string();
        }
    }
    if !text.contains("<wiki_structure>") || !text.contains("</wiki_structure>") {
        return Err("输出中未找到 <wiki_structure>...</wiki_structure> 结构".into());
    }

    let title = extract_tag(&text, "title").unwrap_or_else(|| "项目 Wiki".into());
    let description = extract_tag(&text, "description").unwrap_or_default();

    // 章节：优先自闭合 <section id title parent/>，回退块状 <section id="..">…</section>
    let mut sections: Vec<OutlineSection> = Vec::new();
    let self_closing = regex::Regex::new(
        r#"<section\s+id="([^"]+)"\s+title="([^"]*)"(?:\s+parent="([^"]*)")?\s*/>"#,
    )
    .unwrap();
    for c in self_closing.captures_iter(&text) {
        sections.push(OutlineSection {
            id: xml_unescape(c.get(1).map(|m| m.as_str()).unwrap_or("")),
            title: xml_unescape(c.get(2).map(|m| m.as_str()).unwrap_or("")),
            parent: match c.get(3).map(|m| m.as_str()) {
                Some(p) if !p.trim().is_empty() => Some(xml_unescape(p)),
                _ => None,
            },
        });
    }
    if sections.is_empty() {
        let block_re =
            regex::Regex::new(r#"<section\s+id="([^"]+)">([\s\S]*?)</section>"#).unwrap();
        for c in block_re.captures_iter(&text) {
            let inner = c.get(2).map(|m| m.as_str()).unwrap_or("");
            sections.push(OutlineSection {
                id: xml_unescape(c.get(1).map(|m| m.as_str()).unwrap_or("")),
                title: extract_tag(inner, "title").unwrap_or_default(),
                parent: extract_tag(inner, "parent").filter(|p| !p.is_empty()),
            });
        }
    }

    // 页面块（page 标签不自嵌套，非贪婪抽取安全）
    let page_re = regex::Regex::new(r#"<page(?:\s+id="([^"]+)")?\s*>([\s\S]*?)</page>"#).unwrap();
    let file_re = regex::Regex::new(r"<file>([\s\S]*?)</file>").unwrap();
    let rel_re = regex::Regex::new(r"<page_ref>([\s\S]*?)</page_ref>").unwrap();
    let mut pages: Vec<OutlinePage> = Vec::new();
    for (i, c) in page_re.captures_iter(&text).enumerate() {
        let inner = c.get(2).map(|m| m.as_str()).unwrap_or("");
        let title = extract_tag(inner, "title").unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let importance = extract_tag(inner, "importance")
            .map(|v| match v.as_str() {
                "high" | "medium" | "low" => v,
                _ => "medium".to_string(),
            })
            .unwrap_or_else(|| "medium".into());
        let files: Vec<String> = file_re
            .captures_iter(inner)
            .filter_map(|fc| {
                let p = xml_unescape(fc.get(1).map(|m| m.as_str()).unwrap_or("")).trim().to_string();
                (!p.is_empty()).then_some(p)
            })
            .collect();
        let related: Vec<String> = rel_re
            .captures_iter(inner)
            .map(|rc| xml_unescape(rc.get(1).map(|m| m.as_str()).unwrap_or("")).trim().to_string())
            .filter(|p| !p.is_empty())
            .collect();
        pages.push(OutlinePage {
            id: c
                .get(1)
                .map(|m| xml_unescape(m.as_str()))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| format!("p{}", i + 1)),
            title,
            description: extract_tag(inner, "description").unwrap_or_default(),
            importance,
            brief: extract_tag(inner, "brief").unwrap_or_default(),
            files,
            related,
            section: extract_tag(inner, "section").filter(|s| !s.is_empty()),
        });
    }

    if pages.is_empty() {
        return Err("大纲中没有解析到任何 <page> 页面".into());
    }

    Ok(WikiOutline { version: 2, title, description, sections, pages })
}

/// 归一化：路径清洗、未知文件剔除、无效引用清理、空页丢弃、章节去重。
/// `known_files` 为索引登记的全量文件路径集合。
pub fn normalize_outline(mut outline: WikiOutline, known_files: &HashSet<String>) -> WikiOutline {
    // 章节去重 + 标题兜底
    let mut seen_sections: HashSet<String> = HashSet::new();
    outline.sections.retain_mut(|s| {
        if s.id.is_empty() || seen_sections.contains(&s.id) {
            return false;
        }
        if s.title.trim().is_empty() {
            s.title = s.id.clone();
        }
        seen_sections.insert(s.id.clone());
        true
    });
    // parent 引用必须指向存在章节
    for s in &mut outline.sections {
        if let Some(p) = &s.parent {
            if !seen_sections.contains(p) {
                s.parent = None;
            }
        }
    }

    // 页面 id 兜底去重
    let mut seen_ids: HashSet<String> = HashSet::new();
    for (i, p) in outline.pages.iter_mut().enumerate() {
        if p.id.is_empty() || !seen_ids.insert(p.id.clone()) {
            p.id = format!("p{}", i + 1);
            let _ = seen_ids.insert(p.id.clone());
        }
    }

    let valid_ids: HashSet<String> = outline.pages.iter().map(|p| p.id.clone()).collect();
    let mut dropped_unknown = 0usize;
    outline.pages.retain_mut(|p| {
        let before = p.files.len();
        let mut seen: HashSet<String> = HashSet::new();
        p.files.retain_mut(|f| {
            *f = normalize_path(f);
            known_files.contains(f) && seen.insert(f.clone())
        });
        dropped_unknown += before - p.files.len();
        // 清理互链与章节引用
        p.related.retain(|r| valid_ids.contains(r));
        p.related.sort();
        p.related.dedup();
        if let Some(s) = &p.section {
            if !seen_sections.contains(s) {
                p.section = None;
            }
        }
        !p.files.is_empty()
    });
    if dropped_unknown > 0 {
        tracing::info!(outline_dropped_unknown_files = dropped_unknown, "大纲文件引用归一化");
    }

    // 丢弃无页面的空章节
    let used: HashSet<&String> = outline.pages.iter().filter_map(|p| p.section.as_ref()).collect();
    outline.sections.retain(|s| used.contains(&s.id));

    outline
}

/// 路径归一化：反斜杠→/、去 ./ 前缀、去首尾空白
pub fn normalize_path(p: &str) -> String {
    p.trim().replace('\\', "/").trim_start_matches("./").trim_start_matches('/').to_string()
}

// ============ 测试 ============

#[cfg(test)]
mod tests {
    use super::*;

    fn known() -> HashSet<String> {
        ["README.md", "src/main.rs", "src/lib.rs", "docs/a.md"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn parse_self_closing_sections_with_fences() {
        let raw = "```xml\n<wiki_structure>\n  <title>演示 &amp; 测试</title>\n  \
<description>dd</description>\n  <sections>\n    <section id=\"s1\" title=\"架构\" parent=\"\"/>\n \
</sections>\n  <pages>\n    <page id=\"p1\">\n      <title>入口</title>\n      \
<description>启动流程</description>\n      <importance>high</importance>\n      \
<brief>写启动链路，配时序图</brief>\n      <section>s1</section>\n      <files>\n        \
<file>./src/main.rs</file>\n        <file>src\\lib.rs</file>\n      </files>\n      \
<related><page_ref>p2</page_ref></related>\n    </page>\n  </pages>\n</wiki_structure>\n```";
        let o = parse_outline(raw).unwrap();
        assert_eq!(o.title, "演示 & 测试");
        assert_eq!(o.sections.len(), 1);
        assert_eq!(o.pages.len(), 1);
        assert_eq!(o.pages[0].importance, "high");
        assert_eq!(o.pages[0].files.len(), 2);
        // 路径归一化（\ → /、去 ./）在 normalize 阶段发生
        let n = normalize_outline(o, &known());
        assert!(n.pages[0].files.contains(&"src/main.rs".to_string()), "实际：{:?}", n.pages[0].files);
        assert!(n.pages[0].files.contains(&"src/lib.rs".to_string()), "实际：{:?}", n.pages[0].files);
    }

    #[test]
    fn parse_block_sections_and_importance_fallback() {
        let raw = "<wiki_structure><title>t</title><description>d</description>\
<sections><section id=\"s1\"><title>总览</title></section></sections>\
<pages><page><title>首页</title><importance>whatever</importance><file>README.md</file></page>\
<page><title>无文件页</title></page></pages></wiki_structure>";
        let o = parse_outline(raw).unwrap();
        assert_eq!(o.sections[0].title, "总览");
        assert_eq!(o.pages[0].importance, "medium");
        assert_eq!(o.pages.len(), 2); // 无文件页由 normalize 丢弃
        let o2 = normalize_outline(o, &known());
        assert_eq!(o2.pages.len(), 1);
        assert_eq!(o2.pages[0].files, vec!["README.md"]);
    }

    #[test]
    fn normalize_drops_unknown_files_and_dangling_refs() {
        let o = WikiOutline {
            version: 2,
            title: "t".into(),
            description: String::new(),
            sections: vec![
                OutlineSection { id: "s1".into(), title: "A".into(), parent: None },
                OutlineSection { id: "sX".into(), title: "X".into(), parent: Some("nope".into()) },
            ],
            pages: vec![OutlinePage {
                id: "p1".into(),
                title: "页".into(),
                description: String::new(),
                importance: "high".into(),
                brief: String::new(),
                files: vec!["src/main.rs".into(), "nope/x.py".into(), "src/main.rs".into()],
                related: vec!["p_missing".into()],
                section: Some("sX".into()),
            }],
        };
        let n = normalize_outline(o, &known());
        assert_eq!(n.pages[0].files, vec!["src/main.rs"]);
        assert!(n.pages[0].related.is_empty());
        // sX parent 失效置空，但仍是页面所属章节而保留
        assert_eq!(n.pages[0].section.as_deref(), Some("sX"));
    }

    #[test]
    fn importance_sort_and_section_chain() {
        let o = WikiOutline {
            version: 2,
            title: "t".into(),
            description: String::new(),
            sections: vec![
                OutlineSection { id: "root".into(), title: "系统".into(), parent: None },
                OutlineSection { id: "sub".into(), title: "后端".into(), parent: Some("root".into()) },
            ],
            pages: vec![
                OutlinePage {
                    id: "a".into(), title: "低".into(), description: String::new(),
                    importance: "low".into(), brief: String::new(), files: vec!["README.md".into()],
                    related: vec![], section: None,
                },
                OutlinePage {
                    id: "b".into(), title: "高".into(), description: String::new(),
                    importance: "high".into(), brief: String::new(), files: vec!["README.md".into()],
                    related: vec![], section: Some("sub".into()),
                },
            ],
        };
        let sorted = o.sorted_by_importance();
        assert_eq!(sorted[0].id, "b");
        assert_eq!(o.section_chain(sorted[0]), vec!["系统".to_string(), "后端".to_string()]);
    }

    #[test]
    fn parse_failure_is_actionable() {
        assert!(parse_outline("完全不是 XML").is_err());
        assert!(parse_outline("<wiki_structure><title>x</title></wiki_structure>").is_err());
    }
}
