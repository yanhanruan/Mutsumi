//! Pipeline B — silent memory extraction.
//!
//! A fire-and-forget background task analyzes a **batch** of recent exchanges with
//! the `extract_memory` function-calling tool. Durable memories the model reports
//! are embedded and stored — entirely in the background, invisible to the
//! conversation. Batching (every N turns + on chat close, see `chat::mod`) keeps
//! the per-turn token cost down: one call amortizes the big system prompt over
//! several turns.
//!
//! Two memory *subjects* are extracted in the same pass:
//!   * `user` — durable facts the user stated about themselves;
//!   * `self` — Mutsumi's own memory: feelings toward / promises to the user.
//!
//! Both are **grounded** ([`quote_grounded`]): a fact is dropped unless its
//! `source_quote` actually appears in the right speaker's words (user facts in the
//! user's turns, self-memories in Mutsumi's replies). That guard, enforced in code,
//! is what prevents the character's canon or hallucinations from being stored.

use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::db::memory::{self, MemoryKind, MemorySubject, NewMemory};
use crate::db::{now, Db};
use crate::http::ApiError;
use crate::services::qwen::{ChatMessage, ChatOptions, QwenClient, Tool, ToolCall};
use crate::services::QwenState;

/// One user↔Mutsumi exchange (a single turn). Batched for extraction.
#[derive(Debug, Clone)]
pub struct Exchange {
    pub user: String,
    pub reply: String,
}

/// The registered tool name the model calls to record a fact.
const TOOL_NAME: &str = "extract_memory";

/// Background system prompt. Grounding is enforced by code (see
/// [`quote_grounded`]), but the prompt must still steer the model to emit a
/// verbatim, speaker-correct `source_quote`, or the fact is rejected downstream.
pub(crate) const EXTRACTION_SYSTEM: &str = r#"你是一个在后台运行的"记忆提取器"。下面给你的是【用户】与角色「睦」最近的多轮对话（每轮含【用户说】和【睦回复】）。请提炼值得长期记住的记忆，并为每一条调用一次 extract_memory 工具。

记忆分两类，用 subject 字段区分：
- subject="user"：关于【用户】本人、稳定且长期有效的事实（偏好、习惯、状态、个人信息、重要人际关系）。
- subject="self"：关于「睦」自己的——她对用户流露的情感/态度，以及她对用户许下的约定、和用户之间值得记住的相处片段。

最高原则——每条记忆都要有原文依据，否则会被丢弃：
- 每条都必须给出 source_quote：一字不差、真实出现过的原话片段。
  · subject="user" 的 source_quote 必须来自【用户说】；
  · subject="self" 的 source_quote 必须来自【睦回复】（睦确实说过/承诺过的话）。
- 绝不跨界：不能拿睦的话支撑"用户事实"，也不能拿用户的话支撑"睦自己的记忆"。睦提到的伞、吉他、贝斯、苦瓜/小黄瓜、她说的天气/地点/心情，都是睦的，不是用户的。
- 提问 / 查询不是事实：用户问"武汉天气"不代表他在或住在武汉。只有明确陈述才算。
- 绝不推断、联想、脑补、扩写；绝不记录与原话相矛盾的内容。
- 没有"可长期记住且有原文依据"的内容，就不要调用任何工具。

其他规则：
- 不要提取一次性的寒暄、问候，或无关闲聊。
- 每条独立的记忆调用一次 extract_memory。
- content 用简洁的第三人称陈述：user 类以"用户…"开头（如"用户喜欢喝抹茶拿铁"）；self 类以"睦…"开头（如"睦答应陪用户调音""睦觉得和用户聊天时很安心"）。
- 不要输出任何对话文字，只通过工具调用记录。"#;

/// A fact reported by one `extract_memory` tool call.
#[derive(Debug, Deserialize, PartialEq)]
pub struct ExtractedFact {
    pub category: String,
    pub content: String,
    #[serde(default = "default_importance")]
    pub importance: f32,
    /// The verbatim span — from the *user's* turns for a `user` fact, or from
    /// *Mutsumi's* replies for a `self` memory — that supports this fact. Used by
    /// [`quote_grounded`] to reject anything not actually said by the right
    /// speaker (hallucinations, query-derived inferences, canon leaks).
    #[serde(default)]
    pub source_quote: String,
    /// `"user"` (about the user) or `"self"` (Mutsumi's own memory). Defaults to
    /// user when the model omits it.
    #[serde(default)]
    pub subject: String,
}

impl ExtractedFact {
    pub fn subject(&self) -> MemorySubject {
        MemorySubject::from_str(&self.subject)
    }
}

fn default_importance() -> f32 {
    0.5
}

/// The `extract_memory` tool definition advertised to the model.
pub fn extract_memory_tool() -> Tool {
    Tool::function(
        TOOL_NAME,
        "记录一条值得长期记住的记忆（关于用户，或关于睦自己）。每条独立记忆调用一次。",
        serde_json::json!({
            "type": "object",
            "properties": {
                "subject": {
                    "type": "string",
                    "enum": ["user", "self"],
                    "description": "记忆的主体：'user'=关于用户本人；'self'=关于睦自己（她对用户的情感/态度、对用户的约定、相处片段）。"
                },
                "category": {
                    "type": "string",
                    "enum": ["preference", "habit", "state", "fact", "relationship", "feeling", "promise"],
                    "description": "类别：用户多用 preference/habit/state/fact/relationship；睦自己多用 feeling（情感）/promise（约定）/relationship。"
                },
                "content": {
                    "type": "string",
                    "description": "用简洁的第三人称陈述。user 类以『用户…』开头；self 类以『睦…』开头。"
                },
                "importance": {
                    "type": "number",
                    "minimum": 0,
                    "maximum": 1,
                    "description": "重要/持久程度，0 到 1 之间的小数。"
                },
                "source_quote": {
                    "type": "string",
                    "description": "一字不差、真实出现过的原文片段。user 记忆取自【用户说】，self 记忆取自【睦回复】。不能改写、不能跨界、不能编造。"
                }
            },
            "required": ["subject", "category", "content", "importance", "source_quote"]
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

/// Whitespace-insensitive substring used for grounding (trim + drop all internal
/// whitespace), so a quote that differs only in spacing still matches.
fn normalize_for_match(s: &str) -> String {
    s.split_whitespace().collect()
}

/// True if `quote` is a verbatim span of `user_message` (whitespace-insensitive).
///
/// This is the structural guard that makes hallucinated / reply-sourced /
/// query-inferred facts impossible to store: the supporting quote *must* appear
/// in the user's own words. An empty quote is never grounded.
pub fn quote_grounded(user_message: &str, quote: &str) -> bool {
    let q = normalize_for_match(quote);
    if q.is_empty() {
        return false;
    }
    normalize_for_match(user_message).contains(&q)
}

/// Run extraction over a **batch** of exchanges and return the **grounded**
/// memories: each fact's `source_quote` must appear in the right speaker's words
/// (user facts in the user's turns, self-memories in Mutsumi's replies). One LLM
/// call for the whole batch keeps per-turn token cost down.
///
/// Pure of any database/Tauri concerns so it can be tested live in isolation.
pub async fn extract_from_batch(
    qwen: &QwenClient,
    exchanges: &[Exchange],
) -> Result<Vec<ExtractedFact>, ApiError> {
    if exchanges.is_empty() {
        return Ok(Vec::new());
    }

    let mut transcript = String::new();
    for (i, ex) in exchanges.iter().enumerate() {
        transcript.push_str(&format!(
            "第{}轮\n【用户说】{}\n【睦回复】{}\n\n",
            i + 1,
            ex.user,
            ex.reply
        ));
    }
    let messages = vec![
        ChatMessage::system(EXTRACTION_SYSTEM),
        ChatMessage::user(transcript),
    ];
    let tools = [extract_memory_tool()];

    let completion = qwen
        .chat(&messages, Some(&tools), ChatOptions::default())
        .await?;
    let facts = completion
        .message
        .tool_calls
        .map(|calls| parse_facts(&calls))
        .unwrap_or_default();

    // Speaker corpora for subject-aware grounding.
    let user_corpus = exchanges.iter().map(|e| e.user.as_str()).collect::<Vec<_>>().join("\n");
    let mutsumi_corpus = exchanges.iter().map(|e| e.reply.as_str()).collect::<Vec<_>>().join("\n");

    let (grounded, rejected): (Vec<_>, Vec<_>) = facts.into_iter().partition(|f| {
        let corpus = match f.subject() {
            MemorySubject::User => &user_corpus,
            MemorySubject::Mutsumi => &mutsumi_corpus,
        };
        quote_grounded(corpus, &f.source_quote)
    });
    for f in &rejected {
        log::info!(
            "extraction: dropped ungrounded {} fact {:?} (quote {:?} not in {} corpus)",
            f.subject,
            f.content,
            f.source_quote,
            f.subject
        );
    }
    Ok(grounded)
}

/// Convenience: extract from a single exchange (used by isolated/live tests).
#[cfg(test)]
pub async fn extract_facts(
    qwen: &QwenClient,
    user_message: &str,
    reply: &str,
) -> Result<Vec<ExtractedFact>, ApiError> {
    extract_from_batch(
        qwen,
        &[Exchange {
            user: user_message.to_string(),
            reply: reply.to_string(),
        }],
    )
    .await
}

/// Fire-and-forget background extraction + storage for a batch of exchanges.
///
/// Best-effort: any failure is logged and swallowed so a flaky extraction never
/// affects the conversation.
pub async fn extract_and_store_batch(app: AppHandle, exchanges: Vec<Exchange>) {
    if let Err(e) = run(&app, &exchanges).await {
        log::warn!("memory extraction failed: {e}");
    }
}

async fn run(app: &AppHandle, exchanges: &[Exchange]) -> Result<(), ApiError> {
    // Clone the client out of managed state so we hold no State guard across
    // awaits; bail quietly if state isn't available.
    let Some(qwen_state) = app.try_state::<QwenState>() else {
        return Ok(());
    };
    let qwen = qwen_state.0.clone();

    let facts = extract_from_batch(&qwen, exchanges).await?;
    if facts.is_empty() {
        return Ok(());
    }

    // Embed all memory contents in one batched call.
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
            // Dedup-merge: a restatement refreshes its near-duplicate (within the
            // same subject + category) instead of piling up an overlapping row.
            let stored = memory::store_observation(
                &conn,
                &NewMemory {
                    kind: MemoryKind::Observation,
                    category: Some(fact.category.clone()),
                    content: fact.content.clone(),
                    importance: fact.importance.clamp(0.0, 1.0),
                    embedding: Some(embedding),
                    subject: fact.subject(),
                },
                ts,
            );
            if let Err(e) = stored {
                log::warn!("failed to store extracted memory: {e}");
            }
        }
        log::info!("silently extracted {} mem(ies) from {} turn(s)", facts.len(), exchanges.len());
    }

    // Pipeline C: once enough raw *user* observations have piled up, synthesize
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
    fn subject_defaults_to_user_and_parses_self() {
        let calls = vec![
            call("extract_memory", r#"{"category":"state","content":"用户最近很忙"}"#),
            call(
                "extract_memory",
                r#"{"subject":"self","category":"promise","content":"睦答应陪用户调音","importance":0.6,"source_quote":"我陪你调音"}"#,
            ),
        ];
        let facts = parse_facts(&calls);
        assert_eq!(facts[0].subject(), MemorySubject::User); // omitted → user
        assert_eq!(facts[1].subject(), MemorySubject::Mutsumi);
        assert_eq!(facts[1].source_quote, "我陪你调音");
    }

    #[test]
    fn tool_schema_is_well_formed() {
        let tool = extract_memory_tool();
        assert_eq!(tool.function.name, "extract_memory");
        assert_eq!(tool.function.parameters["type"], "object");
        assert!(tool.function.parameters["properties"]["content"].is_object());
        // source_quote and subject are part of the schema and required.
        assert!(tool.function.parameters["properties"]["source_quote"].is_object());
        assert!(tool.function.parameters["properties"]["subject"].is_object());
        let required = tool.function.parameters["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "source_quote"));
        assert!(required.iter().any(|v| v == "subject"));
    }

    #[test]
    fn parse_facts_reads_source_quote() {
        let calls = vec![call(
            "extract_memory",
            r#"{"category":"fact","content":"用户是程序员","importance":0.8,"source_quote":"我是程序员"}"#,
        )];
        let facts = parse_facts(&calls);
        assert_eq!(facts[0].source_quote, "我是程序员");
    }

    #[test]
    fn quote_grounded_accepts_real_user_span() {
        let user = "我叫阿伦，是一名后端程序员，养了一只猫。";
        assert!(quote_grounded(user, "是一名后端程序员")); // contiguous substring
        assert!(quote_grounded(user, "养了 一只 猫")); // whitespace-insensitive
        // A paraphrase that is NOT a contiguous span is (intentionally) rejected.
        assert!(!quote_grounded(user, "我是一名后端程序员"));
    }

    #[test]
    fn quote_grounded_rejects_ungrounded() {
        let user = "武汉今天天气怎么样？"; // a query about a city
        // The model's reply mentioned Shanghai / an umbrella — neither is the user's.
        assert!(!quote_grounded(user, "上海今天有风"));
        assert!(!quote_grounded(user, "伞还在我手里"));
        // Inferring residence from a query has no supporting user span.
        assert!(!quote_grounded(user, "我住在武汉"));
        // An empty quote is never grounded.
        assert!(!quote_grounded(user, ""));
    }

    //   cargo test --lib live_extract_facts -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "hits the live DashScope API; needs DASHSCOPE_API_KEY in .env"]
    async fn live_extract_facts() {
        dotenvy::dotenv().ok();
        let cfg = crate::services::qwen::config_from_env();
        assert!(!cfg.api_key.is_empty(), "DASHSCOPE_API_KEY not set in .env");
        let qwen = QwenClient::new(cfg).unwrap();

        let user = "我叫阿伦，是一名后端程序员，平时最喜欢喝抹茶拿铁，养了一只叫团子的猫。";
        let reply = "……记住了。";
        let facts = extract_facts(&qwen, user, reply).await.expect("extraction failed");

        for f in &facts {
            println!(
                "[{}/{}] {} (importance {}) ← {:?}",
                f.subject, f.category, f.content, f.importance, f.source_quote
            );
        }
        assert!(!facts.is_empty(), "expected at least one extracted fact");
        // Every returned memory must be grounded in the right speaker's words.
        assert!(
            facts.iter().all(|f| {
                let corpus = match f.subject() {
                    MemorySubject::User => user,
                    MemorySubject::Mutsumi => reply,
                };
                quote_grounded(corpus, &f.source_quote)
            }),
            "extract_from_batch must only return grounded memories"
        );
    }
}
