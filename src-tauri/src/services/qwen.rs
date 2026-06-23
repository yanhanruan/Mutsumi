//! Qwen LLM service (service layer) — Alibaba DashScope, OpenAI-compatible mode.
//!
//! Two capabilities, both layered over the generic [`HttpClient`]:
//!
//!   * [`QwenClient::chat`]  — non-streaming chat completion with tool-calling
//!     and optional web search. Used by the background pipelines (silent memory
//!     extraction, cognitive reflection) which want the *full* response.
//!   * [`QwenClient::embed`] / [`QwenClient::embed_batch`] — text embeddings
//!     (`text-embedding-v3`) for the memory store's semantic retrieval.
//!
//! Streaming chat for the user-facing pipeline is added separately (Phase 2),
//! since it needs Tauri channel wiring; the request types here already carry the
//! `stream` flag so that path reuses them.
//!
//! Endpoints (DashScope compatible mode):
//!   * chat:       `POST {base}/chat/completions`
//!   * embeddings: `POST {base}/embeddings`
//! where `base` defaults to `https://dashscope.aliyuncs.com/compatible-mode/v1`.
//! Auth is `Authorization: Bearer $DASHSCOPE_API_KEY`.

use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

use crate::http::{ApiError, HttpClient, HttpClientConfig};

const DEFAULT_BASE_URL: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1";
const DEFAULT_CHAT_MODEL: &str = "qwen3.7-plus";
/// Vision-capable model for image turns (multimodal). Env-overridable.
const DEFAULT_VISION_MODEL: &str = "qwen-vl-max";
const DEFAULT_EMBED_MODEL: &str = "text-embedding-v3";
/// `text-embedding-v3` supports 1024/768/512; 1024 is the default/highest.
const DEFAULT_EMBED_DIMENSIONS: u32 = 1024;
const HTTP_TIMEOUT: Duration = Duration::from_secs(60);

const CHAT_PATH: &str = "/chat/completions";
const EMBED_PATH: &str = "/embeddings";
/// Max inputs per embeddings request (DashScope `text-embedding-v3` limit).
const EMBED_BATCH_LIMIT: usize = 10;

// Env vars (loaded from src-tauri/.env at startup; see .env.example).
const ENV_API_KEY: &str = "DASHSCOPE_API_KEY";
const ENV_BASE_URL: &str = "QWEN_BASE_URL";
const ENV_CHAT_MODEL: &str = "QWEN_CHAT_MODEL";
const ENV_VISION_MODEL: &str = "QWEN_VISION_MODEL";

// ── Configuration ───────────────────────────────────────────────────

/// Static configuration captured by [`QwenClient::new`].
#[derive(Clone)]
pub struct QwenConfig {
    pub api_key: String,
    pub base_url: String,
    pub chat_model: String,
    pub vision_model: String,
    pub embed_model: String,
    pub embed_dimensions: u32,
}

/// Build config from the environment, falling back to DashScope defaults.
///
/// Only `api_key` is required for live calls; a missing key yields an empty
/// string so the app still boots (calls then fail with an auth error).
pub fn config_from_env() -> QwenConfig {
    QwenConfig {
        api_key: std::env::var(ENV_API_KEY).unwrap_or_default(),
        base_url: std::env::var(ENV_BASE_URL).unwrap_or_else(|_| DEFAULT_BASE_URL.into()),
        chat_model: std::env::var(ENV_CHAT_MODEL).unwrap_or_else(|_| DEFAULT_CHAT_MODEL.into()),
        vision_model: std::env::var(ENV_VISION_MODEL).unwrap_or_else(|_| DEFAULT_VISION_MODEL.into()),
        embed_model: DEFAULT_EMBED_MODEL.into(),
        embed_dimensions: DEFAULT_EMBED_DIMENSIONS,
    }
}

// ── Chat: shared message + tool types ───────────────────────────────

/// A single chat message. Serializes for requests and deserializes from the
/// assistant's reply, so it doubles as the conversation element fed back in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// "system" | "user" | "assistant" | "tool".
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Assistant-issued tool calls (function-calling).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Set on a `role: "tool"` message linking a result back to its call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self::text("system", content)
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self::text("user", content)
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::text("assistant", content)
    }
    /// A `role: "tool"` result message answering a prior tool call.
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }
    fn text(role: &str, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }
}

/// A tool call emitted by the assistant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String, // "function"
    pub function: FunctionCall,
}

/// The called function's name and JSON-encoded argument string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    /// Raw JSON string — parse with `serde_json::from_str` into the tool's args.
    pub arguments: String,
}

/// A tool definition advertised to the model in a request.
#[derive(Debug, Clone, Serialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub kind: String, // "function"
    pub function: FunctionDef,
}

impl Tool {
    /// Convenience constructor for a function tool.
    pub fn function(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        Self {
            kind: "function".into(),
            function: FunctionDef {
                name: name.into(),
                description: description.into(),
                parameters,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    /// JSON Schema for the function arguments.
    pub parameters: serde_json::Value,
}

// ── Chat: request / response ────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
    /// Stream-only: ask the provider to emit a final `usage` chunk so we can see
    /// `cached_tokens` (whether the stable persona prefix hit the context cache).
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<&'a [Tool]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    /// DashScope extension: enable built-in web search for this turn.
    #[serde(skip_serializing_if = "Option::is_none")]
    enable_search: Option<bool>,
    /// DashScope extension: hybrid-thinking switch (Qwen3.x). `Some(false)` skips
    /// chain-of-thought → much lower latency. Omitted (`None`) ⇒ provider default.
    #[serde(skip_serializing_if = "Option::is_none")]
    enable_thinking: Option<bool>,
    /// Cap on generated tokens (the current param; supersedes the deprecated
    /// `max_tokens`). For a thinking model this also bounds the reasoning length.
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<u32>,
}

#[derive(Debug, Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChatMessage,
    finish_reason: Option<String>,
}

/// Token accounting returned by the API (OpenAI-compatible `usage`).
#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    /// Prompt-token breakdown; `cached_tokens` is how much of the prompt was
    /// served from the provider's context cache (the big stable persona prefix).
    #[serde(default)]
    pub prompt_tokens_details: Option<PromptTokensDetails>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PromptTokensDetails {
    #[serde(default)]
    pub cached_tokens: u32,
}

/// Per-call chat tuning. `Default` leaves everything provider-default.
#[derive(Debug, Clone, Default)]
pub struct ChatOptions {
    pub temperature: Option<f32>,
    pub enable_search: bool,
    /// Hybrid-thinking models (Qwen3.x): `Some(false)` skips chain-of-thought for
    /// much lower latency; `None` leaves the provider default.
    pub enable_thinking: Option<bool>,
    /// Cap on generated tokens for this call. `None` = the model's max.
    pub max_completion_tokens: Option<u32>,
}

/// The assistant's reply plus why generation stopped (`"stop"` | `"tool_calls"`).
#[derive(Debug, Clone)]
pub struct ChatCompletion {
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
    /// Token usage, when the API reports it (non-streaming responses).
    pub usage: Option<Usage>,
}

// ── Chat: streaming (SSE) chunk shapes ──────────────────────────────
//
// In streaming mode the endpoint emits `data: {json}\n` lines (OpenAI SSE),
// each carrying an incremental `delta`, terminated by `data: [DONE]`.

#[derive(Debug, Deserialize)]
struct ChatStreamChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
    /// Present only on the final chunk when `stream_options.include_usage` is set.
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    #[serde(default)]
    delta: Delta,
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Delta {
    #[serde(default)]
    content: Option<String>,
}

// ── Embeddings: request / response ──────────────────────────────────

#[derive(Debug, Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    input: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}

// ── Client ──────────────────────────────────────────────────────────

/// API-specific client for Qwen / DashScope, layered over [`HttpClient`].
///
/// Cheap to `Clone` — the HTTP client (and its connection pool) sits behind an
/// `Arc` — so background tasks can own a copy without holding a state guard
/// across `.await`s.
#[derive(Clone)]
pub struct QwenClient {
    http: Arc<HttpClient>,
    chat_model: String,
    vision_model: String,
    embed_model: String,
    embed_dimensions: u32,
}

impl QwenClient {
    /// Build the client, baking the auth header into the HTTP defaults.
    pub fn new(config: QwenConfig) -> Result<Self, ApiError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", config.api_key))
                .map_err(|e| ApiError::Build(e.to_string()))?,
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let http = HttpClient::new(HttpClientConfig {
            base_url: config.base_url,
            timeout: HTTP_TIMEOUT,
            default_headers: headers,
        })?;

        Ok(Self {
            http: Arc::new(http),
            chat_model: config.chat_model,
            vision_model: config.vision_model,
            embed_model: config.embed_model,
            embed_dimensions: config.embed_dimensions,
        })
    }

    /// Non-streaming chat completion.
    ///
    /// Pass `tools` to enable function-calling (the reply's
    /// `message.tool_calls` will be populated when `finish_reason == "tool_calls"`).
    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[Tool]>,
        options: ChatOptions,
    ) -> Result<ChatCompletion, ApiError> {
        let request = ChatRequest {
            model: &self.chat_model,
            messages,
            stream: false,
            stream_options: None,
            tools,
            temperature: options.temperature,
            enable_search: options.enable_search.then_some(true),
            enable_thinking: options.enable_thinking,
            max_completion_tokens: options.max_completion_tokens,
        };

        let resp: ChatResponse = self.http.post_json(CHAT_PATH, &request).await?;
        let ChatResponse { choices, usage } = resp;
        let choice = choices
            .into_iter()
            .next()
            .ok_or_else(|| ApiError::Decode("chat response had no choices".into()))?;

        Ok(ChatCompletion {
            message: choice.message,
            finish_reason: choice.finish_reason,
            usage,
        })
    }

    /// Streaming chat completion.
    ///
    /// Calls `on_delta` with each incremental content token as it arrives, then
    /// returns the fully-assembled completion once the stream ends. SSE framing
    /// (`data:` lines split across network chunks) is reassembled here via a
    /// pending-line buffer.
    pub async fn chat_stream<F>(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[Tool]>,
        options: ChatOptions,
        on_delta: F,
    ) -> Result<ChatCompletion, ApiError>
    where
        F: FnMut(&str),
    {
        let request = ChatRequest {
            model: &self.chat_model,
            messages,
            stream: true,
            stream_options: Some(StreamOptions { include_usage: true }),
            tools,
            temperature: options.temperature,
            enable_search: options.enable_search.then_some(true),
            enable_thinking: options.enable_thinking,
            max_completion_tokens: options.max_completion_tokens,
        };
        let body = serde_json::to_value(&request).map_err(|e| ApiError::Build(e.to_string()))?;
        self.stream_completion(body, on_delta).await
    }

    /// Streaming **vision** chat completion (multimodal).
    ///
    /// Sends the text `messages` (system + history) followed by a final user
    /// message whose content is the OpenAI-style array
    /// `[{type:text,text:caption}, {type:image_url,...} × N]`, to the
    /// vision-capable model. Prior messages keep their plain string `content`;
    /// DashScope accepts both shapes in one `messages` list.
    pub async fn chat_vision_stream<F>(
        &self,
        messages: &[ChatMessage],
        image_data_urls: &[String],
        caption: &str,
        on_delta: F,
    ) -> Result<ChatCompletion, ApiError>
    where
        F: FnMut(&str),
    {
        // Serialize the leading text messages as-is, then append the multimodal turn.
        let mut msg_values: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| serde_json::to_value(m).map_err(|e| ApiError::Build(e.to_string())))
            .collect::<Result<_, _>>()?;

        let mut parts: Vec<serde_json::Value> = Vec::with_capacity(image_data_urls.len() + 1);
        if !caption.is_empty() {
            parts.push(serde_json::json!({ "type": "text", "text": caption }));
        }
        for url in image_data_urls {
            parts.push(serde_json::json!({
                "type": "image_url",
                "image_url": { "url": url }
            }));
        }
        msg_values.push(serde_json::json!({ "role": "user", "content": parts }));

        let body = serde_json::json!({
            "model": self.vision_model,
            "messages": msg_values,
            "stream": true,
        });
        self.stream_completion(body, on_delta).await
    }

    /// Shared SSE driver: POST a streaming chat body, feed each content delta to
    /// `on_delta`, and return the assembled completion. Reassembles `data:` lines
    /// split across network chunks via a pending-line buffer.
    async fn stream_completion<F>(
        &self,
        body: serde_json::Value,
        mut on_delta: F,
    ) -> Result<ChatCompletion, ApiError>
    where
        F: FnMut(&str),
    {
        let mut resp = self.http.post_json_streaming(CHAT_PATH, &body).await?;
        let mut pending = String::new(); // buffer for a not-yet-complete trailing line
        let mut full = String::new();
        let mut finish_reason = None;
        let mut usage: Option<Usage> = None; // carried on the final chunk (include_usage)

        while let Some(chunk) = resp.chunk().await? {
            pending.push_str(&String::from_utf8_lossy(&chunk));
            // Drain every complete line; leave any partial tail buffered.
            while let Some(nl) = pending.find('\n') {
                let line: String = pending.drain(..=nl).collect();
                let line = line.trim();
                let Some(payload) = line.strip_prefix("data:") else {
                    continue;
                };
                let payload = payload.trim();
                if payload.is_empty() || payload == "[DONE]" {
                    continue;
                }
                // A malformed chunk shouldn't abort the stream — skip it.
                if let Ok(parsed) = serde_json::from_str::<ChatStreamChunk>(payload) {
                    let ChatStreamChunk { choices, usage: chunk_usage } = parsed;
                    if chunk_usage.is_some() {
                        usage = chunk_usage; // final usage-only chunk
                    }
                    if let Some(choice) = choices.into_iter().next() {
                        if let Some(content) = choice.delta.content {
                            if !content.is_empty() {
                                full.push_str(&content);
                                on_delta(&content);
                            }
                        }
                        if choice.finish_reason.is_some() {
                            finish_reason = choice.finish_reason;
                        }
                    }
                }
            }
        }

        // Visibility into context-cache effectiveness: how much of the (large,
        // stable persona) prompt was served from cache vs. reprocessed each turn.
        if cfg!(debug_assertions) {
            if let Some(u) = &usage {
                let cached = u.prompt_tokens_details.as_ref().map_or(0, |d| d.cached_tokens);
                log::info!(
                    target: "qwen",
                    "← stream usage: prompt={} (cached {}), completion={}",
                    u.prompt_tokens, cached, u.completion_tokens
                );
            }
        }

        Ok(ChatCompletion {
            message: ChatMessage::assistant(full),
            finish_reason,
            usage,
        })
    }

    /// Embed a single string, returning its vector.
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>, ApiError> {
        let mut out = self.embed_batch(std::slice::from_ref(&text.to_string())).await?;
        out.pop()
            .ok_or_else(|| ApiError::Decode("embedding response was empty".into()))
    }

    /// Embed a batch of strings, returning vectors in input order.
    ///
    /// Transparently chunks into requests of at most [`EMBED_BATCH_LIMIT`]
    /// inputs (the `text-embedding-v3` per-request cap), so callers may pass any
    /// number of texts.
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ApiError> {
        let mut out = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(EMBED_BATCH_LIMIT) {
            let request = EmbeddingRequest {
                model: &self.embed_model,
                input: chunk,
                dimensions: Some(self.embed_dimensions),
            };
            let resp: EmbeddingResponse = self.http.post_json(EMBED_PATH, &request).await?;
            // Sort by `index` (chunk-relative) so order matches the chunk's input
            // order; appending chunk-by-chunk preserves overall input order.
            let mut data = resp.data;
            data.sort_by_key(|d| d.index);
            out.extend(data.into_iter().map(|d| d.embedding));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_request_serializes_minimally() {
        // No tools / temperature / search → those keys are omitted.
        let msgs = vec![ChatMessage::system("be terse"), ChatMessage::user("hi")];
        let req = ChatRequest {
            model: "qwen3.7-plus",
            messages: &msgs,
            stream: false,
            stream_options: None,
            tools: None,
            temperature: None,
            enable_search: None,
            enable_thinking: None,
            max_completion_tokens: None,
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["model"], "qwen3.7-plus");
        assert_eq!(v["stream"], false);
        assert_eq!(v["messages"][0]["role"], "system");
        assert_eq!(v["messages"][1]["content"], "hi");
        assert!(v.get("tools").is_none());
        assert!(v.get("enable_search").is_none());
        assert!(v.get("enable_thinking").is_none());
        assert!(v.get("max_completion_tokens").is_none());
        assert!(v.get("stream_options").is_none());
    }

    #[test]
    fn chat_request_includes_tools_and_search() {
        let msgs = vec![ChatMessage::user("weather?")];
        let tools = vec![Tool::function(
            "extract_memory",
            "save a fact",
            serde_json::json!({"type": "object"}),
        )];
        let req = ChatRequest {
            model: "qwen3.7-plus",
            messages: &msgs,
            stream: false,
            stream_options: None,
            tools: Some(&tools),
            temperature: Some(0.7),
            enable_search: Some(true),
            enable_thinking: Some(false),
            max_completion_tokens: Some(512),
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["enable_thinking"], false);
        assert_eq!(v["max_completion_tokens"], 512);
        assert_eq!(v["tools"][0]["type"], "function");
        assert_eq!(v["tools"][0]["function"]["name"], "extract_memory");
        // f32 → JSON loses exact 0.7; compare with tolerance.
        assert!((v["temperature"].as_f64().unwrap() - 0.7).abs() < 1e-6);
        assert_eq!(v["enable_search"], true);
    }

    #[test]
    fn parses_tool_call_response() {
        let raw = r#"{
            "choices": [{
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "extract_memory", "arguments": "{\"x\":1}"}
                    }]
                }
            }]
        }"#;
        let resp: ChatResponse = serde_json::from_str(raw).unwrap();
        let choice = &resp.choices[0];
        assert_eq!(choice.finish_reason.as_deref(), Some("tool_calls"));
        let calls = choice.message.tool_calls.as_ref().unwrap();
        assert_eq!(calls[0].function.name, "extract_memory");
        assert_eq!(calls[0].function.arguments, "{\"x\":1}");
    }

    #[test]
    fn parses_embedding_response_in_order() {
        let raw = r#"{"data": [
            {"index": 1, "embedding": [0.1, 0.2]},
            {"index": 0, "embedding": [0.9, 0.8]}
        ]}"#;
        let resp: EmbeddingResponse = serde_json::from_str(raw).unwrap();
        let mut data = resp.data;
        data.sort_by_key(|d| d.index);
        assert_eq!(data[0].embedding, vec![0.9, 0.8]);
        assert_eq!(data[1].embedding, vec![0.1, 0.2]);
    }

    // ── Live smoke test (Phase 2 verification) ──────────────────────
    //
    // Hits the real DashScope API, so it is `#[ignore]`d and must be run
    // explicitly. It exercises the actual persona prompt + embeddings + chat —
    // i.e. the real Pipeline A path minus Tauri/db wiring (empty memory set).
    //
    //   cargo test --lib live_embed_and_chat -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "hits the live DashScope API; needs DASHSCOPE_API_KEY in .env"]
    async fn live_embed_and_chat() {
        dotenvy::dotenv().ok();
        let cfg = config_from_env();
        assert!(!cfg.api_key.is_empty(), "DASHSCOPE_API_KEY not set in .env");
        let client = QwenClient::new(cfg).unwrap();

        // 1. Embedding — confirm dimension matches what the memory store expects.
        let emb = client.embed("你好").await.expect("embed failed");
        println!("embed dim = {}", emb.len());
        assert_eq!(emb.len(), DEFAULT_EMBED_DIMENSIONS as usize);

        // 2. Chat with the real persona prompt (empty-memory dynamic block).
        let base = crate::persona::base_system_prompt();
        let messages = vec![
            ChatMessage::system(base),
            ChatMessage::system(
                "# 当前情境\n- 回复语言：请用中文回复。\n- 暂无相关记忆。".to_string(),
            ),
            ChatMessage::user("你今天在园艺部种了什么？".to_string()),
        ];
        let completion = client
            .chat(&messages, None, ChatOptions::default())
            .await
            .expect("chat failed");
        println!("Mutsumi> {:?}", completion.message.content);
        println!("finish_reason = {:?}", completion.finish_reason);
        assert!(completion.message.content.is_some());
    }

    //   cargo test --lib live_chat_stream -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "hits the live DashScope API; needs DASHSCOPE_API_KEY in .env"]
    async fn live_chat_stream() {
        use std::io::Write;
        dotenvy::dotenv().ok();
        let cfg = config_from_env();
        assert!(!cfg.api_key.is_empty(), "DASHSCOPE_API_KEY not set in .env");
        let client = QwenClient::new(cfg).unwrap();

        let base = crate::persona::base_system_prompt();
        let messages = vec![
            ChatMessage::system(base),
            ChatMessage::system(
                "# 当前情境\n- 回复语言：请用中文回复。\n- 暂无相关记忆。".to_string(),
            ),
            ChatMessage::user("最近怎么样？".to_string()),
        ];

        let mut deltas = 0usize;
        print!("Mutsumi(stream)> ");
        let _ = std::io::stdout().flush();
        let completion = client
            .chat_stream(&messages, None, ChatOptions::default(), |delta| {
                deltas += 1;
                print!("{delta}");
                let _ = std::io::stdout().flush();
            })
            .await
            .expect("chat_stream failed");
        println!("\n[{} deltas] finish_reason = {:?}", deltas, completion.finish_reason);
        assert!(deltas > 0, "no streaming deltas received");
        assert!(completion.message.content.is_some());
    }

    //   cargo test --lib live_deep_talk -- --ignored --nocapture
    // Eyeball check for the "drift / can't go deep / over-eager 不知道" fix.
    #[tokio::test]
    #[ignore = "hits the live DashScope API; needs DASHSCOPE_API_KEY in .env"]
    async fn live_deep_talk() {
        dotenvy::dotenv().ok();
        let cfg = config_from_env();
        if cfg.api_key.is_empty() {
            eprintln!("SKIP: DASHSCOPE_API_KEY not set");
            return;
        }
        let client = QwenClient::new(cfg).unwrap();
        let base = crate::persona::base_system_prompt();
        let ctx = ChatMessage::system(
            "# 当前情境\n- 回复语言：请用中文回复。\n- 暂无相关记忆。".to_string(),
        );

        // A — emotional sharing after the bass chat (the drift bug).
        let msgs_a = vec![
            ChatMessage::system(base.clone()),
            ctx.clone(),
            ChatMessage::user("你猜猜我用什么乐器"),
            ChatMessage::assistant("……贝斯。"),
            ChatMessage::user("具体什么型号"),
            ChatMessage::assistant("……塔吉玛。"),
            ChatMessage::user("我贝斯放在国内，准备下个月回国了"),
            ChatMessage::assistant("……回国，演出？"),
            ChatMessage::user("回国找工作吧，日本找不到我喜欢的工作...哭"),
        ];
        let a = client.chat(&msgs_a, None, ChatOptions::default()).await.unwrap();
        println!("\n[A 情绪倾诉] Mutsumi> {:?}", a.message.content);

        // B — in-domain "specific model" should engage, not refuse.
        let msgs_b = vec![
            ChatMessage::system(base.clone()),
            ctx.clone(),
            ChatMessage::user("你猜猜我用什么乐器"),
            ChatMessage::assistant("……贝斯。"),
            ChatMessage::user("具体什么型号"),
        ];
        let b = client.chat(&msgs_b, None, ChatOptions::default()).await.unwrap();
        println!("[B 具体型号·应参与] Mutsumi> {:?}", b.message.content);

        // C — a genuinely out-of-world technical question SHOULD still deflect.
        let msgs_c = vec![
            ChatMessage::system(base),
            ctx,
            ChatMessage::user("帮我用 Python 写一个快速排序"),
        ];
        let c = client.chat(&msgs_c, None, ChatOptions::default()).await.unwrap();
        println!("[C 越界技术·应不知道] Mutsumi> {:?}", c.message.content);

        assert!(a.message.content.is_some());
    }
}
