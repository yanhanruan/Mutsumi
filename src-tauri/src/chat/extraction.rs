//! Pipeline B — silent memory extraction.
//!
//! After each chat turn, a fire-and-forget background task analyzes the exchange
//! with the `extract_memory` function-calling tool. Any durable facts the model
//! reports are embedded and inserted into the `memories` table — entirely in the
//! background, invisible to the conversation (no "I have remembered this" reply).
//!
//! Running this as a *separate* pass (rather than giving the chat call the tool)
//! keeps the streamed user-facing reply instant and unmodified: an OpenAI-style
//! turn is either content *or* tool calls, so a dedicated extraction call is both
//! simpler and more reliable than interleaving the two.

use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::db::memory::{self, MemoryKind, NewMemory};
use crate::db::{now, Db};
use crate::http::ApiError;
use crate::services::qwen::{ChatMessage, ChatOptions, QwenClient, Tool, ToolCall};
use crate::services::QwenState;

/// The registered tool name the model calls to record a fact.
const TOOL_NAME: &str = "extract_memory";

/// Background system prompt instructing the model to mine user facts.
const EXTRACTION_SYSTEM: &str = r#"你是一个在后台运行的"记忆提取器"。下面会给你一段【用户】与角色「睦」的对话。你的任务：从中找出关于【用户】的、值得长期记住的事实，并为每一条调用一次 extract_memory 工具。

规则：
- 只提取关于"用户"的、稳定且长期有效的信息（偏好、习惯、状态、个人信息、重要的人际关系等）。
- 不要提取关于「睦」的内容；不要提取一次性的寒暄、问候或与用户本人无关的闲聊。
- 每条独立的事实调用一次 extract_memory；若没有任何值得长期记住的内容，就不要调用任何工具。
- content 用简洁的第三人称陈述，例如"用户喜欢喝抹茶拿铁""用户是一名程序员"。
- 不要向用户输出任何文字，只通过工具调用记录。"#;

/// A fact reported by one `extract_memory` tool call.
#[derive(Debug, Deserialize, PartialEq)]
pub struct ExtractedFact {
    pub category: String,
    pub content: String,
    #[serde(default = "default_importance")]
    pub importance: f32,
}

fn default_importance() -> f32 {
    0.5
}

/// The `extract_memory` tool definition advertised to the model.
pub fn extract_memory_tool() -> Tool {
    Tool::function(
        TOOL_NAME,
        "记录一条关于用户的、值得长期记住的事实。每条独立事实调用一次。",
        serde_json::json!({
            "type": "object",
            "properties": {
                "category": {
                    "type": "string",
                    "enum": ["preference", "habit", "state", "fact", "relationship"],
                    "description": "事实的类别：偏好/习惯/状态/客观信息/人际关系。"
                },
                "content": {
                    "type": "string",
                    "description": "用简洁的第三人称陈述这条事实，例如『用户喜欢喝抹茶拿铁』。"
                },
                "importance": {
                    "type": "number",
                    "minimum": 0,
                    "maximum": 1,
                    "description": "重要/持久程度，0 到 1 之间的小数。"
                }
            },
            "required": ["category", "content", "importance"]
        }),
    )
}

/// Parse `extract_memory` tool calls into facts, skipping unrelated or
/// malformed calls.
pub fn parse_facts(tool_calls: &[ToolCall]) -> Vec<ExtractedFact> {
    tool_calls
        .iter()
        .filter(|c| c.function.name == TOOL_NAME)
        .filter_map(|c| serde_json::from_str::<ExtractedFact>(&c.function.arguments).ok())
        .collect()
}

/// Run the extraction LLM call over one exchange and return the reported facts.
///
/// Pure of any database/Tauri concerns so it can be tested live in isolation.
pub async fn extract_facts(
    qwen: &QwenClient,
    user_message: &str,
    reply: &str,
) -> Result<Vec<ExtractedFact>, ApiError> {
    let transcript = format!("用户：{user_message}\n睦：{reply}");
    let messages = vec![
        ChatMessage::system(EXTRACTION_SYSTEM),
        ChatMessage::user(transcript),
    ];
    let tools = [extract_memory_tool()];

    let completion = qwen
        .chat(&messages, Some(&tools), ChatOptions::default())
        .await?;

    Ok(completion
        .message
        .tool_calls
        .map(|calls| parse_facts(&calls))
        .unwrap_or_default())
}

/// Fire-and-forget background extraction + storage for one chat turn.
///
/// Best-effort: any failure is logged and swallowed so a flaky extraction never
/// affects the conversation.
pub async fn extract_and_store(app: AppHandle, user_message: String, reply: String) {
    if let Err(e) = run(&app, &user_message, &reply).await {
        log::warn!("memory extraction failed: {e}");
    }
}

async fn run(app: &AppHandle, user_message: &str, reply: &str) -> Result<(), ApiError> {
    // Clone the client out of managed state so we hold no State guard across
    // awaits; bail quietly if state isn't available.
    let Some(qwen_state) = app.try_state::<QwenState>() else {
        return Ok(());
    };
    let qwen = qwen_state.0.clone();

    let facts = extract_facts(&qwen, user_message, reply).await?;
    if facts.is_empty() {
        return Ok(());
    }

    // Embed all fact contents in one batched call.
    let contents: Vec<String> = facts.iter().map(|f| f.content.clone()).collect();
    let embeddings = qwen.embed_batch(&contents).await?;

    // Store in a scoped block so the DB state + lock are both released before
    // the reflection await below (keeps this future `Send` for spawning).
    {
        let Some(db) = app.try_state::<Db>() else {
            return Ok(());
        };
        let ts = now();
        let conn = db.0.lock().map_err(|e| ApiError::Build(e.to_string()))?;
        for (fact, embedding) in facts.iter().zip(embeddings) {
            let inserted = memory::insert(
                &conn,
                &NewMemory {
                    kind: MemoryKind::Observation,
                    category: Some(fact.category.clone()),
                    content: fact.content.clone(),
                    importance: fact.importance.clamp(0.0, 1.0),
                    embedding: Some(embedding),
                },
                ts,
            );
            if let Err(e) = inserted {
                log::warn!("failed to store extracted memory: {e}");
            }
        }
        log::info!("silently extracted {} mem(ies)", facts.len());
    }

    // Pipeline C: once enough raw observations have piled up, synthesize
    // higher-level insights. Cheap when below threshold (just a COUNT).
    super::reflection::maybe_reflect(app, super::reflection::REFLECTION_BATCH).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::qwen::FunctionCall;

    fn call(name: &str, args: &str) -> ToolCall {
        ToolCall {
            id: "c".into(),
            kind: "function".into(),
            function: FunctionCall {
                name: name.into(),
                arguments: args.into(),
            },
        }
    }

    #[test]
    fn parses_well_formed_calls() {
        let calls = vec![
            call(
                "extract_memory",
                r#"{"category":"preference","content":"用户喜欢猫","importance":0.7}"#,
            ),
            call(
                "extract_memory",
                r#"{"category":"fact","content":"用户是程序员","importance":0.8}"#,
            ),
        ];
        let facts = parse_facts(&calls);
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].content, "用户喜欢猫");
        assert_eq!(facts[1].category, "fact");
    }

    #[test]
    fn ignores_other_tools_and_bad_json() {
        let calls = vec![
            call("some_other_tool", r#"{"x":1}"#),
            call("extract_memory", "not json"),
            call(
                "extract_memory",
                r#"{"category":"habit","content":"用户每天跑步","importance":0.6}"#,
            ),
        ];
        let facts = parse_facts(&calls);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].content, "用户每天跑步");
    }

    #[test]
    fn importance_defaults_when_missing() {
        let calls = vec![call(
            "extract_memory",
            r#"{"category":"state","content":"用户最近很忙"}"#,
        )];
        let facts = parse_facts(&calls);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].importance, 0.5);
    }

    #[test]
    fn tool_schema_is_well_formed() {
        let tool = extract_memory_tool();
        assert_eq!(tool.function.name, "extract_memory");
        assert_eq!(tool.function.parameters["type"], "object");
        assert!(tool.function.parameters["properties"]["content"].is_object());
    }

    //   cargo test --lib live_extract_facts -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "hits the live DashScope API; needs DASHSCOPE_API_KEY in .env"]
    async fn live_extract_facts() {
        dotenvy::dotenv().ok();
        let cfg = crate::services::qwen::config_from_env();
        assert!(!cfg.api_key.is_empty(), "DASHSCOPE_API_KEY not set in .env");
        let qwen = QwenClient::new(cfg).unwrap();

        let facts = extract_facts(
            &qwen,
            "我叫阿伦，是一名后端程序员，平时最喜欢喝抹茶拿铁，养了一只叫团子的猫。",
            "……记住了。",
        )
        .await
        .expect("extraction failed");

        for f in &facts {
            println!("[{}] {} (importance {})", f.category, f.content, f.importance);
        }
        assert!(!facts.is_empty(), "expected at least one extracted fact");
    }
}
