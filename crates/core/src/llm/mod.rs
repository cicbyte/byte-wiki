//! LLM 接入层
//!
//! 统一封装 Ollama 与 OpenAI 兼容端点的访问：
//! - [`probe`]：探测 Provider 可达性并列出可用模型（设置页「测试连接」）
//! - [`client::LlmClient`]：chat（流式 / 非流式）与 embedding

pub mod client;
#[cfg(test)]
mod tests;

pub use client::{ChatMessage, ChatOptions, ChatOutcome, LlmClient};

use crate::model::{LlmConfig, LlmProvider};
use crate::{CoreError, CoreResult};

/// Provider 探测结果
#[derive(Debug)]
pub struct ProbeResult {
    /// 可用模型名列表
    pub models: Vec<String>,
}

/// 探测供应商可达性（当前使用的 provider）
pub async fn probe(cfg: &LlmConfig) -> CoreResult<ProbeResult> {
    let Some(p) = cfg.active() else {
        return Err(CoreError::Other(anyhow::anyhow!(
            "未找到当前使用的供应商（active_id={}）",
            cfg.active_id
        )));
    };
    probe_provider(p).await
}

/// 探测指定供应商可达性（设置页「检测」按钮）
///
/// - ollama：`GET {api_endpoint}/api/tags`
/// - openai：`GET {api_endpoint}/v1/models`（endpoint 已含 /v1 时不重复拼）
pub async fn probe_provider(p: &LlmProvider) -> CoreResult<ProbeResult> {
    let base = p.api_endpoint.trim_end_matches('/');
    let url = match p.kind.as_str() {
        "ollama" => format!("{base}/api/tags"),
        "openai" => {
            if base.ends_with("/v1") {
                format!("{base}/models")
            } else {
                format!("{base}/v1/models")
            }
        }
        other => {
            return Err(CoreError::Other(anyhow::anyhow!(
                "未知的 provider 类型：{other}（支持 ollama / openai）"
            )))
        }
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| CoreError::Other(anyhow::anyhow!("HTTP 客户端创建失败：{e}")))?;
    let mut req = client.get(&url);
    if !p.api_key.is_empty() {
        req = req.bearer_auth(&p.api_key);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| CoreError::Other(anyhow::anyhow!("连接失败：{e}")))?;
    if !resp.status().is_success() {
        return Err(CoreError::Other(anyhow::anyhow!(
            "服务返回 {}（{}）",
            resp.status(),
            url
        )));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| CoreError::Other(anyhow::anyhow!("响应解析失败：{e}")))?;

    // ollama: { models: [{ name, ... }] } / openai: { data: [{ id, ... }] }
    let models: Vec<String> = body
        .get("models")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("name").and_then(|n| n.as_str()))
                .map(String::from)
                .collect()
        })
        .or_else(|| {
            body.get("data").and_then(|d| d.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("id").and_then(|n| n.as_str()))
                    .map(String::from)
                    .collect()
            })
        })
        .unwrap_or_default();

    Ok(ProbeResult { models })
}
