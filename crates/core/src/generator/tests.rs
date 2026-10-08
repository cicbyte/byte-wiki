//! 生成流水线测试：临时仓库 + mock LLM（ollama 协议）验证产物结构、
//! 失败保护（不写半成品）、事件序列。

use std::path::Path;

use super::*;
use crate::model::LlmConfig;
use crate::service::AppService;

use axum::routing::post;
use tokio::net::TcpListener;

/// mock ollama：chat 固定返回一段 markdown（内容带模型侧标记，便于断言来源）
async fn start_mock_llm() -> String {
    let app = axum::Router::new().route(
        "/api/chat",
        post(|| async {
            axum::Json(serde_json::json!({
                "message": { "role": "assistant", "content": "# 文档\n\n## 职责定位\n\nmock 模块文档。\n## 目录结构\n\n- a\n" },
                "prompt_eval_count": 100,
                "eval_count": 20,
            }))
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{}", addr)
}

/// mock ollama（总是 500）：验证失败保护
async fn start_broken_llm() -> String {
    let app = axum::Router::new().route(
        "/api/chat",
        post(|| async { (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "boom") }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{}", addr)
}

fn make_repo(dir: &Path) -> std::path::PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    std::fs::write(repo.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
    std::fs::create_dir_all(repo.join("apps").join("cli")).unwrap();
    std::fs::write(repo.join("apps/cli/main.rs"), "use byte_wiki_core::AppService;\nfn main() {}\n").unwrap();
    std::fs::create_dir_all(repo.join("crates").join("core").join("src")).unwrap();
    std::fs::write(
        repo.join("crates/core/Cargo.toml"),
        "[package]\nname = \"byte-wiki-core\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(repo.join("crates/core/src/lib.rs"), "pub struct AppService;\n").unwrap();
    std::fs::write(repo.join("README.md"), "# demo\n").unwrap();
    repo
}

fn svc_with_llm(dir: &Path, base_url: &str) -> AppService {
    let svc = AppService::open_at(dir.join("app"), dir.join("data")).unwrap();
    svc.set_llm_config(LlmConfig {
        providers: vec![crate::model::LlmProvider {
            id: "test".into(),
            name: "测试供应商".into(),
            kind: "ollama".into(),
            api_endpoint: base_url.into(),
            api_key: String::new(),
            enabled: true,
            models: vec![],
            chat_model: "mock".into(),
            embed_model: "mock".into(),
        }],
        active_id: "test".into(),
    })
    .unwrap();
    svc
}

#[tokio::test]
async fn generate_writes_bytewiki_structure() {
    let dir = tempfile::tempdir().unwrap();
    let repo = make_repo(dir.path());
    let base = start_mock_llm().await;
    let svc = svc_with_llm(dir.path(), &base);
    svc.open_repo(repo.to_str().unwrap()).unwrap();

    let events: std::sync::Mutex<Vec<String>> = Default::default();
    let summary = svc
        .generate_wiki(repo.to_str().unwrap(), &|e| {
            events.lock().unwrap().push(e.phase.clone());
        })
        .await
        .unwrap();

    // 事件序列覆盖四阶段 + done
    assert!(events.lock().unwrap().contains(&"indexing".to_string()));
    assert!(events.lock().unwrap().contains(&"planning".to_string()));
    assert!(events.lock().unwrap().contains(&"assembling".to_string()));
    assert_eq!(events.lock().unwrap().last().unwrap(), "done");

    // 产物结构：architecture.md + modules/*.md + index.json + graphs/dependency.mmd
    let wiki = repo.join(".bytewiki");
    assert!(wiki.join("architecture.md").is_file());
    assert!(wiki.join("index.json").is_file());
    assert!(wiki.join("graphs/dependency.mmd").is_file());
    // 图谱：flowchart 语法 + 真实引用边（apps/cli 的 use byte_wiki_core → apps→crates）
    let mmd = std::fs::read_to_string(wiki.join("graphs/dependency.mmd")).unwrap();
    assert!(mmd.starts_with("flowchart LR"), "实际：{mmd}");
    assert!(mmd.contains("apps -->|1| crates"), "缺边：{mmd}");
    // 孤立模块（"."，仅 README）不出现在任何边里
    for line in mmd.lines().filter(|l| l.contains("-->")) {
        assert!(!line.contains("[\".\"]"), "孤立模块不应有边：{line}");
    }
    let modules_dir = wiki.join("modules");
    let modules: Vec<_> = std::fs::read_dir(&modules_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "md").unwrap_or(false))
        .collect();
    assert!(modules.len() >= 3, "模块文档不足：{modules:#?}");

    // index.json 结构
    let idx: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(wiki.join("index.json")).unwrap()).unwrap();
    assert_eq!(idx["version"], 1);
    assert_eq!(idx["generator"]["model"], "mock");
    assert!(idx["files"].as_array().unwrap().len() >= 4);

    // wiki_docs 登记（architecture + 每模块）
    let docs = svc.db.list_wiki_docs(1).unwrap();
    assert!(docs.iter().any(|(p, _, _)| p == "architecture.md"));
    assert!(docs.iter().any(|(p, _, _)| p.contains("apps")));

    // token 统计非零（mock 返回了 usage）
    assert!(summary.prompt_tokens > 0 && summary.completion_tokens > 0);
    assert!(summary.modules_generated >= 3);
    // 暂存目录已清理
    assert!(!repo.join(".bytewiki.staging").exists());
}

#[tokio::test]
async fn generate_failure_preserves_old_wiki() {
    let dir = tempfile::tempdir().unwrap();
    let repo = make_repo(dir.path());

    // 第一次用好的 mock 生成
    let good = start_mock_llm().await;
    {
        let svc = svc_with_llm(dir.path(), &good);
        svc.open_repo(repo.to_str().unwrap()).unwrap();
        svc.generate_wiki(repo.to_str().unwrap(), &|_| {}).await.unwrap();
    }
    let wiki = repo.join(".bytewiki");
    let old_arch = std::fs::read_to_string(wiki.join("architecture.md")).unwrap();

    // 第二次换坏 LLM：失败且旧 wiki 保留
    let broken = start_broken_llm().await;
    let svc = svc_with_llm(dir.path(), &broken);
    svc.open_repo(repo.to_str().unwrap()).unwrap();
    let err = svc
        .generate_wiki(repo.to_str().unwrap(), &|_| {})
        .await
        .unwrap_err();
    assert!(err.to_string().contains("500"), "实际错误：{err}");

    assert_eq!(std::fs::read_to_string(wiki.join("architecture.md")).unwrap(), old_arch);
    // 不产生半成品暂存目录
    assert!(!repo.join(".bytewiki.staging").exists());
}

#[tokio::test]
async fn unregistered_repo_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let base = start_mock_llm().await;
    let svc = svc_with_llm(dir.path(), &base);
    let err = svc
        .generate_wiki(dir.path().join("nope").to_str().unwrap(), &|_| {})
        .await
        .unwrap_err();
    assert!(!err.to_string().is_empty());
}

#[test]
fn scan_existing_wiki_registers_clone() {
    // 克隆加载（F12）：仓库带他人生成的 .bytewiki/，scan 后 db 有清单
    let dir = tempfile::tempdir().unwrap();
    let repo = make_repo(dir.path());
    let wiki = repo.join(".bytewiki").join("modules");
    std::fs::create_dir_all(&wiki).unwrap();
    std::fs::write(repo.join(".bytewiki/architecture.md"), "# 架构\n").unwrap();
    std::fs::write(wiki.join("apps.md"), "# apps\n").unwrap();

    let svc = AppService::open_at(dir.path().join("app"), dir.path().join("data")).unwrap();
    svc.open_repo(repo.to_str().unwrap()).unwrap();

    // db 初始为空 → 扫描登记
    assert!(svc.db.list_wiki_docs(1).unwrap().is_empty());
    let docs = svc.scan_existing_wiki(&repo).unwrap();
    assert_eq!(docs.len(), 2);
    assert!(docs.iter().any(|(p, k, _)| p == "architecture.md" && k == "architecture"));
    assert!(docs.iter().any(|(p, k, t)| p == "modules/apps.md" && k == "module" && t == "apps"));
    // 登记后 db 可查
    assert_eq!(svc.db.list_wiki_docs(1).unwrap().len(), 2);

    // 无 .bytewiki 的仓库返回空
    let plain = dir.path().join("plain-repo");
    std::fs::create_dir_all(plain.join(".git")).unwrap();
    assert!(svc.scan_existing_wiki(&plain).unwrap().is_empty());
}

#[test]
fn topo_and_sanitize_units() {
    let names = vec!["apps".to_string(), "crates".to_string(), "tools".to_string()];
    let edges = vec![("apps".to_string(), "crates".to_string(), 3)];
    let ordered = topo_order(&names, &edges);
    // crates（被依赖）先于 apps
    assert!(ordered.iter().position(|n| n == "crates").unwrap() < ordered.iter().position(|n| n == "apps").unwrap());

    assert_eq!(sanitize_md("```markdown\n# hi\n```"), "# hi");
    assert_eq!(sanitize_md("  # hi  "), "# hi");
    assert_eq!(safe_filename("apps/desktop"), "apps_desktop");
}

/// 真实端到端生成冒烟（默认忽略）：本机 ollama + minicpm5 对临时小仓库生成。
/// `cargo test -p byte-wiki-core real_generate_smoke -- --ignored --nocapture`
#[tokio::test]
#[ignore = "需要本机 ollama 服务（较慢）"]
async fn real_generate_smoke() {
    let dir = tempfile::tempdir().unwrap();
    let repo = make_repo(dir.path());
    let svc = AppService::open_at(dir.path().join("app"), dir.path().join("data")).unwrap();
    svc.set_llm_config(LlmConfig {
        providers: vec![crate::model::LlmProvider {
            id: "real".into(),
            name: "Ollama 本地".into(),
            kind: "ollama".into(),
            api_endpoint: "http://localhost:11434".into(),
            api_key: String::new(),
            enabled: true,
            models: vec![],
            chat_model: "openbmb/minicpm5:latest".into(),
            embed_model: "nomic-embed-text".into(),
        }],
        active_id: "real".into(),
    })
    .unwrap();
    svc.open_repo(repo.to_str().unwrap()).unwrap();
    let summary = svc
        .generate_wiki(repo.to_str().unwrap(), &|e| eprintln!("  [{}] {}", e.phase, e.message))
        .await
        .unwrap();
    eprintln!(
        "生成完成：{} 模块，{}ms，prompt={} completion={}",
        summary.modules_generated, summary.elapsed_ms, summary.prompt_tokens, summary.completion_tokens
    );
    let arch = std::fs::read_to_string(repo.join(".bytewiki/architecture.md")).unwrap();
    assert!(arch.len() > 100, "架构文档过短");
}

/// 增量更新（F15）：改一个模块的文件 → 仅该模块（及其下游）文档重生成，
/// 其他模块文档字节不变；cards 保留；无变更时 no-op。
#[tokio::test]
async fn incremental_update_touches_only_affected_modules() {
    let dir = tempfile::tempdir().unwrap();
    let repo = make_repo(dir.path());
    let base = start_mock_llm().await;
    let svc = svc_with_llm(dir.path(), &base);
    svc.open_repo(repo.to_str().unwrap()).unwrap();
    svc.generate_wiki(repo.to_str().unwrap(), &|_| {}).await.unwrap();

    let wiki = repo.join(".bytewiki");
    // 人工知识卡（F16 联动验证：更新后必须保留）
    let cards = wiki.join("cards");
    std::fs::create_dir_all(&cards).unwrap();
    std::fs::write(cards.join("note.md"), "---\ntitle: note\n---\n# 人工卡\n").unwrap();

    // 记录未受影响模块（"."：仅 README，不在 apps→crates 依赖闭包内）的旧内容
    let root_doc = std::fs::read(wiki.join("modules/..md")).unwrap();
    let arch_before = std::fs::read(wiki.join("architecture.md")).unwrap().len();

    // 修改 apps 模块的文件（mtime/size 变化）
    std::fs::write(
        repo.join("apps/cli/main.rs"),
        "use byte_wiki_core::AppService;\nfn main() { /* changed */ }\n// v2\n",
    )
    .unwrap();

    let summary = svc.update_wiki(repo.to_str().unwrap(), &|_| {}).await.unwrap();
    assert!(!summary.unchanged);
    assert_eq!(summary.changed_files, 1);
    // apps 变更；apps 依赖 crates（use byte_wiki_core → 边 apps→crates）→ 反向依赖闭包含 crates
    assert!(summary.regenerated_modules >= 1 && summary.regenerated_modules <= 2,
        "实际重生成 {} 个模块", summary.regenerated_modules);

    // 未受影响模块内容不变（字节级）
    assert_eq!(std::fs::read(wiki.join("modules/..md")).unwrap(), root_doc);
    // apps 文档被重写（mock 内容相同，但应来自重生成路径——以 changed 计数断言为准）
    // cards 保留
    assert!(cards.join("note.md").exists());
    // 架构重生成（mock 固定内容，长度应与之前一致非空）
    assert_eq!(std::fs::read(wiki.join("architecture.md")).unwrap().len(), arch_before);

    // 无变更再更新 → no-op
    let s2 = svc.update_wiki(repo.to_str().unwrap(), &|_| {}).await.unwrap();
    assert!(s2.unchanged && s2.regenerated_modules == 0);
}
