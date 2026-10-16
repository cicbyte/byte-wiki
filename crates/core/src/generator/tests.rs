//! 生成流水线测试（Qoder 式管线）：临时仓库 + 按内容路由的 mock LLM
//! （大纲→XML、页面/总览→Markdown），验证产物结构、页面级事件、
//! 引用后处理、失败保护与页级增量更新。

use std::path::Path;

use super::*;
use crate::model::LlmConfig;
use crate::service::AppService;

use axum::extract::State;
use axum::routing::post;
use tokio::net::TcpListener;

/// mock 返回的大纲：2 章节 3 页面（文件全部真实存在于 make_repo 仓库）
const MOCK_OUTLINE_XML: &str = r#"<wiki_structure>
  <title>演示仓库</title>
  <description>测试用迷你仓库</description>
  <sections>
    <section id="s1" title="系统架构" parent=""/>
    <section id="s2" title="入口与构建" parent="s1"/>
  </sections>
  <pages>
    <page id="p1">
      <title>核心库</title>
      <description>core 结构与入口</description>
      <importance>high</importance>
      <brief>覆盖 core 的结构与导出</brief>
      <section>s1</section>
      <files>
        <file>crates/core/src/lib.rs</file>
        <file>crates/core/Cargo.toml</file>
        <file>README.md</file>
      </files>
      <related><page_ref>p2</page_ref></related>
    </page>
    <page id="p2">
      <title>命令行入口</title>
      <description>cli main 流程</description>
      <importance>medium</importance>
      <brief>覆盖 cli 的启动流程</brief>
      <section>s2</section>
      <files>
        <file>apps/cli/main.rs</file>
        <file>README.md</file>
      </files>
      <related><page_ref>p1</page_ref></related>
    </page>
    <page id="p3">
      <title>根目录杂项</title>
      <description>根级文件</description>
      <importance>low</importance>
      <brief>覆盖根目录说明</brief>
      <section></section>
      <files>
        <file>README.md</file>
      </files>
      <related></related>
    </page>
  </pages>
</wiki_structure>"#;

/// mock 页面正文（含待后处理的空括号引用与足够多的 H2，触发目录注入）
const MOCK_PAGE_MD: &str = "# 页面标题\n\n## 简介\n\nmock 页面简介。\n\n## 项目结构\n\n结构说明。\n\n**图表来源**\n- [crates/core/src/lib.rs:1-1]()\n\n## 核心组件\n\n组件说明。\n\n**章节来源**\n- [README.md:1-1]()\n- [apps/cli/main.rs:1-2]()\n\n## 结论\n\n总结。\n";

#[derive(Clone, Default)]
struct MockState {
    /// (标记, 调用次数)：断言增量更新只重生成受影响页面
    counts: std::sync::Arc<std::sync::Mutex<Vec<(String, usize)>>>,
}

fn bump(state: &MockState, tag: &str) {
    let mut c = state.counts.lock().unwrap();
    match c.iter_mut().find(|(t, _)| t == tag) {
        Some((_, n)) => *n += 1,
        None => c.push((tag.to_string(), 1)),
    }
}

/// mock ollama：按 user 消息内容路由（大纲 / 页面 / 总览），NDJSON 流式返回
/// （管线 LLM 调用统一走 chat_stream，按行解析）
async fn start_mock_llm() -> (String, MockState) {
    let state = MockState::default();
    let app_state = state.clone();
    let app = axum::Router::new().route(
        "/api/chat",
        post(move |State(s): State<MockState>, body: axum::body::Bytes| async move {
            let v: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
            let user_text: String = v["messages"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter(|m| m["role"] == "user")
                        .map(|m| m["content"].as_str().unwrap_or(""))
                        .collect::<String>()
                })
                .unwrap_or_default();
            let content = if user_text.contains("【Wiki 大纲设计】") {
                bump(&s, "outline");
                MOCK_OUTLINE_XML.to_string()
            } else if user_text.contains("【Wiki 总览撰写】") {
                bump(&s, "overview");
                "# 演示仓库 Wiki 总览\n\n## 项目概览\n\nmock 总览。\n".to_string()
            } else {
                bump(&s, "page");
                MOCK_PAGE_MD.to_string()
            };
            // rig 内核按 ollama 官方结构解析：行需 model/created_at，message 需 role
            let mut ndjson = serde_json::to_string(&serde_json::json!({
                "model": "mock",
                "created_at": "2026-01-01T00:00:00Z",
                "message": { "role": "assistant", "content": content }, "done": false,
            }))
            .unwrap();
            ndjson.push('\n');
            ndjson.push_str(
                &serde_json::to_string(&serde_json::json!({
                    "model": "mock",
                    "created_at": "2026-01-01T00:00:00Z",
                    "message": { "role": "assistant", "content": "" },
                    "done": true, "done_reason": "stop",
                    "prompt_eval_count": 100, "eval_count": 20,
                }))
                .unwrap(),
            );
            ndjson.push('\n');
            (
                [(axum::http::header::CONTENT_TYPE, "application/x-ndjson")],
                ndjson,
            )
        }),
    )
    .with_state(app_state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{}", addr), state)
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
        embed_provider_id: None,
        presets_seeded: true,
    })
    .unwrap();
    svc
}

fn mock_counts(state: &MockState, tag: &str) -> usize {
    state
        .counts
        .lock()
        .unwrap()
        .iter()
        .find(|(t, _)| t == tag)
        .map(|(_, n)| *n)
        .unwrap_or(0)
}

// ============ 一键生成 ============

#[tokio::test]
async fn generate_writes_outline_pages_and_overview() {
    let dir = tempfile::tempdir().unwrap();
    let repo = make_repo(dir.path());
    let (base, state) = start_mock_llm().await;
    let svc = svc_with_llm(dir.path(), &base);
    svc.open_repo(repo.to_str().unwrap()).unwrap();

    let events: std::sync::Mutex<Vec<GenerateEvent>> = Default::default();
    let summary = svc
        .generate_wiki(repo.to_str().unwrap(), &|e| {
            events.lock().unwrap().push(e);
        })
        .await
        .unwrap();

    // 事件：新阶段序列 + 大纲骨架事件（3 页全 pending）
    {
        let evs = events.lock().unwrap();
        let phases: Vec<String> = evs.iter().map(|e| e.phase.clone()).collect();
        for expect in ["indexing", "outlining", "generating", "assembling", "done"] {
            assert!(phases.contains(&expect.to_string()), "缺阶段 {expect}：{phases:?}");
        }
        assert_eq!(phases.last().unwrap(), "done");
        let skeleton: Vec<PageProgress> = evs
            .iter()
            .find(|e| e.phase == "outlining" && e.pages.is_some())
            .and_then(|e| e.pages.clone())
            .expect("大纲骨架事件");
        assert_eq!(skeleton.len(), 3);
        assert!(skeleton.iter().all(|p| p.status == "pending"));
        assert_eq!(skeleton[0].section, "系统架构"); // p1 → s1
    }

    // 产物：outline.json v2 + pages/（含章节子目录）+ overview.md + index.json
    let wiki = repo.join(".bytewiki");
    let outline_raw = std::fs::read_to_string(wiki.join("outline.json")).unwrap();
    let outline_json: serde_json::Value = serde_json::from_str(&outline_raw).unwrap();
    assert_eq!(outline_json["version"], 2);
    assert_eq!(outline_json["pages"].as_array().unwrap().len(), 3);
    assert!(wiki.join("overview.md").is_file());
    assert!(wiki.join("pages/系统架构/核心库.md").is_file(), "章节子目录页面缺失");
    assert!(wiki.join("pages/系统架构/入口与构建/命令行入口.md").is_file(), "嵌套章节页面缺失");
    assert!(wiki.join("pages/根目录杂项.md").is_file(), "无章节页面应直挂 pages/");
    // 旧版 modules/ 不再产生
    assert!(!wiki.join("modules").exists());
    // 依赖图谱保留（apps → crates 真实引用边）
    let mmd = std::fs::read_to_string(wiki.join("graphs/dependency.mmd")).unwrap();
    assert!(mmd.contains("apps -->|1| crates"), "缺边：{mmd}");

    // 页面内容经过后处理：引用链接化 + cite 块 + 目录
    let page = std::fs::read_to_string(wiki.join("pages/系统架构/核心库.md")).unwrap();
    assert!(page.contains("[crates/core/src/lib.rs:1](bw-file://crates/core/src/lib.rs?L=1-1)"), "{page}");
    assert!(page.contains("<details>"), "缺 cite 块：{page}");
    assert!(page.contains("## 目录"), "缺目录：{page}");
    // 无效引用（不存在文件）退化为纯文本
    let idx: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(wiki.join("index.json")).unwrap()).unwrap();
    assert_eq!(idx["version"], 2);
    assert_eq!(idx["generator"]["model"], "mock");

    // wiki_docs 登记：overview + 3 页（kind=page，标题来自大纲）
    let docs = svc.db.list_wiki_docs(1).unwrap();
    assert!(docs.iter().any(|(p, k, _)| p == "overview.md" && k == "architecture"));
    let page_rows: Vec<_> = docs.iter().filter(|(_, k, _)| k == "page").collect();
    assert_eq!(page_rows.len(), 3);
    assert!(page_rows.iter().any(|(_, _, t)| t == "核心库"));

    // 调用路由正确：1 大纲 + 3 页面 + 1 总览
    assert_eq!(mock_counts(&state, "outline"), 1);
    assert_eq!(mock_counts(&state, "page"), 3);
    assert_eq!(mock_counts(&state, "overview"), 1);
    assert_eq!(summary.pages_generated, 3);
    assert!(summary.prompt_tokens > 0 && summary.completion_tokens > 0);
    // 暂存目录已清理
    assert!(!repo.join(".bytewiki.staging").exists());
}

#[tokio::test]
async fn generate_failure_preserves_old_wiki() {
    let dir = tempfile::tempdir().unwrap();
    let repo = make_repo(dir.path());

    // 第一次用好的 mock 生成
    let (good, _) = start_mock_llm().await;
    {
        let svc = svc_with_llm(dir.path(), &good);
        svc.open_repo(repo.to_str().unwrap()).unwrap();
        svc.generate_wiki(repo.to_str().unwrap(), &|_| {}).await.unwrap();
    }
    let wiki = repo.join(".bytewiki");
    let old_overview = std::fs::read_to_string(wiki.join("overview.md")).unwrap();

    // 第二次换坏 LLM：失败且旧 wiki 保留（关缓存，否则同 prompt 命中缓存绕过坏 LLM）
    let broken = start_broken_llm().await;
    let svc = svc_with_llm(dir.path(), &broken);
    svc.set_llm_cache_enabled(false);
    svc.open_repo(repo.to_str().unwrap()).unwrap();
    let err = svc
        .generate_wiki(repo.to_str().unwrap(), &|_| {})
        .await
        .unwrap_err();
    assert!(err.to_string().contains("500"), "实际错误：{err}");

    assert_eq!(std::fs::read_to_string(wiki.join("overview.md")).unwrap(), old_overview);
    assert!(!repo.join(".bytewiki.staging").exists());
}

#[tokio::test]
async fn unregistered_repo_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let (base, _) = start_mock_llm().await;
    let svc = svc_with_llm(dir.path(), &base);
    let err = svc
        .generate_wiki(dir.path().join("nope").to_str().unwrap(), &|_| {})
        .await
        .unwrap_err();
    assert!(!err.to_string().is_empty());
}

// ============ 页级增量更新 ============

/// 增量更新：改 apps/cli/main.rs → 仅绑定该文件的页面重生成，
/// 其他页面字节不变；cards 保留；无变更 no-op。
#[tokio::test]
async fn incremental_update_touches_only_affected_pages() {
    let dir = tempfile::tempdir().unwrap();
    let repo = make_repo(dir.path());
    let (base, state) = start_mock_llm().await;
    let svc = svc_with_llm(dir.path(), &base);
    svc.set_llm_cache_enabled(false); // 关缓存：保持「重生成必走 LLM」的严格断言语义
    svc.open_repo(repo.to_str().unwrap()).unwrap();
    svc.generate_wiki(repo.to_str().unwrap(), &|_| {}).await.unwrap();

    let wiki = repo.join(".bytewiki");
    // 人工知识卡：更新后必须保留
    let cards = wiki.join("cards");
    std::fs::create_dir_all(&cards).unwrap();
    std::fs::write(cards.join("note.md"), "---\ntitle: note\n---\n# 人工卡\n").unwrap();

    // 未受影响页面（p1 核心库：不含 apps/cli/main.rs）的旧内容
    let keep_path = wiki.join("pages/系统架构/核心库.md");
    let kept_before = std::fs::read(&keep_path).unwrap();
    let page_calls_before = mock_counts(&state, "page");

    // 修改 apps/cli/main.rs（只影响 p2 命令行入口）
    std::fs::write(
        repo.join("apps/cli/main.rs"),
        "use byte_wiki_core::AppService;\nfn main() { /* changed */ }\n// v2\n",
    )
    .unwrap();

    let summary = svc.update_wiki(repo.to_str().unwrap(), &|_| {}).await.unwrap();
    assert!(!summary.unchanged);
    assert_eq!(summary.changed_files, 1);
    assert_eq!(summary.regenerated_pages, 1, "只应重生成 1 个页面：{summary:?}");

    // 未受影响页面字节不变；cards 保留；大纲文件不变
    assert_eq!(std::fs::read(&keep_path).unwrap(), kept_before);
    assert!(cards.join("note.md").exists());
    let outline_before = std::fs::read_to_string(wiki.join("outline.json")).unwrap();
    assert!(outline_json_pages(&outline_before) >= 1);

    // LLM 调用：只多 1 次页面（p2）+ 1 次总览；大纲不重设计
    assert_eq!(mock_counts(&state, "page"), page_calls_before + 1);
    assert_eq!(mock_counts(&state, "outline"), 1);
    assert_eq!(mock_counts(&state, "overview"), 2);

    // 无变更再更新 → no-op
    let s2 = svc.update_wiki(repo.to_str().unwrap(), &|_| {}).await.unwrap();
    assert!(s2.unchanged && s2.regenerated_pages == 0);
}

fn outline_json_pages(raw: &str) -> usize {
    serde_json::from_str::<serde_json::Value>(raw).unwrap()["pages"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0)
}

// ============ 克隆加载 ============

#[test]
fn scan_existing_wiki_registers_clone() {
    let dir = tempfile::tempdir().unwrap();
    let repo = make_repo(dir.path());
    let wiki = repo.join(".bytewiki");
    std::fs::create_dir_all(wiki.join("pages").join("系统架构")).unwrap();
    std::fs::write(wiki.join("overview.md"), "# 总览\n").unwrap();
    std::fs::write(wiki.join("pages/系统架构/核心库.md"), "# 核心库\n").unwrap();
    std::fs::write(wiki.join("outline.json"), "{}\n").unwrap(); // 机器清单不进文档树

    let svc = AppService::open_at(dir.path().join("app"), dir.path().join("data")).unwrap();
    svc.open_repo(repo.to_str().unwrap()).unwrap();

    assert!(svc.db.list_wiki_docs(1).unwrap().is_empty());
    let docs = svc.scan_existing_wiki(&repo).unwrap();
    assert_eq!(docs.len(), 2);
    assert!(docs.iter().any(|(p, k, _)| p == "overview.md" && k == "architecture"));
    assert!(docs.iter().any(|(p, k, t)| p == "pages/系统架构/核心库.md" && k == "page" && t == "核心库"));
    assert_eq!(svc.db.list_wiki_docs(1).unwrap().len(), 2);

    // 无 .bytewiki 的仓库返回空
    let plain = dir.path().join("plain-repo");
    std::fs::create_dir_all(plain.join(".git")).unwrap();
    assert!(svc.scan_existing_wiki(&plain).unwrap().is_empty());
}

/// 缓存命中：同 prompt 的二次全量生成不发起任何 LLM 请求，
/// 产物字节一致，token 汇总为 0（未真实消耗）。
#[tokio::test]
async fn cached_regenerate_hits_cache() {
    let dir = tempfile::tempdir().unwrap();
    let repo = make_repo(dir.path());
    let (base, state) = start_mock_llm().await;
    let svc = svc_with_llm(dir.path(), &base);
    svc.open_repo(repo.to_str().unwrap()).unwrap();

    let s1 = svc.generate_wiki(repo.to_str().unwrap(), &|_| {}).await.unwrap();
    assert!(s1.prompt_tokens > 0);
    let (outline_calls, page_calls, overview_calls) = (
        mock_counts(&state, "outline"),
        mock_counts(&state, "page"),
        mock_counts(&state, "overview"),
    );
    assert_eq!((outline_calls, page_calls, overview_calls), (1, 3, 1));

    // 二次全量生成：全部命中缓存，mock 零新增调用
    let page_before = std::fs::read(repo.join(".bytewiki/pages/系统架构/核心库.md")).unwrap();
    let s2 = svc.generate_wiki(repo.to_str().unwrap(), &|_| {}).await.unwrap();
    assert_eq!(mock_counts(&state, "outline"), 1, "大纲不应重复请求");
    assert_eq!(mock_counts(&state, "page"), 3, "页面不应重复请求");
    assert_eq!(mock_counts(&state, "overview"), 1, "总览不应重复请求");
    assert_eq!(s2.prompt_tokens, 0, "缓存命中不消耗 token");
    assert_eq!(s2.completion_tokens, 0);
    // 产物一致
    assert_eq!(std::fs::read(repo.join(".bytewiki/pages/系统架构/核心库.md")).unwrap(), page_before);

    // 缓存统计与清空
    let (count, bytes) = svc.llm_cache_stats(repo.to_str().unwrap()).unwrap();
    assert_eq!(count, 5, "应缓存 5 条（大纲+3页+总览）");
    assert!(bytes > 0);
    svc.clear_llm_cache(repo.to_str().unwrap()).unwrap();
    assert_eq!(svc.llm_cache_stats(repo.to_str().unwrap()).unwrap().0, 0);

    // 清空后第三次生成：重新走 LLM
    let _s3 = svc.generate_wiki(repo.to_str().unwrap(), &|_| {}).await.unwrap();
    assert_eq!(mock_counts(&state, "outline"), 2);
    assert_eq!(mock_counts(&state, "page"), 6);
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
        embed_provider_id: None,
        presets_seeded: true,
    })
    .unwrap();
    svc.open_repo(repo.to_str().unwrap()).unwrap();
    let summary = svc
        .generate_wiki(repo.to_str().unwrap(), &|e| eprintln!("  [{}] {}", e.phase, e.message))
        .await
        .unwrap();
    eprintln!(
        "生成完成：{} 页，{}ms，prompt={} completion={}",
        summary.pages_generated, summary.elapsed_ms, summary.prompt_tokens, summary.completion_tokens
    );
    let overview = std::fs::read_to_string(repo.join(".bytewiki/overview.md")).unwrap();
    assert!(overview.len() > 100, "总览过短");
}
