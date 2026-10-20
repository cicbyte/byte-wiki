//! 多语言调用链提取：tree-sitter 解析 → 函数级调用图 JSON。
//!
//! 产品定位（dev-docs v2）：Wiki → 代码的桥，byte-wiki 核心能力。
//! 本 crate 是纯解析引擎（无 DB/无配置），core 无条件依赖它；
//! `byte-callchain` bin 服务无 app 的流水线场景（任意目录提取 JSON；
//! HTML 导出已退役——渲染统一由桌面端原生承担，Python 版工具承担既有流水线）。
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
    /// 语言/框架约定的入口（fn main、#[command]、@Scheduled 等）：标入口但不进「路由入口」分组
    entry_hint: bool,
    /// go/GoFrame：handler 签名中的 `*XxxReq` 类型名（用于反查 Req struct tag 的路由元数据）
    req_type: Option<String>,
    calls: Vec<(String, Option<String>, usize)>, // (被调名, 接收者文本, 1-based 行号)
}

/// 路由注册调用（框架在运行时把处理函数挂到路由——跨函数边界的入口来源），
/// 如 js `router.get("/p", h)`、go gin `r.GET("/p", h)`、rust `.route("/p", get(h))`
#[derive(Debug, Clone)]
struct RouteReg {
    label: String,   // "GET /api/x"（GoFrame Bind 无 path 时占位 "GF xxx"，解析时经 Req tag 反查升级）
    handler: String, // 注册处引用的函数名（解析时取末段标识符）
    /// 引用带容器时的限定提示（`controller.Auth.Login` → "Auth.Login"），解析优先于裸名
    qual_hint: Option<String>,
}

#[derive(Debug, Clone)]
struct FileRec {
    module: String,
    file: String,
    lang: &'static str,
    funcs: Vec<FuncRec>,
    registrations: Vec<RouteReg>,
    /// go/GoFrame：本文件 Req struct 的路由元数据 (struct名, path, method)
    req_tags: Vec<(String, String, String)>,
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

/// 去掉字符串字面量的引号
fn unquote(t: &str) -> String {
    t.trim()
        .trim_start_matches(['"', '\''])
        .trim_end_matches(['"', '\''])
        .to_string()
}

/// 文本中首个带引号字符串（路径提取用）
fn first_quoted(s: &str) -> Option<String> {
    for (i, c) in s.char_indices() {
        if c == '"' || c == '\'' {
            let rest = &s[i + 1..];
            let end = rest.find(c)?;
            return Some(rest[..end].to_string());
        }
    }
    None
}

/// HTTP 动词名归一（注册调用/注解通用；HandleFunc/Handle/Any 等全方法受理归为 ANY）
fn http_verb(name: &str) -> Option<&'static str> {
    Some(match name.to_ascii_uppercase().as_str() {
        "GET" => "GET",
        "POST" => "POST",
        "PUT" => "PUT",
        "DELETE" => "DELETE",
        "PATCH" => "PATCH",
        "HEAD" => "HEAD",
        "OPTIONS" => "OPTIONS",
        "ANY" | "ALL" | "HANDLE" | "HANDLEFUNC" => "ANY",
        _ => return None,
    })
}

/// python 装饰器中的 Flask/FastAPI 路由识别，标签美化 `METHOD /path`
/// （`.route(` 的方法看 methods= 实参，缺省 GET；解析失败回退装饰器首行截断）
fn routes_from_deco(deco: &str) -> Vec<String> {
    const VERBS: [&str; 8] = [
        ".route(", ".api_route(", ".get(", ".post(", ".put(", ".delete(", ".patch(", ".head(",
    ];
    let Some(hit) = VERBS.iter().find(|v| deco.contains(*v)) else {
        return vec![];
    };
    let path = first_quoted(deco).unwrap_or_default();
    let verb = if *hit == ".route(" || *hit == ".api_route(" {
        deco.find("methods")
            .and_then(|i| first_quoted(&deco[i..]))
            .map(|m| m.to_ascii_uppercase())
            .unwrap_or_else(|| "GET".into())
    } else {
        hit.trim_start_matches('.').trim_end_matches('(').to_ascii_uppercase()
    };
    if path.is_empty() && verb.is_empty() {
        let first = deco.lines().next().unwrap_or(deco).trim();
        return vec![first.chars().take(48).collect()];
    }
    vec![format!("{verb} {path}").trim().to_string()]
}

/// NestJS 方法装饰器 `@Get(':id')` → `GET :id`（无路径则仅动词）
fn routes_from_nest_decorator(deco: &str) -> Vec<String> {
    let name = deco.trim_start_matches('@');
    let head = name.split('(').next().unwrap_or("").trim();
    let Some(verb) = http_verb(head) else {
        return vec![];
    };
    let path = first_quoted(deco).unwrap_or_default();
    vec![if path.is_empty() { verb.to_string() } else { format!("{verb} {path}") }]
}

/// java Spring 系注解：@GetMapping("/x") 等映射注解 → 路由标签；
/// @Scheduled / @*Listener → 仅入口标记（非路由）。返回 (routes, entry_hint)
fn routes_from_java_annotations(node: Node, src: &str) -> (Vec<String>, bool) {
    let mut routes = Vec::new();
    let mut hint = false;
    let mut cur = node.walk();
    let found = node.children(&mut cur).any(|c| {
        if c.kind() != "modifiers" {
            return false;
        }
        let mut m = c.walk();
        for a in c.children(&mut m) {
            if a.kind() != "annotation" {
                continue;
            }
            let name = a
                .child_by_field_name("name")
                .map(|n| text_of(n, src))
                .unwrap_or("");
            let path = a
                .child_by_field_name("arguments")
                .map(|arg| {
                    let mut w = arg.walk();
                    let p = arg
                        .children(&mut w)
                        .find(|x| x.kind() == "string_literal")
                        .map(|x| unquote(text_of(x, src)))
                        .unwrap_or_default();
                    drop(w);
                    p
                })
                .unwrap_or_default();
            let verb = match name {
                "GetMapping" => Some("GET"),
                "PostMapping" => Some("POST"),
                "PutMapping" => Some("PUT"),
                "DeleteMapping" => Some("DELETE"),
                "PatchMapping" => Some("PATCH"),
                "RequestMapping" => Some("ANY"),
                _ => None,
            };
            if let Some(v) = verb {
                routes.push(if path.is_empty() { v.to_string() } else { format!("{v} {path}") });
            } else if name == "Scheduled" || name.ends_with("Listener") {
                hint = true;
            }
        }
        false
    });
    let _ = found;
    (routes, hint)
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
            // 框架入口识别（按语言组装 routes 与 entry_hint）
            let mut routes: Vec<String> =
                decorations.iter().flat_map(|d| routes_from_deco(d)).collect();
            let mut entry_hint = false;
            if conf.name == "java" {
                let (ann_routes, ann_hint) = routes_from_java_annotations(node, src);
                routes.extend(ann_routes);
                entry_hint |= ann_hint;
            }
            if conf.name == "javascript" || conf.name == "typescript" {
                // NestJS 方法装饰器
                if let Some(d) = node.child_by_field_name("decorator") {
                    routes.extend(routes_from_nest_decorator(text_of(d, src)));
                }
            }
            if conf.name == "rust" {
                // 紧邻的 attribute（#[tauri::command] 等）→ 入口标记
                let mut prev = node.prev_sibling();
                while let Some(p) = prev {
                    if p.kind() != "attribute_item" {
                        break;
                    }
                    if text_of(p, src).contains("command") {
                        entry_hint = true;
                    }
                    prev = p.prev_sibling();
                }
            }
            // 语言约定：go/rust 的 main 即进程入口
            if (conf.name == "go" || conf.name == "rust") && name == "main" {
                entry_hint = true;
            }
            // go/GoFrame：handler 签名中的 `*XxxReq` 类型名（跨文件反查 Req struct tag 路由元数据）
            let req_type = if conf.name == "go" {
                node.child_by_field_name("parameters")
                    .map(|p| text_of(p, src))
                    .and_then(|t| {
                        t.split(|c: char| c == ',' || c.is_whitespace())
                            .map(|tok| tok.trim_matches(|c: char| !c.is_alphanumeric() && c != '.'))
                            .filter(|tok| tok.ends_with("Req") && tok.len() > 3)
                            .next()
                            .map(|tok| tok.rsplit('.').next().unwrap_or(tok).to_string())
                    })
            } else {
                None
            };
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
                entry_hint,
                req_type,
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
    // Next.js App Router 文件约定：route.ts/route.js 内导出的 HTTP 动词函数即入口
    let base = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if (conf.name == "typescript" || conf.name == "javascript")
        && matches!(base, "route.ts" | "route.tsx" | "route.js")
    {
        // module 形如 "app/api/items/route" / "src/app/api/items/route" → URL 路径剥掉 app/ 前缀
        let dir: String = module.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
        let url_path = format!(
            "/{}",
            dir.split("app/").last().unwrap_or(&dir)
        );
        for f in deduped.iter_mut() {
            if ["GET", "POST", "PUT", "DELETE", "PATCH"].contains(&f.name.as_str()) {
                f.routes.push(format!("{} {url_path}", f.name));
            }
        }
    }
    // 注册式路由采集（Express/gin/HandleFunc/axum/GoFrame Bind 等）
    let mut registrations = Vec::new();
    collect_registrations(tree.root_node(), &src, conf, &mut registrations);
    // go/GoFrame：Req struct tag 路由元数据
    let mut req_tags = Vec::new();
    if conf.name == "go" {
        collect_go_req_tags(tree.root_node(), &src, &mut req_tags);
    }
    Some(FileRec {
        module,
        file: path.strip_prefix(root).ok()?.to_string_lossy().replace('\\', "/"),
        lang: conf.name,
        funcs: deduped,
        registrations,
        req_tags,
        source_lines: src.lines().map(|l| l.to_string()).collect(),
    })
}

// ----------------------------------------------------------------------------
// 注册式路由采集：框架运行时挂路由（跨函数边界的入口来源）
// ----------------------------------------------------------------------------

/// 调用实参中取「首个字符串 + 首个处理函数引用」（如 `("/p", handler)`）
fn string_then_handler(args: Node, src: &str) -> Option<(String, String)> {
    let mut path: Option<String> = None;
    let mut handler: Option<String> = None;
    let mut cur = args.walk();
    for a in args.children(&mut cur) {
        if !a.is_named() {
            continue;
        }
        match a.kind() {
            "string" | "interpreted_string_literal" | "string_literal" if path.is_none() => {
                path = Some(unquote(text_of(a, src)));
            }
            "identifier" | "member_expression" | "selector_expression" | "field_expression"
            | "path_expression" | "scoped_identifier" if handler.is_none() => {
                let t = text_of(a, src);
                let last = t.rsplit(['.', ':']).next().unwrap_or("").to_string();
                if !last.is_empty() {
                    handler = Some(last);
                }
            }
            _ => {}
        }
    }
    Some((path?, handler?))
}

/// 从路由注册调用提取注册列表（GoFrame Bind 一次可注册多个 handler）。
/// js/ts：`router.get("/p", h)`（Express/Koa/Fastify 风格）
/// go：`r.GET("/p", h)`（gin/echo/fiber）、`mux.HandleFunc("/p", h)`（net/http）、
///     `group.Bind(controller.Auth.Login, ...)`（GoFrame，path/method 在 Req struct tag）
/// rust：`.route("/p", get(h))`（axum）
fn registration_of(node: Node, src: &str, conf: &LangConf) -> Vec<RouteReg> {
    let (Some(args), Some(f)) = (node.child_by_field_name("arguments"), node.child_by_field_name("function")) else {
        return vec![];
    };
    match conf.name {
        "javascript" | "typescript" => {
            if f.kind() != "member_expression" {
                return vec![];
            }
            let prop = f
                .child_by_field_name("property")
                .map(|n| text_of(n, src))
                .unwrap_or("");
            let verb = match http_verb(prop) {
                Some(v) => v,
                None => return vec![],
            };
            match string_then_handler(args, src) {
                Some((path, handler)) => vec![RouteReg { label: format!("{verb} {path}"), handler, qual_hint: None }],
                None => vec![],
            }
        }
        "go" => {
            if f.kind() != "selector_expression" {
                return vec![];
            }
            let field_name = f
                .child_by_field_name("field")
                .map(|n| text_of(n, src))
                .unwrap_or("");
            // GoFrame：group.Bind(h.A.B, c.D.E, ...) —— 实参全是 handler 引用，
            // path/method 在 handler 签名 *XxxReq 对应 struct 的 g.Meta tag（解析阶段反查升级标签）
            if field_name == "Bind" {
                let mut out = Vec::new();
                let mut cur = args.walk();
                for a in args.children(&mut cur) {
                    if !a.is_named() || !matches!(a.kind(), "identifier" | "selector_expression" | "field_expression" | "parenthesized_expression") {
                        continue;
                    }
                    let t = text_of(a, src).replace('(', "").replace(')', "").trim().to_string();
                    let segs: Vec<&str> = t.split('.').collect();
                    let Some(last) = segs.last().copied().filter(|s| !s.is_empty()) else {
                        continue;
                    };
                    if !last.chars().all(|c| c.is_alphanumeric() || c == '_') {
                        continue;
                    }
                    let qual_hint = if segs.len() >= 2 {
                        Some(format!("{}.{}", segs[segs.len() - 2], last))
                    } else {
                        None
                    };
                    out.push(RouteReg { label: format!("GF {t}"), handler: last.to_string(), qual_hint });
                }
                return out;
            }
            match http_verb(field_name).zip(string_then_handler(args, src)) {
                Some((verb, (path, handler))) => vec![RouteReg { label: format!("{verb} {path}"), handler, qual_hint: None }],
                None => vec![],
            }
        }
        "rust" => {
            // axum：.route("/p", get(h)) —— path 是首个字符串实参，动词在内层调用名
            let route_field = f
                .child_by_field_name("field")
                .map(|n| text_of(n, src))
                .unwrap_or("");
            if f.kind() != "field_expression" || route_field != "route" {
                return vec![];
            }
            let mut path: Option<String> = None;
            let mut handler: Option<String> = None;
            let mut verb: Option<&'static str> = None;
            let mut cur = args.walk();
            for a in args.children(&mut cur) {
                if !a.is_named() {
                    continue;
                }
                if a.kind() == "string_literal" && path.is_none() {
                    path = Some(unquote(text_of(a, src)));
                } else if a.kind() == "call_expression" && handler.is_none() {
                    let cf = match a.child_by_field_name("function") {
                        Some(c) if c.kind() == "identifier" => c,
                        _ => continue,
                    };
                    let v = match http_verb(text_of(cf, src)) {
                        Some(v) => v,
                        None => continue,
                    };
                    if let Some(hargs) = a.child_by_field_name("arguments") {
                        let mut w = hargs.walk();
                        let mut got: Option<String> = None;
                        for ha in hargs.children(&mut w) {
                            if matches!(
                                ha.kind(),
                                "identifier" | "path_expression" | "field_expression" | "scoped_identifier"
                            ) {
                                let t = text_of(ha, src);
                                let last = t.rsplit(['.', ':']).next().unwrap_or("").to_string();
                                if !last.is_empty() {
                                    got = Some(last);
                                }
                                break;
                            }
                        }
                        drop(w);
                        if let Some(h) = got {
                            handler = Some(h);
                            verb = Some(v);
                        }
                    }
                }
            }
            match (verb, path, handler) {
                (Some(v), Some(p), Some(h)) => vec![RouteReg { label: format!("{v} {p}"), handler: h, qual_hint: None }],
                _ => vec![],
            }
        }
        _ => vec![],
    }
}

/// 全树递归采集路由注册（独立轻量扫描，不复用 visit 的函数语义）
fn collect_registrations(node: Node, src: &str, conf: &LangConf, out: &mut Vec<RouteReg>) {
    if conf.call_kinds.contains(&node.kind()) {
        out.extend(registration_of(node, src, conf));
    }
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        collect_registrations(child, src, conf, out);
    }
}

/// go/GoFrame：Req struct 的 g.Meta tag 路由元数据收集
/// （`type XxxReq struct { g.Meta \`path:"/x" method:"get"\` }`）
fn collect_go_req_tags(node: Node, src: &str, out: &mut Vec<(String, String, String)>) {
    if node.kind() == "type_declaration" {
        let mut cur = node.walk();
        for spec in node.children(&mut cur) {
            if spec.kind() != "type_spec" {
                continue;
            }
            let Some(name) = spec.child_by_field_name("name").map(|n| text_of(n, src).to_string()) else {
                continue;
            };
            let Some(ty) = spec.child_by_field_name("type") else { continue };
            if ty.kind() != "struct_type" {
                continue;
            }
            let body_text = text_of(ty, src);
            if !body_text.contains("path:\"") {
                continue;
            }
            let path = body_text
                .find("path:")
                .and_then(|i| first_quoted(&body_text[i..]))
                .unwrap_or_default();
            let method = body_text
                .find("method:")
                .and_then(|i| first_quoted(&body_text[i..]))
                .map(|m| m.to_ascii_uppercase())
                .unwrap_or_else(|| "ANY".into());
            if !path.is_empty() {
                out.push((name, path, method));
            }
        }
    }
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        collect_go_req_tags(child, src, out);
    }
}



// ----------------------------------------------------------------------------
// 建图（与 Python 版同 schema 的 JSON；source 为可选字段）
// ----------------------------------------------------------------------------

// ----------------------------------------------------------------------------
// 单文件符号提取（agent 探查工具用）
// ----------------------------------------------------------------------------

/// 单文件符号：AST 提取的函数/方法档案（行号 1-based、end 含端点，
/// 与 JSON 产物的 lineno/end_lineno 语义一致）
pub struct FileSymbol {
    pub qualname: String,
    pub start_row: usize,
    pub end_row: usize,
    pub is_async: bool,
    /// 语言/框架约定的入口（fn main、#[command]、@Scheduled 等）
    pub is_entry: bool,
    /// 路由（HTTP 方法 + 路径；无则空）
    pub routes: Vec<String>,
}

/// 提取单文件符号（按扩展名匹配语言；超大/不支持/解析失败返回 None）。
/// 复用调用链的 tree-sitter 解析，不遍历仓库——agent 按需读取单文件用。
pub fn file_symbols(path: &Path) -> Option<(String, Vec<FileSymbol>)> {
    let ext = path.extension().and_then(|e| e.to_str())?;
    let conf = lang_confs().into_iter().find(|c| c.exts.contains(&ext))?;
    let root = path.parent()?;
    let rec = extract_file(path, root, &conf)?;
    let syms = rec
        .funcs
        .into_iter()
        .map(|f| FileSymbol {
            qualname: f.qualname,
            start_row: f.start_row + 1,
            end_row: f.end_row + 1,
            is_async: f.is_async,
            is_entry: f.entry_hint || !f.routes.is_empty(),
            routes: f.routes,
        })
        .collect();
    Some((conf.name.to_string(), syms))
}

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

    // 全局与模块内名字索引（持 String 键：后续注册解析要可变回写 files，
    // 索引不能借住 files 的 &str）
    let mut global_idx: HashMap<String, Vec<(usize, usize)>> = HashMap::new(); // name -> [(file_i, func_i)]
    let mut local_idx: HashMap<(usize, String), Vec<usize>> = HashMap::new();
    for (fi, f) in files.iter().enumerate() {
        for (gi, g) in f.funcs.iter().enumerate() {
            global_idx.entry(g.name.clone()).or_default().push((fi, gi));
            local_idx
                .entry((fi, g.name.clone()))
                .or_default()
                .push(gi);
        }
    }

    // go/GoFrame：Req struct tag 全局表（struct名 → method path），注册解析时跨文件反查
    let mut req_tags: HashMap<String, (String, String)> = HashMap::new();
    for f in &files {
        for (name, path, method) in &f.req_tags {
            req_tags
                .entry(name.clone())
                .or_insert_with(|| (method.clone(), path.clone()));
        }
    }

    // 注册式路由解析：qual_hint（容器.方法）全局唯一优先，其次本文件同名（全部），
    // 再次全局唯一裸名；GoFrame 占位标签（"GF xxx"）经 handler 的 *XxxReq 反查 tag 升级
    let mut route_hits: Vec<(usize, usize, String)> = Vec::new();
    for (fi, f) in files.iter().enumerate() {
        for reg in &f.registrations {
            if reg.handler.is_empty() {
                continue;
            }
            let by_hint: Option<Vec<(usize, usize)>> = reg.qual_hint.as_ref().map(|hint| {
                files
                    .iter()
                    .enumerate()
                    .flat_map(|(fj, ff)| {
                        ff.funcs
                            .iter()
                            .enumerate()
                            .filter(|(_, g)| &g.qualname == hint)
                            .map(move |(gj, _)| (fj, gj))
                    })
                    .collect()
            });
            let resolved = if let Some(hits) = by_hint {
                (hits.len() == 1).then_some(hits)
            } else {
                None
            }
            .or_else(|| {
                local_idx
                    .get(&(fi, reg.handler.clone()))
                    .map(|gis| gis.iter().map(|&gi| (fi, gi)).collect())
            })
            .or_else(|| {
                global_idx.get(&reg.handler).and_then(|cands| {
                    (cands.len() == 1).then(|| vec![cands[0]])
                })
            });
            let Some(targets) = resolved else { continue };
            for (tfi, tgi) in targets {
                let mut label = reg.label.clone();
                if label.starts_with("GF ") {
                    if let Some(rt) = &files[tfi].funcs[tgi].req_type {
                        if let Some((method, path)) = req_tags.get(rt) {
                            label = format!("{method} {path}");
                        }
                    }
                }
                route_hits.push((tfi, tgi, label));
            }
        }
    }
    for (fi, gi, label) in route_hits {
        files[fi].funcs[gi].routes.push(label);
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
            let is_entry = !g.routes.is_empty() || g.entry_hint || matched;
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
                    if let Some(gis) = local_idx.get(&(fi, callee.clone())) {
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
                    // 方法调用 obj.m()：唯一候选 → heuristic；多候选 → 接收者消歧
                    // （rust trait 分发/变量名调用：cmd.run() 的 8 个 run 候选靠
                    // 接收者文本与容器段模糊匹配收敛；仍不唯一则丢弃，宁缺毋滥）
                    if let Some(cands) = global_idx.get(callee.as_str()) {
                        if cands.len() == 1 {
                            let (cfi, cgi) = cands[0];
                            targets.push((
                                format!("{}::{}", files[cfi].module, files[cfi].funcs[cgi].qualname),
                                "heuristic",
                            ));
                        } else if cands.len() > 1 {
                            let recv_text = recv.as_deref().unwrap_or("");
                            // 接收者提示集：recv 末段（obj.field.method 取 field 段）
                            // + 所属 impl 容器（self 方法）+ 类型限定路径首段
                            let recv_hints: Vec<String> = {
                                let seg = recv_text
                                    .rsplit(['.', ':'])
                                    .next()
                                    .unwrap_or(recv_text)
                                    .trim()
                                    .to_string();
                                let mut v = vec![seg.clone()];
                                if seg == "self" {
                                    // self.m() → 调用方所在 impl 容器（qualname 去掉方法段）
                                    if let Some(container) = g.qualname.rsplit_once('.').map(|(c, _)| c.to_string()) {
                                        v.push(container);
                                    }
                                }
                                // Database::open 形态：recv 即完整路径，首段也是类型提示
                                if recv_text.contains("::") {
                                    let head = recv_text.split("::").next().unwrap_or("").trim().to_string();
                                    if !head.is_empty() {
                                        v.push(head);
                                    }
                                }
                                v.retain(|s| !s.is_empty());
                                v
                            };
                            let matches_hint = |qn: &str| -> bool {
                                let container = qn.rsplit_once('.').map(|(c, _)| c).unwrap_or(qn);
                                recv_hints.iter().any(|h| {
                                    let h = h.to_lowercase();
                                    let c = container.to_lowercase();
                                    c == h || c.starts_with(&h) || h.starts_with(&c)
                                })
                            };
                            let src_id = format!("{}::{}", f.module, g.qualname);
                            let hit: Vec<(usize, usize)> = cands
                                .iter()
                                .copied()
                                .filter(|(cfi2, cgi2)| {
                                    let dst = format!("{}::{}", files[*cfi2].module, files[*cfi2].funcs[*cgi2].qualname);
                                    dst != src_id
                                        && matches_hint(&files[*cfi2].funcs[*cgi2].qualname)
                                })
                                .collect();
                            // 收窄后空集 = recv 提示命中的只有自身（match 臂变量与
                            // 枚举类型同名的 trait 分发：cmd.run() 里 cmd 既是变量又是
                            // 枚举 Cmd）——小候选集时回退全候选减自身（分发到各变体
                            // impl）。候选过多（如全仓 N 个 new）说明是高频构造器，
                            // 广播只会制造入度爆炸的假枢纽，放弃。
                            // 回退触发条件：recv 提示与调用方自身容器同名（match 臂
                            // 变量 == 枚举类型名的精确特征）；Builder/Opts 等无关
                            // 接收者的空收窄一律不广播
                            let self_container = g
                                .qualname
                                .rsplit_once('.')
                                .map(|(c, _)| c.to_lowercase())
                                .unwrap_or_default();
                            let fallback_ok = recv_hints
                                .iter()
                                .any(|h| h.to_lowercase() == self_container);
                            // 上限 12：覆盖主流 trait 分发规模（zoxide run 有 8 候选），
                            // 仍拦住 new/with 这类全仓几十候选的构造器广播
                            let effective: Vec<(usize, usize)> = if hit.is_empty() && fallback_ok && cands.len() <= 12 {
                                cands
                                    .iter()
                                    .copied()
                                    .filter(|(cfi2, cgi2)| {
                                        format!("{}::{}", files[*cfi2].module, files[*cfi2].funcs[*cgi2].qualname) != src_id
                                    })
                                    .collect()
                            } else {
                                hit
                            };
                            for (cfi, cgi) in effective {
                                targets.push((
                                    format!("{}::{}", files[cfi].module, files[cfi].funcs[cgi].qualname),
                                    "receiver",
                                ));
                            }
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
    fn rust_trait_dispatch_resolved_by_receiver() {
        // zoxide 风格：match 臂变量调用 trait 方法（cmd.run()，run 多候选）
        // + self 方法 + 类型限定调用（Database::open）
        let (_d, root) = tmp_repo(&[
            (
                "main.rs",
                r#"
fn main() {
    let cmd = Cmd::parse();
    cmd.run();
}
"#,
            ),
            (
                "cmd/mod.rs",
                r#"
pub trait Run {
    fn run(&self) -> Result<()>;
}

impl Run for Cmd {
    fn run(&self) -> Result<()> {
        match self {
            Cmd::Add(cmd) => cmd.run(),
            Cmd::Query(cmd) => cmd.run(),
        }
    }
}
"#,
            ),
            (
                "cmd/add.rs",
                r#"
impl Run for Add {
    fn run(&self) -> Result<()> {
        let mut db = Database::open()?;
        self.write(&mut db)
    }
}
"#,
            ),
            (
                "cmd/query.rs",
                r#"
impl Run for Query {
    fn run(&self) -> Result<()> {
        Ok(())
    }
}
"#,
            ),
            (
                "db/mod.rs",
                r#"
impl Database {
    pub fn open() -> Result<Database> { Ok(Database {}) }
}
"#,
            ),
        ]);
        let g = extract_default(&root);
        // cmd.run()：recv=cmd ↔ 候选容器 Cmd（唯一命中，其余 Add/Query 不匹配）
        assert!(has_edge(&g, "Cmd.run", "Add.run", "receiver"), "match 臂 cmd.run() → Add.run（接收者消歧）");
        assert!(has_edge(&g, "Cmd.run", "Query.run", "receiver"), "match 臂 cmd.run() → Query.run（接收者消歧）");
        // main 里 cmd.run()：recv=cmd ↔ Cmd.run
        assert!(has_edge(&g, "main", "Cmd.run", "receiver"), "main → Cmd.run");
        // Database::open()：recv 路径首段 Database ↔ 容器 Database（open 唯一，本就是 heuristic）
        assert!(g["edges"].as_array().unwrap().iter().any(|e| e["dst"].as_str().unwrap_or("").ends_with("Database.open")), "Database::open 有边");
    }

    #[test]
    fn rust_self_call_resolved_by_impl_container() {
        // self.query(...) 在 Query impl 内 → Query.query（同名方法他处无）
        let (_d, root) = tmp_repo(&[
            (
                "cmd/query.rs",
                r#"
impl Query {
    fn run(&self) -> Result<()> {
        self.query()
    }
    fn query(&self) -> Result<()> { Ok(()) }
}
"#,
            ),
            (
                "cmd/other.rs",
                r#"
impl Other {
    fn run(&self) -> Result<()> { Ok(()) }
    fn query(&self) -> Result<()> { Ok(()) }
}
"#,
            ),
        ]);
        let g = extract_default(&root);
        assert!(has_edge(&g, "Query.run", "Query.query", "receiver"), "self.query → 同 impl 容器（消歧）");
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

    // ===== 框架入口适配 =====

    #[test]
    fn python_route_label_pretty() {
        let (_d, root) = tmp_repo(&[(
            "a.py",
            "@app.get(\"/users\")\ndef list_users():\n    pass\n\n@bp.route(\"/task/create\", methods=[\"POST\"])\ndef create_task():\n    pass\n",
        )]);
        let g = extract_default(&root);
        let n = find_node(&g, "list_users");
        assert_eq!(n["routes"][0], "GET /users");
        let n = find_node(&g, "create_task");
        assert_eq!(n["routes"][0], "POST /task/create");
    }

    #[test]
    fn js_express_registration() {
        let (_d, root) = tmp_repo(&[(
            "server.js",
            r#"
const router = require("express").Router();

router.get("/users", listUsers);
router.post("/users/:id", updateUser);
router.delete("/users/:id", function inlineHandler(req, res) {});

function listUsers(req, res) {}
function updateUser(req, res) {}
"#,
        )]);
        let g = extract_default(&root);
        let n = find_node(&g, "listUsers");
        assert_eq!(n["is_entry"], true, "router.get 回调引用应标入口");
        assert_eq!(n["routes"][0], "GET /users");
        assert_eq!(find_node(&g, "updateUser")["routes"][0], "POST /users/:id");
        // 匿名内联函数注册不产生入口（无具名 handler 可解析）
        assert!(!g["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["qualname"] == "inlineHandler" && x["is_entry"] == true));
    }

    #[test]
    fn go_gin_and_nethttp_registration() {
        let (_d, root) = tmp_repo(&[(
            "main.go",
            r#"
package main

func setup(r *gin.Engine) {
	r.GET("/ping", pingHandler)
	http.HandleFunc("/health", healthHandler)
}

func pingHandler(c *gin.Context) {}
func healthHandler(w http.ResponseWriter, r *http.Request) {}
"#,
        )]);
        let g = extract_default(&root);
        assert_eq!(find_node(&g, "pingHandler")["routes"][0], "GET /ping");
        assert_eq!(find_node(&g, "healthHandler")["routes"][0], "ANY /health");
        assert_eq!(find_node(&g, "pingHandler")["is_entry"], true);
    }

    #[test]
    fn go_rust_main_convention() {
        let (_d, root) = tmp_repo(&[
            ("m.go", "package main\n\nfunc main() {\n\tsetup()\n}\n\nfunc setup() {}\n"),
            ("m.rs", "fn main() {\n    run();\n}\n\nfn run() {}\n"),
        ]);
        let g = extract_default(&root);
        let go_main = find_node(&g, "main"); // go: module m 的 main
        assert_eq!(go_main["module"], "m");
        assert_eq!(go_main["is_entry"], true, "go fn main 约定入口");
        assert!(go_main["routes"].as_array().unwrap().is_empty(), "约定入口不进路由分组");
        let rust_main = g["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["module"] == "m" && n["file"] == "m.rs")
            .unwrap();
        assert_eq!(rust_main["is_entry"], true, "rust fn main 约定入口");
    }

    #[test]
    fn rust_axum_registration_and_command_attr() {
        let (_d, root) = tmp_repo(&[(
            "app.rs",
            r#"
fn routes() -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/tasks", post(create_task))
}

async fn health() -> &'static str { "ok" }
async fn create_task() {}

#[tauri::command]
fn load_config() -> u32 { 1 }
"#,
        )]);
        let g = extract_default(&root);
        assert_eq!(find_node(&g, "health")["routes"][0], "GET /health");
        assert_eq!(find_node(&g, "create_task")["routes"][0], "POST /tasks");
        let cmd = find_node(&g, "load_config");
        assert_eq!(cmd["is_entry"], true, "#[tauri::command] 标入口");
        assert!(cmd["routes"].as_array().unwrap().is_empty(), "command 属自定义入口而非路由");
    }

    #[test]
    fn java_spring_annotations() {
        let (_d, root) = tmp_repo(&[(
            "UserController.java",
            r#"
@RestController
public class UserController {
    @GetMapping("/users")
    public List<User> listUsers() {
        return repo.findAll();
    }

    @Scheduled(fixedRate = 5000)
    public void syncJob() {}
}
"#,
        )]);
        let g = extract_default(&root);
        let n = find_node(&g, "UserController.listUsers");
        assert_eq!(n["routes"][0], "GET /users");
        assert_eq!(n["is_entry"], true);
        let job = find_node(&g, "UserController.syncJob");
        assert_eq!(job["is_entry"], true, "@Scheduled 标入口");
        assert!(job["routes"].as_array().unwrap().is_empty());
    }

    #[test]
    fn nextjs_route_file_convention() {
        let (_d, root) = tmp_repo(&[(
            "src/app/api/items/route.ts",
            "export async function GET() {\n    return list();\n}\n\nexport async function POST(req: Request) {}\n",
        )]);
        let g = extract_default(&root);
        let n = find_node(&g, "GET");
        assert_eq!(n["is_entry"], true);
        assert_eq!(n["routes"][0], "GET /api/items", "剥掉 src/app 前缀后的 URL 路径");
    }

    #[test]
    fn goframe_bind_with_req_tag() {
        // 三文件跨文件解析：Req struct tag（api 包）← controller 方法签名 *XxxReq ← router Bind 引用
        let (_d, root) = tmp_repo(&[
            (
                "api/setting.go",
                "package api\n\ntype GetProfileReq struct {\n\tg.Meta `path:\"/account/profile\" method:\"get\"`\n}\n",
            ),
            (
                "controller/setting.go",
                "package controller\n\ntype Setting struct{}\n\nfunc (s *Setting) GetProfile(ctx context.Context, req *GetProfileReq) error {\n\treturn nil\n}\n",
            ),
            (
                "router/router.go",
                "package router\n\nfunc BindController(group *ghttp.RouterGroup) {\n\tgroup.Bind(\n\t\tcontroller.Setting.GetProfile,\n\t)\n}\n",
            ),
        ]);
        let g = extract_default(&root);
        let n = find_node(&g, "Setting.GetProfile");
        assert_eq!(n["is_entry"], true, "Bind 引用经 qual_hint 解析到 controller 方法");
        assert_eq!(
            n["routes"][0], "GET /account/profile",
            "handler 签名 *GetProfileReq 反查 tag 升级标签"
        );
    }
}
