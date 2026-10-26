//! 函数级调用链（AppService 集成层）
//!
//! core 无条件依赖 `byte-wiki-callchain` 引擎（定位与决策见 dev-docs v2）：
//! 这里做业务包装——仓库校验、排除目录口径统一（与索引一致）、
//! 产物落 `.bytewiki/graphs/callchain/`、wiki_docs 登记（kind=graph）。
//!
//! 产物约定：app 管线 **不内嵌 source**（详情面板 Monaco 读实时文件），
//! standalone `byte-callchain` 导出才带 source。
//!
//! 存储（#573，设计见 bcode docs/design/callchain-sqlite.md）：
//! 生成时双写 callgraph.db（SQLite 主存储，查询/增量地基）+ callgraph.json
//! （互操作导出——前端中小图整包、克隆共享、外部工具仍读它）；读取优先 db、
//! 缺失回落 json；进程内按 (路径, mtime, len) 缓存解析结果（LRU ≤3 仓），
//! 四查询（概览/子图/搜索/走读链）共享同一类型化内存索引——重复解析税归零。

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use crate::callchain_db::graph_from_doc_json;
use crate::callchain_db::load_callgraph_db;
use crate::callchain_db::write_callgraph_db;
use crate::callchain_db::GraphData;
use crate::callchain_db::CALLGRAPH_DB_FILE;
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
    /// 本次真实解析 / 缓存复用文件数（增量效果可见性；同 commit 复制路径为 0/0）
    pub parsed_files: usize,
    pub reused_files: usize,
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
    /// 边类型（local/heuristic/receiver/event）——UI 标注启发式跳数
    pub kind: String,
}

/// 走读链断点（边界报告）：链走到某函数，其中的调用点没连上边——
/// 断在哪（line/col）、叫什么（name/recv）、可能到哪（candidates）
#[derive(serde::Serialize, Debug, Clone)]
pub struct BreakSite {
    pub name: String,
    pub recv: Option<String>,
    pub line: u64,
    pub col: u64,
    pub candidates: Vec<String>,
}

#[derive(serde::Serialize)]
pub struct ChainEntry {
    pub id: String,
    pub num: String,
    pub node: serde_json::Value,
    pub calls: Vec<ChainCallLink>,
    /// 该函数内未连边的调用点（空=无断点）
    pub breaks: Vec<BreakSite>,
}

#[derive(serde::Serialize)]
pub struct CallgraphChain {
    pub root: String,
    pub entries: Vec<ChainEntry>,
    pub main_path: Vec<String>,
    pub truncated: bool,
}

/// 调用图概览（大图模式：全图由后端持有，前端只拿轻量负载 <200KB）
#[derive(serde::Serialize, Clone)]
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
    /// 生成时的 HEAD commit（无 git / json 老产物为 None）
    pub generated_commit: Option<String>,
    /// 滞后：生成后 HEAD 已前进（图为快照，走读/子图可能过期）
    pub lagging: bool,
}

// ============ 进程内图缓存 ============

/// 缓存槽位上限（多仓/多分支切换场景，单份 ZCode 级索引 ~15MB）
const GRAPH_CACHE_SLOTS: usize = 3;

/// 图缓存条目键：(产物路径, mtime 毫秒, 长度)——重新生成即失效
type GraphCacheKey = (PathBuf, u64, u64);

/// 缓存槽内容：图数据 + 派生只读视图（overview 一次计算重复取用——
/// 它对全图做 20 万次量级的字符串哈希聚合，属「每图一次」而非「每查询一次」的成本）
pub struct PreparedGraph {
    pub data: Arc<GraphData>,
    overview: std::sync::OnceLock<CallgraphOverview>,
}

/// 进程内已解析图缓存（#573 P0：交互式重复查询不再重复解析/扫表）
#[derive(Default)]
pub struct GraphCache {
    /// front = 最近使用
    slots: Vec<(GraphCacheKey, Arc<PreparedGraph>)>,
}

impl GraphCache {
    fn get(&mut self, key: &GraphCacheKey) -> Option<Arc<PreparedGraph>> {
        let pos = self.slots.iter().position(|(k, _)| k == key)?;
        let entry = self.slots.remove(pos);
        self.slots.insert(0, entry);
        Some(self.slots[0].1.clone())
    }

    fn put(&mut self, key: GraphCacheKey, data: Arc<PreparedGraph>) {
        self.slots.insert(0, (key, data));
        self.slots.truncate(GRAPH_CACHE_SLOTS);
    }
}

impl AppService {
    /// 生成收尾：wiki_docs 登记 + change_log（真生成与同 commit 复制共用）
    fn finish_generate(&self, repo_id: i64, branch: &str, nodes: usize, edges: usize) -> CoreResult<()> {
        self.db.upsert_generated_doc(repo_id, branch, "graph", CALLGRAPH_DOC_PATH, "调用链图谱")?;
        self.db.log_change(repo_id, branch, "callchain", &format!("nodes={nodes} edges={edges}"))
    }

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
        // 规则管理：文件排除 / 入口排除 / 自定义入口正则（**按仓库**读取：
        // repos.callchain_rules → 全局 config 回落 → 默认）
        let rules = self.get_callchain_rules(repo_path).unwrap_or_default();
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

        let branch = crate::branch::current_branch(root);
        // 产物写入仓库内（随仓库共享，FR-08）
        let out_dir = crate::branch::wiki_root(root, &branch)
            .join("graphs")
            .join("callchain");
        std::fs::create_dir_all(&out_dir)?;

        // #3 同 commit 免重算：某兄弟分支产物记录的 HEAD 与当前一致
        // （刚从 main 拉 feature 这类场景），直接复制 db+json（秒级），跳过提取
        let head = crate::branch::head_commit(root);
        let target_db = out_dir.join(CALLGRAPH_DB_FILE);
        if !head.is_empty()
            && crate::callchain_db::db_head_commit(&target_db).as_deref() != Some(head.as_str())
        {
            if let Some(src_dir) = find_same_commit_callgraph(root, &crate::branch::branch_dir(&branch), &head) {
                tracing::info!("调用链：分支 {} 与当前 HEAD 同 commit，复制产物免重算", src_dir.file_name().and_then(|n| n.to_str()).unwrap_or("?"));
                std::fs::copy(src_dir.join(CALLGRAPH_DB_FILE), &target_db)?;
                if let Some(json_src) = src_dir.join("callgraph.json").is_file().then(|| src_dir.join("callgraph.json")) {
                    std::fs::copy(json_src, out_dir.join("callgraph.json"))?;
                }
                // 计数从复制的 db 取（meta + 全量加载），后续登记/summary 与真生成同构
                let g = load_callgraph_db(&target_db)?;
                let summary = CallchainSummary {
                    out_path: out_dir.join("callgraph.json").to_string_lossy().into_owned(),
                    nodes: g.nodes.len(),
                    edges: g.edges.len(),
                    entries: g.nodes.iter().filter(|n| n.is_entry).count(),
                    modules: g.modules.len(),
                    langs: g.langs.clone().into_iter().collect(),
                    parsed_files: 0,
                    reused_files: 0,
                    duration_ms: t0.elapsed().as_millis(),
                };
                self.finish_generate(repo.id, &branch, summary.nodes, summary.edges)?;
                return Ok(summary);
            }
        }

        // 增量提取：content 指纹未变的文件整条复用上次提取结果（免 tree-sitter），
        // 消解全量重放——新符号出现后，老文件里原先连不上边的调用点自动复活
        let stored = crate::callchain_db::load_file_recs(&target_db);
        let outcome = byte_wiki_callchain::extract_with_cache(root, &opts, &stored, on_progress)
            .map_err(|e| CoreError::Other(anyhow::anyhow!("调用链提取失败：{e}")))?;
        let graph = &outcome.doc;

        let out_path = out_dir.join("callgraph.json");
        std::fs::write(&out_path, serde_json::to_string_pretty(graph)?)?;
        // #573：SQLite 主存储（查询毫秒级 + 增量更新地基）；
        // meta 记录生成时 HEAD——读取侧与实时值比对做滞后检测
        write_callgraph_db(&target_db, graph, &head)?;
        // 增量缓存整体替换（本次全量 rec 行：复用项也在内，删除项自然消失）
        crate::callchain_db::write_file_recs(&target_db, &outcome.rec_rows)?;

        // 登记 wiki_docs（kind=graph；upsert——wiki 重新生成的整体替换后由 assemble 重新补登）
        self.finish_generate(repo.id, &branch,
            graph["nodes"].as_array().map(|a| a.len()).unwrap_or(0),
            graph["edges"].as_array().map(|a| a.len()).unwrap_or(0))?;

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
            parsed_files: outcome.parsed,
            reused_files: outcome.reused,
            duration_ms: t0.elapsed().as_millis(),
        })
    }

    /// 取图（缓存优先）：db 存在用 db，否则回落 json（老仓库）；
    /// (路径, mtime, len) 任一变化即重新加载。无产物返回 None。
    /// branch=None → 当前 git 分支；Some → 指定分支（全局分支切换的只读查看）
    fn cached_graph(&self, repo_path: &str, branch: Option<&str>) -> CoreResult<Option<Arc<PreparedGraph>>> {
        let root = Path::new(repo_path);
        let branch = branch
            .map(str::to_string)
            .unwrap_or_else(|| crate::branch::current_branch(root));
        let dir = crate::branch::wiki_root(root, &branch)
            .join("graphs")
            .join("callchain");
        let db_p = dir.join(CALLGRAPH_DB_FILE);
        let json_p = dir.join("callgraph.json");
        let (path, is_db) = if db_p.is_file() {
            (db_p, true)
        } else if json_p.is_file() {
            (json_p, false)
        } else {
            return Ok(None);
        };
        let md = std::fs::metadata(&path)?;
        let mtime = md
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let key: GraphCacheKey = (path.clone(), mtime, md.len());
        if let Some(hit) = self.callgraph_cache.lock().unwrap().get(&key) {
            return Ok(Some(hit));
        }
        let data = if is_db {
            load_callgraph_db(&path)?
        } else {
            graph_from_doc_json(&serde_json::from_str(&std::fs::read_to_string(&path)?)?)
        };
        let prepared = Arc::new(PreparedGraph {
            data: Arc::new(data),
            overview: std::sync::OnceLock::new(),
        });
        self.callgraph_cache
            .lock()
            .unwrap()
            .put(key, prepared.clone());
        Ok(Some(prepared))
    }

    /// 大图模式下后端持全图、按需取子图 —— 概览不含 26k 节点清单。
    /// 滞后检测：meta 的生成时 HEAD 与实时 HEAD 不一致 → lagging=true
    pub fn callgraph_overview(
        &self,
        repo_path: &str,
        branch: Option<&str>,
    ) -> CoreResult<Option<CallgraphOverview>> {
        let Some(p) = self.cached_graph(repo_path, branch)? else {
            return Ok(None);
        };
        let mut ov = p.overview.get_or_init(|| overview_of(&p.data)).clone();
        let live = crate::branch::head_commit(Path::new(repo_path));
        ov.lagging = !p.data.head_commit.is_empty()
            && !live.is_empty()
            && p.data.head_commit != live;
        ov.generated_commit = (!p.data.head_commit.is_empty()).then(|| p.data.head_commit.clone());
        Ok(Some(ov))
    }

    /// BFS 子图裁剪：seed 节点 N 跳双向邻域，节点数封顶 cap
    pub fn callgraph_focus(
        &self,
        repo_path: &str,
        branch: Option<&str>,
        seed: &str,
        depth: usize,
        cap: usize,
    ) -> CoreResult<Option<serde_json::Value>> {
        let Some(p) = self.cached_graph(repo_path, branch)? else {
            return Ok(None);
        };
        let g = p.data.clone();
        let node_ids: HashSet<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();
        if !node_ids.contains(seed) {
            return Err(crate::CoreError::Other(anyhow::anyhow!("节点不存在：{seed}")));
        }
        let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
        for e in &g.edges {
            adj.entry(e.src.as_str()).or_default().push(e.dst.as_str());
            adj.entry(e.dst.as_str()).or_default().push(e.src.as_str());
        }
        // 邻接序规范化：cap 截断时保留哪些节点是确定的
        for v in adj.values_mut() {
            v.sort_unstable();
        }
        let mut keep: HashSet<&str> = HashSet::new();
        keep.insert(seed);
        let mut frontier: Vec<&str> = vec![seed];
        let mut truncated = false;
        'outer: for _ in 0..depth.max(1) {
            let mut next = Vec::new();
            for cur in &frontier {
                if let Some(nbrs) = adj.get(cur) {
                    for nb in nbrs {
                        if node_ids.contains(nb) && keep.insert(nb) {
                            next.push(*nb);
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
        let sub_nodes: Vec<serde_json::Value> = g
            .nodes
            .iter()
            .filter(|n| keep.contains(n.id.as_str()))
            .map(|n| n.to_json())
            .collect();
        let sub_edges: Vec<serde_json::Value> = g
            .edges
            .iter()
            .filter(|e| keep.contains(e.src.as_str()) && keep.contains(e.dst.as_str()))
            .map(|e| e.to_json())
            .collect();
        Ok(Some(serde_json::json!({
            "nodes": sub_nodes, "edges": sub_edges, "truncated": truncated, "seed": seed,
        })))
    }

    /// 大图搜索：id/file 包含即命中，limit 截断（后端持图，前端不全量拉清单）
    pub fn callgraph_search(
        &self,
        repo_path: &str,
        branch: Option<&str>,
        pattern: &str,
        limit: usize,
    ) -> CoreResult<Vec<serde_json::Value>> {
        let Some(p) = self.cached_graph(repo_path, branch)? else {
            return Ok(vec![]);
        };
        let g = p.data.clone();
        let pat = pattern.trim().to_lowercase();
        if pat.len() < 2 {
            return Ok(vec![]);
        }
        let mut hits: Vec<serde_json::Value> = g
            .nodes
            .iter()
            .filter(|n| {
                n.id.to_lowercase().contains(&pat) || n.file.to_lowercase().contains(&pat)
            })
            .map(|n| {
                serde_json::json!({
                    "id": n.id, "qualname": n.qualname, "file": n.file,
                    "module": n.module, "is_entry": n.is_entry,
                })
            })
            .collect();
        hits.truncate(limit.clamp(1, 100));
        Ok(hits)
    }

    pub fn callgraph_chain(
        &self,
        repo_path: &str,
        branch: Option<&str>,
        root: &str,
        cap: usize,
    ) -> CoreResult<Option<CallgraphChain>> {
        let Some(p) = self.cached_graph(repo_path, branch)? else {
            return Ok(None);
        };
        let g = p.data.clone();
        Ok(Some(chain_of(&g, root, cap)?))
    }

    /// 读取既有调用链产物（不存在返回 None；CLI query / 测试用）。
    /// #573 后走缓存索引并还原为引擎 json 文档形状（db 优先、json 回落）。
    pub fn read_callgraph(&self, repo_path: &str, branch: Option<&str>) -> CoreResult<Option<serde_json::Value>> {
        Ok(self.cached_graph(repo_path, branch)?.map(|p| p.data.to_doc_json()))
    }
}

// ============ 查询实现（类型化索引上，双后端共享） ============

/// 概览：计数聚合 + 入口清单（零入口库回退公共 API 面 top30）+ 模块聚合边
fn overview_of(g: &GraphData) -> CallgraphOverview {
    let mut indeg: HashMap<&str, usize> = HashMap::new();
    let mut mod_edges: HashMap<(&str, &str), usize> = HashMap::new();
    let node_mod: HashMap<&str, &str> = g
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.module.as_str()))
        .collect();
    for e in &g.edges {
        *indeg.entry(e.dst.as_str()).or_default() += 1;
        if let (Some(m1), Some(m2)) = (node_mod.get(e.src.as_str()), node_mod.get(e.dst.as_str())) {
            if m1 != m2 {
                *mod_edges.entry((m1, m2)).or_default() += 1;
            }
        }
    }
    let light = |n: &crate::callchain_db::GNode| {
        serde_json::json!({
            "id": n.id, "module": n.module, "qualname": n.qualname,
            "name": n.name, "file": n.file, "routes": n.routes,
            "lineno": n.lineno, "end_lineno": n.end_lineno,
            "is_exported": n.is_exported, "is_entry": n.is_entry,
        })
    };
    let mut entries: Vec<serde_json::Value> = g
        .nodes
        .iter()
        .filter(|n| n.is_entry)
        .take(200)
        .map(light)
        .collect();
    let mut entries_are_api = false;
    if entries.is_empty() {
        // 库回退：公共 API（门面文件导出优先）top 30，测试调用量参考
        entries_are_api = true;
        let facade_of = |n: &crate::callchain_db::GNode| {
            let base = n.file.rsplit(['/', '\\']).next().unwrap_or(&n.file);
            let stem = base.split('.').next().unwrap_or(base);
            matches!(stem, "index" | "mod" | "lib" | "main" | "__init__")
        };
        let mut pool: Vec<(std::cmp::Reverse<bool>, std::cmp::Reverse<usize>, &crate::callchain_db::GNode)> = g
            .nodes
            .iter()
            .filter(|n| n.is_exported)
            .map(|n| {
                (
                    std::cmp::Reverse(facade_of(n)),
                    std::cmp::Reverse(indeg.get(n.id.as_str()).copied().unwrap_or(0)),
                    n,
                )
            })
            .collect();
        if pool.is_empty() {
            pool = g.nodes
                .iter()
                .map(|n| {
                    (
                        std::cmp::Reverse(facade_of(n)),
                        std::cmp::Reverse(indeg.get(n.id.as_str()).copied().unwrap_or(0)),
                        n,
                    )
                })
                .collect();
        }
        // 键预计算后排序（闭包内做文件名切分会放大到 O(n log n) 次）；
        // 稳定排序平局保 id 序（nodes 已按 id 规范序）
        pool.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
        entries = pool.into_iter().take(30).map(|(_, _, n)| light(n)).collect();
    }
    entries.truncate(200);
    let mut me: Vec<(String, String, usize)> = mod_edges
        .into_iter()
        .map(|((a, b), w)| (a.to_string(), b.to_string(), w))
        .collect();
    // 权重降序 + 字典序平局决议（与旧 BTreeMap 迭代 + 稳定排序的输出序一致）
    me.sort_by(|x, y| y.2.cmp(&x.2).then_with(|| (&x.0, &x.1).cmp(&(&y.0, &y.1))));
    me.truncate(250);
    let mut mods: Vec<&str> = node_mod.into_values().collect();
    mods.sort_unstable();
    mods.dedup();
    // 大图模块可达数千——前端下拉用不了那么多，截断控负载
    mods.truncate(500);
    let modules: Vec<String> = mods.into_iter().map(String::from).collect();
    CallgraphOverview {
        total_nodes: g.nodes.len(),
        total_edges: g.edges.len(),
        total_entries: g.nodes.iter().filter(|n| n.is_entry).count(),
        entries,
        entries_are_api,
        modules,
        module_edges: me,
        // 滞后状态由调用方按实时 HEAD 盖章（OnceLock 缓存的是无状态部分）
        generated_commit: None,
        lagging: false,
    }
}

/// 走读链 DFS：先序、子节点按首次调用行（同行为 id 序）排序、
/// 已访问仅作引用、主路径贪心最深子树、环防护、cap 封顶
fn chain_of(g: &GraphData, root: &str, cap: usize) -> CoreResult<CallgraphChain> {
    let node_by_id: HashMap<&str, &crate::callchain_db::GNode> =
        g.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    if !node_by_id.contains_key(root) {
        return Err(crate::CoreError::Other(anyhow::anyhow!("节点不存在：{root}")));
    }
    let light = |n: &crate::callchain_db::GNode| {
        serde_json::json!({
            "id": n.id, "module": n.module, "qualname": n.qualname,
            "name": n.name, "file": n.file,
            "lineno": n.lineno, "end_lineno": n.end_lineno,
            // routes 必带：走读页标题路由徽标/存卡标题/全链导出都读它
            // （#571 起服务端链曾漏发，前端 routes.length 直接崩——见 Walkthrough.vue:361）
            "routes": n.routes, "async": n.is_async,
            "is_entry": n.is_entry, "is_exported": n.is_exported,
        })
    };
    // 出边表：目标 -> (首次调用行, 边类型)（边在加载时已按 (src,dst) 归并取最早行）
    let mut out: HashMap<&str, Vec<(&str, u64, &str)>> = HashMap::new();
    for e in &g.edges {
        if node_by_id.contains_key(e.dst.as_str()) {
            out.entry(e.src.as_str())
                .or_default()
                .push((e.dst.as_str(), e.line, e.kind.as_str()));
        }
    }
    for v in out.values_mut() {
        v.sort_by(|a, b| (a.1, a.0).cmp(&(b.1, b.0)));
    }
    // 断链边界报告素材：链上函数的未连边调用点（断在哪、可能到哪）
    let mut breaks_by: HashMap<&str, Vec<&crate::callchain_db::GUnresolved>> = HashMap::new();
    for u in &g.unresolved {
        breaks_by.entry(u.src.as_str()).or_default().push(u);
    }
    // 子树深度（环防护）
    fn depth_of<'a>(
        id: &'a str,
        out: &HashMap<&'a str, Vec<(&'a str, u64, &'a str)>>,
        cache: &mut HashMap<&'a str, usize>,
    ) -> usize {
        if let Some(&d) = cache.get(id) {
            return d;
        }
        cache.insert(id, 0); // 环防护：进行中按 0
        let mut d = 1;
        if let Some(children) = out.get(id) {
            for (t, _, _) in children {
                let c = depth_of(t, out, cache);
                d = d.max(1 + c);
            }
        }
        cache.insert(id, d);
        d
    }
    let mut depth_cache: HashMap<&str, usize> = HashMap::new();

    let mut num: HashMap<&str, String> = HashMap::new();
    let mut entries: Vec<ChainEntry> = Vec::new();
    let mut main_path: Vec<String> = vec![root.to_string()];
    let mut truncated = false;

    // 显式栈 DFS（先序）：栈元素 (id, prefix, is_main)
    let mut stack: Vec<(&str, String, bool)> = vec![(root, "1".into(), true)];
    let mut seen: HashSet<&str> = HashSet::new();
    seen.insert(root);
    while let Some((id, prefix, is_main)) = stack.pop() {
        if entries.len() >= cap {
            truncated = true;
            break;
        }
        let Some(node) = node_by_id.get(id) else { continue };
        num.insert(id, prefix.clone());
        let children: Vec<(&str, u64, &str)> = out.get(id).cloned().unwrap_or_default();
        // 主路径贪心：最深子树
        let main_child = if is_main {
            children
                .iter()
                .max_by_key(|(t, _, _)| depth_of(t, &out, &mut depth_cache))
                .map(|(t, _, _)| *t)
        } else {
            None
        };        let mut calls: Vec<ChainCallLink> = Vec::new();
        for (t, line, kind) in &children {
            let target_name = node_by_id
                .get(t)
                .map(|n| n.qualname.clone())
                .unwrap_or_default();
            if seen.contains(t) || num.contains_key(t) {
                // 引用：目标已渲染/在栈上将渲染
                calls.push(ChainCallLink {
                    target_id: (*t).to_string(),
                    target_num: num.get(t).cloned().unwrap_or_else(|| "?".into()),
                    target_name,
                    line: *line,
                    is_ref: true,
                    kind: (*kind).to_string(),
                });
            } else {
                calls.push(ChainCallLink {
                    target_id: (*t).to_string(),
                    target_num: String::new(), // 新块编号渲染时才定——两段式回填
                    target_name,
                    line: *line,
                    is_ref: false,
                    kind: (*kind).to_string(),
                });
            }
        }
        let breaks = breaks_by
            .get(id)
            .map(|us| {
                us.iter()
                    .map(|u| BreakSite {
                        name: u.name.clone(),
                        recv: u.recv.clone(),
                        line: u.line,
                        col: u.col,
                        candidates: u.candidates.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        entries.push(ChainEntry {
            id: id.to_string(),
            num: prefix.clone(),
            node: light(node),
            calls,
            breaks,
        });
        // 子节点逆序压栈（先序：首个子最先弹出）
        let n_children = children.len();
        for (i, (t, _, _)) in children.iter().enumerate().rev() {
            if seen.contains(t) || num.contains_key(t) {
                continue;
            }
            if entries.len() + stack.len() >= cap {
                truncated = true;
                break;
            }
            seen.insert(t);
            let child_num = format!("{prefix}.{}", n_children - i);
            let child_is_main = is_main && Some(*t) == main_child;
            if child_is_main {
                main_path.push((*t).to_string());
            }
            stack.push((t, child_num, child_is_main));
        }
    }
    // 新块引用编号回填（两段式：全部 num 定后再补非 ref 链接的 target_num）
    for e in entries.iter_mut() {
        for c in e.calls.iter_mut() {
            if c.target_num.is_empty() {
                c.target_num = num.get(c.target_id.as_str()).cloned().unwrap_or_else(|| "?".into());
            }
        }
    }
    Ok(CallgraphChain {
        root: root.to_string(),
        entries,
        main_path,
        truncated,
    })
}

// ============ 测试 ============

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

    fn artifacts_dir(repo: &Path) -> std::path::PathBuf {
        crate::branch::wiki_root(repo, "main").join("graphs").join("callchain")
    }

    #[test]
    fn generate_registers_and_writes_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let repo = setup_repo(dir.path());
        let svc = AppService::open_at(dir.path().join("app"), dir.path().join("data")).unwrap();

        let summary = svc.generate_callchain(repo.to_str().unwrap(), &|_| {}).unwrap();
        assert!(summary.nodes >= 2, "至少 hi/helper 两个函数");
        assert!(summary.entries >= 1, "路由入口识别");
        let out_dir = artifacts_dir(&repo);
        assert!(out_dir.join("callgraph.json").is_file(), "json 导出落位");
        // #573：db 主存储双写
        assert!(out_dir.join(CALLGRAPH_DB_FILE).is_file(), "callgraph.db 落位");

        // 登记为 kind=graph 的生成区文档
        let docs = svc.db.list_wiki_docs(1, "main").unwrap();
        assert!(docs.iter().any(|(p, k, _)| p == CALLGRAPH_DOC_PATH && k == "graph"));

        // app 管线产物不内嵌 source（v2 决策）
        let g = svc.read_callgraph(repo.to_str().unwrap(), None).unwrap().unwrap();
        assert!(g["nodes"][0].get("source").is_none());

        // 重新生成幂等（upsert 不报错、文件覆盖）
        svc.generate_callchain(repo.to_str().unwrap(), &|_| {}).unwrap();
    }

    #[test]
    fn scan_existing_wiki_registers_callgraph() {
        // 克隆加载路径：.bytewiki/graphs/callchain/ 被识别为 graph
        let dir = tempfile::tempdir().unwrap();
        let repo = setup_repo(dir.path());
        let wiki = artifacts_dir(&repo);
        std::fs::create_dir_all(&wiki).unwrap();
        std::fs::write(wiki.join("callgraph.json"), "{}").unwrap();
        // db 及 WAL sidecar 是机器产物，不进文档树（#573）
        std::fs::write(wiki.join(CALLGRAPH_DB_FILE), "sqlite").unwrap();
        std::fs::write(wiki.join(format!("{CALLGRAPH_DB_FILE}-wal")), "").unwrap();

        let svc = AppService::open_at(dir.path().join("app"), dir.path().join("data")).unwrap();
        let docs = svc.scan_existing_wiki(&repo).unwrap();
        assert!(docs
            .iter()
            .any(|(p, k, _)| p == CALLGRAPH_DOC_PATH && k == "graph"));
        assert!(
            !docs.iter().any(|(p, _, _)| p.contains(CALLGRAPH_DB_FILE)),
            "db/wal 不应登记为文档：{docs:?}"
        );
    }

    /// #573 验收：db 与 json 双后端的四查询输出必须一致（同一缓存索引实现 + 规范序）
    #[test]
    fn queries_parity_between_db_and_json_backends() {
        let dir = tempfile::tempdir().unwrap();
        let repo = setup_repo(dir.path());
        let svc = AppService::open_at(dir.path().join("app"), dir.path().join("data")).unwrap();
        svc.generate_callchain(repo.to_str().unwrap(), &|_| {}).unwrap();
        let repo_str = repo.to_str().unwrap();

        let overview_db = serde_json::to_string(&svc.callgraph_overview(repo_str, None).unwrap().unwrap()).unwrap();
        let focus_db = serde_json::to_string(
            &svc.callgraph_focus(repo_str, None, "app::hi", 2, 50).unwrap().unwrap(),
        )
        .unwrap();
        let search_db = serde_json::to_string(&svc.callgraph_search(repo_str, None, "hel", 20).unwrap()).unwrap();
        let chain_db = serde_json::to_string(&svc.callgraph_chain(repo_str, None, "app::hi", 50).unwrap().unwrap()).unwrap();
        // 走读链节点必带 routes（标题徽标/存卡/全链导出消费；#571 起曾漏发致前端崩）
        let ch: serde_json::Value = serde_json::from_str(&chain_db).unwrap();
        assert!(
            ch["entries"][0]["node"]["routes"].as_array().is_some(),
            "链节点缺 routes 字段"
        );

        // 删 db 走 json 回落（缓存键不同，强制重载）
        std::fs::remove_file(artifacts_dir(&repo).join(CALLGRAPH_DB_FILE)).unwrap();
        let overview_json = serde_json::to_string(&svc.callgraph_overview(repo_str, None).unwrap().unwrap()).unwrap();
        let focus_json = serde_json::to_string(
            &svc.callgraph_focus(repo_str, None, "app::hi", 2, 50).unwrap().unwrap(),
        )
        .unwrap();
        let search_json = serde_json::to_string(&svc.callgraph_search(repo_str, None, "hel", 20).unwrap()).unwrap();
        let chain_json = serde_json::to_string(&svc.callgraph_chain(repo_str, None, "app::hi", 50).unwrap().unwrap()).unwrap();

        assert_eq!(overview_db, overview_json, "overview 双后端不一致");
        assert_eq!(focus_db, focus_json, "focus 双后端不一致");
        assert_eq!(search_db, search_json, "search 双后端不一致");
        assert_eq!(chain_db, chain_json, "chain 双后端不一致");
    }

    /// #573 P0：产物重写（mtime/len 变化）后缓存必须失效
    #[test]
    fn cache_invalidates_on_artifact_rewrite() {
        let dir = tempfile::tempdir().unwrap();
        let repo = setup_repo(dir.path());
        let svc = AppService::open_at(dir.path().join("app"), dir.path().join("data")).unwrap();
        svc.generate_callchain(repo.to_str().unwrap(), &|_| {}).unwrap();
        let repo_str = repo.to_str().unwrap();
        let before = svc.callgraph_overview(repo_str, None).unwrap().unwrap();
        assert!(before.total_nodes >= 2);

        // 直接覆写 db 为一张更小的图（内容长度明显不同 → 缓存键失效）
        let smaller = serde_json::json!({
            "root": repo_str,
            "nodes": [{"id": "z::only", "module": "z", "qualname": "only", "name": "only",
                        "file": "z.py", "lineno": 1, "end_lineno": 2, "routes": [],
                        "async": false, "is_entry": false, "is_exported": false}],
            "edges": [],
            "modules": {"z": {"file": "z.py", "funcs": 1}},
            "langs": {"python": 1}
        });
        write_callgraph_db(&artifacts_dir(&repo).join(CALLGRAPH_DB_FILE), &smaller, "zz").unwrap();
        let after = svc.callgraph_overview(repo_str, None).unwrap().unwrap();
        assert_eq!(after.total_nodes, 1, "重写产物后缓存未失效");
    }

    /// #调用链规则按项目管理：仓库间互不串台；未自定义回落默认
    #[test]
    fn callchain_rules_persisted_per_repo() {
        let dir = tempfile::tempdir().unwrap();
        let repo_a = setup_repo(dir.path());
        // 第二个仓（同一 app 服务内登记）
        let repo_b = dir.path().join("repo2");
        std::fs::create_dir_all(repo_b.join(".git")).unwrap();
        std::fs::write(repo_b.join(".git/HEAD"), "ref: refs/heads/main
").unwrap();
        std::fs::write(repo_b.join("app.py"), "def hi():
    return 1
").unwrap();
        let svc = AppService::open_at(dir.path().join("app"), dir.path().join("data")).unwrap();
        svc.open_repo(repo_b.to_str().unwrap()).unwrap();

        // A 自定义规则
        let rules = crate::model::CallchainRules {
            exclude_globs: vec!["*.py".into()],
            entry_exclude_globs: vec![],
            entry_regex: Some("^my_".into()),
        };
        svc.set_callchain_rules(repo_a.to_str().unwrap(), rules).unwrap();

        // 隔离：A 读回自定义；B 未设置读默认（默认 entry_exclude 有 build.rs 等）
        let ra = svc.get_callchain_rules(repo_a.to_str().unwrap()).unwrap();
        assert_eq!(ra.exclude_globs, vec!["*.py".to_string()]);
        assert_eq!(ra.entry_regex.as_deref(), Some("^my_"));
        let rb = svc.get_callchain_rules(repo_b.to_str().unwrap()).unwrap();
        assert!(rb.exclude_globs.is_empty(), "B 不应读到 A 的规则");
        assert!(rb.entry_regex.is_none());
        assert!(!rb.entry_exclude_globs.is_empty(), "B 回落默认入口排除表");

        // 生成端消费仓规则：A 的 *.py 排除把 app.py 挡掉 → 零节点
        let sum = svc.generate_callchain(repo_a.to_str().unwrap(), &|_| {}).unwrap();
        assert_eq!(sum.nodes, 0, "仓规则 exclude_globs 必须作用于生成");
    }

    /// #1 滞后检测：生成时 HEAD 与实时 HEAD 不一致 → overview.lagging
    #[test]
    fn overview_reports_lagging_after_head_moves() {
        let dir = tempfile::tempdir().unwrap();
        let repo = setup_repo(dir.path());
        // 补齐 ref 文件让 head_commit 可解析
        std::fs::create_dir_all(repo.join(".git/refs/heads")).unwrap();
        std::fs::write(repo.join(".git/refs/heads/main"), "aa11bb22\n").unwrap();
        let svc = AppService::open_at(dir.path().join("app"), dir.path().join("data")).unwrap();
        let repo_str = repo.to_str().unwrap();
        svc.generate_callchain(repo_str, &|_| {}).unwrap();

        let fresh = svc.callgraph_overview(repo_str, None).unwrap().unwrap();
        assert!(!fresh.lagging, "生成后即刻不滞后");
        assert_eq!(fresh.generated_commit.as_deref(), Some("aa11bb22"));

        // HEAD 前进（提交了一个新 sha）：图未重生成 → 滞后
        std::fs::write(repo.join(".git/refs/heads/main"), "cc33dd44\n").unwrap();
        let stale = svc.callgraph_overview(repo_str, None).unwrap().unwrap();
        assert!(stale.lagging, "HEAD 前进后必须报滞后");
        assert_eq!(stale.generated_commit.as_deref(), Some("aa11bb22"));
    }

    /// #3 同 commit 免重算：feature 指向 main 生成时的同一 HEAD，
    /// 生成走复制路径（快照语义：新加的未提交文件不进图——证明没重提取）
    #[test]
    fn same_commit_generation_copies_from_sibling_branch() {
        let dir = tempfile::tempdir().unwrap();
        let repo = setup_repo(dir.path());
        std::fs::create_dir_all(repo.join(".git/refs/heads")).unwrap();
        std::fs::write(repo.join(".git/refs/heads/main"), "aa11bb22\n").unwrap();
        let svc = AppService::open_at(dir.path().join("app"), dir.path().join("data")).unwrap();
        let repo_str = repo.to_str().unwrap();
        let main_sum = svc.generate_callchain(repo_str, &|_| {}).unwrap();

        // 切 feature（同 sha），并新增一个未提交源文件——若走提取会多出节点
        std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/feature\n").unwrap();
        std::fs::write(repo.join(".git/refs/heads/feature"), "aa11bb22\n").unwrap();
        std::fs::write(repo.join("extra.py"), "def extra_fn(x):\n    return x\n").unwrap();
        let feat_sum = svc.generate_callchain(repo_str, &|_| {}).unwrap();

        assert_eq!(feat_sum.nodes, main_sum.nodes, "同 commit 复制：计数与来源分支一致（未提交新文件不进图）");
        let feat_db = crate::branch::wiki_root(&repo, "feature").join("graphs").join("callchain").join(CALLGRAPH_DB_FILE);
        assert!(feat_db.is_file());
        assert_eq!(
            crate::callchain_db::db_head_commit(&feat_db).as_deref(),
            Some("aa11bb22"),
            "复制产物保留来源的生成时 HEAD"
        );
        // 指定分支只读查询：feature 分支视角可查，main 视角也可查
        let ov = svc.callgraph_overview(repo_str, Some("main")).unwrap().unwrap();
        assert_eq!(ov.total_nodes, main_sum.nodes);
        assert!(!ov.lagging, "指定分支查看时按该分支视角（main 无落后）");
    }

    /// P1 增量验收：未变文件缓存复用 + 新符号「复活」断链调用点（unresolved 重试语义）
    #[test]
    fn incremental_regenerate_revives_unresolved_calls() {
        let dir = tempfile::tempdir().unwrap();
        let repo = setup_repo(dir.path());
        let svc = AppService::open_at(dir.path().join("app"), dir.path().join("data")).unwrap();
        let repo_str = repo.to_str().unwrap();

        // ghost 尚未定义：top 里的调用记为未连边
        std::fs::write(repo.join("caller.py"), "def top(z):\n    return ghost(z)\n").unwrap();
        let s1 = svc.generate_callchain(repo_str, &|_| {}).unwrap();
        assert_eq!(s1.parsed_files, 2, "首跑全量解析：app.py + caller.py");
        let g1 = svc.read_callgraph(repo_str, None).unwrap().unwrap();
        assert!(
            g1["unresolved_refs"].as_array().unwrap().iter().any(|u| u["name"] == "ghost"),
            "调用点未连边应留档：{g1:?}"
        );

        // 新文件定义 ghost → 重生成：老文件全复用，断链自动接上
        std::fs::write(repo.join("ghost.py"), "def ghost(z):\n    return z\n").unwrap();
        let s2 = svc.generate_callchain(repo_str, &|_| {}).unwrap();
        assert_eq!(s2.reused_files, 2, "未变文件免解析：{s2:?}");
        assert_eq!(s2.parsed_files, 1, "只解析新文件");
        let g2 = svc.read_callgraph(repo_str, None).unwrap().unwrap();
        assert!(
            g2["edges"].as_array().unwrap().iter().any(|e| {
                e["src"].as_str().unwrap().ends_with("top")
                    && e["dst"].as_str().unwrap().ends_with("ghost")
            }),
            "新符号出现后断链调用点应复活成 heuristic 边"
        );
        assert!(
            !g2["unresolved_refs"].as_array().unwrap().iter().any(|u| u["name"] == "ghost"),
            "连上边后从未连边清单消失"
        );
    }

    /// P2 边界报告：走读链带出未连边调用点（断在哪、可能到哪）+ 出边 kind
    #[test]
    fn chain_reports_break_sites_and_link_kinds() {
        let dir = tempfile::tempdir().unwrap();
        let repo = setup_repo(dir.path());
        std::fs::write(
            repo.join("amb.py"),
            "def drive(x):\n    return x.run()\n\nclass A:\n    def run(self):\n        return 1\n\nclass B:\n    def run(self):\n        return 2\n",
        )
        .unwrap();
        let svc = AppService::open_at(dir.path().join("app"), dir.path().join("data")).unwrap();
        let repo_str = repo.to_str().unwrap();
        svc.generate_callchain(repo_str, &|_| {}).unwrap();

        let chain = svc.callgraph_chain(repo_str, None, "amb::drive", 50).unwrap().unwrap();
        let entry = &chain.entries[0];
        assert_eq!(entry.id, "amb::drive");
        assert_eq!(entry.breaks.len(), 1, "x.run() 双候选未连边 → 断点：{:?}", entry.breaks);
        assert_eq!(entry.breaks[0].name, "run");
        assert_eq!(entry.breaks[0].candidates.len(), 2, "候选目标留档");

        // 出边 kind 随链带出（hi→helper 是 local）
        let chain2 = svc.callgraph_chain(repo_str, None, "app::hi", 50).unwrap().unwrap();
        assert_eq!(chain2.entries[0].calls[0].kind, "local");
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

    /// #573 验收：ZCode 级大图查询耗时（目标 <50ms；首次加载为 db 全表扫描，
    /// 后续查询缓存命中。本机为多进程桌面环境，取 5 次最小值过滤调度噪声）
    #[test]
    #[ignore = "性能验收：需本机存在 ZCode 仓库且已生成调用链"]
    fn zcode_query_perf() {
        let path = r"C:\dev-data\fork\ZCode";
        if !Path::new(path).is_dir() {
            println!("[skip] {path} 不存在");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let svc = AppService::open_at(tmp.path().join("app"), tmp.path().join("data")).unwrap();
        svc.open_repo(path).unwrap();

        let t0 = std::time::Instant::now();
        let ov = svc.callgraph_overview(path, None).unwrap().unwrap();
        println!("overview(首次含加载) {}ms", t0.elapsed().as_millis());
        assert!(ov.total_nodes > 10_000, "ZCode 应为万级节点");

        let reps = 5;
        let seed = ov.entries[0]["id"].as_str().unwrap().to_string();
        let mut best = (u128::MAX, u128::MAX, u128::MAX, u128::MAX);
        for _ in 0..reps {
            let t = std::time::Instant::now();
            let _ = svc.callgraph_overview(path, None).unwrap();
            best.0 = best.0.min(t.elapsed().as_millis());

            let t = std::time::Instant::now();
            let hits = svc.callgraph_search(path, None, "handler", 50).unwrap();
            let _ = hits.len();
            best.1 = best.1.min(t.elapsed().as_millis());

            let t = std::time::Instant::now();
            let _ = svc.callgraph_focus(path, None, &seed, 2, 300).unwrap();
            best.2 = best.2.min(t.elapsed().as_millis());

            let t = std::time::Instant::now();
            let chain = svc.callgraph_chain(path, None, &seed, 400).unwrap().unwrap();
            let _ = chain.entries.len();
            best.3 = best.3.min(t.elapsed().as_millis());
        }
        println!(
            "min over {reps}：overview {}ms · search {}ms · focus {}ms · chain {}ms",
            best.0, best.1, best.2, best.3
        );
        assert!(best.0 < 50, "overview 超预算");
        assert!(best.2 < 50, "focus 超预算");
        assert!(best.3 < 50, "chain 超预算");
    }
}

/// 在 `.bytewiki/branches/*/graphs/callchain/` 里找记录了同一 HEAD 的产物目录
/// （排除目标分支自身；仅 db 产物可比对——json 无 meta）
fn find_same_commit_callgraph(root: &Path, exclude_dir: &str, head: &str) -> Option<std::path::PathBuf> {
    let branches = root.join(".bytewiki").join("branches");
    let rd = std::fs::read_dir(&branches).ok()?;
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name == exclude_dir {
            continue;
        }
        let db = e.path().join("graphs").join("callchain").join(CALLGRAPH_DB_FILE);
        if db.is_file() && crate::callchain_db::db_head_commit(&db).as_deref() == Some(head) {
            return Some(e.path().join("graphs").join("callchain"));
        }
    }
    None
}
