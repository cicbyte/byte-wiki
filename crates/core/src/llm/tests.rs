//! LLM client 单元测试：本地 mock HTTP 服务覆盖双协议的
//! 非流式 / 流式 chat 与 embed，以及错误路径。

use std::net::SocketAddr;

use crate::llm::client::{ChatMessage, LlmClient};
use crate::model::{LlmConfig, LlmProvider};

/// 单供应商测试配置
fn llm_cfg(kind: &str, endpoint: &str) -> LlmConfig {
    LlmConfig {
        providers: vec![LlmProvider {
            id: "test".into(),
            name: "测试供应商".into(),
            kind: kind.into(),
            api_endpoint: endpoint.into(),
            api_key: "sk-test".into(),
            enabled: true,
            models: vec![],
            chat_model: "mock-model".into(),
            embed_model: "mock-embed".into(),
        }],
        active_id: "test".into(),
        embed_provider_id: None,
        presets_seeded: true,
    }
}

use axum::{
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use tokio::net::TcpListener;

/// 启动 mock 服务，返回根地址（如 http://127.0.0.1:port）
async fn start_mock() -> String {
    let app = Router::new()
        // ollama
        .route("/api/tags", get(|| async {
            Json(serde_json::json!({ "models": [{ "name": "mock-model" }] }))
        }))
        .route("/api/chat", post(mock_ollama_chat))
        .route("/api/embed", post(mock_ollama_embed))
        // openai 兼容
        .route("/v1/models", get(|| async {
            Json(serde_json::json!({ "data": [{ "id": "mock-model" }] }))
        }))
        .route("/v1/chat/completions", post(mock_openai_chat))
        .route("/v1/embeddings", post(mock_openai_embeddings))
        // 错误路径
        .route("/err/api/chat", post(|| async {
            (StatusCode::INTERNAL_SERVER_ERROR, "boom")
        }));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{}", addr)
}

/// 按请求体里的 stream 字段返回 NDJSON 流或完整 JSON
/// （rig 内核按 ollama 官方响应结构解析：每行需 model/created_at，message 需 role）
async fn mock_ollama_chat(body: String) -> String {
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    if v["stream"].as_bool().unwrap_or(false) {
        concat!(
            "{\"model\":\"mock-model\",\"created_at\":\"2026-01-01T00:00:00Z\",\"message\":{\"role\":\"assistant\",\"content\":\"你\"},\"done\":false}\n",
            "{\"model\":\"mock-model\",\"created_at\":\"2026-01-01T00:00:00Z\",\"message\":{\"role\":\"assistant\",\"content\":\"好\"},\"done\":false}\n",
            "{\"model\":\"mock-model\",\"created_at\":\"2026-01-01T00:00:00Z\",\"message\":{\"role\":\"assistant\",\"content\":\"\"},\"done\":true,\"done_reason\":\"stop\",\"prompt_eval_count\":11,\"eval_count\":2}\n"
        )
        .into()
    } else {
        serde_json::json!({
            "model": "mock-model",
            "created_at": "2026-01-01T00:00:00Z",
            "message": { "role": "assistant", "content": "你好" },
            "done": true,
            "done_reason": "stop",
            "prompt_eval_count": 11,
            "eval_count": 2,
        })
        .to_string()
    }
}

async fn mock_ollama_embed(body: String) -> Json<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let n = v["input"].as_array().map(|a| a.len()).unwrap_or(0);
    Json(serde_json::json!({ "model": "mock-embed", "embeddings": vec![[0.1f64, 0.2, 0.3]; n] }))
}

/// SSE 流或完整 JSON（rig 内核要求 SSE 响应带 text/event-stream 头）
async fn mock_openai_chat(body: String) -> ([(axum::http::header::HeaderName, &'static str); 1], String) {
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let body = if v["stream"].as_bool().unwrap_or(false) {
        concat!(
            "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"model\":\"mock-model\",\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"He\"}}]}\n\n",
            "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"model\":\"mock-model\",\"choices\":[{\"delta\":{\"content\":\"llo\"}}]}\n\n",
            "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"model\":\"mock-model\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":2,\"total_tokens\":9}}\n\n",
            "data: [DONE]\n\n"
        )
        .to_string()
    } else {
        serde_json::json!({
            "id": "c1",
            "object": "chat.completion",
            "model": "mock-model",
            "choices": [ { "message": { "role": "assistant", "content": "Hello" }, "finish_reason": "stop" } ],
            "usage": { "prompt_tokens": 7, "completion_tokens": 2, "total_tokens": 9 }
        })
        .to_string()
    };
    ([(axum::http::header::CONTENT_TYPE, "text/event-stream")], body)
}

async fn mock_openai_embeddings(body: String) -> Json<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let n = v["input"].as_array().map(|a| a.len()).unwrap_or(0);
    Json(serde_json::json!({
        "object": "list",
        "model": "mock-embed",
        "data": (0..n).map(|i| serde_json::json!({ "object": "embedding", "index": i, "embedding": [0.4f64, 0.5] })).collect::<Vec<_>>(),
        "usage": { "prompt_tokens": 1, "total_tokens": 1 }
    }))
}



#[tokio::test]
async fn ollama_chat_non_stream() {
    let base = start_mock().await;
    let client = LlmClient::new(&llm_cfg("ollama", &base)).unwrap();
    let text = client
        .chat(&[ChatMessage::user("hi")], &Default::default())
        .await
        .unwrap();
    assert_eq!(text, "你好");
}

#[tokio::test]
async fn ollama_chat_stream_collects_deltas() {
    let base = start_mock().await;
    let client = LlmClient::new(&llm_cfg("ollama", &base)).unwrap();
    let deltas: std::sync::Mutex<Vec<String>> = Default::default();
    let out = client
        .chat_stream(
            &[ChatMessage::user("hi")],
            &Default::default(),
            &|d| deltas.lock().unwrap().push(d.to_string()),
        )
        .await
        .unwrap();
    let deltas = deltas.into_inner().unwrap();
    assert_eq!(out.text, "你好");
    assert_eq!(deltas, vec!["你", "好"]);
    assert_eq!((out.prompt_tokens, out.completion_tokens), (11, 2));
}

#[tokio::test]
async fn openai_chat_stream_sse() {
    let base = start_mock().await;
    let client = LlmClient::new(&llm_cfg("openai", &base)).unwrap();
    let text = client
        .chat(&[ChatMessage::user("hi")], &Default::default())
        .await
        .unwrap();
    assert_eq!(text, "Hello");

    let deltas: std::sync::Mutex<Vec<String>> = Default::default();
    let out = client
        .chat_stream(
            &[ChatMessage::user("hi")],
            &Default::default(),
            &|d| deltas.lock().unwrap().push(d.to_string()),
        )
        .await
        .unwrap();
    let deltas = deltas.into_inner().unwrap();
    assert_eq!(out.text, "Hello");
    assert_eq!(deltas, vec!["He", "llo"]);
    assert_eq!(out.prompt_tokens, 7);
}

#[tokio::test]
async fn embed_both_providers() {
    let base = start_mock().await;
    let ollama = LlmClient::new(&llm_cfg("ollama", &base)).unwrap();
    let vecs = ollama.embed(&["a".into(), "b".into()]).await.unwrap();
    assert_eq!(vecs.len(), 2);
    assert_eq!(vecs[0], vec![0.1, 0.2, 0.3]);

    let openai = LlmClient::new(&llm_cfg("openai", &base)).unwrap();
    let vecs = openai.embed(&["x".into()]).await.unwrap();
    assert_eq!(vecs, vec![vec![0.4, 0.5]]);
}

#[tokio::test]
async fn empty_embed_input_is_noop() {
    let client = LlmClient::new(&llm_cfg("ollama", "http://127.0.0.1:1")).unwrap();
    assert!(client.embed(&[]).await.unwrap().is_empty());
}

#[tokio::test]
async fn http_error_maps_to_message() {
    let base = start_mock().await;
    let mut cfg = llm_cfg("ollama", &base);
    cfg.providers[0].api_endpoint = format!("{base}/err");
    let client = LlmClient::new(&cfg).unwrap();
    let err = client
        .chat(&[ChatMessage::user("hi")], &Default::default())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("500"), "实际错误：{err}");
}

#[tokio::test]
async fn unknown_provider_rejected() {
    // rig 内核下未知 kind 在构造期即失败（fail-fast）
    let err = match LlmClient::new(&llm_cfg("anthropic", "http://127.0.0.1:1")) {
        Ok(_) => panic!("未知供应商类型应在构造期被拒绝"),
        Err(e) => e,
    };
    assert!(err.to_string().contains("未知供应商类型"), "实际错误：{err}");
}

/// 真实 ollama 冒烟（默认忽略）：`cargo test -p byte-wiki-core -- --ignored`
/// 依赖本机 ollama 服务与 OLLAMA_SMOKE_MODEL 指定的已拉取模型。
#[tokio::test]
#[ignore = "需要本机 ollama 服务"]
async fn real_ollama_smoke() {
    let model = std::env::var("OLLAMA_SMOKE_MODEL").unwrap_or_else(|_| "openbmb/minicpm5:latest".into());
    let cfg = LlmConfig {
        providers: vec![LlmProvider {
            id: "real".into(),
            name: "Ollama".into(),
            kind: "ollama".into(),
            api_endpoint: "http://localhost:11434".into(),
            api_key: String::new(),
            enabled: true,
            models: vec![],
            chat_model: model.clone(),
            embed_model: model,
        }],
        active_id: "real".into(),
        embed_provider_id: None,
        presets_seeded: true,
    };
    let client = LlmClient::new(&cfg).unwrap();
    let text = client
        .chat(&[ChatMessage::user("用一个词回答：1+1=?")], &Default::default())
        .await
        .unwrap();
    assert!(!text.trim().is_empty(), "真实 chat 返回空");
    let deltas: std::sync::Mutex<Vec<String>> = Default::default();
    let out = client
        .chat_stream(
            &[ChatMessage::user("用一个词回答：2+2=?")],
            &Default::default(),
            &|d| deltas.lock().unwrap().push(d.to_string()),
        )
        .await
        .unwrap();
    assert!(!out.text.is_empty() && !deltas.into_inner().unwrap().is_empty(), "流式返回空");
}

/// 真实 openai 兼容端点思考流冒烟（默认忽略）：读本机 app 配置的当前供应商，
/// 验证 rig 内核下思考（reasoning/reasoning_content）与正文双通道流。
/// `cargo test -p byte-wiki-core real_openai_compat_reasoning_smoke -- --ignored --nocapture`
#[tokio::test]
#[ignore = "真实供应商调用（消耗少量 token）"]
async fn real_openai_compat_reasoning_smoke() {
    let svc = crate::service::AppService::open_default().unwrap();
    let cfg = svc.llm_config().unwrap();
    let client = LlmClient::new(&cfg).unwrap();
    eprintln!("供应商：{} · 模型：{}", client.provider_desc(), client.model_name());
    let think: std::sync::Mutex<Vec<String>> = Default::default();
    let deltas: std::sync::Mutex<Vec<String>> = Default::default();
    let out = client
        .chat_stream_ext(
            &[ChatMessage::user("1+1 等于几？先思考再回答。")],
            &Default::default(),
            &|d| deltas.lock().unwrap().push(d.to_string()),
            &|t| think.lock().unwrap().push(t.to_string()),
            &|| false,
        )
        .await
        .unwrap();
    let n_delta = deltas.into_inner().unwrap().len();
    let think_len: usize = think.into_inner().unwrap().iter().map(|s| s.len()).sum();
    eprintln!("正文 {} 段（{} 字），思考 {} 字，tokens p{}+c{}", n_delta, out.text.len(), think_len, out.prompt_tokens, out.completion_tokens);
    assert!(!out.text.trim().is_empty(), "正文为空");
    assert!(n_delta >= 1, "正文无流式增量");
}
