//! 分支维度基础设施
//!
//! 项目身份 = 仓库路径（不变）；产物/索引/向量按分支独立：
//! - 当前分支：解析 `.git/HEAD`（`ref: refs/heads/<b>` → b；detached → ""）
//! - 分支产物根：`.bytewiki/branches/<branch_dir>/`（pages/overview/outline/
//!   index/graphs 都在其中）；知识卡例外——`.bytewiki/cards/` 跨分支共享
//! - 旧布局（.bytewiki 顶层直接放 pages 等）首次打开时整体搬入当前分支目录

use std::path::{Path, PathBuf};

use crate::CoreResult;

/// 解析当前分支（无 git / detached HEAD 返回 ""，归入 default 桶）
pub fn current_branch(root: &Path) -> String {
    let head = std::fs::read_to_string(root.join(".git").join("HEAD")).unwrap_or_default();
    let head = head.trim();
    head.strip_prefix("ref: refs/heads/")
        .map(str::to_string)
        .unwrap_or_default()
}

/// 分支的目录安全名（"" → default；路径分隔符等替换为 -）
pub fn branch_dir(branch: &str) -> String {
    let name: String = branch
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => c,
            _ => '-',
        })
        .collect();
    if name.is_empty() {
        "default".to_string()
    } else {
        name
    }
}

/// 分支维度的 wiki 产物根（cards 等共享内容不在此）
pub fn wiki_root(root: &Path, branch: &str) -> PathBuf {
    root.join(".bytewiki").join("branches").join(branch_dir(branch))
}

/// 共享知识卡目录（跨分支）
pub fn cards_root(root: &Path) -> PathBuf {
    root.join(".bytewiki").join("cards")
}

/// 旧布局迁移：顶层 pages/outline 等搬入当前分支目录（幂等）
pub fn migrate_legacy_wiki(root: &Path, branch: &str) -> CoreResult<()> {
    let base = root.join(".bytewiki");
    if !base.join("outline.json").is_file() && !base.join("pages").is_dir() {
        return Ok(());
    }
    let target = wiki_root(root, branch);
    std::fs::create_dir_all(&target)?;
    for name in ["pages", "modules", "graphs", "cards"] {
        // cards 先搬走再继续（保留在共享位置），避免 target 嵌套
        if name == "cards" {
            continue;
        }
        let from = base.join(name);
        if from.exists() {
            let to = target.join(name);
            std::fs::create_dir_all(to.parent().unwrap_or(&target))?;
            std::fs::rename(&from, &to)
                .map_err(|e| crate::CoreError::Other(anyhow::anyhow!("分支迁移失败 {name}：{e}")))?;
        }
    }
    for name in ["outline.json", "index.json", "overview.md", "architecture.md", ".gen-meta.json"] {
        let from = base.join(name);
        if from.is_file() {
            let _ = std::fs::rename(&from, target.join(name));
        }
    }
    tracing::info!("旧版 .bytewiki 布局已迁移至分支目录：{}", branch_dir(branch));
    Ok(())
}

/// LLM 缓存命名空间：按仓库路径哈希（跨分支共享——缓存是内容寻址的，
/// 不同分支的相同文件命中同一缓存）
pub fn cache_ns(repo_path: &str) -> String {
    use md5::{Digest, Md5};
    let mut h = Md5::new();
    h.update(repo_path.as_bytes());
    format!("{:x}", h.finalize())[..12].to_string()
}

/// 已生成 wiki 产物的分支目录列表（分支名恢复：目录名仅做了字符替换，
/// 恢复不可逆，因此目录内以 outline.json 无分支信息为由，直接用目录名
/// 作为展示名——生成时的真实分支名记录在 .gen-meta.json，可读则优先）
pub fn generated_branches(root: &Path) -> Vec<String> {
    let branches = root.join(".bytewiki").join("branches");
    let Ok(rd) = std::fs::read_dir(&branches) else {
        return vec![];
    };
    let mut out: Vec<String> = rd
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter(|e| {
            // 有产物特征（outline.json 或 pages/）才算已生成分支
            let d = e.path();
            d.join("outline.json").is_file() || d.join("pages").is_dir()
        })
        .filter_map(|e| {
            let dir_name = e.file_name().to_string_lossy().into_owned();
            // 优先 .gen-meta 不可用；直接展示目录名（生成时 branch_dir 的转写形式）
            Some(dir_name)
        })
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_parse_and_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".git/HEAD"), "ref: refs/heads/feature/x\n").unwrap();
        assert_eq!(current_branch(dir.path()), "feature/x");
        assert_eq!(branch_dir("feature/x"), "feature-x");
        assert_eq!(branch_dir(""), "default");
        assert_eq!(
            wiki_root(dir.path(), "feature/x"),
            dir.path().join(".bytewiki").join("branches").join("feature-x")
        );
        // detached
        std::fs::write(dir.path().join(".git/HEAD"), "6789abc\n").unwrap();
        assert_eq!(current_branch(dir.path()), "");
    }

    #[test]
    fn legacy_migration_moves_products() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join(".bytewiki");
        std::fs::create_dir_all(base.join("pages")).unwrap();
        std::fs::write(base.join("outline.json"), "{}").unwrap();
        std::fs::write(base.join("pages/a.md"), "# a").unwrap();
        std::fs::create_dir_all(base.join("cards")).unwrap();
        std::fs::write(base.join("cards/note.md"), "# 卡").unwrap();
        migrate_legacy_wiki(dir.path(), "main").unwrap();
        let target = wiki_root(dir.path(), "main");
        assert!(target.join("pages/a.md").is_file());
        assert!(target.join("outline.json").is_file());
        // cards 保留共享位置
        assert!(base.join("cards/note.md").is_file());
        // 幂等
        migrate_legacy_wiki(dir.path(), "main").unwrap();
    }
}

#[cfg(test)]
mod branch_isolation_tests {
    use super::*;
    use crate::service::AppService;
    use crate::model::{LlmConfig, LlmProvider};

    /// 端到端分支隔离：main 全量生成 → 切 develop 更新 → 切回 main
    /// 文档原样（字节一致）、零 LLM 调用（缓存+断点续跑全沿用）。
    #[tokio::test]
    async fn switch_branch_isolates_and_reuses() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::create_dir_all(repo.join("apps/cli")).unwrap();
        std::fs::write(repo.join("apps/cli/main.rs"), "fn main() {}\n").unwrap();
        std::fs::create_dir_all(repo.join("crates/core/src")).unwrap();
        std::fs::write(repo.join("crates/core/src/lib.rs"), "pub struct A;\n").unwrap();
        std::fs::write(repo.join("README.md"), "# demo\n").unwrap();
        // mock LLM：大纲固定三页
        let outline_xml = r#"<wiki_structure><title>t</title><description>d</description>
<sections><section id="s1" title="S1" parent=""/></sections>
<pages>
<page id="p1"><title>A</title><importance>high</importance><section>s1</section><file>apps/cli/main.rs</file></page>
<page id="p2"><title>B</title><importance>medium</importance><section>s1</section><file>crates/core/src/lib.rs</file></page>
<page id="p3"><title>C</title><importance>low</importance><section>s1</section><file>README.md</file></page>
</pages></wiki_structure>"#;
        let calls = std::sync::Arc::new(std::sync::Mutex::new(0usize));
        let c2 = calls.clone();
        let app = axum::Router::new().route("/api/chat", axum::routing::post(move |body: String| {
            let c2 = c2.clone();
            async move {
                *c2.lock().unwrap() += 1;
                let is_outline = body.contains("【Wiki 大纲设计】");
                let content = if is_outline { outline_xml.to_string() } else { "# 页\n\n## 简介\nx\n\n## 核心\ny\n\n## 结论\nz\n".to_string() };
                let mut nd = serde_json::to_string(&serde_json::json!({
                    "model": "mock", "created_at": "2026-01-01T00:00:00Z",
                    "message": {"role": "assistant", "content": content}, "done": false,
                })).unwrap();
                nd.push('\n');
                nd.push_str(&serde_json::to_string(&serde_json::json!({
                    "model": "mock", "created_at": "2026-01-01T00:00:00Z",
                    "message": {"role": "assistant", "content": ""},
                    "done": true, "done_reason": "stop", "prompt_eval_count": 1, "eval_count": 1,
                })).unwrap());
                nd.push('\n');
                ([(axum::http::header::CONTENT_TYPE, "application/x-ndjson")], nd)
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { let _ = axum::serve(listener, app).await; });
        let svc = AppService::open_at(dir.path().join("app"), dir.path().join("data")).unwrap();
        svc.set_llm_config(LlmConfig {
            providers: vec![LlmProvider {
                id: "t".into(), name: "t".into(), kind: "ollama".into(),
                api_endpoint: format!("http://{}", addr), api_key: String::new(),
                enabled: true, models: vec![], chat_model: "m".into(), embed_model: "m".into(),
            }],
            active_id: "t".into(), embed_provider_id: None, presets_seeded: true,
        }).unwrap();
        svc.open_repo(repo.to_str().unwrap()).unwrap();
        // 开缓存验证完整语义：页面维度磁盘隔离 + 大纲/总览内容寻址复用

        // 1) main 全量生成
        svc.generate_wiki(repo.to_str().unwrap(), &|_| {}).await.unwrap();
        let main_page = std::fs::read(wiki_root(&repo, "main").join("pages/S1/A.md")).unwrap();
        assert_eq!(*calls.lock().unwrap(), 5, "main: outline+3页+总览");

        // 2) 切 develop：无产物 → 空文档清单（隔离），生成走真调用
        std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/develop\n").unwrap();
        let docs = svc.db.list_wiki_docs(1, "develop").unwrap();
        assert!(docs.iter().all(|(_, k, _)| k != "page"), "develop 无 main 的页面");
        let files = svc.db.get_index_files_by_repo(1, "develop").unwrap();
        assert!(files.is_empty(), "develop 无 main 的索引快照");
        svc.generate_wiki(repo.to_str().unwrap(), &|_| {}).await.unwrap();
        assert!(wiki_root(&repo, "develop").join("pages/S1/A.md").is_file());
        let dev_calls = *calls.lock().unwrap();
        assert_eq!(dev_calls, 5, "develop 零新增 LLM 调用——文件未变时内容寻址缓存全命中（token 复用），产物落分支目录");

        // 3) 切回 main：文档字节原样、断点续跑全沿用（零 LLM 调用）
        std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        let again = std::fs::read(wiki_root(&repo, "main").join("pages/S1/A.md")).unwrap();
        assert_eq!(again, main_page, "main 产物未被 develop 覆盖");
        svc.generate_wiki(repo.to_str().unwrap(), &|_| {}).await.unwrap();
        assert_eq!(*calls.lock().unwrap(), dev_calls, "切回 main 零 LLM 调用（页面沿用，大纲/总览缓存）");
    }
}
