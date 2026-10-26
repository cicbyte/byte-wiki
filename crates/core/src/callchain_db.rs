//! 调用链 SQLite 存储（callgraph.db，#573）
//!
//! 设计见 bcode docs/design/callchain-sqlite.md：
//! - db 是查询主存储（点/边/邻接查询毫秒级、增量更新地基）；
//! - json 保留为互操作导出（前端中小图整包读取、克隆共享、外部工具仍读它）——生成时双写；
//! - 读取侧：优先 callgraph.db，缺失回落 callgraph.json（老仓库不迁移）；
//! - 两种后端统一加载为类型化 GraphData（比 serde_json::Value 树省 5-10 倍内存），
//!   四查询（概览/子图/搜索/走读链）共享同一实现——db/json 语义天然一致。

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::Path;

use crate::{CoreError, CoreResult};

/// 调用链 db 产物文件名（分支目录 graphs/callchain/ 下）
pub const CALLGRAPH_DB_FILE: &str = "callgraph.db";

// ============ 类型化图模型 ============

/// 函数节点（查询引擎统一输入；字段口径与引擎 json 产物一致）
#[derive(Debug, Clone, PartialEq)]
pub struct GNode {
    pub id: String,
    pub module: String,
    pub qualname: String,
    pub name: String,
    pub file: String,
    pub lineno: u64,
    pub end_lineno: u64,
    pub is_async: bool,
    pub is_entry: bool,
    pub is_exported: bool,
    pub routes: Vec<String>,
    /// 入度/出度（按归一化边集重算，双后端一致）
    pub callers: usize,
    pub callees: usize,
}

impl GNode {
    /// 引擎 json 节点形状（前端依赖的字段集合，含度数）
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id, "module": self.module, "qualname": self.qualname,
            "name": self.name, "file": self.file,
            "lineno": self.lineno, "end_lineno": self.end_lineno,
            "routes": self.routes, "async": self.is_async,
            "is_entry": self.is_entry, "is_exported": self.is_exported,
            "callers": self.callers, "callees": self.callees,
        })
    }
}

/// 调用边（src/dst 为节点全限定 id）
#[derive(Debug, Clone, PartialEq)]
pub struct GEdge {
    pub src: String,
    pub dst: String,
    pub line: u64,
    /// 调用点被调名 token 的 0-based 字节列（LSP definition 定位用；老产物为 0）
    pub col: u64,
    pub kind: String,
}

impl GEdge {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({"src": self.src, "dst": self.dst, "line": self.line, "col": self.col, "kind": self.kind})
    }
}

/// 未连边的调用点（边界报告素材）：断在哪（src/line/col）、叫什么（name/recv）、
/// 可能到哪（candidates——消歧失败的候选 qualname）
#[derive(Debug, Clone, PartialEq)]
pub struct GUnresolved {
    pub src: String,
    pub name: String,
    pub recv: Option<String>,
    pub line: u64,
    pub col: u64,
    pub candidates: Vec<String>,
}

impl GUnresolved {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "src": self.src, "name": self.name, "recv": self.recv,
            "line": self.line, "col": self.col, "candidates": self.candidates,
        })
    }
}

/// 全图（加载完成后查询全在内存索引上做，无重复 IO/解析）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GraphData {
    pub root: String,
    pub nodes: Vec<GNode>,
    pub edges: Vec<GEdge>,
    /// 未连边调用点（边界报告素材）
    pub unresolved: Vec<GUnresolved>,
    /// module -> (代表 file, 函数数)；同 module 多文件后者覆盖（与引擎 json Map 语义一致）
    pub modules: BTreeMap<String, (String, usize)>,
    pub langs: BTreeMap<String, usize>,
    /// 生成时的 HEAD commit（滞后检测：与实时 head_commit 比对；无 git 为空）
    pub head_commit: String,
}

impl GraphData {
    /// 还原为引擎 json 文档形状（CLI query / 既有 read_callgraph 消费方）
    pub fn to_doc_json(&self) -> serde_json::Value {
        let modules: serde_json::Map<String, serde_json::Value> = self
            .modules
            .iter()
            .map(|(m, (f, n))| (m.clone(), serde_json::json!({"file": f, "funcs": n})))
            .collect();
        serde_json::json!({
            "root": self.root,
            "nodes": self.nodes.iter().map(|n| n.to_json()).collect::<Vec<_>>(),
            "edges": self.edges.iter().map(|e| e.to_json()).collect::<Vec<_>>(),
            "modules": modules,
            "langs": self.langs,
            "unresolved_refs": self.unresolved.iter().map(|u| u.to_json()).collect::<Vec<_>>(),
        })
    }

    /// 规范化：节点按 id 排序并去重（同 id 保首个——.js/.ts 迁移并存的双胞胎文件）、
    /// 边按 (src,dst) 排序 + 重算度数。db（主键序）与 json（文件遍历序）
    /// 必须产出一致的内存序与内容，查询输出才可复现。
    fn finalize(&mut self) {
        if !self.nodes.is_sorted_by(|a, b| a.id <= b.id) {
            self.nodes.sort_by(|a, b| a.id.cmp(&b.id));
        }
        self.nodes.dedup_by(|a, b| a.id == b.id);
        if !self.edges.is_sorted_by(|a, b| (&a.src, &a.dst) <= (&b.src, &b.dst)) {
            self.edges.sort_by(|a, b| (&a.src, &a.dst).cmp(&(&b.src, &b.dst)));
        }
        self.unresolved.sort_by(|a, b| {
            (&a.src, &a.name, a.line, a.col).cmp(&(&b.src, &b.name, b.line, b.col))
        });
        self.recompute_degrees();
    }

    /// 重算入度/出度（边归一化后）
    fn recompute_degrees(&mut self) {
        let mut indeg: HashMap<&str, usize> = HashMap::new();
        let mut outdeg: HashMap<&str, usize> = HashMap::new();
        for e in &self.edges {
            *indeg.entry(e.dst.as_str()).or_default() += 1;
            *outdeg.entry(e.src.as_str()).or_default() += 1;
        }
        for n in &mut self.nodes {
            n.callers = indeg.get(n.id.as_str()).copied().unwrap_or(0);
            n.callees = outdeg.get(n.id.as_str()).copied().unwrap_or(0);
        }
    }
}

/// 边归一化：同 (src,dst) 多边取最早调用行（引擎按 (callee,recv) 去重，
/// 不同 callee/recv 仍可能连到同一目标；json 原样保留，加载进索引时归并——
/// db 主键 (src,dst) 落库时同规则，双后端语义一致）
fn normalize_edges(raw: impl Iterator<Item = (String, String, u64, u64, String)>) -> Vec<GEdge> {
    let mut merged: HashMap<(String, String), (u64, u64, String)> = HashMap::new();
    for (src, dst, line, col, kind) in raw {
        match merged.entry((src, dst)) {
            std::collections::hash_map::Entry::Occupied(mut o) => {
                let (l, c, _) = o.get_mut();
                if line < *l {
                    *l = line;
                    *c = col;
                }
            }
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert((line, col, kind));
            }
        }
    }
    // 插入序无关：加载完成后 finalize 统一按 (src,dst) 排序
    merged
        .into_iter()
        .map(|((src, dst), (line, col, kind))| GEdge { src, dst, line, col, kind })
        .collect()
}

/// 从引擎 json 产物解析（json 回落路径 / 测试基线）
pub fn graph_from_doc_json(doc: &serde_json::Value) -> GraphData {
    let nodes: Vec<GNode> = doc["nodes"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|n| GNode {
                    id: n["id"].as_str().unwrap_or_default().to_string(),
                    module: n["module"].as_str().unwrap_or_default().to_string(),
                    qualname: n["qualname"].as_str().unwrap_or_default().to_string(),
                    name: n["name"].as_str().unwrap_or_default().to_string(),
                    file: n["file"].as_str().unwrap_or_default().to_string(),
                    lineno: n["lineno"].as_u64().unwrap_or(0),
                    end_lineno: n["end_lineno"].as_u64().unwrap_or(0),
                    is_async: n["async"].as_bool().unwrap_or(false),
                    is_entry: n["is_entry"].as_bool().unwrap_or(false),
                    is_exported: n["is_exported"].as_bool().unwrap_or(false),
                    routes: n["routes"].as_array().map(|r| r.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default(),
                    callers: 0,
                    callees: 0,
                })
                .collect()
        })
        .unwrap_or_default();
    let edges = normalize_edges(
        doc["edges"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|e| {
                (
                    e["src"].as_str().unwrap_or_default().to_string(),
                    e["dst"].as_str().unwrap_or_default().to_string(),
                    e["line"].as_u64().unwrap_or(0),
                    e["col"].as_u64().unwrap_or(0),
                    e["kind"].as_str().unwrap_or("local").to_string(),
                )
            }),
    );
    let unresolved: Vec<GUnresolved> = doc["unresolved_refs"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|u| GUnresolved {
            src: u["src"].as_str().unwrap_or_default().to_string(),
            name: u["name"].as_str().unwrap_or_default().to_string(),
            recv: u["recv"].as_str().map(String::from),
            line: u["line"].as_u64().unwrap_or(0),
            col: u["col"].as_u64().unwrap_or(0),
            candidates: u["candidates"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default(),
        })
        .collect();
    let modules = doc["modules"]
        .as_object()
        .map(|m| {
            m.iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        (
                            v["file"].as_str().unwrap_or_default().to_string(),
                            v["funcs"].as_u64().unwrap_or(0) as usize,
                        ),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let langs = doc["langs"]
        .as_object()
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_u64().unwrap_or(0) as usize)).collect())
        .unwrap_or_default();
    let mut g = GraphData {
        root: doc["root"].as_str().unwrap_or_default().to_string(),
        nodes,
        edges,
        unresolved,
        modules,
        langs,
        head_commit: String::new(), // json 产物无 meta（db 才有滞后检测）
    };
    g.finalize();
    g
}

// ============ 写库 ============

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS nodes (
  id          TEXT PRIMARY KEY,
  module      TEXT NOT NULL,
  qualname    TEXT NOT NULL,
  name        TEXT NOT NULL,
  file        TEXT NOT NULL,
  lineno      INTEGER NOT NULL,
  end_lineno  INTEGER NOT NULL,
  is_async    INTEGER NOT NULL DEFAULT 0,
  is_entry    INTEGER NOT NULL DEFAULT 0,
  is_exported INTEGER NOT NULL DEFAULT 0,
  routes      TEXT NOT NULL DEFAULT '[]'
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_nodes_file   ON nodes(file);
CREATE INDEX IF NOT EXISTS idx_nodes_module ON nodes(module);
CREATE INDEX IF NOT EXISTS idx_nodes_entry  ON nodes(is_entry) WHERE is_entry = 1;
CREATE INDEX IF NOT EXISTS idx_nodes_export ON nodes(is_exported) WHERE is_exported = 1;

CREATE TABLE IF NOT EXISTS edges (
  src  TEXT NOT NULL,
  dst  TEXT NOT NULL,
  line INTEGER NOT NULL,
  col  INTEGER NOT NULL DEFAULT 0,
  kind TEXT NOT NULL,
  PRIMARY KEY (src, dst)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_edges_dst ON edges(dst, src);

CREATE TABLE IF NOT EXISTS unresolved_refs (
  src        TEXT NOT NULL,
  name       TEXT NOT NULL,
  recv       TEXT,
  line       INTEGER NOT NULL,
  col        INTEGER NOT NULL DEFAULT 0,
  candidates TEXT NOT NULL DEFAULT '[]',
  PRIMARY KEY (src, name, line)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_unres_name ON unresolved_refs(name);

CREATE TABLE IF NOT EXISTS file_recs (
  path TEXT PRIMARY KEY,
  hash INTEGER NOT NULL,
  size INTEGER NOT NULL,
  rec  TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
) WITHOUT ROWID;
";

/// 老库自愈：v1 产物的 edges 无 col 列（LSP 精确化定位坐标是后加的）——
/// CREATE IF NOT EXISTS 不会补列，显式 ALTER；已存在时报重复列，忽略即可。
/// 读写两侧都调（克隆共享的老 db 只读路径也要能查）。
fn migrate_conn(conn: &rusqlite::Connection) -> CoreResult<()> {
    conn.execute_batch(SCHEMA)?;
    let _ = conn.execute("ALTER TABLE edges ADD COLUMN col INTEGER NOT NULL DEFAULT 0", []);
    Ok(())
}

/// 把引擎 json 产物写入 callgraph.db（整图事务替换；INSERT OR REPLACE 容忍重复 (src,dst)）。
/// head_commit：生成时的 git HEAD（滞后检测用，无 git 传空）
pub fn write_callgraph_db(db_path: &Path, doc: &serde_json::Value, head_commit: &str) -> CoreResult<()> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut conn = rusqlite::Connection::open(db_path)?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(CoreError::Db)?;
    conn.execute_batch(
        "PRAGMA synchronous = NORMAL; PRAGMA busy_timeout = 5000; PRAGMA user_version = 2;",
    )?;
    migrate_conn(&conn)?;
    let tx = conn.transaction().map_err(CoreError::Db)?;
    tx.execute("DELETE FROM nodes", []).map_err(CoreError::Db)?;
    tx.execute("DELETE FROM edges", []).map_err(CoreError::Db)?;
    tx.execute("DELETE FROM unresolved_refs", []).map_err(CoreError::Db)?;
    tx.execute("DELETE FROM meta", []).map_err(CoreError::Db)?;
    {
        let mut stmt = tx
            .prepare(
                "INSERT OR IGNORE INTO nodes (id,module,qualname,name,file,lineno,end_lineno,is_async,is_entry,is_exported,routes)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            )
            .map_err(CoreError::Db)?;
        // OR IGNORE：同 id 重复节点（如 .js/.ts 迁移并存的双胞胎文件）保首个——
        // 与加载侧 finalize 的去重规则一致，双后端语义对齐
        for n in doc["nodes"].as_array().unwrap_or(&Vec::new()) {
            let routes = match n["routes"].as_array() {
                Some(r) => serde_json::to_string(r)?,
                None => "[]".to_string(),
            };
            stmt.execute(rusqlite::params![
                n["id"].as_str().unwrap_or_default(),
                n["module"].as_str().unwrap_or_default(),
                n["qualname"].as_str().unwrap_or_default(),
                n["name"].as_str().unwrap_or_default(),
                n["file"].as_str().unwrap_or_default(),
                n["lineno"].as_u64().unwrap_or(0) as i64,
                n["end_lineno"].as_u64().unwrap_or(0) as i64,
                n["async"].as_bool().unwrap_or(false) as i64,
                n["is_entry"].as_bool().unwrap_or(false) as i64,
                n["is_exported"].as_bool().unwrap_or(false) as i64,
                routes,
            ])
            .map_err(CoreError::Db)?;
        }
    }
    {
        // 归一化后落库：同 (src,dst) 取最早调用行（主键约束下重复边必须归并）
        let edges = normalize_edges(
            doc["edges"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|e| {
                    (
                        e["src"].as_str().unwrap_or_default().to_string(),
                        e["dst"].as_str().unwrap_or_default().to_string(),
                        e["line"].as_u64().unwrap_or(0),
                        e["col"].as_u64().unwrap_or(0),
                        e["kind"].as_str().unwrap_or("local").to_string(),
                    )
                }),
        );
        let mut stmt = tx
            .prepare("INSERT INTO edges (src,dst,line,col,kind) VALUES (?1,?2,?3,?4,?5)")
            .map_err(CoreError::Db)?;
        for e in edges {
            stmt.execute(rusqlite::params![e.src, e.dst, e.line as i64, e.col as i64, e.kind])
                .map_err(CoreError::Db)?;
        }
    }
    {
        // 未连边调用点（边界报告素材）——同 (src,name,line) 主键 OR IGNORE 容忍重复
        let mut stmt = tx
            .prepare("INSERT OR IGNORE INTO unresolved_refs (src,name,recv,line,col,candidates) VALUES (?1,?2,?3,?4,?5,?6)")
            .map_err(CoreError::Db)?;
        for u in doc["unresolved_refs"].as_array().into_iter().flatten() {
            stmt.execute(rusqlite::params![
                u["src"].as_str().unwrap_or_default(),
                u["name"].as_str().unwrap_or_default(),
                u["recv"].as_str(),
                u["line"].as_u64().unwrap_or(0) as i64,
                u["col"].as_u64().unwrap_or(0) as i64,
                serde_json::to_string(&u["candidates"])?,
            ])
            .map_err(CoreError::Db)?;
        }
    }
    {
        let mut stmt = tx
            .prepare("INSERT INTO meta (key,value) VALUES (?1,?2)")
            .map_err(CoreError::Db)?;
        let mut put = |k: &str, v: String| -> CoreResult<()> {
            stmt.execute(rusqlite::params![k, v]).map_err(CoreError::Db)?;
            Ok(())
        };
        put("root", doc["root"].as_str().unwrap_or_default().to_string())?;
        put("head_commit", head_commit.to_string())?;
        put("langs", serde_json::to_string(&doc["langs"])?)?;
        put("modules", serde_json::to_string(&doc["modules"])?)?;
        put("generated_at", crate::db::now_iso())?;
        put("nodes_count", doc["nodes"].as_array().map(|a| a.len()).unwrap_or(0).to_string())?;
        put("edges_count", doc["edges"].as_array().map(|a| a.len()).unwrap_or(0).to_string())?;
    }
    tx.commit().map_err(CoreError::Db)?;
    // 收尾并截断 WAL：共享/克隆 .bytewiki 时不留大 sidecar
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    Ok(())
}

// ============ 读库 ============

/// 只读某 db 的 meta head_commit（同 commit 免重算的探测用；无值返回 None）
pub fn db_head_commit(db_path: &Path) -> Option<String> {
    let conn = rusqlite::Connection::open(db_path).ok()?;
    conn.query_row(
        "SELECT value FROM meta WHERE key = 'head_commit'",
        [],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .filter(|s| !s.is_empty())
}

/// 从 callgraph.db 加载全图（26k 节点 + 60k 边量级约数百 ms 一次性加载，缓存摊销）
pub fn load_callgraph_db(db_path: &Path) -> CoreResult<GraphData> {
    let conn = rusqlite::Connection::open(db_path)?;
    // 读侧性能 pragma：mmap 免 read 拷贝、页缓存给足（WITHOUT ROWID 全表扫 = 主键序）
    conn.execute_batch("PRAGMA mmap_size = 268435456; PRAGMA cache_size = -65536;")?;
    migrate_conn(&conn)?;
    let mut nodes: Vec<GNode> = Vec::new();
    {
        let mut stmt = conn
            .prepare("SELECT id,module,qualname,name,file,lineno,end_lineno,is_async,is_entry,is_exported,routes FROM nodes")?;
        let rows = stmt.query_map([], |r| {
            let routes_json: String = r.get(10)?;
            let routes: Vec<String> =
                serde_json::from_str(&routes_json).unwrap_or_default();
            Ok(GNode {
                id: r.get(0)?,
                module: r.get(1)?,
                qualname: r.get(2)?,
                name: r.get(3)?,
                file: r.get(4)?,
                lineno: r.get::<_, i64>(5)? as u64,
                end_lineno: r.get::<_, i64>(6)? as u64,
                is_async: r.get::<_, i64>(7)? != 0,
                is_entry: r.get::<_, i64>(8)? != 0,
                is_exported: r.get::<_, i64>(9)? != 0,
                routes,
                callers: 0,
                callees: 0,
            })
        })?;
        for row in rows {
            nodes.push(row?);
        }
    }
    let mut edges: Vec<GEdge> = Vec::new();
    {
        let mut stmt = conn.prepare("SELECT src,dst,line,col,kind FROM edges")?;
        let rows = stmt.query_map([], |r| {
            Ok(GEdge {
                src: r.get(0)?,
                dst: r.get(1)?,
                line: r.get::<_, i64>(2)? as u64,
                col: r.get::<_, i64>(3)? as u64,
                kind: r.get(4)?,
            })
        })?;
        for row in rows {
            edges.push(row?);
        }
    }
    let mut unresolved: Vec<GUnresolved> = Vec::new();
    {
        let mut stmt =
            conn.prepare("SELECT src,name,recv,line,col,candidates FROM unresolved_refs")?;
        let rows = stmt.query_map([], |r| {
            let cands_json: String = r.get(5)?;
            Ok(GUnresolved {
                src: r.get(0)?,
                name: r.get(1)?,
                recv: r.get(2)?,
                line: r.get::<_, i64>(3)? as u64,
                col: r.get::<_, i64>(4)? as u64,
                candidates: serde_json::from_str(&cands_json).unwrap_or_default(),
            })
        })?;
        for row in rows {
            unresolved.push(row?);
        }
    }
    let meta: HashMap<String, String> = {
        let mut stmt = conn.prepare("SELECT key,value FROM meta")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.collect::<Result<HashMap<String, String>, _>>()?
    };
    let modules = meta
        .get("modules")
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|v| v.as_object().map(|m| {
            m.iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        (
                            v["file"].as_str().unwrap_or_default().to_string(),
                            v["funcs"].as_u64().unwrap_or(0) as usize,
                        ),
                    )
                })
                .collect()
        }))
        .unwrap_or_default();
    let langs = meta
        .get("langs")
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|v| v.as_object().map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_u64().unwrap_or(0) as usize)).collect()))
        .unwrap_or_default();
    let mut g = GraphData {
        root: meta.get("root").cloned().unwrap_or_default(),
        nodes,
        edges,
        unresolved,
        modules,
        langs,
        head_commit: meta.get("head_commit").cloned().unwrap_or_default(),
    };
    g.finalize();
    Ok(g)
}

// ============ 增量缓存（file_recs） ============

/// 读增量缓存表：rel_path → (content 指纹, 字节数, rec_json)。无库/无表返回空
///（首跑或老产物 → 全量解析，属预期路径）
pub fn load_file_recs(db_path: &Path) -> HashMap<String, (u64, u64, String)> {
    if !db_path.is_file() {
        return HashMap::new();
    }
    let Ok(conn) = rusqlite::Connection::open(db_path) else {
        return HashMap::new();
    };
    if migrate_conn(&conn).is_err() {
        return HashMap::new();
    }
    let mut out = HashMap::new();
    let Ok(mut stmt) = conn.prepare("SELECT path,hash,size,rec FROM file_recs") else {
        return out;
    };
    let Ok(rows) = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)? as u64,
            r.get::<_, i64>(2)? as u64,
            r.get::<_, String>(3)?,
        ))
    }) else {
        return out;
    };
    for row in rows.flatten() {
        out.insert(row.0, (row.1, row.2, row.3));
    }
    out
}

/// 整体替换增量缓存表（rows 是本次生成的全量 rec 行——含缓存复用项；
/// 不在表里的文件自然消失，无需单独删）
pub fn write_file_recs(db_path: &Path, rows: &[(String, u64, u64, String)]) -> CoreResult<()> {
    let mut conn = rusqlite::Connection::open(db_path)?;
    migrate_conn(&conn)?;
    let tx = conn.transaction().map_err(CoreError::Db)?;
    tx.execute("DELETE FROM file_recs", []).map_err(CoreError::Db)?;
    {
        let mut stmt = tx
            .prepare("INSERT INTO file_recs (path,hash,size,rec) VALUES (?1,?2,?3,?4)")
            .map_err(CoreError::Db)?;
        for (p, h, s, rec) in rows {
            stmt.execute(rusqlite::params![p, *h as i64, *s as i64, rec])
                .map_err(CoreError::Db)?;
        }
    }
    tx.commit().map_err(CoreError::Db)?;
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    Ok(())
}

// ============ 测试 ============

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_doc() -> serde_json::Value {
        serde_json::json!({
            "root": "C:/tmp/repo",
            "nodes": [
                {"id": "app::main", "module": "app", "qualname": "main", "name": "main",
                 "file": "app/main.py", "lineno": 1, "end_lineno": 9, "routes": [],
                 "async": false, "is_entry": true, "is_exported": false},
                {"id": "app::svc.handle", "module": "app", "qualname": "svc.handle", "name": "handle",
                 "file": "app/svc.py", "lineno": 10, "end_lineno": 20, "routes": ["GET /x"],
                 "async": true, "is_entry": true, "is_exported": true},
                {"id": "app::util.trim", "module": "app", "qualname": "util.trim", "name": "trim",
                 "file": "app/util.py", "lineno": 3, "end_lineno": 5, "routes": [],
                 "async": false, "is_entry": false, "is_exported": true}
            ],
            // 重复边：main→handle 两条（不同行）——归一化取最早行
            "edges": [
                {"src": "app::main", "dst": "app::svc.handle", "line": 5, "kind": "heuristic"},
                {"src": "app::main", "dst": "app::svc.handle", "line": 2, "kind": "local"},
                {"src": "app::svc.handle", "dst": "app::util.trim", "line": 15, "kind": "local"}
            ],
            "modules": {"app": {"file": "app/main.py", "funcs": 3}},
            "langs": {"python": 3}
        })
    }

    #[test]
    fn db_roundtrip_matches_json_parse() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(CALLGRAPH_DB_FILE);
        let doc = sample_doc();
        write_callgraph_db(&db, &doc, "").unwrap();

        let from_db = load_callgraph_db(&db).unwrap();
        let from_json = graph_from_doc_json(&doc);
        assert_eq!(from_db, from_json, "db 与 json 双后端加载结果必须一致");
        assert_eq!(from_db.nodes.len(), 3);
        // 归一化：重复边归并为 1 条、取最早行 2
        assert_eq!(from_db.edges.len(), 2);
        assert_eq!(from_db.edges[0].line, 2);
        // 度数按归一化边集：main 出度 1（非原始 2）
        let main = from_db.nodes.iter().find(|n| n.id == "app::main").unwrap();
        assert_eq!(main.callees, 1);
        let handle = from_db.nodes.iter().find(|n| n.id == "app::svc.handle").unwrap();
        assert_eq!(handle.callers, 1);
        assert!(handle.is_async && handle.is_entry && handle.is_exported);
        assert_eq!(handle.routes, vec!["GET /x".to_string()]);
        // 还原 json 形状完整
        let redoc = from_db.to_doc_json();
        assert_eq!(redoc["nodes"].as_array().unwrap().len(), 3);
        assert_eq!(redoc["langs"]["python"], 3);
        assert_eq!(redoc["modules"]["app"]["file"], "app/main.py");
    }

    #[test]
    fn duplicate_node_ids_deduped_keep_first() {
        // ZCode 实测场景：armsRumShared.js 与 .ts 并存（迁移残留）→ 同 id 双节点，
        // json 数组容忍、db 主键不容忍——双后端统一「保首个」
        let mut doc = sample_doc();
        let twin = serde_json::json!({
            "id": "app::main", "module": "app", "qualname": "main", "name": "main",
            "file": "app/main.ts", "lineno": 100, "end_lineno": 200, "routes": [],
            "async": true, "is_entry": false, "is_exported": false
        });
        doc["nodes"].as_array_mut().unwrap().push(twin);
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(CALLGRAPH_DB_FILE);
        write_callgraph_db(&db, &doc, "aa11").unwrap(); // 不再 PRIMARY KEY 冲突

        let from_db = load_callgraph_db(&db).unwrap();
        assert_eq!(from_db.head_commit, "aa11", "meta head_commit 落库可回读");
        let from_json = graph_from_doc_json(&doc);
        assert_eq!(from_db.nodes, from_json.nodes);
        assert_eq!(from_db.edges, from_json.edges);
        assert_eq!(from_db.modules, from_json.modules);
        assert_eq!(from_db.langs, from_json.langs);
        assert_eq!(from_db.nodes.len(), 3, "重复 id 去重");
        let main = from_db.nodes.iter().find(|n| n.id == "app::main").unwrap();
        assert_eq!(main.file, "app/main.py", "保首个（js 在前 ts 在后被丢）");
    }

    #[test]
    fn rewrite_replaces_whole_graph() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(CALLGRAPH_DB_FILE);
        write_callgraph_db(&db, &sample_doc(), "aa11").unwrap();
        // 第二次生成只含 1 节点——旧节点/边/meta 必须清干净
        let smaller = serde_json::json!({
            "root": "C:/tmp/repo2",
            "nodes": [{"id": "x::f", "module": "x", "qualname": "f", "name": "f",
                        "file": "x.py", "lineno": 1, "end_lineno": 2, "routes": [],
                        "async": false, "is_entry": false, "is_exported": false}],
            "edges": [],
            "modules": {"x": {"file": "x.py", "funcs": 1}},
            "langs": {"python": 1}
        });
        write_callgraph_db(&db, &smaller, "bb22").unwrap();
        let g = load_callgraph_db(&db).unwrap();
        assert_eq!(g.nodes.len(), 1);
        assert_eq!(g.edges.len(), 0);
        assert_eq!(g.root, "C:/tmp/repo2");
    }

    #[test]
    fn wal_sidecar_truncated_after_write() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(CALLGRAPH_DB_FILE);
        write_callgraph_db(&db, &sample_doc(), "aa11").unwrap();
        let wal = dir.path().join(format!("{CALLGRAPH_DB_FILE}-wal"));
        if wal.is_file() {
            let sz = wal.metadata().unwrap().len();
            assert_eq!(sz, 0, "checkpoint(TRUNCATE) 后 wal 应为空，实际 {sz} 字节");
        }
    }

    #[test]
    fn file_recs_roundtrip_and_whole_replace() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(CALLGRAPH_DB_FILE);
        write_callgraph_db(&db, &sample_doc(), "aa11").unwrap();
        write_file_recs(&db, &[("a.py".into(), 11, 22, "{\"module\":\"a\"}".into())]).unwrap();
        let loaded = load_file_recs(&db);
        assert_eq!(loaded.get("a.py"), Some(&(11, 22, "{\"module\":\"a\"}".to_string())));
        // 整体替换语义：不在本次行集里的 key 自然消失（文件删除场景）
        write_file_recs(&db, &[("b.py".into(), 1, 2, "{}".into())]).unwrap();
        let loaded = load_file_recs(&db);
        assert!(!loaded.contains_key("a.py") && loaded.contains_key("b.py"));
    }

    #[test]
    fn unresolved_refs_roundtrip_with_candidates() {
        let mut doc = sample_doc();
        doc["unresolved_refs"] = serde_json::json!([
            {"src": "app::main", "name": "run", "recv": "x", "line": 7, "col": 4,
             "candidates": ["app::A.run", "app::B.run"]}
        ]);
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(CALLGRAPH_DB_FILE);
        write_callgraph_db(&db, &doc, "aa11").unwrap();
        let from_db = load_callgraph_db(&db).unwrap();
        let from_json = graph_from_doc_json(&doc);
        assert_eq!(from_db.unresolved, from_json.unresolved, "unresolved 双后端一致");
        assert_eq!(from_db.unresolved.len(), 1);
        let u = &from_db.unresolved[0];
        assert_eq!(u.name, "run");
        assert_eq!(u.recv.as_deref(), Some("x"));
        assert_eq!(u.candidates, vec!["app::A.run".to_string(), "app::B.run".to_string()]);
    }
}
