//! LSP definition 精确化（P3）：对 heuristic/receiver 级调用边发
//! `textDocument/definition`（用边自带的 line/col 对准被调名 token），命中图内
//! 节点即把边升级为 `precise`（目标不一致时改写 dst）——tree-sitter 启发式消歧的
//! 置信天花板由语言服务器补足。rust-analyzer 起步（仅精化 Rust 文件的边）。
//!
//! 取舍（对齐 codegraph-rust 的 LSP tier 与 codegraph 的「错边比缺边糟」）：
//! - **只升级不删除**：definition 落在图外（std/外部 crate）时保留原启发边
//!   （kind 仍标 heuristic，不冒充精确）——可见性诚实优先于激进修正；
//! - byte 列 → UTF-16 列换算是 LSP position 的硬要求（tree-sitter 列是字节偏移）；
//! - 工具缺失 fail-fast（提示安装 rust-analyzer 或设 BYTEWIKI_LSP），不静默降级；
//! - 只精化当前分支工作区（LSP 读磁盘实时文件，跨分支快照精化没有意义）。
//!
//! 本模块 ~300 行自研 stdio JSON-RPC client，无 LSP 框架依赖（同 codegraph-rust
//! 的路线：请求串行、通知忽略、逐文件 didOpen）。

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::{json, Value};

use crate::callchain_db::{load_callgraph_db, GEdge, GraphData, CALLGRAPH_DB_FILE};
use crate::service::AppService;
use crate::{CoreError, CoreResult};

/// 精化结果摘要
#[derive(Debug, Clone, serde::Serialize)]
pub struct LspRefineSummary {
    /// 参与精化的候选边（heuristic/receiver 且有列坐标、Rust 文件）
    pub targets: usize,
    /// LSP 给出定义且能映射回图内节点
    pub resolved: usize,
    /// 升级为 precise 的边数（含目标改写）
    pub upgraded: usize,
    /// 其中目标被 LSP 改写的条数（原启发边连错人）
    pub retargeted: usize,
    /// LSP 无定义 / 落点不在图内（保留原启发边）
    pub unresolved: usize,
    pub duration_ms: u128,
}

// ============ URI / position 换算 ============

/// 路径 → file:// URI（UTF-8 字节百分号编码；Windows 盘符保留）
fn path_to_uri(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    // Unix 绝对路径自带前导 /（file:// + /home → file:///home）；Windows 盘符补三斜杠
    let mut out = if s.starts_with('/') { String::from("file://") } else { String::from("file:///") };
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn percent_decode(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if input[i] == b'%' && i + 3 <= input.len() {
            if let Ok(v) = u8::from_str_radix(std::str::from_utf8(&input[i + 1..i + 3]).unwrap_or(""), 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(input[i]);
        i += 1;
    }
    out
}

/// file:// URI → 路径（Windows 盘符形态 `file:///C:/x` 与 Unix `file:///abs` 都认）
fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let rest = match rest.as_bytes() {
        // /C:/x → C:/x（Windows 盘符）
        [b'/', _, b':', ..] => &rest[1..],
        _ => rest,
    };
    let s = String::from_utf8(percent_decode(rest.as_bytes())).ok()?;
    Some(PathBuf::from(s))
}

/// tree-sitter 字节列 → LSP UTF-16 列（position.character 的单位是 UTF-16 code unit）
fn byte_col_to_utf16(line: &str, byte_col: usize) -> u32 {
    let mut units = 0u32;
    for (i, ch) in line.char_indices() {
        if i >= byte_col {
            break;
        }
        units += ch.len_utf16() as u32;
    }
    units
}

/// definition 响应 → (uri, 1-based 行)。认 Location / Location[] / LocationLink[]
fn def_location(result: &Value) -> Option<(String, u64)> {
    let first = match result {
        Value::Null => return None,
        Value::Array(a) => a.first()?,
        v => v,
    };
    if let Some(uri) = first.get("targetUri") {
        let line = first["targetSelectionRange"]["start"]["line"].as_u64()? + 1;
        return Some((uri.as_str()?.to_string(), line));
    }
    let uri = first["uri"].as_str()?;
    let line = first["range"]["start"]["line"].as_u64()? + 1;
    Some((uri.to_string(), line))
}

/// 定义落点 → 图内节点：文件相同且行落在函数跨度内者取最窄（嵌套函数取内层）
fn node_at<'a>(g: &'a GraphData, file: &str, line: u64) -> Option<&'a crate::callchain_db::GNode> {
    g.nodes
        .iter()
        .filter(|n| n.file == file && n.lineno <= line && line <= n.end_lineno)
        .min_by_key(|n| n.end_lineno - n.lineno)
}

// ============ 极简 LSP client（stdio JSON-RPC，串行请求） ============

struct LspClient {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    /// 后台读线程 → 调用方的消息通道（阻塞读不卡调用方：recv_timeout 兜底）
    rx: std::sync::mpsc::Receiver<Value>,
    next_id: i64,
}

/// 读一条 LSP 消息（Content-Length 帧）；EOF（服务退出）返回 None
fn read_message(r: &mut BufReader<std::process::ChildStdout>) -> CoreResult<Option<Value>> {
    let mut content_len: Option<usize> = None;
    loop {
        let mut line = String::new();
        let n = r.read_line(&mut line)?;
        if n == 0 {
            return Ok(None);
        }
        let t = line.trim_end();
        if t.is_empty() {
            break;
        }
        if let Some(v) = t.strip_prefix("Content-Length:") {
            content_len = Some(v.trim().parse().map_err(|_| {
                CoreError::Other(anyhow::anyhow!("LSP 响应 Content-Length 非法：{t}"))
            })?);
        }
    }
    let len = content_len.ok_or_else(|| CoreError::Other(anyhow::anyhow!("LSP 消息缺 Content-Length")))?;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(Some(serde_json::from_slice(&buf)?))
}

impl LspClient {
    /// 启动语言服务器。命令名可用 BYTEWIKI_LSP 覆盖（默认 rust-analyzer）
    fn start(root: &Path) -> CoreResult<Self> {
        let exe = std::env::var("BYTEWIKI_LSP").unwrap_or_else(|_| "rust-analyzer".into());
        let mut child = std::process::Command::new(&exe)
            .current_dir(root)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| {
                CoreError::Other(anyhow::anyhow!(
                    "无法启动 {exe}：{e}（Rust 精确化需 rust-analyzer，或设 BYTEWIKI_LSP 指向兼容服务）"
                ))
            })?;
        let stdin = child.stdin.take().expect("stdin piped");
        let mut stdout = BufReader::new(child.stdout.take().expect("stdout piped"));
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(Some(msg)) = read_message(&mut stdout) {
                if tx.send(msg).is_err() {
                    break;
                }
            }
        });
        Ok(Self { child, stdin, rx, next_id: 0 })
    }

    fn send(&mut self, msg: &Value) -> CoreResult<()> {
        let body = serde_json::to_string(msg)?;
        write!(self.stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body)?;
        self.stdin.flush()?;
        Ok(())
    }

    fn notify(&mut self, method: &str, params: Value) -> CoreResult<()> {
        self.send(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    /// 串行请求：忽略服务端通知（进度/诊断），等回同 id 响应；单请求 60s 超时
    fn request(&mut self, method: &str, params: Value) -> CoreResult<Value> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        let deadline = Instant::now() + std::time::Duration::from_secs(60);
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(CoreError::Other(anyhow::anyhow!("LSP 请求超时（{method}）")));
            }
            let msg = match self.rx.recv_timeout(deadline - now) {
                Ok(m) => m,
                Err(_) => return Err(CoreError::Other(anyhow::anyhow!("LSP 请求超时（{method}）"))),
            };
            if msg.get("method").is_some() && msg.get("id").is_none() {
                continue; // notification：$/progress、publishDiagnostics 等
            }
            if msg.get("id").and_then(|i| i.as_i64()) == Some(id) {
                if let Some(err) = msg.get("error") {
                    return Err(CoreError::Other(anyhow::anyhow!("LSP {method} 错误：{err}")));
                }
                return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
            }
        }
    }

    /// initialize 握手（声明 UTF-16 position 编码）
    fn initialize(&mut self, root: &Path) -> CoreResult<()> {
        let uri = path_to_uri(root);
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "workspace".into());
        let _ = self.request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": uri,
                "workspaceFolders": [{"uri": uri, "name": name}],
                "capabilities": {
                    "general": {"positionEncodings": ["utf-16"]},
                    "textDocument": {"definition": {"linkSupport": false}},
                    "experimental": {"serverStatusNotification": true}
                }
            }),
        )?;
        self.notify("initialized", json!({}))?;
        Ok(())
    }

    /// didOpen 一份文档（definition 要求文档处于打开态）
    fn open_file(&mut self, uri: &str, text: &str) -> CoreResult<()> {
        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri, "languageId": "rust", "version": 1, "text": text
                }
            }),
        )
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ============ 精化流程 ============

impl AppService {
    /// LSP 精确化：把当前分支产物里 heuristic/receiver 级的 Rust 调用边升级为
    /// precise（`textDocument/definition` 命中图内节点时）。完成后双写回
    /// callgraph.db + callgraph.json（与生成路径同一落库口径）。
    ///
    /// `on_progress(已完成, 总数)` 每 50 条回调一次。
    pub fn refine_callgraph_lsp(
        &self,
        repo_path: &str,
        on_progress: &(dyn Fn(usize, usize) + Send + Sync),
    ) -> CoreResult<LspRefineSummary> {
        let t0 = Instant::now();
        let root = Path::new(repo_path);
        let branch = crate::branch::current_branch(root);
        let dir = crate::branch::wiki_root(root, &branch)
            .join("graphs")
            .join("callchain");
        let db_path = dir.join(CALLGRAPH_DB_FILE);
        let json_path = dir.join("callgraph.json");
        if !db_path.is_file() && !json_path.is_file() {
            return Err(CoreError::Other(anyhow::anyhow!("尚未生成调用链，无可精化产物")));
        }

        let mut g: GraphData = if db_path.is_file() {
            load_callgraph_db(&db_path)?
        } else {
            crate::callchain_db::graph_from_doc_json(&serde_json::from_str(&std::fs::read_to_string(
                &json_path,
            )?)?)
        };

        // 候选：heuristic/receiver 边 + 有列坐标 + 调用方是 Rust 文件
        //（rust-analyzer 只懂 Rust；其他语言待各自 server 接入）
        let node_file: HashMap<&str, &str> = g
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), n.file.as_str()))
            .collect();
        let mut targets: Vec<(String, String, u64, u64, String)> = Vec::new();
        for e in &g.edges {
            if matches!(e.kind.as_str(), "heuristic" | "receiver") && e.col > 0 {
                if let Some(file) = node_file.get(e.src.as_str()) {
                    if file.ends_with(".rs") {
                        targets.push((e.src.clone(), e.dst.clone(), e.line, e.col, (*file).to_string()));
                    }
                }
            }
        }
        if targets.is_empty() {
            return Ok(LspRefineSummary {
                targets: 0,
                resolved: 0,
                upgraded: 0,
                retargeted: 0,
                unresolved: 0,
                duration_ms: t0.elapsed().as_millis(),
            });
        }

        let mut client = LspClient::start(root)?;
        client.initialize(root)?;

        // 就绪探针：同文件 local 边的调用点是**语义真值**（RA 分析完必然答得出）——
        // 反复问它的 definition，非空即语义分析就绪。documentSymbol 是句法级、会
        // 早出（实测 248ms 就有符号但 definition 仍空），serverStatus 静默不可靠，
        // 问一个必答的真问题最稳。无 local 边时退化为首个精化目标（可能空转到
        // 上限，但不会误判就绪）。
        let probe: (String, u64, u64) = g
            .edges
            .iter()
            .find(|e| {
                e.kind == "local"
                    && e.col > 0
                    && e.line > 0
                    && node_file
                        .get(e.src.as_str())
                        .map(|f| f.ends_with(".rs"))
                        .unwrap_or(false)
            })
            .map(|e| (node_file[e.src.as_str()].to_string(), e.line, e.col))
            .or_else(|| targets.first().map(|(_, _, l, c, f)| (f.clone(), *l, *c)))
            .expect("targets 非空");

        // 按文件汇总调用点：一次 didOpen，多条 definition（探针文件一并打开）
        let mut file_lines: HashMap<String, Vec<String>> = HashMap::new();
        let mut open_uri: HashMap<String, String> = HashMap::new();
        let open_files: std::collections::HashSet<&str> = targets
            .iter()
            .map(|(_, _, _, _, f)| f.as_str())
            .chain(std::iter::once(probe.0.as_str()))
            .collect();
        for file in open_files {
            let text = std::fs::read_to_string(root.join(file)).map_err(|e| {
                CoreError::Other(anyhow::anyhow!("读取 {file} 失败（LSP 精确化需要工作区实时文件）：{e}"))
            })?;
            let uri = path_to_uri(&root.join(file));
            client.open_file(&uri, &text)?;
            open_uri.insert(file.to_string(), uri);
            file_lines.insert(file.to_string(), text.split('\n').map(|s| s.to_string()).collect());
        }
        {
            let (pfile, pline, pcol) = &probe;
            let puri = open_uri[pfile].clone();
            let ptext = file_lines
                .get(pfile)
                .and_then(|ls| ls.get((*pline as usize).saturating_sub(1)))
                .map(|s| s.as_str())
                .unwrap_or("");
            let pch = byte_col_to_utf16(ptext, *pcol as usize);
            // 真仓冷启动（cargo metadata + 依赖 proc-macro 构建）可达分钟级——
            // 探针预算 120s；就绪后 definition 即秒回
            for _ in 0..240 {
                let ready = client
                    .request(
                        "textDocument/definition",
                        json!({
                            "textDocument": {"uri": puri},
                            "position": {"line": pline.saturating_sub(1), "character": pch}
                        }),
                    )
                    .map(|v| def_location(&v).is_some())
                    .unwrap_or(false);
                if ready {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }

        // (src, old_dst) → Some(new_dst) / None 未解析（保留原启发边）
        let mut fix: HashMap<(String, String), Option<String>> = HashMap::new();
        for (i, (src, dst, line, col, file)) in targets.iter().enumerate() {
            if (i + 1) % 50 == 0 {
                on_progress(i + 1, targets.len());
            }
            let uri = open_uri[file].clone();
            let line_text = file_lines
                .get(file)
                .and_then(|ls| ls.get((*line as usize).saturating_sub(1)))
                .map(|s| s.as_str())
                .unwrap_or("");
            let ch = byte_col_to_utf16(line_text, *col as usize);
            // content modified 等服务端瞬态错误（-32801）重试 2 次
            let mut result: Option<Value> = None;
            for attempt in 0..3 {
                match client.request(
                    "textDocument/definition",
                    json!({
                        "textDocument": {"uri": uri},
                        "position": {"line": line.saturating_sub(1), "character": ch}
                    }),
                ) {
                    Ok(v) => {
                        result = Some(v);
                        break;
                    }
                    Err(e) if attempt < 2 => {
                        tracing::debug!("LSP definition 瞬态失败（{file}:{line}），重试：{e}");
                        std::thread::sleep(std::time::Duration::from_millis(300));
                    }
                    Err(e) => {
                        tracing::debug!("LSP definition 请求失败（{file}:{line}）：{e}");
                        break;
                    }
                }
            }
            let hit = result.as_ref().and_then(|result| {
                def_location(result).and_then(|(uri, tline)| {
                    let p = uri_to_path(&uri)?;
                    // 落点文件 → 图内相对路径：后缀匹配节点文件表
                    //（绕开 canonicalize verbatim 前缀/盘符大小写的 strip_prefix 陷阱；
                    //  落点不在图内文件上本来就不收，匹配天然收敛）
                    let s = p.to_string_lossy().replace('\\', "/");
                    let rel = node_file
                        .values()
                        .find(|f| s.ends_with(&format!("/{f}")) || s.ends_with(*f))?;
                    let target = node_at(&g, rel, tline)?;
                    Some(target.id.clone())
                })
            });
            fix.insert((src.clone(), dst.clone()), hit);
        }
        on_progress(targets.len(), targets.len());

        // 应用：命中 → precise（dst 不一致则改写）；未命中 → 原边保留（kind 不动）
        let mut resolved = 0usize;
        let mut retargeted = 0usize;
        let mut new_edges: Vec<GEdge> = Vec::with_capacity(g.edges.len());
        for e in g.edges.drain(..) {
            match fix.get(&(e.src.clone(), e.dst.clone())) {
                Some(Some(new_dst)) => {
                    resolved += 1;
                    if *new_dst != e.dst {
                        retargeted += 1;
                    }
                    new_edges.push(GEdge {
                        src: e.src,
                        dst: new_dst.clone(),
                        line: e.line,
                        col: e.col,
                        kind: "precise".to_string(),
                    });
                }
                _ => new_edges.push(e),
            }
        }
        g.edges = new_edges;

        // 双写回产物（与生成路径同口径：db 主存储 + json 互操作导出）
        let doc = g.to_doc_json();
        std::fs::write(&json_path, serde_json::to_string_pretty(&doc)?)?;
        crate::callchain_db::write_callgraph_db(&db_path, &doc, &g.head_commit)?;

        Ok(LspRefineSummary {
            targets: targets.len(),
            resolved,
            upgraded: resolved,
            retargeted,
            unresolved: targets.len() - resolved,
            duration_ms: t0.elapsed().as_millis(),
        })
    }
}

// ============ 测试 ============

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_col_to_utf16_handles_multibyte() {
        // "let 中文 = foo()"：foo 的字节列 → UTF-16 列
        let line = "let 中文 = foo()";
        let byte_col = line.find("foo").unwrap();
        // 字节列 13（中文各 3 字节）→ UTF-16 列 9（BMP 字符各 1 单元）
        assert_eq!(byte_col_to_utf16(line, byte_col), 9);
        // emoji（代理对，占 2 单位）
        let line2 = "a😀b";
        assert_eq!(byte_col_to_utf16(line2, line2.find('b').unwrap()), 3);
        // 纯 ASCII 直通
        assert_eq!(byte_col_to_utf16("call(x)", 5), 5);
    }

    #[test]
    fn uri_roundtrip_windows_and_unix() {
        let p = PathBuf::from(r"C:\dev\my repo\src\main.rs");
        let uri = path_to_uri(&p);
        assert_eq!(uri, "file:///C:/dev/my%20repo/src/main.rs");
        assert_eq!(uri_to_path(&uri).unwrap(), PathBuf::from(r"C:\dev\my repo\src\main.rs"));

        let u = PathBuf::from("/home/u/proj/src/lib.rs");
        let uri2 = path_to_uri(&u);
        assert_eq!(uri2, "file:///home/u/proj/src/lib.rs");
        assert_eq!(uri_to_path(&uri2).unwrap(), u);
    }

    #[test]
    fn def_location_parses_all_shapes() {
        let loc = json!({"uri": "file:///C:/x/a.rs", "range": {"start": {"line": 4, "character": 3}, "end": {"line": 4, "character": 6}}});
        assert_eq!(def_location(&loc), Some(("file:///C:/x/a.rs".into(), 5)));
        let arr = json!([loc]);
        assert_eq!(def_location(&arr), Some(("file:///C:/x/a.rs".into(), 5)));
        let link = json!({"targetUri": "file:///C:/x/b.rs", "targetSelectionRange": {"start": {"line": 9, "character": 0}}});
        assert_eq!(def_location(&link), Some(("file:///C:/x/b.rs".into(), 10)));
        assert_eq!(def_location(&Value::Null), None);
    }

    #[test]
    fn node_at_prefers_innermost_span() {
        let g = GraphData {
            nodes: vec![
                crate::callchain_db::GNode {
                    id: "a::outer".into(),
                    module: "a".into(),
                    qualname: "outer".into(),
                    name: "outer".into(),
                    file: "a.rs".into(),
                    lineno: 1,
                    end_lineno: 20,
                    is_async: false,
                    is_entry: false,
                    is_exported: false,
                    routes: vec![],
                    callers: 0,
                    callees: 0,
                },
                crate::callchain_db::GNode {
                    id: "a::outer.inner".into(),
                    module: "a".into(),
                    qualname: "outer.inner".into(),
                    name: "inner".into(),
                    file: "a.rs".into(),
                    lineno: 5,
                    end_lineno: 8,
                    is_async: false,
                    is_entry: false,
                    is_exported: false,
                    routes: vec![],
                    callers: 0,
                    callees: 0,
                },
            ],
            ..Default::default()
        };
        assert_eq!(node_at(&g, "a.rs", 6).unwrap().id, "a::outer.inner", "嵌套取最窄");
        assert_eq!(node_at(&g, "a.rs", 12).unwrap().id, "a::outer");
        assert!(node_at(&g, "b.rs", 6).is_none());
        assert!(node_at(&g, "a.rs", 99).is_none());
    }

    #[test]
    fn percent_decode_mixed() {
        assert_eq!(percent_decode(b"a%20b%2Fc"), b"a b/c");
        assert_eq!(percent_decode(b"plain"), b"plain");
    }

    /// 真机端到端：rust-analyzer 把 heuristic 边升级为 precise
    ///（tempdir 小 cargo 工程 + 真 rust-analyzer 握手/definition 全链路）
    #[test]
    #[ignore = "真机验收：需本机 rust-analyzer"]
    fn refine_upgrades_heuristic_edges_with_real_ra() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(
            repo.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        // lib.rs 调用 util.rs 的唯一同名函数 → tree-sitter 只能给 heuristic 边；
        // use 语句只影响真实语义（RA 解析），不影响提取（裸调用仍走全局唯一归约）。
        // inner 是同文件 local 边——就绪探针用它（语义真值，RA 分析完必答）
        std::fs::write(
            repo.join("src/lib.rs"),
            "mod util;\nuse crate::util::helper;\n\npub fn run() {\n    inner(1);\n    helper(1);\n}\n\nfn inner(x: i32) -> i32 {\n    x * 2\n}\n",
        )
        .unwrap();
        std::fs::write(repo.join("src/util.rs"), "pub fn helper(x: i32) -> i32 {\n    x + 1\n}\n").unwrap();

        let svc = AppService::open_at(dir.path().join("app"), dir.path().join("data")).unwrap();
        svc.open_repo(repo.to_str().unwrap()).unwrap();
        let sum = svc.generate_callchain(repo.to_str().unwrap(), &|_| {}).unwrap();
        assert!(sum.edges >= 1, "应有 heuristic 边可精化：{sum:?}");

        let s = svc.refine_callgraph_lsp(repo.to_str().unwrap(), &|_, _| {}).unwrap();
        println!("精化摘要：{s:?}");
        assert!(s.upgraded >= 1, "heuristic 边应升级为 precise：{s:?}");
        let g = svc.read_callgraph(repo.to_str().unwrap(), None).unwrap().unwrap();
        let precise: Vec<&serde_json::Value> = g["edges"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "precise")
            .collect();
        assert!(!precise.is_empty(), "产物应带 precise 边：{g:?}");
    }

    /// 真仓综合验收（P1+P3）：byte-code-cli（Rust）上增量生成 + LSP 精化
    #[test]
    #[ignore = "真机验收：需本机存在靶子仓库"]
    fn real_repo_incremental_and_refine_smoke() {
        let path = r"C:\dev-data\code\cicbyte\byte-code-cli";
        if !Path::new(path).is_dir() {
            println!("[skip] {path} 不存在");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let svc = AppService::open_at(tmp.path().join("app"), tmp.path().join("data")).unwrap();
        svc.open_repo(path).unwrap();
        let sum = svc.generate_callchain(path, &|_| {}).unwrap();
        println!(
            "生成：{} 函数 / {} 边（解析 {} 复用 {}）",
            sum.nodes, sum.edges, sum.parsed_files, sum.reused_files
        );
        // 增量：未改动的第二次生成应零解析
        let sum2 = svc.generate_callchain(path, &|_| {}).unwrap();
        assert_eq!(sum2.parsed_files, 0, "增量：第二次生成应零解析：{sum2:?}");
        assert_eq!(sum2.reused_files, sum.reused_files, "复用面与文件面一致");

        let s = svc.refine_callgraph_lsp(path, &|_, _| {}).unwrap();
        println!("精化：{s:?}");
        assert!(s.upgraded > 0, "真仓应有 heuristic 边可升级：{s:?}");
    }
}
