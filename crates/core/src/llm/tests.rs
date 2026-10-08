//! LLM client 单元测试：本地 mock HTTP 服务覆盖双协议的
//! 非流式 / 流式 chat 与 embed，以及错误路径。

use std::net::SocketAddr;

use crate::llm::client::{ChatMessage, LlmClient};
use crate::model::LlmConfig;

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
async fn mock_ollama_chat(body: String) -> String {
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    if v["stream"].as_bool().unwrap_or(false) {
        concat!(
            "{\"message\":{\"content\":\"你\"},\"done\":false}\n",
            "{\"message\":{\"content\":\"好\"},\"done\":false}\n",
            "{\"done\":true,\"prompt_eval_count\":11,\"eval_count\":2}\n"
        )
        .into()
    } else {
        serde_json::json!({
            "message": { "role": "assistant", "content": "你好" },
            "prompt_eval_count": 11,
            "eval_count": 2,
        })
        .to_string()
    }
}

async fn mock_ollama_embed(body: String) -> Json<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let n = v["input"].as_array().map(|a| a.len()).unwrap_or(0);
    Json(serde_json::json!({ "embeddings": vec![[0.1f64, 0.2, 0.3]; n] }))
}

/// SSE 流或完整 JSON
async fn mock_openai_chat(body: String) -> String {
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    if v["stream"].as_bool().unwrap_or(false) {
        concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"He\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"llo\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{}}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":2}}\n\n",
            "data: [DONE]\n\n"
        )
        .into()
    } else {
        serde_json::json!({
            "choices": [ { "message": { "role": "assistant", "content": "Hello" } } ],
            "usage": { "prompt_tokens": 7, "completion_tokens": 2 }
        })
        .to_string()
    }
}

async fn mock_openai_embeddings(body: String) -> Json<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let n = v["input"].as_array().map(|a| a.len()).unwrap_or(0);
    Json(serde_json::json!({
        "data": (0..n).map(|i| serde_json::json!({ "index": i, "embedding": [0.4f64, 0.5] })).collect::<Vec<_>>()
    }))
}

fn cfg_for(base: &str, provider: &str) -> LlmConfig {
    LlmConfig {
        provider: provider.into(),
        base_url: base.into(),
        chat_model: "mock-model".into(),
        embed_model: "mock-embed".into(),
        api_key: "sk-test".into(),
    }
}

#[tokio::test]
async fn ollama_chat_non_stream() {
    let base = start_mock().await;
    let client = LlmClient::new(&cfg_for(&base, "ollama")).unwrap();
    let text = client
        .chat(&[ChatMessage::user("hi")], &Default::default())
        .await
        .unwrap();
    assert_eq!(text, "你好");
}

#[tokio::test]
async fn ollama_chat_stream_collects_deltas() {
    let base = start_mock().await;
    let client = LlmClient::new(&cfg_for(&base, "ollama")).unwrap();
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
    let client = LlmClient::new(&cfg_for(&base, "openai")).unwrap();
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
    let ollama = LlmClient::new(&cfg_for(&base, "ollama")).unwrap();
    let vecs = ollama.embed(&["a".into(), "b".into()]).await.unwrap();
    assert_eq!(vecs.len(), 2);
    assert_eq!(vecs[0], vec![0.1, 0.2, 0.3]);

    let openai = LlmClient::new(&cfg_for(&base, "openai")).unwrap();
    let vecs = openai.embed(&["x".into()]).await.unwrap();
    assert_eq!(vecs, vec![vec![0.4, 0.5]]);
}

#[tokio::test]
async fn empty_embed_input_is_noop() {
    let client = LlmClient::new(&cfg_for("http://127.0.0.1:1", "ollama")).unwrap();
    assert!(client.embed(&[]).await.unwrap().is_empty());
}

#[tokio::test]
async fn http_error_maps_to_message() {
    let base = start_mock().await;
    let mut cfg = cfg_for(&base, "ollama");
    cfg.base_url = format!("{base}/err");
    let client = LlmClient::new(&cfg).unwrap();
    let err = client
        .chat(&[ChatMessage::user("hi")], &Default::default())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("500"), "实际错误：{err}");
}

#[tokio::test]
async fn unknown_provider_rejected() {
    let client = LlmClient::new(&cfg_for("http://127.0.0.1:1", "anthropic")).unwrap();
    let err = client
        .chat(&[ChatMessage::user("hi")], &Default::default())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("未知的 provider"));
}

/// 真实 ollama 冒烟（默认忽略）：`cargo test -p byte-wiki-core -- --ignored`
/// 依赖本机 ollama 服务与 OLLAMA_SMOKE_MODEL 指定的已拉取模型。
#[tokio::test]
#[ignore = "需要本机 ollama 服务"]
async fn real_ollama_smoke() {
    let model = std::env::var("OLLAMA_SMOKE_MODEL").unwrap_or_else(|_| "openbmb/minicpm5:latest".into());
    let cfg = LlmConfig {
        provider: "ollama".into(),
        base_url: "http://localhost:11434".into(),
        chat_model: model.clone(),
        embed_model: model,
        api_key: String::new(),
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
