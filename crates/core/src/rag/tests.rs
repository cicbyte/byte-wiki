//! RAG 测试：mock embedding（按内容关键字返回正交向量）验证
//! 分块、重建、检索排序、空库与降级路径。

use super::*;
use crate::model::{LlmConfig, LlmProvider};
use crate::service::AppService;

use axum::routing::post;
use tokio::net::TcpListener;

/// mock ollama embed：内容含 alpha → [1,0]，含 beta → [0,1]，其余 [0.7,0.7]
async fn start_mock_embed() -> String {
    let app = axum::Router::new().route(
        "/api/embed",
        post(|body: String| async move {
            let v: serde_json::Value = serde_json::from_str(&body).unwrap();
            let inputs = v["input"].as_array().cloned().unwrap_or_default();
            let embeddings: Vec<Vec<f64>> = inputs
                .iter()
                .map(|t| {
                    let s = t.as_str().unwrap_or("");
                    if s.contains("alpha") {
                        vec![1.0, 0.0]
                    } else if s.contains("beta") {
                        vec![0.0, 1.0]
                    } else {
                        vec![0.7, 0.7]
                    }
                })
                .collect();
            axum::Json(serde_json::json!({ "model": "mock-embed", "embeddings": embeddings }))
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{}", addr)
}

fn make_repo_with_wiki(dir: &std::path::Path) -> std::path::PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    std::fs::write(repo.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
    std::fs::create_dir_all(repo.join(".bytewiki/branches/main/modules")).unwrap();
    std::fs::write(
        repo.join(".bytewiki/branches/main/architecture.md"),
        "# 架构\n\n## Alpha 模块\n\n这里介绍 alpha 服务。alpha 是核心。\n\n## Beta 模块\n\n这里介绍 beta 服务。beta 是辅助。\n",
    )
    .unwrap();
    std::fs::write(
        repo.join(".bytewiki/branches/main/modules/alpha.md"),
        "# alpha\n\n## 用法\n\n调用 alpha 接口即可。alpha alpha alpha。\n",
    )
    .unwrap();
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/main.rs"), "fn main() {\n    // alpha entry\n    println!(\"alpha\");\n}\n").unwrap();
    repo
}

fn svc_with_embed(dir: &std::path::Path, base_url: &str) -> AppService {
    let svc = AppService::open_at(dir.join("app"), dir.join("data")).unwrap();
    svc.set_llm_config(LlmConfig {
        providers: vec![LlmProvider {
            id: "test".into(),
            name: "测试供应商".into(),
            kind: "ollama".into(),
            api_endpoint: base_url.into(),
            api_key: String::new(),
            enabled: true,
            models: vec![],
            chat_model: "mock".into(),
            embed_model: "mock-embed".into(),
        }],
        active_id: "test".into(),
        embed_provider_id: None,
        presets_seeded: true,
    })
    .unwrap();
    svc
}

#[tokio::test]
async fn rebuild_and_search_ranks_by_similarity() {
    let dir = tempfile::tempdir().unwrap();
    let repo = make_repo_with_wiki(dir.path());
    let base = start_mock_embed().await;
    let svc = svc_with_embed(dir.path(), &base);
    svc.open_repo(repo.to_str().unwrap()).unwrap();
    svc.index_repo(repo.to_str().unwrap(), &|_| {}).unwrap();

    let progress: std::sync::Mutex<Vec<String>> = Default::default();
    let summary = svc
        .rebuild_embeddings(repo.to_str().unwrap(), &|m| progress.lock().unwrap().push(m.to_string()))
        .await
        .unwrap();
    // architecture 2 段 + alpha.md 1 段 + main.rs 1 段
    assert!(summary.chunked >= 4, "实际：{summary:?}");
    assert_eq!(summary.embedded, summary.chunked);

    // 查询 "alpha"：命中应主要来自含 alpha 的块
    let hits = svc.search_similar(repo.to_str().unwrap(), None, "alpha 是什么", 3).await.unwrap();
    assert!(!hits.is_empty());
    assert!(hits[0].score > 0.9, "首命中率异常：{:?}", hits[0].score);
    assert!(
        hits.iter().take(2).all(|h| h.content.contains("alpha")),
        "top 命中应含 alpha：{:?}",
        hits.iter().map(|h| h.doc_path.clone()).collect::<Vec<_>>()
    );

    // 查询 "beta"：首命中应来自 beta 段
    let hits = svc.search_similar(repo.to_str().unwrap(), None, "beta 是什么", 1).await.unwrap();
    assert!(hits[0].content.contains("beta"));
}

#[tokio::test]
async fn replace_semantics_and_empty_repo() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("plain");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    let base = start_mock_embed().await;
    let svc = svc_with_embed(dir.path(), &base);
    svc.open_repo(repo.to_str().unwrap()).unwrap();
    svc.index_repo(repo.to_str().unwrap(), &|_| {}).unwrap();

    // 空仓库（无 wiki 无源码）：0 块，不报错
    let summary = svc.rebuild_embeddings(repo.to_str().unwrap(), &|_| {}).await.unwrap();
    assert_eq!(summary.chunked, 0);
    let hits = svc.search_similar(repo.to_str().unwrap(), None, "x", 3).await.unwrap();
    assert!(hits.is_empty());
}

/// mock：embed 按关键字 + chat 流式两段
async fn start_mock_chat_stack() -> String {
    use axum::routing::post;
    let app = axum::Router::new()
        .route(
            "/api/embed",
            post(|body: String| async move {
                let v: serde_json::Value = serde_json::from_str(&body).unwrap();
                let inputs = v["input"].as_array().cloned().unwrap_or_default();
                let embeddings: Vec<Vec<f64>> = inputs
                    .iter()
                    .map(|t| {
                        let s = t.as_str().unwrap_or("");
                        if s.contains("alpha") { vec![1.0, 0.0] } else { vec![0.7, 0.7] }
                    })
                    .collect();
                axum::Json(serde_json::json!({ "model": "mock-embed", "embeddings": embeddings }))
            }),
        )
        .route(
            "/api/chat",
            post(|body: String| async move {
                let v: serde_json::Value = serde_json::from_str(&body).unwrap();
                // rig 内核按 ollama 官方结构解析：行需 model/created_at，message 需 role
                if v["stream"].as_bool().unwrap_or(false) {
                    concat!(
                        "{\"model\":\"mock\",\"created_at\":\"2026-01-01T00:00:00Z\",\"message\":{\"role\":\"assistant\",\"content\":\"根据 \"},\"done\":false}\n",
                        "{\"model\":\"mock\",\"created_at\":\"2026-01-01T00:00:00Z\",\"message\":{\"role\":\"assistant\",\"content\":\".bytewiki/architecture.md\"},\"done\":false}\n",
                        "{\"model\":\"mock\",\"created_at\":\"2026-01-01T00:00:00Z\",\"message\":{\"role\":\"assistant\",\"content\":\"：alpha 是核心服务。\"},\"done\":false}\n",
                        "{\"model\":\"mock\",\"created_at\":\"2026-01-01T00:00:00Z\",\"message\":{\"role\":\"assistant\",\"content\":\"\"},\"done\":true,\"done_reason\":\"stop\",\"prompt_eval_count\":50,\"eval_count\":10}\n"
                    ).into()
                } else {
                    serde_json::json!({
                        "model": "mock",
                        "created_at": "2026-01-01T00:00:00Z",
                        "message": { "role": "assistant", "content": "ok" },
                        "done": true, "done_reason": "stop",
                        "prompt_eval_count": 50, "eval_count": 10,
                    }).to_string()
                }
            }),
        );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{}", addr)
}

#[tokio::test]
async fn chat_with_repo_streams_and_cites_sources() {
    let dir = tempfile::tempdir().unwrap();
    let repo = make_repo_with_wiki(dir.path());
    let base = start_mock_chat_stack().await;
    let svc = svc_with_embed(dir.path(), &base);
    svc.open_repo(repo.to_str().unwrap()).unwrap();
    svc.index_repo(repo.to_str().unwrap(), &|_| {}).unwrap();
    svc.rebuild_embeddings(repo.to_str().unwrap(), &|_| {}).await.unwrap();

    let deltas: std::sync::Mutex<Vec<String>> = Default::default();
    let answer = svc
        .chat_with_repo(
            repo.to_str().unwrap(),
            "alpha 是什么",
            &[("user".into(), "之前问过别的".into())],
            &|d| deltas.lock().unwrap().push(d.to_string()),
        )
        .await
        .unwrap();

    assert!(answer.text.contains("alpha 是核心服务"));
    assert!(deltas.into_inner().unwrap().len() >= 3, "流式增量不足");
    assert!(
        answer.sources.iter().any(|s| s.contains("architecture.md")),
        "出处缺失：{:?}",
        answer.sources
    );
}

#[tokio::test]
async fn chat_without_index_errors_clearly() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("empty");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    let base = start_mock_chat_stack().await;
    let svc = svc_with_embed(dir.path(), &base);
    svc.open_repo(repo.to_str().unwrap()).unwrap();
    svc.index_repo(repo.to_str().unwrap(), &|_| {}).unwrap();

    let err = svc
        .chat_with_repo(repo.to_str().unwrap(), "x", &[], &|_| {})
        .await
        .unwrap_err();
    assert!(err.to_string().contains("尚无向量索引"), "实际：{err}");
}

#[test]
fn cosine_and_blob_roundtrip() {
    let v = vec![0.1f32, -0.2, 0.3];
    let blob = vec_to_blob(&v);
    assert_eq!(blob.len(), 12);
    let back = blob_to_vec(&blob, 3).unwrap();
    assert_eq!(v, back);

    assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
    assert!((cosine(&[1.0, 0.0], &[0.0, 1.0])).abs() < 1e-6);
    assert_eq!(cosine(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
}

#[test]
fn chunk_markdown_splits_by_heading() {
    let mut out = Vec::new();
    chunk_markdown(
        "doc.md",
        "# T\n\n## A\n内容A\n\n## B\n内容B\n",
        &mut out,
    );
    // 3 块：H1 前言块 + A 段 + B 段（标题块对 RAG 定位有用）
    assert_eq!(out.len(), 3);
    assert!(out[0].1.contains("# T"));
    assert!(out[1].1.contains("[doc.md] A"));
    assert!(out[2].1.contains("内容B"));

    let mut out = Vec::new();
    chunk_code("a.rs", &"line\n".repeat(200), &mut out);
    assert_eq!(out.len(), 3); // 80+80+40
    assert!(out[0].1.contains("[a.rs:1-80]"));
}

/// M1 端到端真实冒烟（默认忽略）：复制本仓库到临时目录（git init），
/// 真实 ollama（qwen3.5:9b + nomic-embed-text）走完 打开→生成→问答 全链路。
/// `cargo test -p byte-wiki-core real_e2e_smoke -- --ignored --nocapture`
#[tokio::test]
#[ignore = "需要本机 ollama 服务与已拉取模型（较慢）"]
async fn real_e2e_smoke() {
    let src = std::path::Path::new("D:/code/cicbyte/byte-wiki");
    if !src.exists() {
        eprintln!("跳过：源仓库不存在");
        return;
    }
    // 1) 复制源码到临时仓库（排除 .git/target/node_modules/dist/.bytewiki）
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    for entry in walkdir::WalkDir::new(src)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let rel = entry.path().strip_prefix(src).unwrap();
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if rel_str.starts_with(".git/")
            || rel_str.starts_with("target/")
            || rel_str.starts_with("node_modules/")
            || rel_str.starts_with("apps/desktop/node_modules/")
            || rel_str.starts_with("apps/desktop/dist/")
            || rel_str.starts_with(".bytewiki/")
            || rel_str.starts_with(".scratch/")
            || rel_str.starts_with("dev-docs/")
        {
            continue;
        }
        let to = repo.join(rel);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(entry.path(), &to).unwrap();
    }
    // git init（最小 HEAD 即可被识别）
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();

    // 2) 服务 + 真实模型配置
    let svc = AppService::open_at(tmp.path().join("app"), tmp.path().join("data")).unwrap();
    svc.set_llm_config(LlmConfig {
        providers: vec![LlmProvider {
            id: "real".into(),
            name: "Ollama 本地".into(),
            kind: "ollama".into(),
            api_endpoint: "http://localhost:11434".into(),
            api_key: String::new(),
            enabled: true,
            models: vec![],
            chat_model: "qwen3.5:9b".into(),
            embed_model: "nomic-embed-text:latest".into(),
        }],
        active_id: "real".into(),
        embed_provider_id: None,
        presets_seeded: true,
    })
    .unwrap();

    // 3) 打开 → 生成（含索引与向量）
    svc.open_repo(repo.to_str().unwrap()).unwrap();
    let summary = svc
        .generate_wiki(repo.to_str().unwrap(), &|e| eprintln!("  [{}] {}", e.phase, e.message))
        .await
        .unwrap();
    eprintln!(
        "生成：{} 页 / {}ms / prompt={} completion={}",
        summary.pages_generated, summary.elapsed_ms, summary.prompt_tokens, summary.completion_tokens
    );
    assert!(summary.pages_generated >= 1);

    // 4) 问答
    let answer = svc
        .chat_with_repo(
            repo.to_str().unwrap(),
            "这个仓库是做什么的？整体架构分哪几个模块？",
            &[],
            &|_| {},
        )
        .await
        .unwrap();
    eprintln!("回答（前 300 字）：{}", &answer.text.chars().take(300).collect::<String>());
    eprintln!("出处：{:#?}", answer.sources);
    assert!(answer.text.len() > 50, "回答过短");
    assert!(!answer.sources.is_empty(), "无出处");
}

/// M2 真实增量冒烟（默认忽略）：本仓库副本 → 全量生成 → 改一个文件 → 增量更新。
/// `cargo test -p byte-wiki-core real_incremental_smoke -- --ignored --nocapture`
#[tokio::test]
#[ignore = "需要本机 ollama 服务（较慢）"]
async fn real_incremental_smoke() {
    use crate::generator::UpdateSummary;
    let src = std::path::Path::new("D:/code/cicbyte/byte-wiki");
    if !src.exists() {
        eprintln!("跳过：源仓库不存在");
        return;
    }
    // 复制源码（同 real_e2e_smoke 的排除规则，仅保留源码子集以提速：crates/apps/tests）
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    for sub in ["crates", "apps/desktop/src", "apps/desktop/src-tauri/src", "apps/cli/src", "tests"] {
        let from = src.join(sub);
        if !from.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&from).into_iter().filter_map(|e| e.ok()).filter(|e| e.file_type().is_file()) {
            let rel = entry.path().strip_prefix(src).unwrap();
            let to = repo.join(rel);
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::fs::copy(entry.path(), &to).unwrap();
        }
    }
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();

    let svc = AppService::open_at(tmp.path().join("app"), tmp.path().join("data")).unwrap();
    svc.set_llm_config(LlmConfig {
        providers: vec![LlmProvider {
            id: "real".into(),
            name: "Ollama 本地".into(),
            kind: "ollama".into(),
            api_endpoint: "http://localhost:11434".into(),
            api_key: String::new(),
            enabled: true,
            models: vec![],
            chat_model: "qwen3.5:9b".into(),
            embed_model: "nomic-embed-text:latest".into(),
        }],
        active_id: "real".into(),
        embed_provider_id: None,
        presets_seeded: true,
    })
    .unwrap();

    svc.open_repo(repo.to_str().unwrap()).unwrap();
    let g = svc.generate_wiki(repo.to_str().unwrap(), &|_| {}).await.unwrap();
    eprintln!("全量生成：{} 页 {}ms", g.pages_generated, g.elapsed_ms);

    // 页级增量基线：记录全部页面内容，改 tests/conftest.py 后应有页面保持字节不变
    let mut baseline: Vec<(std::path::PathBuf, String)> = Vec::new();
    for entry in walkdir::WalkDir::new(repo.join(".bytewiki/pages")).into_iter().filter_map(|e| e.ok()).filter(|e| e.file_type().is_file()) {
        if let Ok(c) = std::fs::read_to_string(entry.path()) {
            baseline.push((entry.path().to_path_buf(), c));
        }
    }

    let orig_py = std::fs::read_to_string(repo.join("tests/conftest.py")).unwrap();
    std::fs::write(repo.join("tests/conftest.py"), format!("# incremental smoke edit\n{orig_py}")).unwrap();

    let report = svc.check_staleness(repo.to_str().unwrap()).unwrap();
    assert!(report.stale, "应检出滞后");
    eprintln!("滞后：{} 变更 → {:?}", report.changes.len(), report.affected_modules);

    let u: UpdateSummary = svc.update_wiki(repo.to_str().unwrap(), &|e| eprintln!("  [{}] {}", e.phase, e.message)).await.unwrap();
    eprintln!("增量更新：{} 页重生成 / {} 文件变更 / {}ms", u.regenerated_pages, u.changed_files, u.elapsed_ms);
    assert!(!u.unchanged && u.changed_files >= 1);
    // 未受影响页面内容不变（绑定文件不含 tests/conftest.py 的页面）
    let mut kept = 0;
    for (path, base) in &baseline {
        if let Ok(now) = std::fs::read_to_string(path) {
            if &now == base {
                kept += 1;
            }
        }
    }
    eprintln!("页面保持不变：{}/{}", kept, baseline.len());
    // 二次更新 no-op
    let u2 = svc.update_wiki(repo.to_str().unwrap(), &|_| {}).await.unwrap();
    assert!(u2.unchanged);
}
