//! Swagger UI 页面 + OpenAPI 3.0 JSON spec（无需鉴权）

use axum::response::{IntoResponse, Json};

/// Swagger UI HTML 页面（CDN 加载，spec 指向 /api/v1/openapi.json）
pub(super) async fn swagger_ui() -> impl IntoResponse {
    let html = r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>byte-wiki REST API</title>
  <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css" />
  <style>body { margin: 0; }</style>
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js" crossorigin></script>
  <script>
    window.onload = () => {
      window.ui = SwaggerUIBundle({
        url: "/api/v1/openapi.json",
        dom_id: "#swagger-ui",
        deepLinking: true,
        presets: [SwaggerUIBundle.presets.apis],
        layout: "BaseLayout",
      });
    };
  </script>
</body>
</html>"##;
    (
        [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
        html,
    )
}

/// OpenAPI 3.0 JSON spec
pub(super) async fn openapi_spec() -> impl IntoResponse {
    let spec = serde_json::json!({
        "openapi": "3.0.3",
        "info": {
            "title": "byte-wiki REST API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "byte-wiki（Repo Wiki）REST API。当前为骨架端点，Wiki 查询随后续版本提供。"
        },
        "components": {
            "securitySchemes": {
                "ApiKey": {
                    "type": "apiKey",
                    "in": "header",
                    "name": "X-API-Key"
                }
            }
        },
        "paths": {
            "/api/v1/health": {
                "get": {
                    "summary": "健康检查",
                    "responses": {"200": {"description": "服务状态"}}
                }
            },
            "/api/v1/status": {
                "get": {
                    "summary": "服务概要（版本/数据目录/仓库数）",
                    "security": [{"ApiKey": []}],
                    "responses": {"200": {"description": "状态信息"}}
                }
            }
        }
    });
    Json(spec)
}
