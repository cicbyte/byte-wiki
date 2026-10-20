//! Agent 大纲：多轮自主探查生成 Wiki 结构（#482）
//!
//! 与单发大纲的本质差异：LLM 拿到初始结构摘要后，用只读工具按需读取
//! 源码/目录/AST 符号，再产出大纲——绑定文件从「路径名猜」变成「读过
//! 再绑」。工具全部限额 + 路径沙箱（防提示注入读仓库外文件）；
//! 轮次/读取双预算护栏；每轮推流式事件（终端浮窗可见）与模型日志；
//! 失败由调用方兜底回退单发管线。
//!
//! rig 0.42 无现成 multi_turn agent——循环自写：请求 → AssistantContent
//! 里的 ToolCall → 执行 → ToolResult 回填 → 下一轮；纯文本即出纲。

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use rig_core::completion::message::{AssistantContent, Message, ToolCall, ToolResultContent, UserContent};
use rig_core::completion::{CompletionResponse, ToolDefinition};
use serde_json::json;

use crate::generator::GenerateEvent;
use crate::llm::LlmClient;
use crate::service::AppService;
use crate::{CoreError, CoreResult};

/// 轮次上限（超出注入收尾指令）
const MAX_ROUNDS: usize = 16;
/// read_file 累计字符预算（触顶同上）
const READ_BUDGET_CHARS: usize = 400_000;
/// 单次 read_file 行数上限
const READ_MAX_LINES: usize = 300;

// ============ 工具定义 ============

fn tool_defs() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "read_file".into(),
            description: "读取仓库内源文件的指定行区间（带行号）。单次最多 300 行；建议先 list_symbols 看结构再精读关键函数。".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "仓库相对路径，/ 分隔"},
                    "start_line": {"type": "integer", "description": "起始行（1-based，可省略=从头）"},
                    "end_line": {"type": "integer", "description": "结束行（含端点，可省略=+300 行）"}
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "list_symbols".into(),
            description: "AST 解析单文件，列出函数/方法符号（限定名、行区间、是否 async、是否入口、HTTP 路由）。快速了解一个文件有哪些方法，比读全文省得多。".into(),
            parameters: json!({
                "type": "object",
                "properties": {"path": {"type": "string", "description": "仓库相对路径"}},
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "list_dir".into(),
            description: "列出目录的直接子项（文件大小/类型）。".into(),
            parameters: json!({
                "type": "object",
                "properties": {"path": {"type": "string", "description": "仓库相对目录，空=根目录"}},
            }),
        },
        ToolDefinition {
            name: "search".into(),
            description: "跨仓库关键字扫描（在索引收录的文本文件内，最多 50 条命中，file:line:text）。".into(),
            parameters: json!({
                "type": "object",
                "properties": {"pattern": {"type": "string", "description": "关键字（大小写不敏感）"}},
                "required": ["pattern"]
            }),
        },
        ToolDefinition {
            name: "module_stats".into(),
            description: "模块统计与依赖边（谁依赖谁、被依赖次数）。".into(),
            parameters: json!({"type": "object", "properties": {}}),
        },
    ]
}

// ============ 路径沙箱 ============

/// 相对路径解析进仓库根（拒绝绝对路径、`..` 穿越；符号链接规范化后必须仍在根内）。
/// LLM 的工具参数来自被分析仓库的内容——恶意仓库可注入提示词诱导读取
/// 任意文件（如 ~/.ssh/id_rsa），必须沙箱。
fn resolve_within(root: &Path, rel: &str) -> CoreResult<PathBuf> {
    let rel = rel.trim().replace('\\', "/");
    let p = Path::new(&rel);
    // Windows 上 "/etc/passwd" 不算 is_absolute（无盘符），用 has_root 一并拒
    if p.is_absolute() || p.has_root() {
        return Err(CoreError::Other(anyhow::anyhow!("禁止绝对路径：{rel}")));
    }
    if p.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(CoreError::Other(anyhow::anyhow!("禁止 .. 穿越：{rel}")));
    }
    let joined = root.join(p);
    if let (Ok(canon), Ok(root_canon)) = (joined.canonicalize(), root.canonicalize()) {
        if !canon.starts_with(&root_canon) {
            return Err(CoreError::Other(anyhow::anyhow!("路径越出仓库根：{rel}")));
        }
    }
    Ok(joined)
}

// ============ 工具执行 ============

struct ToolCtx {
    root: PathBuf,
    /// 索引收录的文件清单（search 范围；也是 read/list 的存在性参考）
    known_files: Vec<String>,
    module_stats: String,
    read_total: usize,
}

fn clamp_chars(s: &mut String, max: usize) {
    if s.chars().count() > max {
        let cut: String = s.chars().take(max).collect();
        *s = format!("{cut}\n…（内容超限截断）");
    }
}

fn exec_read_file(ctx: &mut ToolCtx, args: &serde_json::Value) -> String {
    let rel = args["path"].as_str().unwrap_or_default();
    let Ok(abs) = resolve_within(&ctx.root, rel) else {
        return "错误：路径不合法（拒绝）".into();
    };
    let Ok(content) = std::fs::read_to_string(&abs) else {
        return format!("错误：读取失败或非 UTF-8：{rel}");
    };
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();
    let start = args["start_line"].as_u64().unwrap_or(1).max(1) as usize;
    let start_idx = (start - 1).min(total);
    let default_end = start_idx + READ_MAX_LINES;
    let end = args["end_line"].as_u64().map(|e| e as usize).unwrap_or(default_end).min(start_idx + READ_MAX_LINES);
    let mut out = String::new();
    for (i, l) in lines.iter().enumerate().skip(start_idx).take(end.saturating_sub(start_idx)) {
        let t: String = l.chars().take(400).collect();
        out.push_str(&format!("{}| {}\n", i + 1, t));
    }
    if total > end {
        out.push_str(&format!("…（共 {total} 行，本次读取 {start}-{}）\n", end.min(total)));
    }
    ctx.read_total += out.len();
    clamp_chars(&mut out, 20_000);
    out
}

fn exec_list_symbols(ctx: &ToolCtx, args: &serde_json::Value) -> String {
    let rel = args["path"].as_str().unwrap_or_default();
    let Ok(abs) = resolve_within(&ctx.root, rel) else {
        return "错误：路径不合法（拒绝）".into();
    };
    match byte_wiki_callchain::file_symbols(&abs) {
        Some((lang, syms)) if !syms.is_empty() => {
            let mut out = format!("[{lang}] {rel}：{} 个符号\n", syms.len());
            for s in syms.iter().take(80) {
                let flags = [
                    if s.is_async { "async" } else { "" },
                    if s.is_entry { "entry" } else { "" },
                ]
                .into_iter()
                .filter(|f| !f.is_empty())
                .collect::<Vec<_>>()
                .join(",");
                let routes = if s.routes.is_empty() {
                    String::new()
                } else {
                    format!("  {}", s.routes.join(" "))
                };
                let flag_str = if flags.is_empty() { String::new() } else { format!(" [{flags}]") };
                out.push_str(&format!("- {} (L{}-{}){flag_str}{routes}\n", s.qualname, s.start_row, s.end_row));
            }
            if syms.len() > 80 {
                out.push_str(&format!("…（另有 {} 个符号未列出）\n", syms.len() - 80));
            }
            out
        }
        Some((lang, _)) => format!("[{lang}] {rel}：未提取到函数/方法符号"),
        None => format!("{rel}：不支持的语言或解析失败"),
    }
}

fn exec_list_dir(ctx: &ToolCtx, args: &serde_json::Value) -> String {
    let rel = args["path"].as_str().unwrap_or("").trim().replace('\\', "/");
    let Ok(abs) = resolve_within(&ctx.root, &rel) else {
        return "错误：路径不合法（拒绝）".into();
    };
    let Ok(entries) = std::fs::read_dir(&abs) else {
        return format!("错误：目录不存在：{}（空）", rel);
    };
    let prefix = if rel.is_empty() { String::new() } else { format!("{rel}/") };
    let mut rows: Vec<String> = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let rel_path = format!("{prefix}{name}");
        let meta = e.metadata().ok();
        let row = if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            let count = ctx.known_files.iter().filter(|f| f.starts_with(&format!("{rel_path}/"))).count();
            format!("{rel_path}/ （目录，索引内 {count} 文件）")
        } else {
            let size = meta.map(|m| m.len()).unwrap_or(0);
            format!("{rel_path} （{size} B）")
        };
        rows.push(row);
    }
    rows.sort();
    if rows.is_empty() {
        return format!("（{rel} 为空目录）");
    }
    let mut out = rows.join("\n");
    clamp_chars(&mut out, 8_000);
    format!("{out}\n")
}

fn exec_search(ctx: &ToolCtx, args: &serde_json::Value) -> String {
    let pattern = args["pattern"].as_str().unwrap_or_default().trim().to_lowercase();
    if pattern.len() < 2 {
        return "错误：关键字过短（≥2 字符）".into();
    }
    let mut hits = 0;
    let mut out = String::new();
    for path in &ctx.known_files {
        if hits >= 50 {
            out.push_str("…（命中过多，仅列 50 条）\n");
            break;
        }
        let Ok(abs) = resolve_within(&ctx.root, path) else { continue };
        let Ok(content) = std::fs::read_to_string(&abs) else { continue };
        if content.len() > 512 * 1024 {
            continue;
        }
        for (i, line) in content.lines().enumerate() {
            if line.to_lowercase().contains(&pattern) {
                let t: String = line.trim().chars().take(160).collect();
                let line_no = i + 1;
                out.push_str(&format!("{path}:{line_no}: {t}\n"));
                hits += 1;
                if hits >= 50 {
                    break;
                }
            }
        }
    }
    if out.is_empty() {
        format!("（无命中：{pattern}）")
    } else {
        out
    }
}

fn exec_tool(ctx: &mut ToolCtx, name: &str, args: &serde_json::Value) -> String {
    match name {
        "read_file" => exec_read_file(ctx, args),
        "list_symbols" => exec_list_symbols(ctx, args),
        "list_dir" => exec_list_dir(ctx, args),
        "search" => exec_search(ctx, args),
        "module_stats" => format!("{}\n", ctx.module_stats),
        other => format!("错误：未知工具 {other}"),
    }
}

// ============ Agent 循环 ============

/// 增量缓冲：流 delta 按 400ms 批量推送（避免每 token 一个 IPC 事件）
struct DeltaBuf {
    think: String,
    content: String,
    last_flush: std::time::Instant,
}

impl DeltaBuf {
    fn new() -> Self {
        Self { think: String::new(), content: String::new(), last_flush: std::time::Instant::now() }
    }
    fn push(&mut self, channel: &str, delta: &str) {
        let buf = if channel == "think" { &mut self.think } else { &mut self.content };
        buf.push_str(delta);
    }
    fn maybe_flush(&mut self, call: &str, on_event: &dyn Fn(GenerateEvent)) {
        if self.last_flush.elapsed() < std::time::Duration::from_millis(400) {
            return;
        }
        self.flush(call, on_event);
    }
    fn flush(&mut self, call: &str, on_event: &dyn Fn(GenerateEvent)) {
        if !self.think.is_empty() {
            push_stream(on_event, call, "think", std::mem::take(&mut self.think));
        }
        if !self.content.is_empty() {
            push_stream(on_event, call, "content", std::mem::take(&mut self.content));
        }
        self.last_flush = std::time::Instant::now();
    }
}

fn push_stream(on_event: &dyn Fn(GenerateEvent), call: &str, channel: &str, delta: String) {
    on_event(GenerateEvent {
        phase: "outlining".into(),
        current: 0,
        total: 0,
        message: String::new(),
        prompt_tokens: 0,
        completion_tokens: 0,
        pages: None,
        repo_id: None,
        branch: None,
        stream: Some(crate::generator::StreamChunk {
            call: call.to_string(),
            channel: channel.into(),
            delta,
            page_id: None,
            repo_id: None,
            branch: None,
        }),
    });
}

/// 事件助手：终端浮窗流一行 meta
fn stream_note(on_event: &dyn Fn(GenerateEvent), text: String) {
    on_event(GenerateEvent {
        phase: "outlining".into(),
        current: 0,
        total: 0,
        message: String::new(),
        prompt_tokens: 0,
        completion_tokens: 0,
        pages: None,
        repo_id: None,
        branch: None,
        stream: Some(crate::generator::StreamChunk {
            call: "大纲 agent".into(),
            channel: "meta".into(),
            delta: format!("{text}\n"),
            page_id: None,
            repo_id: None,
            branch: None,
        }),
    });
}

/// Agent 大纲主入口：多轮探查 → 最终大纲 XML 文本（解析归一化由调用方做）
pub async fn run_agent_outline(
    svc: &AppService,
    client: &LlmClient,
    repo_id: i64,
    repo_name: &str,
    repo_path: &str,
    initial_user: &str,
    on_event: &(dyn Fn(GenerateEvent) + Send + Sync),
) -> CoreResult<String> {
    let root = PathBuf::from(repo_path);
    // search 范围 = 当前分支索引收录清单（已含排除体系治理）
    let branch = crate::branch::current_branch(&root);
    let known_files: Vec<String> = svc
        .db
        .get_index_files_by_repo(repo_id, &branch)?
        .into_iter()
        .map(|(p, _, _, _, _)| p)
        .filter(|p| p.ends_with(".rs") || p.ends_with(".py") || p.ends_with(".ts") || p.ends_with(".tsx")
            || p.ends_with(".js") || p.ends_with(".jsx") || p.ends_with(".go") || p.ends_with(".java")
            || p.ends_with(".md") || p.ends_with(".toml") || p.ends_with(".json") || p.ends_with(".yml")
            || p.ends_with(".yaml") || p.ends_with(".vue"))
        .collect();
    // 模块统计现取（入度信号）
    let module_stats = {
        let files = svc.db.get_index_files_by_repo(repo_id, &branch)?;
        let edges = svc.db.get_index_edges_by_repo(repo_id, &branch)?;
        let mods: Vec<(String, usize, Vec<String>)> = {
            let mut by_mod: HashMap<String, (usize, std::collections::BTreeSet<String>)> = HashMap::new();
            for (_p, lang, _, module, _) in &files {
                let e = by_mod.entry(module.clone()).or_default();
                e.0 += 1;
                e.1.insert(lang.to_string());
            }
            let mut v: Vec<_> = by_mod.into_iter().map(|(k, (c, l))| (k, c, l.into_iter().collect())).collect();
            v.sort_by(|a, b| a.0.cmp(&b.0));
            v
        };
        crate::generator::outline::render_modules(&mods, &edges)
    };

    let mut ctx = ToolCtx {
        root,
        known_files,
        module_stats,
        read_total: 0,
    };

    let system = concat!(
        "你是资深软件架构师，为代码仓库设计技术 Wiki 大纲。你可以调用只读工具自主探查仓库：",
        "先用 list_symbols/search 快速定位核心机制，再精读关键文件片段，",
        "探查充分后输出最终大纲 XML（格式见用户消息）。\n",
        "纪律：工具参数里的路径必须是仓库相对路径；每轮优先选信息密度最高的动作，",
        "不要逐文件通读全仓；输出大纲前必须停止调用工具。"
    );
    let mut history = vec![
        Message::system(system),
        Message::user(initial_user.to_string()),
    ];

    let defs = tool_defs();
    let mut rounds = 0usize;
    #[allow(unused_assignments)]
    let mut final_text = String::new();
    let mut tokens = (0u64, 0u64);

    loop {
        if rounds >= MAX_ROUNDS {
            stream_note(on_event, "⚠ 轮次预算用尽，要求基于已有信息输出大纲".into());
            history.push(Message::user(
                "【预算用尽】请立即基于已掌握的信息输出最终大纲 XML，不要再调用任何工具。".to_string(),
            ));
        }
        let round_label = format!("大纲 agent 轮次 {rounds}");
        // 回调签名要求 Fn：内部可变性走 Mutex（单消费者，锁无争用）
        let dbuf = std::sync::Mutex::new(DeltaBuf::new());
        let flush_buf = |dbuf: &std::sync::Mutex<DeltaBuf>, force: bool| {
            let mut b = dbuf.lock().unwrap();
            if force {
                b.flush(&round_label, on_event);
            } else {
                b.maybe_flush(&round_label, on_event);
            }
        };
        let on_text = |d: &str| {
            dbuf.lock().unwrap().push("content", d);
            flush_buf(&dbuf, false);
        };
        let on_thinking = |d: &str| {
            dbuf.lock().unwrap().push("think", d);
            flush_buf(&dbuf, false);
        };
        let resp: CompletionResponse = client
            .completion_round_stream(history.clone(), defs.clone(), &on_text, &on_thinking)
            .await
            .map_err(|e| {
                stream_note(on_event, format!("✘ 大纲 agent 第 {} 轮调用失败：{e}", rounds + 1));
                e
            })?;
        flush_buf(&dbuf, true);
        tokens.0 += resp.usage.input_tokens;
        tokens.1 += resp.usage.output_tokens;
        rounds += 1;

        let choice = resp.choice.clone();
        let tool_calls: Vec<&ToolCall> = choice
            .iter()
            .filter_map(|c| match c {
                AssistantContent::ToolCall(tc) => Some(tc),
                _ => None,
            })
            .collect();
        let text: String = choice
            .iter()
            .filter_map(|c| match c {
                AssistantContent::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect();

        // 模型日志：每轮一条（工具参数与结果摘要在 title/delta 里体现）
        let mut round_title = format!("Wiki 大纲 agent 轮次 {rounds}");
        if !tool_calls.is_empty() {
            let names: Vec<String> = tool_calls
                .iter()
                .map(|tc| {
                    let p = tc.function.arguments.get("path").or_else(|| tc.function.arguments.get("pattern"))
                        .and_then(|v| v.as_str()).unwrap_or("");
                    format!("{}({p})", tc.function.name)
                })
                .collect();
            round_title = format!("Wiki 大纲 agent 轮次 {rounds}：{}", names.join("、"));
        }
        stream_note(on_event, format!("▶ {round_title}{}", if text.is_empty() { String::new() } else { " → 输出大纲".to_string() }));

        // 回填 assistant 消息（含 tool call），再补工具结果
        history.push(Message::Assistant { id: None, content: choice.clone() });
        let budget_hit = ctx.read_total >= READ_BUDGET_CHARS;
        let had_tool_calls = !tool_calls.is_empty();
        for tc in tool_calls {
            let name = tc.function.name.clone();
            let result = exec_tool(&mut ctx, &name, &tc.function.arguments);
            history.push(Message::User {
                content: vec![UserContent::tool_result_for(
                    tc.id.clone(),
                    tc.provider.clone(),
                    name.clone(),
                    vec![ToolResultContent::Text(result.into())],
                )],
            });
        }
        if text.is_empty() && !had_tool_calls {
            return Err(CoreError::Other(anyhow::anyhow!("大纲 agent 空响应")));
        }
        if !text.is_empty() && !had_tool_calls {
            final_text = text;
            break;
        }
        if budget_hit || ctx.read_total >= READ_BUDGET_CHARS {
            stream_note(on_event, format!("⚠ 读取预算触顶（{:.0}k 字符），要求收尾", ctx.read_total as f64 / 1000.0));
            history.push(Message::user(
                "【预算用尽】请立即基于已掌握的信息输出最终大纲 XML，不要再调用任何工具。".to_string(),
            ));
        }
        // 保险：进入下一轮前如果已给出完整文本片段（部分模型文本+工具同轮），优先收文本
        if !text.is_empty() && text.contains("<wiki_structure>") {
            final_text = text;
            break;
        }
    }

    // 汇总日志一条（token 合计）
    svc.append_llm_log(&crate::llm_log::LlmLogEntry {
        ts: crate::db::now_iso(),
        repo_id,
        repo_name: repo_name.to_string(),
        branch,
        stage: "outline".into(),
        provider: client.provider_desc(),
        model: client.model_name().to_string(),
        title: format!("Wiki 大纲 agent（{rounds} 轮，读取 {:.0}k 字符）", ctx.read_total as f64 / 1000.0),
        attempt: 1,
        duration_ms: 0,
        prompt_tokens: tokens.0,
        completion_tokens: tokens.1,
        ok: true,
        cached: false,
        error: String::new(),
        system: system.into(),
        user: initial_user.to_string(),
        response: final_text.clone(),
    });
    Ok(final_text)
}

// ============ 测试 ============

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_rejects_escape() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "x").unwrap();
        assert!(resolve_within(dir.path(), "a.txt").is_ok());
        assert!(resolve_within(dir.path(), "../a.txt").is_err());
        assert!(resolve_within(dir.path(), "/etc/passwd").is_err());
        assert!(resolve_within(dir.path(), "C:/Windows/system32").is_err());
    }

    #[test]
    fn read_file_caps_lines_and_counts_budget() {
        let dir = tempfile::tempdir().unwrap();
        let mut body = String::new();
        for i in 0..1000 {
            body.push_str(&format!("line-{i}\n"));
        }
        std::fs::write(dir.path().join("big.txt"), body).unwrap();
        let mut ctx = ToolCtx {
            root: dir.path().to_path_buf(),
            known_files: vec!["big.txt".into()],
            module_stats: String::new(),
            read_total: 0,
        };
        let out = exec_read_file(&mut ctx, &json!({"path": "big.txt"}));
        assert!(out.contains("1| line-0"));
        assert!(out.contains("共 1000 行"));
        assert!(ctx.read_total > 0);
        // 显式区间
        let out2 = exec_read_file(&mut ctx, &json!({"path": "big.txt", "start_line": 10, "end_line": 12}));
        assert!(out2.contains("10| line-9"));
        assert!(out2.contains("12| line-11"));
    }

    #[test]
    fn symbols_and_search_work() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("m.py"),
            "def handler():\n    pass\n\nclass Svc:\n    def run(self):\n        pass\n",
        )
        .unwrap();
        let ctx = ToolCtx {
            root: dir.path().to_path_buf(),
            known_files: vec!["m.py".into()],
            module_stats: String::new(),
            read_total: 0,
        };
        let sym = exec_list_symbols(&ctx, &json!({"path": "m.py"}));
        assert!(sym.contains("handler"), "实际：{sym}");
        assert!(sym.contains("Svc.run"), "实际：{sym}");
        let hit = exec_search(&ctx, &json!({"pattern": "handler"}));
        assert!(hit.contains("m.py:1"), "实际：{hit}");
    }
}
