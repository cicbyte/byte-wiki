//! REST API 服务（axum）
//!
//! 标准 HTTP REST 接口，端点前缀 `/api/v1/`，支持 API Key 认证（X-API-Key 请求头）。
//! 骨架阶段仅提供 health / status / docs；Wiki 查询端点随后续功能补充。

mod docs;
mod state;

use std::sync::Arc;

use axum::{
    http::StatusCode,
    middleware::{from_fn_with_state, Next},
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};

use crate::AppService;

pub use state::ApiState;

use docs::{openapi_spec, swagger_ui};

// ============ 启动函数 ============

/// 启动 REST API HTTP 服务（managed，返回 JoinHandle，外部用 cancellation token 停止）
pub fn serve_api_http_managed(
    svc: Arc<AppService>,
    host: &str,
    port: u16,
    api_key: String,
    allow_local_no_key: bool,
    _cancel: tokio_util::sync::CancellationToken,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    let state = ApiState {
        svc,
        api_key,
        allow_local_no_key,
    };
    let app = api_router(state);
    let addr = format!("{host}:{port}");
    let rt = tokio::runtime::Handle::current();
    let handle = rt.spawn(async move {
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(listener) => {
                tracing::info!("REST API server listening on http://{addr}/api/v1");
                let _ = axum::serve(listener, app).await;
            }
            Err(e) => tracing::error!("REST API bind {addr} 失败：{e}"),
        }
    });
    Ok(handle)
}

// ============ handler ============

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn status(axum::extract::State(state): axum::extract::State<ApiState>) -> Response {
    let repos = state.svc.db.repo_count().unwrap_or(-1);
    Json(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "data_dir": state.svc.data_dir().to_string_lossy(),
        "repos": repos,
    }))
    .into_response()
}

// ============ router ============

/// API Key 中间件（简化版：在 router 层用 layer 挂载）
async fn api_key_middleware(
    axum::extract::State(state): axum::extract::State<ApiState>,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    if state.api_key.is_empty() || (state.allow_local_no_key && is_local(&req)) {
        return next.run(req).await;
    }
    // 检查 X-API-Key 头
    let provided = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if provided == state.api_key {
        return next.run(req).await;
    }
    // 检查 Bearer token
    let auth = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if let Some(token) = auth.strip_prefix("Bearer ") {
        if token == state.api_key {
            return next.run(req).await;
        }
    }
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({"error": "Invalid or missing API key. Provide X-API-Key header."})),
    )
        .into_response()
}

fn is_local(req: &axum::extract::Request) -> bool {
    req.headers()
        .get("host")
        .and_then(|v| v.to_str().ok())
        .map(|h| h.starts_with("127.0.0.1") || h.starts_with("localhost"))
        .unwrap_or(false)
}

/// 构建带认证中间件的完整 router（health/docs/openapi.json 不需认证）
fn api_router(state: ApiState) -> Router {
    Router::new()
        // 免认证：健康检查 + Swagger UI + OpenAPI spec
        .route("/api/v1/health", get(health))
        .route("/api/v1/docs", get(swagger_ui))
        .route("/api/v1/openapi.json", get(openapi_spec))
        // 需认证
        .route("/api/v1/status", get(status))
        .with_state(state.clone())
        .route_layer(from_fn_with_state(state, api_key_middleware))
}
