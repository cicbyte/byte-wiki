//! 多语言调用链提取：tree-sitter 解析 → 函数级调用图 JSON。
//!
//! 产品定位（dev-docs v2）：Wiki → 代码的桥，byte-wiki 核心能力。
//! 本 crate 是纯解析引擎（无 DB/无配置），core 无条件依赖它；
//! `byte-callchain` bin 服务无 app 的流水线场景（HTML 三件套导出）。
//!
//! 解析策略（尽力而为，tree-sitter 无符号表，动态分派只能启发式）：
//!   1) local    —— 裸调用：同文件内同名定义（较可信）
//!   2) heuristic—— 被调名在全局唯一（方法调用 `obj.m()` 按方法名唯一性归约）
//!   3) 其余丢弃 —— 内建/标准库/跨文件多义名，不产生噪声边
//! 方法调用（recv 存在）一律只走 heuristic，不进 local——避免虚标精确性
//! （渲染层 local 是实线 = 较可信）。
//!
//! `source` 内嵌由 [`ExtractOptions::embed_source`] 控制：standalone 导出开启
//! （自包含），app 管线关闭（详情面板 Monaco 读实时文件，产物瘦身便于随仓库共享）。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{json, Value};
use tree_sitter::{Node, Parser};
use tree_sitter_language::LanguageFn;

// ----------------------------------------------------------------------------
// 语言配置
// ----------------------------------------------------------------------------

struct LangConf {
    name: &'static str,
    /// 扩展名映射与 core `lang_of` 对齐（py+pyi / ts+mts+cts / js+jsx+mjs+cjs）
    exts: &'static [&'static str],
    lang: LanguageFn,
    /// 函数/方法定义节点类型（均有 name 字段；匿名箭头函数等自动跳过）
    func_kinds: &'static [&'static str],
    /// 调用表达式节点类型
    call_kinds: &'static [&'static str],
    /// 类/接口等容器节点类型（名字入栈 → 成员 qualname 携带 `Class.method`）；
    /// go 用接收者、rust 用 impl/trait 块特判，不在此列
    class_kinds: &'static [&'static str],
}

fn lang_confs() -> Vec<LangConf> {
    vec![
        LangConf {
            name: "python",
            exts: &["py", "pyi"],
            lang: tree_sitter_python::LANGUAGE,
            func_kinds: &["function_definition"],
            call_kinds: &["call"],
            class_kinds: &["class_definition"],
        },
        LangConf {
            name: "javascript",
            exts: &["js", "jsx", "mjs", "cjs"],
            lang: tree_sitter_javascript::LANGUAGE,
            func_kinds: &["function_declaration", "generator_function_declaration", "method_definition"],
            call_kinds: &["call_expression"],
            class_kinds: &["class_declaration"],
        },
        LangConf {
            name: "typescript",
            exts: &["ts", "mts", "cts"],
            lang: tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
            func_kinds: &["function_declaration", "generator_function_declaration", "method_definition"],
            call_kinds: &["call_expression"],
            class_kinds: &["class_declaration", "abstract_class_declaration"],
        },
        LangConf {
            // tsx 需要 TSX grammar（泛型箭头 `<T>(x)=>` 在纯 TS grammar 下会误解析）
            name: "typescript",
            exts: &["tsx"],
            lang: tree_sitter_typescript::LANGUAGE_TSX,
            func_kinds: &["function_declaration", "generator_function_declaration", "method_definition"],
            call_kinds: &["call_expression"],
            class_kinds: &["class_declaration", "abstract_class_declaration"],
        },
        LangConf {
            name: "go",
            exts: &["go"],
            lang: tree_sitter_go::LANGUAGE,
            func_kinds: &["function_declaration", "method_declaration"],
            call_kinds: &["call_expression"],
            class_kinds: &[],
        },
        LangConf {
            name: "java",
            exts: &["java"],
            lang: tree_sitter_java::LANGUAGE,
            func_kinds: &["method_declaration", "constructor_declaration"],
            call_kinds: &["method_invocation"],
            class_kinds: &["class_declaration", "interface_declaration", "enum_declaration", "record_declaration"],
        },
        LangConf {
            name: "rust",
            exts: &["rs"],
            lang: tree_sitter_rust::LANGUAGE,
            func_kinds: &["function_item"],
            call_kinds: &["call_expression"],
            class_kinds: &[],
        },
    ]
}

/// 内置排除目录（叠加在调用方注入的 excludes 之上）。
/// 必须含 `.bytewiki` 与 `callchain-out`：否则第二次运行会解析自己产出的
/// callchain_app.js / 既往产物，自己污染自己。
const SKIP_DIRS: &[&str] = &[
    "node_modules", ".git", "target", "dist", "build", "__pycache__", ".venv", "venv",
    ".idea", ".vscode", ".bytewiki", "callchain-out", "vendor", ".next", "coverage",
];

/// 单文件最大解析体积（对齐 core indexer 的 MAX_PARSE_SIZE，超过则跳过）
const MAX_PARSE_SIZE: u64 = 1024 * 1024;

// ----------------------------------------------------------------------------
// 提取
// ----------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct FuncRec {
    qualname: String,
    name: String,
    is_async: bool,
    start_row: usize, // 0-based
    end_row: usize,
    routes: Vec<String>,
    calls: Vec<(String, Option<String>, usize)>, // (被调名, 接收者文本, 1-based 行号)
}

#[derive(Debug, Clone)]
struct FileRec {
    module: String,
    file: String,
    lang: &'static str,
    funcs: Vec<FuncRec>,
    source_lines: Vec<String>,
}

fn text_of<'a>(node: Node, src: &'a str) -> &'a str {
    node.utf8_text(src.as_bytes()).unwrap_or("")
}

/// 取定义节点的函数名（各语言定义节点均有 name 字段，此函数是通用的）
fn def_name(node: Node, src: &str) -> Option<String> {
    node.child_by_field_name("name")
        .map(|n| text_of(n, src).to_string())
        .filter(|s| !s.is_empty())
}

/// 从调用表达式提取（被调名, 接收者）。按被调节点的结构分派：
/// identifier → 直接名；attribute/member/selector/field → 取成员名，接收者记原文供人工核对。
fn callee_of(node: Node, src: &str) -> Option<(String, Option<String>)> {
    let f = node
        .child_by_field_name("function")
        .or_else(|| node.child_by_field_name("name")) // java method_invocation
        ?;
    let recv_text = |n: Node| -> Option<String> {
        n.child_by_field_name("object")
            // rust field_expression 的接收者字段是 value；go selector 是 operand
            .or_else(|| n.child_by_field_name("operand"))
            .or_else(|| n.child_by_field_name("value"))
            .map(|o| text_of(o, src).to_string())
    };
    match f.kind() {
        "identifier" => Some((text_of(f, src).to_string(), None)),
        // python: obj.attr(...)
        "attribute" => f
            .child_by_field_name("attribute")
            .map(|a| (text_of(a, src).to_string(), Some(text_of(f, src).to_string()))),
        // js/ts: obj.member(...) / obj[...](...)
        "member_expression" => f
            .child_by_field_name("property")
            .filter(|p| p.kind() == "identifier" || p.kind() == "property_identifier")
            .map(|p| (text_of(p, src).to_string(), Some(text_of(f, src).to_string()))),
        // go: pkg.Fn(...) / obj.Method(...)
        "selector_expression" => f
            .child_by_field_name("field")
            .map(|p| (text_of(p, src).to_string(), recv_text(f))),
        // rust: obj.method(...)
        "field_expression" => f
            .child_by_field_name("field")
            .map(|p| (text_of(p, src).to_string(), recv_text(f))),
        // go/js 内层调用链 f()()、rust 路径调用等：取末尾标识符兜底
        _ => {
            let t = text_of(f, src);
            t.rsplit(['.', ':'])
                .next()
                .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_'))
                .map(|s| (s.to_string(), Some(t.to_string())))
        }
    }
}

/// python 装饰器中的 Flask/FastAPI 路由识别（宽松：命中常见动词即算）
fn routes_from_deco(deco: &str) -> Vec<String> {
    const VERBS: [&str; 6] = [".route(", ".get(", ".post(", ".put(", ".delete(", ".patch("];
    if VERBS.iter().any(|v| deco.contains(v)) {
        let first = deco.lines().next().unwrap_or(deco).trim();
        let label: String = first.chars().take(48).collect();
        vec![label.to_string()]
    } else {
        vec![]
    }
}

/// 装饰器/async 的识别是语法层探测（各 grammar 表示不一）：
/// - python 0.23：`async` 是 function_definition 的无名关键字子节点；
///   decorated_definition 无 decoration 字段，装饰器是 `decorator` 类型子节点
/// - rust：async 藏在 modifiers 子节点里
fn has_async_keyword(node: Node, src: &str) -> bool {
    let mut cur = node.walk();
    let found = node.children(&mut cur).any(|c| {
        (!c.is_named() && text_of(c, src) == "async")
            || (c.kind() == "modifiers" && text_of(c, src).contains("async"))
    });
    drop(cur);
    found
}

/// 递归访问：stack 维护限定名（函数嵌套 + 类/impl 容器）；decorations 携带 python
/// 装饰器文本（decorated_definition → 内层定义），进入函数体后清空不再下传
fn visit(
    node: Node,
    src: &str,
    conf: &LangConf,
    stack: &mut Vec<String>,
    funcs: &mut Vec<FuncRec>,
    rec_idx: Option<usize>,
    decorations: &[String],
) {
    let kind = node.kind();

    if conf.func_kinds.contains(&kind) {
        if let Some(name) = def_name(node, src) {
            let routes: Vec<String> = decorations.iter().flat_map(|d| routes_from_deco(d)).collect();
            // go 方法：接收者类型作容器前缀（`func (s *Svc) Transform` → Svc.Transform）
            let recv_type = node
                .child_by_field_name("receiver")
                .map(|r| text_of(r, src))
                .map(|t| {
                    t.split_whitespace()
                        .last()
                        .unwrap_or(t)
                        .trim_matches(|c: char| c == '*' || c == '&' || c == '(' || c == ')')
                        .split('<')
                        .next()
                        .unwrap_or("")
                        .to_string()
                })
                .filter(|s| !s.is_empty());
            let mut parts: Vec<String> = stack.clone();
            if let Some(rt) = recv_type {
                parts.push(rt);
            }
            parts.push(name.clone());
            let rec = FuncRec {
                qualname: parts.join("."),
                name: name.clone(),
                is_async: has_async_keyword(node, src),
                start_row: node.start_position().row,
                end_row: node.end_position().row,
                routes,
                calls: Vec::new(),
            };
            let idx = funcs.len();
            funcs.push(rec);
            stack.push(name);
            let mut cur = node.walk();
            for child in node.children(&mut cur) {
                visit(child, src, conf, stack, funcs, Some(idx), &[]);
            }
            stack.pop();
            return;
        }
    }

    // python: decorated_definition（装饰器是 decorator 子节点，路由信息下传给内层定义）
    if kind == "decorated_definition" {
        let decos: Vec<String> = {
            let mut cur = node.walk();
            node.children(&mut cur)
                .filter(|c| c.kind() == "decorator")
                .map(|c| text_of(c, src).to_string())
                .collect()
        };
        let mut cur = node.walk();
        for child in node.children(&mut cur) {
            visit(child, src, conf, stack, funcs, rec_idx, &decos);
        }
        return;
    }

    // 类/接口容器：名字入栈，成员 qualname 携带容器（Class.method）
    if conf.class_kinds.contains(&kind) {
        if let Some(cname) = def_name(node, src) {
            stack.push(cname);
            let mut cur = node.walk();
            for child in node.children(&mut cur) {
                visit(child, src, conf, stack, funcs, rec_idx, &[]);
            }
            stack.pop();
            return;
        }
    }

    // rust: impl 块（`impl Svc`/`impl Trait for Svc` → Svc）与 trait 块同理
    if kind == "impl_item" {
        let tname = node
            .child_by_field_name("type")
            .map(|t| text_of(t, src).split('<').next().unwrap_or("").trim().to_string())
            .filter(|s| !s.is_empty());
        if let Some(t) = tname {
            stack.push(t);
            let mut cur = node.walk();
            for child in node.children(&mut cur) {
                visit(child, src, conf, stack, funcs, rec_idx, &[]);
            }
            stack.pop();
            return;
        }
    } else if kind == "trait_item" {
        if let Some(tname) = def_name(node, src) {
            stack.push(tname);
            let mut cur = node.walk();
            for child in node.children(&mut cur) {
                visit(child, src, conf, stack, funcs, rec_idx, &[]);
            }
            stack.pop();
            return;
        }
    }

    if conf.call_kinds.contains(&kind) {
        if let (Some((callee, recv)), Some(idx)) = (callee_of(node, src), rec_idx) {
            funcs[idx].calls.push((callee, recv, node.start_position().row + 1));
        }
    }

    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        visit(child, src, conf, stack, funcs, rec_idx, decorations);
    }
}

fn extract_file(path: &Path, root: &Path, conf: &LangConf) -> Option<FileRec> {
    if std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) > MAX_PARSE_SIZE {
        return None;
    }
    let src = std::fs::read_to_string(path).ok()?;
    let mut parser = Parser::new();
    parser.set_language(&conf.lang.into()).ok()?;
    let tree = parser.parse(&src, None)?;
    let module = path
        .strip_prefix(root)
        .ok()?
        .with_extension("")
        .to_string_lossy()
        .replace('\\', "/");
    let mut funcs = Vec::new();
    let mut stack = Vec::new();
    visit(
        tree.root_node(),
        &src,
        conf,
        &mut stack,
        &mut funcs,
        None,
        &[],
    );
    // 同文件重复 qualname：后者覆盖前者（对齐 Python 版 dict 语义），保持首次出现位置
    let mut seen_at: HashMap<String, usize> = HashMap::new();
    let mut deduped: Vec<FuncRec> = Vec::new();
    for f in funcs {
        match seen_at.get(&f.qualname) {
            Some(&i) => deduped[i] = f,
            None => {
                seen_at.insert(f.qualname.clone(), deduped.len());
                deduped.push(f);
            }
        }
    }
    Some(FileRec {
        module,
        file: path.strip_prefix(root).ok()?.to_string_lossy().replace('\\', "/"),
        lang: conf.name,
        funcs: deduped,
        source_lines: src.lines().map(|l| l.to_string()).collect(),
    })
}

// ----------------------------------------------------------------------------
// 建图（与 Python 版同 schema 的 JSON；source 为可选字段）
// ----------------------------------------------------------------------------

/// 提取选项。`excludes` 由调用方注入（bs/桌面端合并 core EXTRA_EXCLUDES + 用户偏好；
/// standalone 传空走内置表）。
pub struct ExtractOptions {
    /// 额外入口的 qualname 正则（路由装饰器自动识别之外）
    pub entry_regex: Option<regex::Regex>,
    /// 额外排除目录名（叠加内置 SKIP_DIRS）
    pub excludes: Vec<String>,
    /// 节点是否内嵌函数源码（standalone 导出 true；app 管线 false）
    pub embed_source: bool,
}

impl Default for ExtractOptions {
    fn default() -> Self {
        Self {
            entry_regex: None,
            excludes: Vec::new(),
            embed_source: true,
        }
    }
}

/// 提取调用图。`on_progress` 在遍历解析阶段回调（已处理文件数，每 20 个一次 + 收尾一次），
/// 对齐 core 的 `&(dyn Fn(usize) + Send + Sync)` 惯例。
pub fn extract(
    root: &Path,
    opts: &ExtractOptions,
    on_progress: &(dyn Fn(usize) + Send + Sync),
) -> Result<Value> {
    let confs = lang_confs();
    let excludes: HashSet<String> = SKIP_DIRS
        .iter()
        .map(|s| s.to_string())
        .chain(opts.excludes.iter().cloned())
        .collect();

    // 遍历：ignore crate（遵循 .gitignore）+ 排除目录注入
    let walker = ignore::WalkBuilder::new(root)
        .hidden(false)
        .filter_entry(move |e| {
            e.depth() == 0
                || e.file_name()
                    .to_str()
                    .map(|n| !excludes.contains(n))
                    .unwrap_or(true)
        })
        .build();
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in walker.flatten() {
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let ext = entry.path().extension().and_then(|e| e.to_str()).unwrap_or("");
        if confs.iter().any(|c| c.exts.contains(&ext)) {
            paths.push(entry.path().to_path_buf());
        }
    }
    // 排序保证产物确定性（重复运行 JSON 稳定，便于 diff/基线对照）
    paths.sort();

    let mut files: Vec<FileRec> = Vec::new();
    for (i, path) in paths.iter().enumerate() {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let Some(conf) = confs.iter().find(|c| c.exts.contains(&ext)) else {
            continue;
        };
        if let Some(rec) = extract_file(path, root, conf) {
            files.push(rec);
        }
        if (i + 1) % 20 == 0 {
            on_progress(i + 1);
        }
    }
    on_progress(paths.len());

    // 全局与模块内名字索引
    let mut global_idx: HashMap<&str, Vec<(usize, usize)>> = HashMap::new(); // name -> [(file_i, func_i)]
    let mut local_idx: HashMap<(usize, &str), Vec<usize>> = HashMap::new();
    for (fi, f) in files.iter().enumerate() {
        for (gi, g) in f.funcs.iter().enumerate() {
            global_idx.entry(g.name.as_str()).or_default().push((fi, gi));
            local_idx
                .entry((fi, g.name.as_str()))
                .or_default()
                .push(gi);
        }
    }

    let mut nodes = Vec::new();
    let mut edges: Vec<Value> = Vec::new();
    let mut seen: HashSet<(String, String, usize)> = HashSet::new();

    for (fi, f) in files.iter().enumerate() {
        for g in f.funcs.iter() {
            let id = format!("{}::{}", f.module, g.qualname);
            let matched = opts
                .entry_regex
                .as_ref()
                .map(|re| re.is_match(&g.qualname) || re.is_match(&id))
                .unwrap_or(false);
            let is_entry = !g.routes.is_empty() || matched;
            let mut node = json!({
                "id": id,
                "module": f.module,
                "qualname": g.qualname,
                "name": g.name,
                "file": f.file,
                "lineno": g.start_row + 1,
                "end_lineno": g.end_row + 1,
                "routes": g.routes,
                "async": g.is_async,
                "is_entry": is_entry,
            });
            if opts.embed_source {
                let source: Vec<&str> = f
                    .source_lines
                    .iter()
                    .skip(g.start_row)
                    .take(g.end_row - g.start_row + 1)
                    .map(|s| s.as_str())
                    .collect();
                node["source"] = json!(source.join("\n"));
            }
            nodes.push(node);

            let mut dedup_call: HashSet<(&str, Option<&str>)> = HashSet::new();
            for (callee, recv, line) in &g.calls {
                if !dedup_call.insert((callee.as_str(), recv.as_deref())) {
                    continue;
                }
                let mut targets: Vec<(String, &str)> = Vec::new();
                if recv.is_none() {
                    // 裸调用：local（同文件同名）→ heuristic（全局唯一）→ 丢弃
                    if let Some(gis) = local_idx.get(&(fi, callee.as_str())) {
                        for t in gis {
                            targets.push((format!("{}::{}", f.module, f.funcs[*t].qualname), "local"));
                        }
                    } else if let Some(cands) = global_idx.get(callee.as_str()) {
                        if cands.len() == 1 {
                            let (cfi, cgi) = cands[0];
                            targets.push((
                                format!("{}::{}", files[cfi].module, files[cfi].funcs[cgi].qualname),
                                "heuristic",
                            ));
                        }
                    }
                } else {
                    // 方法调用 obj.m()：仅 heuristic（方法名全局唯一），不得进 local——
                    // 同文件存在同名自由函数时连 local 边是虚标精确性
                    if let Some(cands) = global_idx.get(callee.as_str()) {
                        if cands.len() == 1 {
                            let (cfi, cgi) = cands[0];
                            targets.push((
                                format!("{}::{}", files[cfi].module, files[cfi].funcs[cgi].qualname),
                                "heuristic",
                            ));
                        }
                    }
                }
                for (dst, kind) in targets {
                    let key = (id.clone(), dst.clone(), *line);
                    if seen.insert(key) {
                        edges.push(json!({"src": id, "dst": dst, "line": line, "kind": kind}));
                    }
                }
            }
        }
    }

    // 入度/出度
    let mut indeg: HashMap<String, usize> = HashMap::new();
    let mut outdeg: HashMap<String, usize> = HashMap::new();
    for e in &edges {
        *indeg.entry(e["dst"].as_str().unwrap_or("").to_string()).or_default() += 1;
        *outdeg.entry(e["src"].as_str().unwrap_or("").to_string()).or_default() += 1;
    }
    let nodes: Vec<Value> = nodes
        .into_iter()
        .map(|mut n| {
            let id = n["id"].as_str().unwrap_or("").to_string();
            n["callers"] = json!(indeg.get(&id).copied().unwrap_or(0));
            n["callees"] = json!(outdeg.get(&id).copied().unwrap_or(0));
            n
        })
        .collect();

    let modules: serde_json::Map<String, Value> = files
        .iter()
        .map(|f| {
            (
                f.module.clone(),
                json!({"file": f.file, "funcs": f.funcs.len()}),
            )
        })
        .collect();

    // 语言统计（文件数）
    let mut lang_files: HashMap<&str, usize> = HashMap::new();
    for f in &files {
        *lang_files.entry(f.lang).or_insert(0) += 1;
    }

    Ok(json!({
        "root": root.to_string_lossy(),
        "nodes": nodes,
        "edges": edges,
        "modules": Value::Object(modules),
        "langs": lang_files,
    }))
}

// ----------------------------------------------------------------------------
// 渲染三件套（standalone bin 导出用；app 管线不走这里——产物只有 JSON）
// ----------------------------------------------------------------------------

const HTML_TPL: &str = include_str!("../assets/callchain.html");
const APP_JS: &str = include_str!("../assets/callchain_app.js");

pub fn render(graph: &Value, title: &str, out_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(out_dir)?;
    let payload = serde_json::to_string(graph)?;
    let html = HTML_TPL.replace("__TITLE__", title);
    std::fs::write(out_dir.join("callchain.html"), html)?;
    std::fs::write(out_dir.join("callchain_app.js"), format!("{APP_JS}\n"))?;
    std::fs::write(
        out_dir.join("callchain_data.js"),
        format!("const DATA = {payload};\n"),
    )?;
    Ok(())
}

// ----------------------------------------------------------------------------
// 测试：三处正确性修正 + 五语言小样本（tempdir 写样例，不依赖真机仓库）
// ----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp_repo(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        for (rel, content) in files {
            let p = root.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, content).unwrap();
        }
        (dir, root)
    }

    fn extract_default(root: &Path) -> Value {
        extract(root, &ExtractOptions::default(), &|_| {}).unwrap()
    }

    fn find_node<'a>(g: &'a Value, qual: &str) -> &'a Value {
        g["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["qualname"] == qual)
            .unwrap_or_else(|| panic!("节点不存在: {qual}"))
    }

    fn has_edge(g: &Value, src_qual: &str, dst_qual: &str, kind: &str) -> bool {
        g["edges"].as_array().unwrap().iter().any(|e| {
            e["src"].as_str().unwrap_or("").ends_with(&format!("::{src_qual}"))
                && e["dst"].as_str().unwrap_or("").ends_with(&format!("::{dst_qual}"))
                && e["kind"] == kind
        })
    }

    #[test]
    fn python_async_route_and_local_edges() {
        let (_d, root) = tmp_repo(&[(
            "sample.py",
            r#"
@bp.route("/task/create", methods=["POST"])
async def create_task(req):
    body = await parse(req)
    return wrap(body)

def parse(x):
    return x

def wrap(z):
    return z
"#,
        )]);
        let g = extract_default(&root);
        // 修正 1：async def 必须成节点且 async=true（漏掉则 FastAPI 入口全丢）
        let n = find_node(&g, "create_task");
        assert_eq!(n["async"], true);
        assert!(n["is_entry"] == true, "路由装饰器应标记入口");
        assert!(!n["routes"].as_array().unwrap().is_empty());
        // 裸调用 → local
        assert!(has_edge(&g, "create_task", "parse", "local"));
        assert!(has_edge(&g, "create_task", "wrap", "local"));
    }

    #[test]
    fn python_method_call_is_heuristic_not_local() {
        let (_d, root) = tmp_repo(&[(
            "svc.py",
            r#"
def process(obj):
    return obj.transform()

class Impl:
    def transform(self):
        return 1
"#,
        )]);
        let g = extract_default(&root);
        // 修正 2：recv 存在 → 仅 heuristic，即使同文件有同名定义也不得标 local
        assert!(has_edge(&g, "process", "Impl.transform", "heuristic"));
        assert!(!has_edge(&g, "process", "Impl.transform", "local"));
    }

    #[test]
    fn python_duplicate_qualname_later_wins() {
        let (_d, root) = tmp_repo(&[(
            "dup.py",
            "def f():\n    return 1\n\ndef f():\n    return 2\n",
        )]);
        let g = extract_default(&root);
        // 修正 3：同文件重复 qualname 只留一个节点（后者覆盖），不产生重复 id
        let count = g["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|n| n["qualname"] == "f")
            .count();
        assert_eq!(count, 1);
        assert_eq!(find_node(&g, "f")["source"], "def f():\n    return 2");
    }

    #[test]
    fn js_method_and_member_calls() {
        let (_d, root) = tmp_repo(&[(
            "repo.js",
            r#"
class Repo {
  load(id) {
    return this.fetchOne(id);
  }
  fetchOne(id) {
    return db.query(id);
  }
}

function main() {
  const r = new Repo();
  return r.load(1);
}
"#,
        )]);
        let g = extract_default(&root);
        assert!(has_edge(&g, "main", "Repo.load", "heuristic"), "r.load() 方法名全局唯一 → heuristic");
        assert!(has_edge(&g, "Repo.load", "Repo.fetchOne", "heuristic"));
        // db.query 的 query 无定义 → 丢弃，不产生噪声边
        let q_edges = g["edges"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["dst"].as_str().unwrap_or("").contains("query"))
            .count();
        assert_eq!(q_edges, 0);
    }

    #[test]
    fn go_selector_call() {
        let (_d, root) = tmp_repo(&[(
            "main.go",
            r#"
package main

func process(s *Svc) int {
	return s.Transform()
}

func (s *Svc) Transform() int {
	return helper(1)
}

func helper(x int) int {
	return x
}
"#,
        )]);
        let g = extract_default(&root);
        assert!(has_edge(&g, "process", "Svc.Transform", "heuristic"), "selector 调用 → heuristic");
        assert!(has_edge(&g, "Svc.Transform", "helper", "local"), "裸调用同文件 → local");
    }

    #[test]
    fn java_method_invocation() {
        let (_d, root) = tmp_repo(&[(
            "Calc.java",
            r#"
public class Calc {
    public int compute(int x) {
        return square(x);
    }
    public int square(int x) {
        return x * x;
    }
}
"#,
        )]);
        let g = extract_default(&root);
        assert!(has_edge(&g, "Calc.compute", "Calc.square", "local"), "类内方法互调 → local（qualname 携带类名）");
    }

    #[test]
    fn rust_field_expression_call() {
        let (_d, root) = tmp_repo(&[(
            "main.rs",
            r#"
fn main() {
    let s = Svc::new();
    let r = s.transform(1);
    consume(r);
}

struct Svc;

impl Svc {
    fn new() -> Self {
        Svc
    }
    fn transform(&self, x: i32) -> i32 {
        double(x)
    }
}

fn double(x: i32) -> i32 {
    x * 2
}

fn consume(v: i32) {}
"#,
        )]);
        let g = extract_default(&root);
        assert!(has_edge(&g, "main", "consume", "local"), "裸调用同文件 → local");
        assert!(has_edge(&g, "Svc.transform", "double", "local"), "impl 块方法裸调用同文件 → local");
        assert!(has_edge(&g, "main", "Svc.transform", "heuristic"), "s.transform() 字段调用 → heuristic");
    }

    #[test]
    fn embed_source_toggle() {
        let (_d, root) = tmp_repo(&[("a.py", "def f():\n    return 1\n")]);
        let with_src = extract_default(&root);
        assert!(with_src["nodes"][0]["source"].is_string());

        let no_src = extract(&root, &ExtractOptions { embed_source: false, ..Default::default() }, &|_| {}).unwrap();
        assert!(no_src["nodes"][0].get("source").is_none(), "app 管线产物不应内嵌 source");
    }

    #[test]
    fn excludes_injection_and_builtin() {
        let (_d, root) = tmp_repo(&[
            ("app.py", "def main():\n    pass\n"),
            ("node_modules/x.py", "def junk():\n    pass\n"),
            ("skipme/y.py", "def also_junk():\n    pass\n"),
        ]);
        let g = extract(
            &root,
            &ExtractOptions { excludes: vec!["skipme".into()], ..Default::default() },
            &|_| {},
        )
        .unwrap();
        let quals: Vec<&str> = g["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["qualname"].as_str().unwrap())
            .collect();
        assert_eq!(quals, vec!["main"], "内置表排除 node_modules，注入排除 skipme");
    }

    #[test]
    fn entry_regex_marks_custom_entry() {
        let (_d, root) = tmp_repo(&[("w.py", "def worker_main():\n    pass\n\ndef other():\n    pass\n")]);
        let re = regex::Regex::new("worker").unwrap();
        let g = extract(&root, &ExtractOptions { entry_regex: Some(re), ..Default::default() }, &|_| {}).unwrap();
        assert_eq!(find_node(&g, "worker_main")["is_entry"], true);
        assert_eq!(find_node(&g, "other")["is_entry"], false);
    }
}
