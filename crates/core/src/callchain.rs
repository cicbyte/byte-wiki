//! 函数级调用链（AppService 集成层）
//!
//! core 无条件依赖 `byte-wiki-callchain` 引擎（定位与决策见 dev-docs v2）：
//! 这里做业务包装——仓库校验、排除目录口径统一（与索引一致）、
//! 产物落 `.bytewiki/graphs/callchain/`、wiki_docs 登记（kind=graph）。
//!
//! 产物约定：app 管线 **不内嵌 source**（详情面板 Monaco 读实时文件），
//! standalone `byte-callchain` 导出才带 source。

use std::collections::BTreeMap;
use std::path::Path;

use crate::service::AppService;
use crate::{CoreError, CoreResult};

/// 调用链产物在 .bytewiki/ 内的相对路径（wiki_docs.path 口径）
pub const CALLGRAPH_DOC_PATH: &str = "graphs/callchain/callgraph.json";

/// 生成结果摘要（CLI --json / 桌面端进度完成事件共用）
#[derive(Debug, Clone, serde::Serialize)]
pub struct CallchainSummary {
    /// callgraph.json 绝对路径
    pub out_path: String,
    pub nodes: usize,
    pub edges: usize,
    pub entries: usize,
    pub modules: usize,
    /// 语言 → 解析文件数
    pub langs: BTreeMap<String, usize>,
    pub duration_ms: u128,
}

/// 走读链（服务端 DFS：语义与前端 buildChain 一致——先序、子节点按首次
/// 调用行排序、已访问仅作引用、主路径贪心最深子树、环防护）。返回轻量
/// 链元数据（不含源码，源码走既有 getChainSources 按需批量取）。
#[derive(serde::Serialize)]
pub struct ChainCallLink {
    pub target_id: String,
    pub target_num: String,
    pub target_name: String,
    pub line: u64,
    /// 引用（目标已在前文渲染）vs 新块
    pub is_ref: bool,
}

#[derive(serde::Serialize)]
pub struct ChainEntry {
    pub id: String,
    pub num: String,
    pub node: serde_json::Value,
    pub calls: Vec<ChainCallLink>,
}

#[derive(serde::Serialize)]
pub struct CallgraphChain {
    pub root: String,
    pub entries: Vec<ChainEntry>,
    pub main_path: Vec<String>,
    pub truncated: bool,
}

/// 调用图概览（大图模式：全图由后端持有，前端只拿轻量负载 <200KB）
    #[derive(serde::Serialize)]
pub struct CallgraphOverview {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub total_entries: usize,
        /// 入口面板清单（is_entry 节点；零入口库回退公共 API 面 top30）
    pub entries: Vec<serde_json::Value>,
        /// 是否公共 API 回退（前端分组标题用）
    pub entries_are_api: bool,
        /// 模块列表
    pub modules: Vec<String>,
        /// 模块聚合边（模块全景图：u/v/w，按权重 top 截断）
    pub module_edges: Vec<(String, String, usize)>,
    }


impl AppService {
    /// 为已登记仓库生成函数级调用图（tree-sitter 多语言）。
    /// `on_progress` 回调已解析文件数（引擎每 20 个文件一次 + 收尾一次）。
    pub fn generate_callchain(
        &self,
        repo_path: &str,
        on_progress: &(dyn Fn(usize) + Send + Sync),
    ) -> CoreResult<CallchainSummary> {
        let t0 = std::time::Instant::now();
        let root = Path::new(repo_path);
        if !root.join(".git").exists() {
            return Err(CoreError::Other(anyhow::anyhow!("不是 Git 仓库：{repo_path}")));
        }
        let Some(repo) = self.db.get_repo_by_path(&crate::canonical_key(root)?)? else {
            return Err(CoreError::Other(anyhow::anyhow!("仓库未登记，请先打开：{repo_path}")));
        };

        // 排除目录：与 index_repo 同口径（内置表 + 用户偏好）
        let mut excludes: std::collections::HashSet<String> =
            crate::indexer::EXTRA_EXCLUDES.iter().map(|s| s.to_string()).collect();
        if let Ok(cfg) = self.config() {
            excludes.extend(cfg.preferences.exclude_dirs);
        }
        // 规则管理：文件排除 / 入口排除 / 自定义入口正则（config preferences.callchain）
        let rules = self
            .config()
            .map(|c| c.preferences.callchain)
            .unwrap_or_default();
        let entry_re = rules
            .entry_regex
            .as_deref()
            .and_then(|p| regex::Regex::new(p).ok());
        let opts = byte_wiki_callchain::ExtractOptions {
            entry_regex: entry_re,
            excludes: excludes.into_iter().collect(),
            embed_source: false,
            exclude_globs: rules.exclude_globs,
            entry_exclude_globs: rules.entry_exclude_globs,
        };

        let graph = byte_wiki_callchain::extract(root, &opts, on_progress)
            .map_err(|e| CoreError::Other(anyhow::anyhow!("调用链提取失败：{e}")))?;

        // 产物写入仓库内（随仓库共享，FR-08）
        let out_dir = crate::branch::wiki_root(root, &crate::branch::current_branch(root))
            .join("graphs")
            .join("callchain");
        std::fs::create_dir_all(&out_dir)?;
        let out_path = out_dir.join("callgraph.json");
        std::fs::write(&out_path, serde_json::to_string_pretty(&graph)?)?;

        // 登记 wiki_docs（kind=graph；upsert——wiki 重新生成的整体替换后由 assemble 重新补登）
        self.db
            .upsert_generated_doc(repo.id, &crate::branch::current_branch(std::path::Path::new(&repo_path)), "graph", CALLGRAPH_DOC_PATH, "调用链图谱")?;
        self.db.log_change(
            repo.id,
            &crate::branch::current_branch(std::path::Path::new(&repo_path)),
            "callchain",
            &format!("nodes={} edges={}", graph["nodes"].as_array().map(|a| a.len()).unwrap_or(0), graph["edges"].as_array().map(|a| a.len()).unwrap_or(0)),
        )?;

        let langs: BTreeMap<String, usize> = graph["langs"]
            .as_object()
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_u64().unwrap_or(0) as usize)).collect())
            .unwrap_or_default();
        Ok(CallchainSummary {
            out_path: out_path.to_string_lossy().into_owned(),
            nodes: graph["nodes"].as_array().map(|a| a.len()).unwrap_or(0),
            edges: graph["edges"].as_array().map(|a| a.len()).unwrap_or(0),
            entries: graph["nodes"]
                .as_array()
                .map(|a| a.iter().filter(|n| n["is_entry"].as_bool().unwrap_or(false)).count())
                .unwrap_or(0),
            modules: graph["modules"].as_object().map(|m| m.len()).unwrap_or(0),
            langs,
            duration_ms: t0.elapsed().as_millis(),
        })
    }

    /// 大图模式下后端持全图、按需取子图 —— 概览不含 26k 节点清单
    pub fn callgraph_overview(&self, repo_path: &str) -> CoreResult<Option<CallgraphOverview>> {
        let Some(g) = self.read_callgraph(repo_path)? else {
            return Ok(None);
        };
        let nodes = g["nodes"].as_array().cloned().unwrap_or_default();
        let edges = g["edges"].as_array().cloned().unwrap_or_default();
        let mut indeg: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut mod_edges: std::collections::BTreeMap<(String, String), usize> = std::collections::BTreeMap::new();
        let node_mod: std::collections::HashMap<String, String> = nodes
            .iter()
            .map(|n| {
                (
                    n["id"].as_str().unwrap_or("").to_string(),
                    n["module"].as_str().unwrap_or("").to_string(),
                )
            })
            .collect();
        for e in &edges {
            let (s, d) = (
                e["src"].as_str().unwrap_or("").to_string(),
                e["dst"].as_str().unwrap_or("").to_string(),
            );
            *indeg.entry(d.clone()).or_default() += 1;
            if let (Some(m1), Some(m2)) = (node_mod.get(&s), node_mod.get(&d.clone())) {
                if m1 != m2 {
                    *mod_edges.entry((m1.clone(), m2.clone())).or_default() += 1;
                }
            }
        }
        let light = |n: &serde_json::Value| {
            serde_json::json!({
                "id": n["id"], "module": n["module"], "qualname": n["qualname"],
                "name": n["name"], "file": n["file"], "routes": n["routes"],
                "is_exported": n.get("is_exported").unwrap_or(&serde_json::json!(false)),
                "is_entry": n["is_entry"],
            })
        };
        let mut entries: Vec<serde_json::Value> =
            nodes.iter().filter(|n| n["is_entry"] == true).map(light).collect();
        let mut entries_are_api = false;
        if entries.is_empty() {
            // 库回退：公共 API（门面文件导出优先）top 30，测试调用量参考
            entries_are_api = true;
            let is_facade = |n: &serde_json::Value| {
                n["file"].as_str().map(|f| {
                    let base = f.rsplit(['/', '\\']).next().unwrap_or(f);
                    let stem = base.split('.').next().unwrap_or(base);
                    matches!(stem, "index" | "mod" | "lib" | "main" | "__init__")
                }).unwrap_or(false)
            };
            let mut pool: Vec<&serde_json::Value> = nodes
                .iter()
                .filter(|n| n.get("is_exported").and_then(|v| v.as_bool()).unwrap_or(false))
                .collect();
            if pool.is_empty() {
                pool = nodes.iter().collect();
            }
            pool.sort_by_key(|n| {
                let id = n["id"].as_str().unwrap_or("");
                (
                    std::cmp::Reverse(is_facade(n)),
                    std::cmp::Reverse(indeg.get(id).copied().unwrap_or(0)),
                )
            });
            entries = pool.into_iter().take(30).map(light).collect();
        }
        entries.truncate(200);
        let mut me: Vec<(String, String, usize)> = mod_edges
            .into_iter()
            .map(|((a, b), w)| (a, b, w))
            .collect();
        me.sort_by_key(|(_, _, w)| std::cmp::Reverse(*w));
        me.truncate(250);
        let mut modules: Vec<String> = node_mod.into_values().collect();
        modules.sort();
        modules.dedup();
        // 大图模块可达数千——前端下拉用不了那么多，截断控负载
        modules.truncate(500);
        let total_entries = entries_len(&nodes);
        Ok(Some(CallgraphOverview {
            total_nodes: nodes.len(),
            total_edges: edges.len(),
            total_entries,
            entries,
            entries_are_api,
            modules,
            module_edges: me,
        }))
    }

    /// BFS 子图裁剪：seed 节点 N 跳双向邻域，节点数封顶 cap
    pub fn callgraph_focus(
        &self,
        repo_path: &str,
        seed: &str,
        depth: usize,
        cap: usize,
    ) -> CoreResult<Option<serde_json::Value>> {
        let Some(g) = self.read_callgraph(repo_path)? else {
            return Ok(None);
        };
        let nodes = g["nodes"].as_array().cloned().unwrap_or_default();
        let edges = g["edges"].as_array().cloned().unwrap_or_default();
        let start_id: String = nodes
            .iter()
            .find(|n| n["id"].as_str() == Some(seed))
            .map(|n| n["id"].as_str().unwrap_or("").to_string())
            .ok_or_else(|| crate::CoreError::Other(anyhow::anyhow!("节点不存在：{seed}")))?;
        let mut adj: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        for e in &edges {
            let (s, d) = (
                e["src"].as_str().unwrap_or("").to_string(),
                e["dst"].as_str().unwrap_or("").to_string(),
            );
            adj.entry(s.clone()).or_default().push(d.clone());
            adj.entry(d).or_default().push(s);
        }
        let mut keep: std::collections::HashSet<String> = std::collections::HashSet::new();
        keep.insert(start_id.clone());
        let mut frontier: Vec<String> = vec![start_id];
        let mut truncated = false;
        'outer: for _ in 0..depth.max(1) {
            let mut next = Vec::new();
            for cur in &frontier {
                if let Some(nbrs) = adj.get(cur) {
                    for nb in nbrs {
                        if keep.insert(nb.clone()) {
                            next.push(nb.clone());
                            if keep.len() >= cap {
                                truncated = true;
                                break 'outer;
                            }
                        }
                    }
                }
            }
            frontier = next;
        }
        let sub_nodes: Vec<serde_json::Value> = nodes
            .iter()
            .filter(|n| keep.contains(n["id"].as_str().unwrap_or("")))
            .cloned()
            .collect();
        let sub_edges: Vec<serde_json::Value> = edges
            .iter()
            .filter(|e| {
                keep.contains(e["src"].as_str().unwrap_or(""))
                    && keep.contains(e["dst"].as_str().unwrap_or(""))
            })
            .cloned()
            .collect();
        Ok(Some(serde_json::json!({
            "nodes": sub_nodes, "edges": sub_edges, "truncated": truncated, "seed": seed,
        })))
    }

    /// 大图搜索：id/file 包含即命中，limit 截断（后端持图，前端不全量拉清单）
    pub fn callgraph_search(
        &self,
        repo_path: &str,
        pattern: &str,
        limit: usize,
    ) -> CoreResult<Vec<serde_json::Value>> {
        let Some(g) = self.read_callgraph(repo_path)? else {
            return Ok(vec![]);
        };
        let pat = pattern.trim().to_lowercase();
        if pat.len() < 2 {
            return Ok(vec![]);
        }
        let mut hits: Vec<serde_json::Value> = g["nodes"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter(|n| {
                        n["id"].as_str().unwrap_or("").to_lowercase().contains(&pat)
                            || n["file"].as_str().unwrap_or("").to_lowercase().contains(&pat)
                    })
                    .map(|n| {
                        serde_json::json!({
                            "id": n["id"], "qualname": n["qualname"], "file": n["file"],
                            "module": n["module"], "is_entry": n["is_entry"],
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        hits.truncate(limit.clamp(1, 100));
        Ok(hits)
    }

    pub fn callgraph_chain(
        &self,
        repo_path: &str,
        root: &str,
        cap: usize,
    ) -> CoreResult<Option<CallgraphChain>> {
        let Some(g) = self.read_callgraph(repo_path)? else {
            return Ok(None);
        };
        let nodes = g["nodes"].as_array().cloned().unwrap_or_default();
        let edges = g["edges"].as_array().cloned().unwrap_or_default();
        let light = |n: &serde_json::Value| {
            serde_json::json!({
                "id": n["id"], "module": n["module"], "qualname": n["qualname"],
                "name": n["name"], "file": n["file"],
                "lineno": n["lineno"], "end_lineno": n["end_lineno"],
                "is_entry": n["is_entry"], "is_exported": n.get("is_exported").unwrap_or(&serde_json::json!(false)),
            })
        };
        let node_by_id: std::collections::HashMap<String, serde_json::Value> = nodes
            .into_iter()
            .map(|n| (n["id"].as_str().unwrap_or("").to_string(), n))
            .collect();
        let Some(root_node) = node_by_id.get(root) else {
            return Err(crate::CoreError::Other(anyhow::anyhow!("节点不存在：{root}")));
        };
        let root_node = root_node.clone();
        // 出边表：目标 -> 首次调用行（同目标多边取最早）
        let mut out: std::collections::HashMap<String, Vec<(String, u64)>> = std::collections::HashMap::new();
        for e in &edges {
            let (s, d) = (e["src"].as_str().unwrap_or(""), e["dst"].as_str().unwrap_or(""));
            if !node_by_id.contains_key(d) {
                continue;
            }
            let line = e["line"].as_u64().unwrap_or(0);
            let v = out.entry(s.to_string()).or_default();
            match v.iter_mut().find(|(t, _)| t == d) {
                Some((_, l)) => {
                    if line < *l {
                        *l = line;
                    }
                }
                None => v.push((d.to_string(), line)),
            }
        }
        // 子树深度（环防护）
        fn depth_of(
            id: &str,
            out: &std::collections::HashMap<String, Vec<(String, u64)>>,
            cache: &mut std::collections::HashMap<String, usize>,
        ) -> usize {
            if let Some(&d) = cache.get(id) {
                return d;
            }
            cache.insert(id.to_string(), 0); // 环防护：进行中按 0
            let mut d = 1;
            if let Some(children) = out.get(id) {
                for (t, _) in children {
                    let c = depth_of(t, out, cache);
                    d = d.max(1 + c);
                }
            }
            cache.insert(id.to_string(), d);
            d
        }
        let mut depth_cache: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        let mut num: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let mut entries: Vec<ChainEntry> = Vec::new();
        let mut main_path: Vec<String> = vec![root.to_string()];
        let mut truncated = false;

        // 显式栈 DFS（先序）：栈元素 (id, prefix, is_main)
        let mut stack: Vec<(String, String, bool)> = vec![(root.to_string(), "1".into(), true)];
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        seen.insert(root.to_string());
        while let Some((id, prefix, is_main)) = stack.pop() {
            if entries.len() >= cap {
                truncated = true;
                break;
            }
            let node = node_by_id.get(&id).cloned();
            let Some(node) = node else { continue };
            num.insert(id.clone(), prefix.clone());
            // 子节点按首次调用行排序
            let mut children: Vec<(String, u64)> = out.get(&id).cloned().unwrap_or_default();
            children.sort_by_key(|(_, l)| *l);
            // 主路径贪心：最深子树
            let main_child = if is_main {
                children
                    .iter()
                    .max_by_key(|(t, _)| depth_of(t, &out, &mut depth_cache))
                    .map(|(t, _)| t.clone())
            } else {
                None
            };
            let mut calls: Vec<ChainCallLink> = Vec::new();
            for (t, line) in &children {
                if seen.contains(t) || num.contains_key(t) {
                    // 引用：目标已渲染/在栈上将渲染
                    calls.push(ChainCallLink {
                        target_id: t.clone(),
                        target_num: num.get(t).cloned().unwrap_or_else(|| "?".into()),
                        target_name: node_by_id
                            .get(t)
                            .map(|n| n["qualname"].as_str().unwrap_or("").to_string())
                            .unwrap_or_default(),
                        line: *line,
                        is_ref: true,
                    });
                } else {
                    calls.push(ChainCallLink {
                        target_id: t.clone(),
                        target_num: String::new(), // 新块编号在渲染时才定（下方 push 时回填不了——改两段式）
                        target_name: node_by_id
                            .get(t)
                            .map(|n| n["qualname"].as_str().unwrap_or("").to_string())
                            .unwrap_or_default(),
                        line: *line,
                        is_ref: false,
                    });
                }
            }
            entries.push(ChainEntry {
                id: id.clone(),
                num: prefix.clone(),
                node: light(&node),
                calls,
            });
            // 子节点逆序压栈（先序：首个子最先弹出）
            let n_children = children.len();
            for (i, (t, _)) in children.iter().enumerate().rev() {
                if seen.contains(t) || num.contains_key(t) {
                    continue;
                }
                if entries.len() + stack.len() >= cap {
                    truncated = true;
                    break;
                }
                seen.insert(t.clone());
                let child_num = format!("{prefix}.{}", n_children - i);
                let child_is_main = is_main && Some(t.clone()) == main_child;
                if child_is_main {
                    main_path.push(t.clone());
                }
                stack.push((t.clone(), child_num, child_is_main));
            }
        }
        // 新块引用编号回填（两段式：全部 num 定后再补非 ref 链接的 target_num）
        for e in entries.iter_mut() {
            for c in e.calls.iter_mut() {
                if c.target_num.is_empty() {
                    c.target_num = num.get(&c.target_id).cloned().unwrap_or_else(|| "?".into());
                }
            }
        }
        let _ = root_node;
        Ok(Some(CallgraphChain {
            root: root.to_string(),
            entries,
            main_path,
            truncated,
        }))
    }

    /// 读取既有调用链产物（不存在返回 None；克隆加载 / 桌面端调用链 tab 用）


    pub fn read_callgraph(&self, repo_path: &str) -> CoreResult<Option<serde_json::Value>> {
        let p = crate::branch::wiki_root(Path::new(repo_path), &crate::branch::current_branch(Path::new(repo_path)))
            .join(CALLGRAPH_DOC_PATH);
        if !p.is_file() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&std::fs::read_to_string(p)?)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_repo(dir: &Path) -> std::path::PathBuf {
        let repo = dir.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/main
").unwrap();
        std::fs::write(
            repo.join("app.py"),
            "@bp.route(\"/hi\")\ndef hi(req):\n    return helper(req)\n\ndef helper(x):\n    return x\n",
        )
        .unwrap();
        let svc = AppService::open_at(dir.join("app"), dir.join("data")).unwrap();
        svc.open_repo(repo.to_str().unwrap()).unwrap();
        repo
    }

    #[test]
    fn generate_registers_and_writes_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let repo = setup_repo(dir.path());
        let svc = AppService::open_at(dir.path().join("app"), dir.path().join("data")).unwrap();

        let summary = svc.generate_callchain(repo.to_str().unwrap(), &|_| {}).unwrap();
        assert!(summary.nodes >= 2, "至少 hi/helper 两个函数");
        assert!(summary.entries >= 1, "路由入口识别");
        let out = crate::branch::wiki_root(&repo, "main").join("graphs").join("callchain").join("callgraph.json");
        assert!(out.is_file(), "产物落位 分支目录 graphs/callchain/");

        // 登记为 kind=graph 的生成区文档
        let docs = svc.db.list_wiki_docs(1, "main").unwrap();
        assert!(docs.iter().any(|(p, k, _)| p == CALLGRAPH_DOC_PATH && k == "graph"));

        // app 管线产物不内嵌 source（v2 决策）
        let g = svc.read_callgraph(repo.to_str().unwrap()).unwrap().unwrap();
        assert!(g["nodes"][0].get("source").is_none());

        // 重新生成幂等（upsert 不报错、文件覆盖）
        svc.generate_callchain(repo.to_str().unwrap(), &|_| {}).unwrap();
    }

    #[test]
    fn scan_existing_wiki_registers_callgraph() {
        // 克隆加载路径：.bytewiki/graphs/callchain/ 被识别为 graph
        let dir = tempfile::tempdir().unwrap();
        let repo = setup_repo(dir.path());
        let wiki = crate::branch::wiki_root(&repo, "main").join("graphs").join("callchain");
        std::fs::create_dir_all(&wiki).unwrap();
        std::fs::write(wiki.join("callgraph.json"), "{}").unwrap();

        let svc = AppService::open_at(dir.path().join("app"), dir.path().join("data")).unwrap();
        let docs = svc.scan_existing_wiki(&repo).unwrap();
        assert!(docs
            .iter()
            .any(|(p, k, _)| p == CALLGRAPH_DOC_PATH && k == "graph"));
    }

    /// 真实仓库端到端验收（dev-docs 验收标准 2）：git 根走服务层路径。
    /// 非 git 根的靶子（bid-eval/backend、review-task）由 standalone
    /// `byte-callchain` 引擎直跑覆盖，不在此列。
    #[test]
    #[ignore = "真实仓库验收：需本机存在靶子仓库"]
    fn real_repos_e2e() {
        let targets: &[(&str, &str)] = &[
            ("bid-eval(Python+JS)", r"C:\dev-data\code\bid-eval"),
            ("byte-code(Go)", r"C:\dev-data\code\cicbyte\byte-code"),
            ("byte-code-cli(Rust)", r"C:\dev-data\code\cicbyte\byte-code-cli"),
        ];
        let tmp = tempfile::tempdir().unwrap();
        let svc = AppService::open_at(tmp.path().join("app"), tmp.path().join("data")).unwrap();
        for (name, path) in targets {
            if !Path::new(path).is_dir() {
                println!("[skip] {name}: {path} 不存在");
                continue;
            }
            svc.open_repo(path).unwrap();
            let s = svc.generate_callchain(path, &|_| {}).unwrap();
            println!(
                "[{name}] 函数 {} · 边 {} · 入口 {} · 模块 {} · 语言 {:?} · {}ms → {}",
                s.nodes, s.edges, s.entries, s.modules, s.langs, s.duration_ms, s.out_path
            );
            assert!(s.nodes > 0, "{name} 未提取到函数");
            assert!(Path::new(&s.out_path).is_file(), "产物未落位");
        }
    }
}

fn entries_len(nodes: &[serde_json::Value]) -> usize {
    nodes.iter().filter(|n| n["is_entry"] == true).count()
}
