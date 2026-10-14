//! LLM 客户端：chat（流式 / 非流式）与 embedding
//!
//! 双协议（按 `LlmConfig.provider` 分派）：
//! - ollama：`/api/chat`（NDJSON 流）、`/api/embed`
//! - openai ：`/chat/completions`（SSE 流）、`/embeddings`（OpenAI 兼容端点）
//!
//! 流式实现用 `Response::chunk()` 逐块读取 + 行缓冲解析，无额外依赖。

use crate::model::{LlmConfig, LlmProvider};
use crate::{CoreError, CoreResult};

/// 对话消息（role: system / user / assistant）
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".into(), content: content.into() }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".into(), content: content.into() }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: "assistant".into(), content: content.into() }
    }
}

/// 对话参数（全部可选，默认由服务端决定）
#[derive(Debug, Clone, Default)]
pub struct ChatOptions {
    pub temperature: Option<f32>,
    pub max_tokens: Option<u64>,
}

/// 一次对话的结果（全文 + token 用量，用量不可得时为 0）
#[derive(Debug, Clone, Default)]
pub struct ChatOutcome {
    pub text: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

/// LLM 客户端（只读配置，可复用；无内部状态）
pub struct LlmClient {
    http: reqwest::Client,
    cfg: LlmProvider,
}

impl LlmClient {
    /// 按 embedding 独立供应商创建（embed_provider_id 优先，回退当前供应商）——
    /// 检索 query 向量化与建库必须同一模型，两侧都走这里
    pub fn for_embed(cfg: &LlmConfig) -> CoreResult<Self> {
        let p = match cfg.embed_provider_id.as_deref() {
            Some(id) if !id.is_empty() => cfg.providers.iter().find(|p| p.id == id),
            _ => None,
        };
        match p {
            Some(p) if p.enabled => Self::for_provider(p),
            _ => Self::new(cfg),
        }
    }

    /// 按对话供应商覆盖创建（AI 对话栏的模型选择）：provider_id 指定且可用时用之，
    /// 否则回退当前供应商（全局默认）
    pub fn for_chat(cfg: &LlmConfig, provider_id: Option<&str>) -> CoreResult<Self> {
        let p = match provider_id {
            Some(id) if !id.is_empty() => cfg.providers.iter().find(|p| p.id == id),
            _ => None,
        };
        match p {
            Some(p) if p.enabled => Self::for_provider(p),
            _ => Self::new(cfg),
        }
    }

    /// 按当前使用的供应商创建
    pub fn new(cfg: &LlmConfig) -> CoreResult<Self> {
        let Some(p) = cfg.active() else {
            return Err(CoreError::Other(anyhow::anyhow!(
                "未找到当前使用的供应商（active_id={}）",
                cfg.active_id
            )));
        };
        Self::for_provider(p)
    }

    /// 模型名（日志/调试展示）
    pub fn model_name(&self) -> &str {
        &self.cfg.chat_model
    }

    /// 供应商描述（id · 名称，日志用）
    pub fn provider_desc(&self) -> String {
        format!("{} · {}", self.cfg.id, self.cfg.name)
    }

    /// 按指定供应商创建
    pub fn for_provider(p: &LlmProvider) -> CoreResult<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| CoreError::Other(anyhow::anyhow!("HTTP 客户端创建失败：{e}")))?;
        Ok(Self { http, cfg: p.clone() })
    }

    /// 非流式对话
    pub async fn chat(&self, messages: &[ChatMessage], opts: &ChatOptions) -> CoreResult<String> {
        Ok(self.chat_detailed(messages, opts).await?.text)
    }

    /// 非流式对话（带 token 用量）
    pub async fn chat_detailed(
        &self,
        messages: &[ChatMessage],
        opts: &ChatOptions,
    ) -> CoreResult<ChatOutcome> {
        match self.cfg.kind.as_str() {
            "ollama" => self.ollama_chat(messages, opts).await,
            "openai" => self.openai_chat(messages, opts).await,
            other => Err(unknown_provider(other)),
        }
    }

    /// 流式对话：每收到增量片段调用 `on_delta`，返回全文与用量
    pub async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        opts: &ChatOptions,
        on_delta: &(dyn Fn(&str) + Send + Sync),
    ) -> CoreResult<ChatOutcome> {
        self.chat_stream_ext(messages, opts, on_delta, &|_| {}, &|| false)
            .await
    }

    /// 流式对话（扩展）：思考过程通道 + 取消检查（token 边界检测，返回 ChatCancelled）
    pub async fn chat_stream_ext(
        &self,
        messages: &[ChatMessage],
        opts: &ChatOptions,
        on_delta: &(dyn Fn(&str) + Send + Sync),
        on_thinking: &(dyn Fn(&str) + Send + Sync),
        is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    ) -> CoreResult<ChatOutcome> {
        match self.cfg.kind.as_str() {
            "ollama" => {
                self.ollama_chat_stream_ext(messages, opts, on_delta, on_thinking, is_cancelled)
                    .await
            }
            "openai" => {
                self.openai_chat_stream_ext(messages, opts, on_delta, on_thinking, is_cancelled)
                    .await
            }
            other => Err(unknown_provider(other)),
        }
    }

    /// 批量 embedding（返回与输入等长的向量数组）
    pub async fn embed(&self, texts: &[String]) -> CoreResult<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }
        match self.cfg.kind.as_str() {
            "ollama" => self.ollama_embed(texts).await,
            "openai" => self.openai_embed(texts).await,
            other => Err(unknown_provider(other)),
        }
    }

    // ============ ollama ============

    async fn ollama_chat(
        &self,
        messages: &[ChatMessage],
        opts: &ChatOptions,
    ) -> CoreResult<ChatOutcome> {
        let url = self.endpoint("/api/chat");
        let body = serde_json::json!({
            "model": self.cfg.chat_model,
            "messages": messages,
            "stream": false,
            "options": ollama_options(opts),
        });
        let resp = self.post_json(&url, &body).await?;
        let v: serde_json::Value = resp.json().await.map_err(bad_body)?;
        let text = v
            .pointer("/message/content")
            .and_then(|t| t.as_str())
            .ok_or_else(|| CoreError::Other(anyhow::anyhow!("ollama 响应缺少 message.content")))?;
        Ok(ChatOutcome {
            text: text.to_string(),
            prompt_tokens: v.get("prompt_eval_count").and_then(|n| n.as_u64()).unwrap_or(0),
            completion_tokens: v.get("eval_count").and_then(|n| n.as_u64()).unwrap_or(0),
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn ollama_chat_stream_ext(
        &self,
        messages: &[ChatMessage],
        opts: &ChatOptions,
        on_delta: &(dyn Fn(&str) + Send + Sync),
        on_thinking: &(dyn Fn(&str) + Send + Sync),
        is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    ) -> CoreResult<ChatOutcome> {
        let url = self.endpoint("/api/chat");
        let body = serde_json::json!({
            "model": self.cfg.chat_model,
            "messages": messages,
            "stream": true,
            "options": ollama_options(opts),
        });
        let mut resp = self.post_json(&url, &body).await?;
        let mut outcome = ChatOutcome::default();
        let mut lines = LineBuffer::default();
        loop {
            let chunk = resp
                .chunk()
                .await
                .map_err(|e| CoreError::Other(anyhow::anyhow!("流读取失败：{e}")))?;
            let Some(chunk) = chunk else { break };
            if is_cancelled() {
                drop(resp);
                return Err(CoreError::Other(anyhow::anyhow!("__CHAT_CANCELLED__")));
            }
            for line in lines.push(&chunk) {
                if line.trim().is_empty() {
                    continue;
                }
                let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
                    tracing::warn!("ollama 流行解析失败，跳过：{line}");
                    continue;
                };
                // 思考通道：message.thinking（Ollama 新版）/ message.reasoning_content（兼容）
                if let Some(th) = v
                    .pointer("/message/thinking")
                    .or_else(|| v.pointer("/message/reasoning_content"))
                    .and_then(|t| t.as_str())
                {
                    if !th.is_empty() {
                        on_thinking(th);
                    }
                }
                if let Some(delta) = v.pointer("/message/content").and_then(|t| t.as_str()) {
                    if !delta.is_empty() {
                        on_delta(delta);
                        outcome.text.push_str(delta);
                    }
                }
                if v.get("done").and_then(|d| d.as_bool()).unwrap_or(false) {
                    outcome.prompt_tokens =
                        v.get("prompt_eval_count").and_then(|n| n.as_u64()).unwrap_or(0);
                    outcome.completion_tokens =
                        v.get("eval_count").and_then(|n| n.as_u64()).unwrap_or(0);
                }
            }
        }
        Ok(outcome)
    }

    async fn ollama_embed(&self, texts: &[String]) -> CoreResult<Vec<Vec<f32>>> {
        let url = self.endpoint("/api/embed");
        let body = serde_json::json!({ "model": self.cfg.embed_model, "input": texts });
        let resp = self.post_json(&url, &body).await?;
        let v: serde_json::Value = resp.json().await.map_err(bad_body)?;
        let arr = v
            .get("embeddings")
            .and_then(|e| e.as_array())
            .ok_or_else(|| CoreError::Other(anyhow::anyhow!("ollama 响应缺少 embeddings")))?;
        arr.iter()
            .map(|row| {
                row.as_array()
                    .map(|nums| nums.iter().filter_map(|n| n.as_f64().map(|f| f as f32)).collect())
                    .ok_or_else(|| CoreError::Other(anyhow::anyhow!("embedding 行格式错误")))
            })
            .collect()
    }

    // ============ openai 兼容 ============

    async fn openai_chat(
        &self,
        messages: &[ChatMessage],
        opts: &ChatOptions,
    ) -> CoreResult<ChatOutcome> {
        let url = self.endpoint_openai("/chat/completions");
        let body = serde_json::json!({
            "model": self.cfg.chat_model,
            "messages": messages,
            "stream": false,
            "temperature": opts.temperature,
            "max_tokens": opts.max_tokens,
        });
        let resp = self.post_json(&url, &body).await?;
        let v: serde_json::Value = resp.json().await.map_err(bad_body)?;
        let text = v
            .pointer("/choices/0/message/content")
            .and_then(|t| t.as_str())
            .ok_or_else(|| CoreError::Other(anyhow::anyhow!("响应缺少 choices[0].message.content")))?;
        Ok(ChatOutcome {
            text: text.to_string(),
            prompt_tokens: v.pointer("/usage/prompt_tokens").and_then(|n| n.as_u64()).unwrap_or(0),
            completion_tokens: v
                .pointer("/usage/completion_tokens")
                .and_then(|n| n.as_u64())
                .unwrap_or(0),
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn openai_chat_stream_ext(
        &self,
        messages: &[ChatMessage],
        opts: &ChatOptions,
        on_delta: &(dyn Fn(&str) + Send + Sync),
        on_thinking: &(dyn Fn(&str) + Send + Sync),
        is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    ) -> CoreResult<ChatOutcome> {
        let url = self.endpoint_openai("/chat/completions");
        let body = serde_json::json!({
            "model": self.cfg.chat_model,
            "messages": messages,
            "stream": true,
            "stream_options": { "include_usage": true },
            "temperature": opts.temperature,
            "max_tokens": opts.max_tokens,
        });
        let mut resp = self.post_json(&url, &body).await?;
        let mut outcome = ChatOutcome::default();
        let mut lines = LineBuffer::default();
        loop {
            let chunk = resp
                .chunk()
                .await
                .map_err(|e| CoreError::Other(anyhow::anyhow!("流读取失败：{e}")))?;
            let Some(chunk) = chunk else { break };
            if is_cancelled() {
                drop(resp);
                return Err(CoreError::Other(anyhow::anyhow!("__CHAT_CANCELLED__")));
            }
            for line in lines.push(&chunk) {
                let Some(data) = line.strip_prefix("data:") else { continue };
                let data = data.trim();
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }
                let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
                    tracing::warn!("openai 流行解析失败，跳过：{data}");
                    continue;
                };
                // 思考通道：delta.reasoning_content（DeepSeek-R1/GLM 兼容端点通用约定）
                if let Some(th) = v
                    .pointer("/choices/0/delta/reasoning_content")
                    .and_then(|t| t.as_str())
                {
                    if !th.is_empty() {
                        on_thinking(th);
                    }
                }
                if let Some(delta) = v.pointer("/choices/0/delta/content").and_then(|t| t.as_str()) {
                    if !delta.is_empty() {
                        on_delta(delta);
                        outcome.text.push_str(delta);
                    }
                }
                if let Some(u) = v.get("usage") {
                    outcome.prompt_tokens = u.get("prompt_tokens").and_then(|n| n.as_u64()).unwrap_or(0);
                    outcome.completion_tokens =
                        u.get("completion_tokens").and_then(|n| n.as_u64()).unwrap_or(0);
                }
            }
        }
        Ok(outcome)
    }

    async fn openai_embed(&self, texts: &[String]) -> CoreResult<Vec<Vec<f32>>> {
        let url = self.endpoint_openai("/embeddings");
        let body = serde_json::json!({ "model": self.cfg.embed_model, "input": texts });
        let resp = self.post_json(&url, &body).await?;
        let v: serde_json::Value = resp.json().await.map_err(bad_body)?;
        let arr = v
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| CoreError::Other(anyhow::anyhow!("响应缺少 data 数组")))?;
        arr.iter()
            .map(|row| {
                row.get("embedding")
                    .and_then(|e| e.as_array())
                    .map(|nums| nums.iter().filter_map(|n| n.as_f64().map(|f| f as f32)).collect())
                    .ok_or_else(|| CoreError::Other(anyhow::anyhow!("embedding 行格式错误")))
            })
            .collect()
    }

    // ============ 辅助 ============

    /// ollama 端点（base_url 直拼）
    fn endpoint(&self, path: &str) -> String {
        format!("{}{}", self.cfg.api_endpoint.trim_end_matches('/'), path)
    }

    /// openai 兼容端点（base_url 缺 /v1 时补上）
    fn endpoint_openai(&self, path: &str) -> String {
        let base = self.cfg.api_endpoint.trim_end_matches('/');
        if base.ends_with("/v1") {
            format!("{base}{path}")
        } else {
            format!("{base}/v1{path}")
        }
    }

    async fn post_json(
        &self,
        url: &str,
        body: &serde_json::Value,
    ) -> CoreResult<reqwest::Response> {
        let mut req = self.http.post(url).json(body);
        if !self.cfg.api_key.is_empty() {
            req = req.bearer_auth(&self.cfg.api_key);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| CoreError::Other(anyhow::anyhow!("请求失败（{url}）：{e}")))?;
        let status = resp.status();
        if !status.is_success() {
            let detail = resp.text().await.unwrap_or_default();
            return Err(CoreError::Other(anyhow::anyhow!(
                "服务返回 {status}（{url}）：{detail}"
            )));
        }
        Ok(resp)
    }
}

fn unknown_provider(other: &str) -> CoreError {
    CoreError::Other(anyhow::anyhow!(
        "未知的 provider 类型：{other}（支持 ollama / openai）"
    ))
}

fn bad_body(e: reqwest::Error) -> CoreError {
    CoreError::Other(anyhow::anyhow!("响应解析失败：{e}"))
}

fn ollama_options(opts: &ChatOptions) -> serde_json::Value {
    serde_json::json!({
        "temperature": opts.temperature,
        "num_predict": opts.max_tokens,
    })
}

/// 流式行缓冲：喂入字节块，产出完整行（LF 换行的协议均适用）
#[derive(Default)]
struct LineBuffer {
    buf: String,
}

impl LineBuffer {
    fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buf.push_str(&String::from_utf8_lossy(chunk));
        let mut lines = Vec::new();
        while let Some(pos) = self.buf.find('\n') {
            let line: String = self.buf.drain(..=pos).collect();
            lines.push(line.trim_end_matches(['\r', '\n']).to_string());
        }
        lines
    }
}
