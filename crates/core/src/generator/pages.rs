//! 逐页生成：页上下文构建 + 强化 prompt
//!
//! 与旧「逐模块」管线的本质差异：LLM 真正读到绑定文件的源码（带行号，
//! 支撑 `Sources: [path:line-line]()` 精确引用），而不是只看文件清单。
//! 大文件做「头部 + 签名行」切片：保住模块结构认知，压住 token。

use std::path::Path;
use std::sync::OnceLock;

use crate::llm::ChatMessage;

/// 小文件整读阈值（行）
const FULL_FILE_LINES: usize = 320;
/// 大文件切片：保留头部行数（模块文档、imports、常量）
const SLICE_HEAD_LINES: usize = 200;
/// 单文件渲染行数硬上限
const MAX_RENDER_LINES: usize = 1000;
/// 行内容截断（字符）
const MAX_LINE_CHARS: usize = 400;
/// 单页源码上下文总预算（字符）。收紧自 120k：实测页面 prompt 曾达 6.7 万
/// 字符（≈2 万+ token），连续多页很快打爆供应商 TPM 限额（429）。
/// deepwiki-rs 同思路（64K token 阈值触发压缩），我们用确定性切片替代
/// LLM 压缩——零额外调用且缓存键稳定。
const PAGE_CONTEXT_CHARS: usize = 72_000;

/// 签名行识别（函数/类型/导出/装饰器等结构行），切片时保留真实行号
fn signature_set() -> &'static regex::RegexSet {
    static SET: OnceLock<regex::RegexSet> = OnceLock::new();
    SET.get_or_init(|| {
        regex::RegexSet::new([
            r"^\s*(pub\s+)?(async\s+)?(unsafe\s+)?(const\s+)?fn\s",
            r"^\s*func\s",
            r"^\s*(local\s+)?function\s",
            r"^\s*(export\s+)?(default\s+)?(abstract\s+)?(async\s+)?(function|class|interface|enum)\s",
            r"^\s*export\s+(const|let|var|type|abstract\s+class)\s",
            r"^\s*(pub\s+)?(struct|enum|trait|type|mod|union|macro_rules)\s",
            r"^\s*(pub\s+)?type\s+\w+\s*(\(|=|\{)",
            r"^\s*(public|private|protected|internal|static|final|abstract|override|virtual)\s+.*((class|interface|enum|struct|void|Task|func|def)|\()\s",
            r"^\s*(def|class)\s",
            r"^\s*interface\s+\w+",
            r"^\s*type\s+\w+\s*=",
            r"^\s*impl\b",
            r"^#\[(derive|cfg|test|tokio|get|post|put|delete|patch|route|controller|service|component)\b",
            r"^@(Component|Injectable|NgModule|Controller|Service|Get|Post|Put|Delete|Patch|Bean|Configuration|RestController)",
            r"^\s*(package|namespace)\s",
        ])
        .expect("签名行正则集合编译失败")
    })
}

/// 判断是否签名/结构行
fn is_signature_line(line: &str) -> bool {
    signature_set().is_match(line)
}

/// 源码行渲染：`  12| code`
fn render_line(no: usize, line: &str, out: &mut String) {
    let l: String = line.chars().take(MAX_LINE_CHARS).collect();
    out.push_str(&format!("{no:>5}| {l}\n"));
}

/// 渲染单个文件（带行号）。小文件整读；大文件 = 头部 + 省略标记 + 全部签名行。
/// 返回 (渲染文本, 是否被切片)。
pub fn render_file(path: &Path) -> (String, bool) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return (format!("（读取失败：{}）\n", path.display()), false);
    };
    let lines: Vec<&str> = content.lines().collect();
    let mut out = String::new();
    if lines.len() <= FULL_FILE_LINES {
        for (i, l) in lines.iter().take(MAX_RENDER_LINES).enumerate() {
            render_line(i + 1, l, &mut out);
        }
        if lines.len() > MAX_RENDER_LINES {
            out.push_str(&format!("…（共 {} 行，仅展示前 {} 行）\n", lines.len(), MAX_RENDER_LINES));
            return (out, true);
        }
        (out, false)
    } else {
        for (i, l) in lines.iter().take(SLICE_HEAD_LINES).enumerate() {
            render_line(i + 1, l, &mut out);
        }
        out.push_str(&format!("……（省略 {}-{} 行，保留下方全部结构签名行）……\n",
            SLICE_HEAD_LINES + 1, lines.len()));
        let mut kept = 0;
        for (i, l) in lines.iter().enumerate().skip(SLICE_HEAD_LINES) {
            if kept >= MAX_RENDER_LINES - SLICE_HEAD_LINES {
                break;
            }
            if is_signature_line(l) {
                render_line(i + 1, l, &mut out);
                kept += 1;
            }
        }
        (out, true)
    }
}

/// 页上下文：绑定文件按序渲染，超总预算的文件降级为「仅列路径与行数」
pub fn build_file_context(repo_root: &Path, files: &[String]) -> (String, usize, usize) {
    let mut ctx = String::new();
    let mut full = 0usize;
    let mut skipped = 0usize;
    for path in files {
        let abs = repo_root.join(path);
        let (rendered, sliced) = render_file(&abs);
        if ctx.len() + rendered.len() > PAGE_CONTEXT_CHARS {
            let line_count = std::fs::read_to_string(&abs).map(|c| c.lines().count()).unwrap_or(0);
            ctx.push_str(&format!("### {path}（{} 行，因上下文预算未载入源码）\n\n", line_count));
            skipped += 1;
            continue;
        }
        if sliced {
            ctx.push_str(&format!("### {path}（大文件已切片：头部 + 结构签名行）\n```\n{rendered}```\n\n"));
        } else {
            ctx.push_str(&format!("### {path}\n```\n{rendered}```\n\n"));
        }
        full += 1;
    }
    (ctx, full, skipped)
}

// ============ Prompt ============

/// 页面写作规范（system）：Qoder 固定骨架 + deepwiki-open 引用纪律 + 反脑补
pub const PAGE_SYSTEM_PROMPT: &str = r#"你是资深技术作家与软件架构师，为代码仓库撰写高质量中文技术 Wiki 页面。

【铁律】
- 只依据给定源文件内容写作，不臆造任何文件、函数、接口或行为；不确定的不写。
- 直接以 H1 标题 `# 页面标题` 开头输出 Markdown 正文，不要任何前置说明、寒暄或代码块包裹。

【页面骨架（固定章节，顺序输出）】
1. `## 简介` —— 1-2 段：本页主题的用途、范围与全仓定位。
2. `## 项目结构` —— 主题相关的代码组织方式（如适用）。
3. `## 核心组件` —— 要点列表：关键模块/函数/数据结构及其职责。
4. `## 架构总览` —— 主题的整体结构与数据流，配 Mermaid 图。
5. `## 详细分析` —— 用 H3（`###`）小节展开核心机制，至少 3 个小节，每节深入到函数级行为。
6. `## 依赖关系分析` —— 本主题依赖与被依赖的模块（如适用），配 Mermaid 图。
7. `## 故障排查指南` —— 常见问题与定位手段（如适用）。
8. `## 结论` —— 简短总结。
不适用的章节可省略，但 1/3/4/5/8 必须存在。

【Mermaid 图（强制，每页至少 2 张）】
- 结构图用 `graph TB`（可用 subgraph 分层）；调用/交互流程用 `sequenceDiagram`（participant 定义齐全）；逻辑分支用 `flowchart TD`；依赖用 `graph LR`。
- 流程图禁止 LR 方向；节点文字 3-5 个词；每张图后必须紧跟来源标注。

【引用纪律（最重要）】
- 每个有文件依据的章节末尾加 `**章节来源**` 列表；每张 Mermaid 图后加 `**图表来源**` 列表。
- 引用格式严格为：`- [完整相对路径:起始行-结束行]()`（单行可省区间；括号必须留空，后续处理会转为可点击链接）。行号必须来自源文件档案中的行号标注，不得编造。
- 全页至少引用 N 个不同源文件（模板会给出具体数值）。
- 纯总结性、无文件依据的章节，末尾标注一行：`[本节为总结性内容，不直接分析具体文件]`。

【质量要求】
- 深入代码行为：写清关键函数的输入输出、分支、错误处理与副作用，不停留在目录描述。
- 适度使用 Markdown 表格总结参数/配置/数据结构。
- 中文技术写作，术语准确，段落紧凑。"#;

/// 组装单页生成的 user prompt
#[allow(clippy::too_many_arguments)]
pub fn build_page_user_prompt(
    page_title: &str,
    brief: &str,
    description: &str,
    related: &[String],           // 相关页 "标题（定位）"
    module_ctx: &str,             // 相关模块统计与依赖
    file_ctx: &str,               // 带行号的源文件档案
    rag_ctx: &str,                // 向量检索补充（可为空）
    min_files: usize,
) -> String {
    let mut s = String::from("【Wiki 页面撰写】请为以下页面撰写完整 Wiki 页面。\n\n");
    s.push_str(&format!("# 页面定位\n- 标题：{page_title}\n"));
    if !description.is_empty() {
        s.push_str(&format!("- 大纲定位：{description}\n"));
    }
    if !brief.is_empty() {
        s.push_str(&format!("- 生成指令（大纲阶段给定）：{brief}\n"));
    }
    if !related.is_empty() {
        s.push_str(&format!("- 相关页面（可在行文中以「另见《标题》」提及）：{}\n", related.join("；")));
    }
    if !module_ctx.is_empty() {
        s.push_str(&format!("\n# 模块上下文（依赖方向：A 依赖 B）\n{module_ctx}\n"));
    }
    s.push_str(&format!(
        "\n# 源文件档案（行号用于引用，格式 [路径:起始行-结束行]()）\n{file_ctx}\n"
    ));
    if !rag_ctx.is_empty() {
        s.push_str(&format!("\n# 补充检索（仓库向量索引命中，仅供参考，引用仍以源文件档案为准）\n{rag_ctx}\n"));
    }
    s.push_str(&format!(
        "\n# 输出要求\n按系统规范输出：`# {page_title}` 开头，固定骨架，≥2 张 Mermaid，至少引用 {min_files} 个不同文件，引用列表用章节来源/图表来源标注。"
    ));
    s
}

/// 页面消息对
#[allow(clippy::too_many_arguments)]
pub fn page_messages(
    page_title: &str,
    brief: &str,
    description: &str,
    related: &[String],
    module_ctx: &str,
    file_ctx: &str,
    rag_ctx: &str,
    min_files: usize,
) -> Vec<ChatMessage> {
    vec![
        ChatMessage::system(PAGE_SYSTEM_PROMPT),
        ChatMessage::user(build_page_user_prompt(
            page_title, brief, description, related, module_ctx, file_ctx, rag_ctx, min_files,
        )),
    ]
}

/// 总览页 prompt（生成 wiki 的入口 overview.md；页面目录带真实相对链接，
/// 前端拦截 .md 链接实现文档间跳转）
#[allow(clippy::too_many_arguments)]
pub fn overview_messages(
    repo_name: &str,
    outline_title: &str,
    outline_desc: &str,
    catalog: &str, // "章节路径" -> 页面 [标题](相对链接) 清单
    readme_excerpt: &str,
    module_ctx: &str,
    dependency_mmd: &str,
) -> Vec<ChatMessage> {
    let system = "你是资深技术作家，为代码仓库撰写 Wiki 总览入口页。\
只输出中文 Markdown 正文（`# 标题` 开头），不臆造，不做前置说明。\
输出章节：项目概览 / 如何阅读本 Wiki（按给定目录逐章介绍，保留给定链接原样）\
/ 架构速览（基于给定依赖图改写为 Mermaid graph TB，可用 subgraph 分层）/ 技术栈 / 建议阅读路径。";
    let user = format!(
        "【Wiki 总览撰写】仓库：{repo_name}\n\n大纲标题：{outline_title}\n大纲描述：{outline_desc}\n\n\
<页面目录（链接必须原样保留）>\n{catalog}</页面目录>\n\n\
<README 摘录>\n{readme_excerpt}</README 摘录>\n\n\
<模块统计（A 依赖 B）>\n{module_ctx}</模块统计>\n\n\
<模块依赖图（Mermaid 源码，可整理进正文）>\n{dependency_mmd}</模块依赖图>"
    );
    vec![ChatMessage::system(system), ChatMessage::user(user)]
}

// ============ 测试 ============

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_small_file_with_line_numbers() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.rs");
        std::fs::write(&f, "fn main() {\n    println!(\"hi\");\n}\n").unwrap();
        let (out, sliced) = render_file(&f);
        assert!(!sliced);
        assert!(out.contains("    1| fn main() {"));
        assert!(out.contains("    2|"));
    }

    #[test]
    fn big_file_sliced_with_signature_lines() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("big.py");
        let mut body = String::from("# head\n");
        for i in 0..600 {
            body.push_str(&format!("filler_{i} = {i}\n"));
        }
        body.push_str("def core_handler():\n    pass\n");
        std::fs::write(&f, body).unwrap();
        let (out, sliced) = render_file(&f);
        assert!(sliced);
        assert!(out.contains("省略"));
        // 签名行保留且带真实行号（1 头 + 600 填充 → def 在第 602 行）
        assert!(out.contains("  602| def core_handler():"), "实际：\n{out}");
    }

    #[test]
    fn signature_regex_hits_common_langs() {
        for line in [
            "pub async fn run(&self) -> Result<()> {",
            "fn main() {}",
            "impl AppService {",
            "pub struct Foo;",
            "def handler(self):",
            "class Bar:",
            "export function useThing() {",
            "export const store = defineStore(",
            "interface Node {",
            "func Serve() {",
            "    private void Handle(object s) {",
        ] {
            assert!(is_signature_line(line), "未识别签名行：{line}");
        }
        assert!(!is_signature_line("    let x = 1;"));
        assert!(!is_signature_line("console.log('nope')"));
    }

    #[test]
    fn file_context_budget_degrades_gracefully() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for i in 0..3 {
            std::fs::write(root.join(format!("f{i}.txt")), "x\n".repeat(200)).unwrap();
        }
        let (ctx, full, _skipped) = build_file_context(root, &["f0.txt".into(), "f1.txt".into(), "f2.txt".into()]);
        assert_eq!(full, 3);
        assert!(ctx.contains("### f0.txt"));
        assert!(ctx.contains("    1| x"));
    }
}
