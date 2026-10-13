//! 页面后处理：引用链接化 + 引用清单块 + 目录 + 文件名安全化
//!
//! LLM 按 deepwiki-open 纪律输出 `Sources: [path:12-40]()`（空括号），
//! 这里确定性解析为内部跳转链接 `bw-file://<编码路径>?L=12-40`（前端
//! WikiViewer 拦截后切文件浏览页并定位行）。`<details>` 引用块与页面目录
//! 不信任 LLM，由代码从实际引用生成（Qoder 的 cite 块习惯）。

use std::collections::HashSet;

// ============ bw-file 链接 ============

/// 百分号编码（保留 / 与非保留字符；Windows 路径合法但 markdown 链接内
/// 需转义的字符全部编码）
pub fn percent_encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for b in path.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn percent_decode_path(enc: &str) -> String {
    let bytes = enc.as_bytes();
    let hex = |c: u8| -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            b'A'..=b'F' => Some(c - b'A' + 10),
            _ => None,
        }
    };
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 解析 `[路径:12-40]()` / `[路径:12]()` / `[路径]()` 形态的引用标注
fn parse_citation(text: &str) -> Option<(String, Option<(usize, usize)>)> {
    let (path_part, range) = match text.rfind(':') {
        Some(idx) if text[idx + 1..].chars().all(|c| c.is_ascii_digit() || c == '-') => {
            let path = &text[..idx];
            let rest = &text[idx + 1..];
            let range = match rest.split_once('-') {
                Some((a, b)) if !a.is_empty() && !b.is_empty() => {
                    Some((a.parse().ok()?, b.parse().ok()?))
                }
                _ => {
                    let n: usize = rest.parse().ok()?;
                    Some((n, n))
                }
            };
            (path, range)
        }
        _ => (text, None),
    };
    if path_part.is_empty() || path_part.contains(' ') && !path_part.contains('/') {
        return None;
    }
    Some((path_part.to_string(), range))
}

/// 全页引用解析与链接化：
/// - 已知路径 → `[text](bw-file://enc?L=s-e)`（无行号 → `bw-file://enc`）
/// - 未知路径 → 保留文本、退化为普通文本（去链接形态，防假链）
///
/// 返回 (处理后正文, 引用过的已知路径按出现序)
pub fn resolve_citations(content: &str, known_files: &HashSet<String>) -> (String, Vec<String>) {
    let re = regex::Regex::new(r"\[([^\[\]]+?)\]\(\s*\)").unwrap();
    let mut cited: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let out = re
        .replace_all(content, |caps: &regex::Captures| {
            let inner = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let Some((path, range)) = parse_citation(inner) else {
                return inner.to_string(); // 非引用形态的空链接 → 纯文本
            };
            if !known_files.contains(&path) {
                return inner.to_string();
            }
            if seen.insert(path.clone()) {
                cited.push(path.clone());
            }
            let enc = percent_encode_path(&path);
            let label = match range {
                Some((s, e)) if s != e => format!("{path}:{s}-{e}"),
                Some((s, _)) => format!("{path}:{s}"),
                None => path.clone(),
            };
            match range {
                Some((s, e)) => format!("[{label}](bw-file://{enc}?L={s}-{e})"),
                None => format!("[{label}](bw-file://{enc})"),
            }
        })
        .into_owned();
    (out, cited)
}

/// 构建 `<details>` 引用块（deepwiki-open 首块习惯 + Qoder cite 标题）
pub fn build_cite_block(cited: &[String]) -> String {
    if cited.is_empty() {
        return String::new();
    }
    let mut s = String::from("<details>\n<summary>本文引用的文件</summary>\n\n");
    for p in cited {
        s.push_str(&format!("- [{}]({})\n", p, percent_encode_path_owned(p)));
    }
    s.push_str("</details>\n");
    s
}

fn percent_encode_path_owned(p: &str) -> String {
    format!("bw-file://{}", percent_encode_path(p))
}

// ============ 目录注入 ============

/// GitHub 风格 heading anchor（中文保留、小写、空格→-、去标点）
pub fn heading_anchor(title: &str) -> String {
    let mut s = String::new();
    for c in title.trim().chars() {
        if c.is_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(&c) {
            s.extend(c.to_lowercase());
        } else if (c == ' ' || c == '-' || c == '_') && !s.ends_with('-') {
            s.push('-');
        }
    }
    s.trim_matches('-').to_string()
}

/// 在 H1 之后注入 `## 目录`（LLM 已写目录则跳过；无 H1 或章节过少不注入）
pub fn inject_toc(content: &str) -> String {
    if content.contains("\n## 目录") || content.starts_with("## 目录") {
        return content.to_string();
    }
    let headings: Vec<&str> = content
        .lines()
        .filter(|l| l.starts_with("## "))
        .map(|l| l.trim_start_matches("## ").trim())
        .collect();
    if headings.len() < 3 {
        return content.to_string();
    }
    let mut toc = String::from("\n## 目录\n");
    for (i, h) in headings.iter().enumerate() {
        toc.push_str(&format!("{}. [{}](#{})\n", i + 1, h, heading_anchor(h)));
    }
    // 插入点：H1 行之后；无 H1 则整个前置
    if content.starts_with("# ") {
        match content.find('\n') {
            Some(pos) => {
                let (head, rest) = content.split_at(pos);
                format!("{head}\n{toc}{rest}")
            }
            None => format!("{content}\n{toc}"),
        }
    } else {
        format!("{toc}\n{content}")
    }
}

// ============ 文件名安全化（中文友好） ============

/// Windows 保留名（无扩展名形态）
const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// 标题 → 安全文件名（不含扩展名）：保留中日韩文字与常用符号，
/// 替换路径非法字符；截断到 50 字符；空结果回退 "page"
pub fn safe_name(title: &str) -> String {
    let mut s: String = title
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '(' | ')' | '（' | '）' | '+' => c,
            c if c.is_alphanumeric() => c, // 中文等 unicode 字母数字
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#' | '%' | '&' | '{' | '}' | '$' | '@' | '!' | '`' | '\'' | '[' | ']' | ';' | ',' | '=' => '-',
            _ => '-',
        })
        .collect();
    // 压缩连续 -
    let mut collapsed = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '-' && collapsed.ends_with('-') {
            continue;
        }
        collapsed.push(c);
    }
    s = collapsed.trim_matches('-').trim_matches('.').trim().to_string();
    s.truncate(50);
    let out = s.trim_end_matches('-').trim().to_string();
    if out.is_empty() {
        return "page".into();
    }
    if RESERVED.contains(&out.to_uppercase().as_str()) {
        return format!("_{out}");
    }
    out
}

/// 在已占用名集合内生成唯一名（同名追加 -2/-3…）
pub fn unique_name(base: &str, taken: &mut HashSet<String>) -> String {
    if taken.insert(base.to_string()) {
        return base.to_string();
    }
    for n in 2.. {
        let cand = format!("{base}-{n}");
        if taken.insert(cand.clone()) {
            return cand;
        }
    }
    unreachable!()
}

// ============ 页面组装 ============

/// 整页后处理：剥围栏 → 引用链接化 → 目录注入 → details 引用块紧贴 H1
pub fn postprocess_page(raw: &str, known_files: &HashSet<String>) -> String {
    let content = sanitize_md(raw);
    let (mut out, cited) = resolve_citations(&content, known_files);
    out = inject_toc(&out);
    let cite = build_cite_block(&cited);
    if !cite.is_empty() {
        if out.starts_with("# ") {
            match out.find('\n') {
                Some(pos) => out.insert_str(pos, &format!("\n{cite}")),
                None => out = format!("{out}\n\n{cite}"),
            }
        } else {
            out = format!("{cite}\n{out}");
        }
    }
    out
}

/// 清理 LLM 输出：去掉整篇代码块包裹与首尾空白
pub fn sanitize_md(text: &str) -> String {
    let t = text.trim();
    let t = t
        .strip_prefix("```markdown")
        .or_else(|| t.strip_prefix("```md"))
        .unwrap_or(t);
    let t = t.strip_prefix("```").unwrap_or(t);
    let t = t.strip_suffix("```").unwrap_or(t);
    t.trim().to_string()
}

// ============ 测试 ============

#[cfg(test)]
mod tests {
    use super::*;

    fn known() -> HashSet<String> {
        ["src/main.rs", "docs/指南.md", "app.py"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn citations_resolve_with_ranges() {
        let md = "## 简介\n内容。\n\n**图表来源**\n- [src/main.rs:12-40]()\n- [docs/指南.md:5]()\n- [nope/x.rs:1-2]()\n- [app.py]()";
        let (out, cited) = resolve_citations(md, &known());
        // 路径分隔符 / 保留可读，其余字符编码
        assert!(out.contains("[src/main.rs:12-40](bw-file://src/main.rs?L=12-40)"), "{out}");
        assert!(out.contains("[docs/指南.md:5](bw-file://docs/%E6%8C%87%E5%8D%97.md?L=5-5)"), "{out}");
        assert!(out.contains("[app.py](bw-file://app.py)"));
        // 未知文件退化为纯文本（无链接）
        assert!(out.contains("- nope/x.rs:1-2\n"), "{out}");
        assert_eq!(cited, vec!["src/main.rs", "docs/指南.md", "app.py"]);
    }

    #[test]
    fn percent_encode_roundtrip() {
        let p = "docs/组件库/Button 按钮.vue";
        let enc = percent_encode_path(p);
        assert_eq!(percent_decode_path(&enc), p);
    }

    #[test]
    fn postprocess_injects_cite_and_toc_after_h1() {
        let raw = "```markdown\n# 标题页\n\n## 简介\na\n\n## 核心\nb\n\n## 结论\nc\n\n**章节来源**\n- [app.py:1-9]()\n```";
        let out = postprocess_page(raw, &known());
        assert!(out.starts_with("# 标题页\n"));
        let h1_end = out.find('\n').unwrap();
        let after_h1 = &out[h1_end + 1..];
        assert!(after_h1.starts_with("<details>"), "{out}");
        assert!(out.contains("## 目录"));
        assert!(out.contains("1. [简介](#简介)"));
        assert!(out.contains("[app.py:1-9](bw-file://app.py?L=1-9)"));
    }

    #[test]
    fn safe_name_keeps_chinese_and_replaces_illegal() {
        assert_eq!(safe_name("API接口文档"), "API接口文档");
        assert_eq!(safe_name("a/b:c*d"), "a-b-c-d");
        assert_eq!(safe_name("评分项：提取与合并"), "评分项-提取与合并");
        assert_eq!(safe_name("///"), "page");
        assert_eq!(safe_name("CON"), "_CON");
    }

    #[test]
    fn unique_name_appends_counter() {
        let mut taken = HashSet::new();
        assert_eq!(unique_name("入口", &mut taken), "入口");
        assert_eq!(unique_name("入口", &mut taken), "入口-2");
        assert_eq!(unique_name("入口", &mut taken), "入口-3");
    }

    #[test]
    fn toc_skipped_when_present_or_few_headings() {
        assert_eq!(inject_toc("# a\n\n## 目录\n1. x\n\n## b\nc\n\n## d\ne\n"), "# a\n\n## 目录\n1. x\n\n## b\nc\n\n## d\ne\n");
        let few = "# a\n\n## b\nc\n";
        assert_eq!(inject_toc(few), few);
    }
}
