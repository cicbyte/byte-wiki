//! LLM 客户端：rig-core 内核 + 项目门面
//!
//! 内核全面走 rig（openai Completions 兼容 / ollama 两个 provider）：
//! 流式增量、思考通道（`reasoning_content` / `reasoning` 字段变体由 rig
//! 归一为 ReasoningDelta）、usage 归一化、多供应商生态都交给 rig；
//! 门面保持既有 API 与配置模型不变（`LlmProvider.kind = ollama|openai`），
//! 缓存、模型日志、心跳、流式终端、`__CHAT_CANCELLED__` 取消哨兵等
//! 上层钩子零改动。
//!
//! 超时：给 rig 注入自建 reqwest 0.13 后端（总 600s / 连接 10s），
//! 与旧手写客户端语义一致。

use crate::model::{LlmConfig, LlmProvider};
use crate::{CoreError, CoreResult};

use futures_util::StreamExt;
use rig_core::client::{CompletionClient, EmbeddingsClient};
use rig_core::completion::CompletionModel;
use rig_core::message::Message as RigMessage;
use rig_core::providers::openai::CompletionsClient;
use rig_core::providers::ollama;
use rig_core::streaming::{StreamFinal, StreamedAssistantContent, StreamingCompletionResponse};

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

/// 取消哨兵：token 边界中断时返回（前端识别为用户主动停止）
const CANCEL_SENTINEL: &str = "__CHAT_CANCELLED__";

/// rig 网关：按供应商 kind 分派
enum Gateway {
    OpenAi(CompletionsClient),
    Ollama(ollama::Client),
}

/// LLM 客户端（只读配置，可复用；无内部状态）
pub struct LlmClient {
    cfg: LlmProvider,
    gateway: Gateway,
}

fn http_backend() -> CoreResult<reqwest13::Client> {
    reqwest13::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| CoreError::Other(anyhow::anyhow!("HTTP 客户端创建失败：{e}")))
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

    /// 按指定供应商创建
    pub fn for_provider(p: &LlmProvider) -> CoreResult<Self> {
        let gateway = match p.kind.as_str() {
            "ollama" => Gateway::Ollama(
                ollama::Client::builder()
                    .api_key(ollama::OllamaApiKey::default())
                    .base_url(p.api_endpoint.trim_end_matches('/'))
                    .http_client(http_backend()?)
                    .build()
                    .map_err(|e| CoreError::Other(anyhow::anyhow!("ollama 客户端构建失败：{e}")))?,
            ),
            // openai 兼容端点：base 缺 /v1 时补上（deepseek/sensenova 裸域名都能用）
            "openai" => {
                let mut base = p.api_endpoint.trim_end_matches('/').to_string();
                if !base.ends_with("/v1") {
                    base.push_str("/v1");
                }
                // 空 key（本地网关）用占位串，rig 构建要求 key 必填
                let key = if p.api_key.trim().is_empty() { "EMPTY".to_string() } else { p.api_key.clone() };
                Gateway::OpenAi(
                    CompletionsClient::builder()
                        .api_key(key)
                        .base_url(base)
                        .http_client(http_backend()?)
                        .build()
                        .map_err(|e| CoreError::Other(anyhow::anyhow!("openai 兼容客户端构建失败：{e}")))?,
                )
            }
            other => return Err(unknown_provider(other)),
        };
        Ok(Self { cfg: p.clone(), gateway })
    }

    /// 模型名（日志/调试展示）
    pub fn model_name(&self) -> &str {
        &self.cfg.chat_model
    }

    /// 供应商描述（id · 名称，日志用）
    pub fn provider_desc(&self) -> String {
        format!("{} · {}", self.cfg.id, self.cfg.name)
    }

    /// 非流式对话
    pub async fn chat(&self, messages: &[ChatMessage], opts: &ChatOptions) -> CoreResult<String> {
        Ok(self.chat_detailed(messages, opts).await?.text)
    }

    /// 非流式对话（带 token 用量；内部复用流式通道累积）
    pub async fn chat_detailed(
        &self,
        messages: &[ChatMessage],
        opts: &ChatOptions,
    ) -> CoreResult<ChatOutcome> {
        self.chat_stream_ext(messages, opts, &|_| {}, &|_| {}, &|| false)
            .await
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

    /// 流式对话（扩展）：思考过程通道 + 取消检查（token 边界检测，返回取消哨兵错误）
    pub async fn chat_stream_ext(
        &self,
        messages: &[ChatMessage],
        opts: &ChatOptions,
        on_delta: &(dyn Fn(&str) + Send + Sync),
        on_thinking: &(dyn Fn(&str) + Send + Sync),
        is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    ) -> CoreResult<ChatOutcome> {
        match &self.gateway {
            Gateway::OpenAi(c) => {
                let model = c.completion_model(self.cfg.chat_model.clone());
                drive_stream(&model, messages, opts, on_delta, on_thinking, is_cancelled).await
            }
            Gateway::Ollama(c) => {
                let model = c.completion_model(self.cfg.chat_model.clone());
                drive_stream(&model, messages, opts, on_delta, on_thinking, is_cancelled).await
            }
        }
    }

    /// 文本向量化（建库与检索共用；与建库同模型由调用方保证）
    pub async fn embed(&self, texts: &[String]) -> CoreResult<Vec<Vec<f32>>> {
        use rig_core::embeddings::EmbeddingModel;
        if texts.is_empty() {
            return Ok(vec![]);
        }
        let embeddings = match &self.gateway {
            Gateway::OpenAi(c) => {
                let m = c.embedding_model(self.cfg.embed_model.clone());
                m.embed_texts(texts.to_vec()).await
            }
            Gateway::Ollama(c) => {
                let m = c.embedding_model(self.cfg.embed_model.clone());
                m.embed_texts(texts.to_vec()).await
            }
        };
        let embeddings = embeddings
            .map_err(|e| CoreError::Other(anyhow::anyhow!("embedding 失败：{e}")))?;
        Ok(embeddings.into_iter().map(|e| e.vec.iter().map(|&f| f as f32).collect()).collect())
    }
}

/// 驱动 rig 流：消息映射（system→preamble，末条 user 作 prompt）→
/// 流项分派（Text→正文、Reasoning*→思考、Final→usage），token 边界检查取消
async fn drive_stream<M: CompletionModel + Clone>(
    model: &M,
    messages: &[ChatMessage],
    opts: &ChatOptions,
    on_delta: &(dyn Fn(&str) + Send + Sync),
    on_thinking: &(dyn Fn(&str) + Send + Sync),
    is_cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> CoreResult<ChatOutcome> {
    let preamble: Option<String> = messages
        .iter()
        .find(|m| m.role == "system")
        .map(|m| m.content.clone());
    let convo: Vec<RigMessage> = messages
        .iter()
        .filter(|m| m.role != "system")
        .map(|m| {
            if m.role == "assistant" {
                RigMessage::assistant(m.content.clone())
            } else {
                RigMessage::user(m.content.clone())
            }
        })
        .collect();
    let (prompt, history) = match convo.split_last() {
        Some((last, rest)) => (last.clone(), rest.to_vec()),
        None => return Err(CoreError::Other(anyhow::anyhow!("消息列表为空（缺 user 消息）"))),
    };

    let mut builder = model.completion_request(prompt);
    if let Some(sys) = preamble {
        builder = builder.preamble(sys);
    }
    builder = builder
        .messages(history)
        .temperature_opt(opts.temperature.map(f64::from))
        .max_tokens_opt(opts.max_tokens);

    let mut stream: StreamingCompletionResponse = builder
        .stream()
        .await
        .map_err(rig_err)?;
    let mut out = ChatOutcome::default();
    while let Some(item) = stream.next().await {
        if is_cancelled() {
            drop(stream); // 中断底层请求（真取消，不再计费）
            return Err(CoreError::Other(anyhow::anyhow!(CANCEL_SENTINEL)));
        }
        match item.map_err(rig_err)? {
            StreamedAssistantContent::Text(t) => {
                let s = t.text.clone();
                if !s.is_empty() {
                    on_delta(&s);
                    out.text.push_str(&s);
                }
            }
            StreamedAssistantContent::ReasoningDelta { reasoning, .. } => {
                if !reasoning.is_empty() {
                    on_thinking(&reasoning);
                }
            }
            StreamedAssistantContent::Reasoning { reasoning, .. } => {
                let s = reasoning_text(&reasoning);
                if !s.is_empty() {
                    on_thinking(&s);
                }
            }
            StreamedAssistantContent::Final(f) => {
                apply_usage(&f, &mut out);
            }
            other => {
                tracing::debug!("忽略流项：{other:?}");
            }
        }
    }
    Ok(out)
}

fn reasoning_text(r: &rig_core::message::Reasoning) -> String {
    use rig_core::message::ReasoningContent;
    r.content
        .iter()
        .map(|c| match c {
            ReasoningContent::Text { text, .. } => text.as_str(),
            ReasoningContent::Summary(s) => s.as_str(),
            _ => "",
        })
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("
")
}

fn apply_usage(f: &StreamFinal, out: &mut ChatOutcome) {
    let u = &f.usage;
    out.prompt_tokens = u.input_tokens;
    out.completion_tokens = u.output_tokens;
}

fn rig_err(e: rig_core::completion::CompletionError) -> CoreError {
    CoreError::Other(anyhow::anyhow!("LLM 调用失败：{e}"))
}

fn unknown_provider(kind: &str) -> CoreError {
    CoreError::Other(anyhow::anyhow!("未知供应商类型：{kind}（支持 ollama / openai）"))
}
