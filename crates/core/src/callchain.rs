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
        let opts = byte_wiki_callchain::ExtractOptions {
            entry_regex: None,
            excludes: excludes.into_iter().collect(),
            embed_source: false,
        };

        let graph = byte_wiki_callchain::extract(root, &opts, on_progress)
            .map_err(|e| CoreError::Other(anyhow::anyhow!("调用链提取失败：{e}")))?;

        // 产物写入仓库内（随仓库共享，FR-08）
        let out_dir = root.join(".bytewiki").join("graphs").join("callchain");
        std::fs::create_dir_all(&out_dir)?;
        let out_path = out_dir.join("callgraph.json");
        std::fs::write(&out_path, serde_json::to_string_pretty(&graph)?)?;

        // 登记 wiki_docs（kind=graph；upsert——wiki 重新生成的整体替换后由 assemble 重新补登）
        self.db
            .upsert_generated_doc(repo.id, "graph", CALLGRAPH_DOC_PATH, "调用链图谱")?;
        self.db.log_change(
            repo.id,
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

    /// 读取既有调用链产物（不存在返回 None；克隆加载 / 桌面端调用链 tab 用）
    pub fn read_callgraph(&self, repo_path: &str) -> CoreResult<Option<serde_json::Value>> {
        let p = Path::new(repo_path).join(".bytewiki").join(CALLGRAPH_DOC_PATH);
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
        let out = repo.join(".bytewiki").join("graphs").join("callchain").join("callgraph.json");
        assert!(out.is_file(), "产物落位 .bytewiki/graphs/callchain/");

        // 登记为 kind=graph 的生成区文档
        let docs = svc.db.list_wiki_docs(1).unwrap();
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
        let wiki = repo.join(".bytewiki").join("graphs").join("callchain");
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
