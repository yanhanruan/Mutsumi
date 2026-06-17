//! Pipeline C — cognitive reflection.
//!
//! Periodically the backend takes a batch of raw, un-reflected observations and
//! asks the model to synthesize 1–2 higher-level *insights* about the user (from
//! Mutsumi's perspective). The insights are stored as high-importance
//! `Reflection` memories, and the source observations are marked `reflected` so
//! they are not reprocessed. Over time the AI's understanding compounds.
//!
//! Triggers (see callers):
//!   * after silent extraction, once [`REFLECTION_BATCH`] raw observations pile up;
//!   * on app startup, if at least [`REFLECTION_STARTUP_MIN`] are pending.

use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::db::memory::{self, MemoryKind, NewMemory};
use crate::db::{now, Db};
use crate::http::ApiError;
use crate::services::qwen::{ChatMessage, ChatOptions, QwenClient, Tool, ToolCall};
use crate::services::QwenState;

/// The registered tool name the model calls to record an insight.
const TOOL_NAME: &str = "record_insight";

/// Runtime trigger: reflect once this many raw observations have accumulated.
pub const REFLECTION_BATCH: usize = 20;
/// Startup trigger: reflect if at least this many observations are pending.
pub const REFLECTION_STARTUP_MIN: usize = 5;
/// Cap on observations fed into a single reflection pass.
const MAX_BATCH: usize = 50;

const REFLECTION_SYSTEM: &str = r#"你是角色「睦」的内在反思。下面会给你最近积累的、关于【用户】的一些零散观察记录。请你综合这些观察，提炼出 1 到 2 条更高层次、更概括的洞察——关于用户的性格、行为模式、深层需求或当前状态，从睦的视角去理解这个人。

规则：
- 不要简单复述或罗列原始观察，要给出综合、归纳之后的理解。
- 每条洞察调用一次 record_insight；最多 2 条，宁缺毋滥。
- content 用简洁的第三人称陈述。
- 不要输出任何对话文字，只通过工具调用记录。"#;

/// A synthesized insight reported by one `record_insight` tool call.
#[derive(Debug, Deserialize, PartialEq)]
pub struct Insight {
    pub content: String,
    #[serde(default = "default_importance")]
    pub importance: f32,
}

fn default_importance() -> f32 {
    0.85
}

/// The `record_insight` tool definition advertised to the model.
pub fn record_insight_tool() -> Tool {
    Tool::function(
        TOOL_NAME,
        "记录一条综合提炼出的、关于用户的高层次洞察。",
        serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "综合归纳后的洞察，用简洁的第三人称陈述。"
                },
                "importance": {
                    "type": "number",
                    "minimum": 0,
                    "maximum": 1,
                    "description": "洞察的重要程度，0 到 1 之间的小数。"
                }
            },
            "required": ["content", "importance"]
        }),
    )
}

/// Parse `record_insight` tool calls into insights, skipping malformed ones.
pub fn parse_insights(tool_calls: &[ToolCall]) -> Vec<Insight> {
    tool_calls
        .iter()
        .filter(|c| c.function.name == TOOL_NAME)
        .filter_map(|c| serde_json::from_str::<Insight>(&c.function.arguments).ok())
        .collect()
}

/// Run the reflection LLM call over a batch of observations.
///
/// Free of database/Tauri concerns so it can be tested live in isolation.
pub async fn reflect_once(
    qwen: &QwenClient,
    observations: &[String],
) -> Result<Vec<Insight>, ApiError> {
    let listed = observations
        .iter()
        .map(|o| format!("- {o}"))
        .collect::<Vec<_>>()
        .join("\n");
    let messages = vec![
        ChatMessage::system(REFLECTION_SYSTEM),
        ChatMessage::user(format!("观察记录：\n{listed}")),
    ];
    let tools = [record_insight_tool()];

    let completion = qwen
        .chat(&messages, Some(&tools), ChatOptions::default())
        .await?;

    Ok(completion
        .message
        .tool_calls
        .map(|calls| parse_insights(&calls))
        .unwrap_or_default())
}

/// Best-effort background reflection: reflect if at least `min_count`
/// un-reflected observations are pending. Logs and swallows failures.
pub async fn maybe_reflect(app: &AppHandle, min_count: usize) {
    if let Err(e) = run(app, min_count).await {
        log::warn!("reflection failed: {e}");
    }
}

async fn run(app: &AppHandle, min_count: usize) -> Result<(), ApiError> {
    // Clone the client out of state so no guard is held across awaits.
    let qwen: QwenClient = match app.try_state::<QwenState>() {
        Some(s) => s.0.clone(),
        None => return Ok(()),
    };

    // Read the pending batch (short sync block).
    let batch = {
        let Some(db) = app.try_state::<Db>() else {
            return Ok(());
        };
        let conn = db.0.lock().map_err(|e| ApiError::Build(e.to_string()))?;
        let count = memory::unreflected_count(&conn).map_err(map_db)? as usize;
        if count < min_count {
            return Ok(());
        }
        memory::unreflected_observations(&conn, MAX_BATCH).map_err(map_db)?
    };
    if batch.is_empty() {
        return Ok(());
    }
    let ids: Vec<i64> = batch.iter().map(|m| m.id).collect();

    let observations: Vec<String> = batch.iter().map(|m| m.content.clone()).collect();
    let insights = reflect_once(&qwen, &observations).await?;

    // Always mark the batch reflected (even if no insight) so it is not
    // reprocessed on every future trigger.
    if insights.is_empty() {
        let Some(db) = app.try_state::<Db>() else {
            return Ok(());
        };
        let conn = db.0.lock().map_err(|e| ApiError::Build(e.to_string()))?;
        memory::mark_reflected(&conn, &ids).map_err(map_db)?;
        return Ok(());
    }

    let contents: Vec<String> = insights.iter().map(|i| i.content.clone()).collect();
    let embeddings = qwen.embed_batch(&contents).await?;

    let Some(db) = app.try_state::<Db>() else {
        return Ok(());
    };
    let conn = db.0.lock().map_err(|e| ApiError::Build(e.to_string()))?;
    let ts = now();
    for (insight, embedding) in insights.iter().zip(embeddings) {
        let _ = memory::insert(
            &conn,
            &NewMemory {
                kind: MemoryKind::Reflection,
                category: Some("insight".into()),
                content: insight.content.clone(),
                importance: insight.importance.clamp(0.0, 1.0),
                embedding: Some(embedding),
            },
            ts,
        );
    }
    memory::mark_reflected(&conn, &ids).map_err(map_db)?;
    log::info!(
        "reflected {} observation(s) into {} insight(s)",
        ids.len(),
        insights.len()
    );
    Ok(())
}

fn map_db(e: impl std::fmt::Display) -> ApiError {
    ApiError::Build(e.to_string())
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
    fn parses_insights() {
        let calls = vec![
            call(
                "record_insight",
                r#"{"content":"用户在用工作填补孤独","importance":0.9}"#,
            ),
            call("other", r#"{"x":1}"#),
            call("record_insight", "bad json"),
        ];
        let insights = parse_insights(&calls);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].content, "用户在用工作填补孤独");
        assert_eq!(insights[0].importance, 0.9);
    }

    #[test]
    fn importance_defaults() {
        let calls = vec![call("record_insight", r#"{"content":"用户重视陪伴"}"#)];
        let insights = parse_insights(&calls);
        assert_eq!(insights[0].importance, 0.85);
    }

    #[test]
    fn tool_schema_is_well_formed() {
        let tool = record_insight_tool();
        assert_eq!(tool.function.name, "record_insight");
        assert_eq!(tool.function.parameters["type"], "object");
    }

    //   cargo test --lib live_reflect_once -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "hits the live DashScope API; needs DASHSCOPE_API_KEY in .env"]
    async fn live_reflect_once() {
        dotenvy::dotenv().ok();
        let cfg = crate::services::qwen::config_from_env();
        assert!(!cfg.api_key.is_empty(), "DASHSCOPE_API_KEY not set in .env");
        let qwen = QwenClient::new(cfg).unwrap();

        let observations = vec![
            "用户是一名后端程序员".to_string(),
            "用户经常加班到很晚".to_string(),
            "用户最喜欢喝抹茶拿铁".to_string(),
            "用户养了一只叫团子的猫".to_string(),
            "用户说周末也常常一个人待着".to_string(),
        ];
        let insights = reflect_once(&qwen, &observations).await.expect("reflect failed");
        for i in &insights {
            println!("[insight] {} (importance {})", i.content, i.importance);
        }
        assert!(!insights.is_empty(), "expected at least one insight");
    }
}
