//! Pipeline A — RAG-enhanced chat (orchestration + Tauri command).
//!
//! Flow per turn:
//!   1. **Vectorize** the user input (Qwen embeddings).
//!   2. **Retrieve** the top-K memories by the weighted recency·importance·
//!      relevance blend, plus the current relationship + profile (one short,
//!      synchronous SQLite read — the lock is released before any `.await`).
//!   3. **Assemble** persona + dynamic context + history + input ([`prompt`]).
//!   4. **Generate** a character-constrained reply (non-streaming for v1;
//!      streaming + the chat UI land in Phase 2b).
//!
//! Silent memory extraction (Pipeline B) and reflection (Pipeline C) build on
//! this same retrieval/assembly machinery in later phases.

mod prompt;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;

use crate::db::memory::{self, RetrievalWeights};
use crate::db::{now, state, Db};
use crate::http::ApiError;
use crate::persona;
use crate::services::qwen::{ChatMessage, ChatOptions, QwenClient};
use crate::services::QwenState;

/// How many memories to inject into the prompt per turn.
const MEMORY_TOP_K: usize = 6;

/// Errors surfaced from the chat pipeline. Serializes as its message so it lands
/// in the rejected-promise path of the frontend `invoke`.
#[derive(Debug, thiserror::Error)]
pub enum ChatError {
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error("database unavailable: {0}")]
    Db(String),
}

impl Serialize for ChatError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

/// Map a rusqlite/lock error into [`ChatError::Db`].
fn db_err(e: impl std::fmt::Display) -> ChatError {
    ChatError::Db(e.to_string())
}

/// A streamed chat event sent to the frontend over a Tauri [`Channel`].
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChatEvent {
    /// One incremental content token.
    Delta { text: String },
    /// Terminal event carrying the full assembled reply.
    Done { content: String },
}

/// Shared steps 1–3 of Pipeline A: vectorize → retrieve → assemble.
///
/// The SQLite read is a short synchronous block; the `MutexGuard` is confined to
/// it and dropped before the caller's next `.await`.
async fn build_messages(
    qwen: &QwenClient,
    db: &Db,
    message: &str,
    locale: &str,
    history: &[ChatMessage],
) -> Result<Vec<ChatMessage>, ChatError> {
    let query_embedding = qwen.embed(message).await?;

    let (memories, relationship, profile) = {
        let conn = db.0.lock().map_err(db_err)?;
        let memories = memory::search(
            &conn,
            &query_embedding,
            MEMORY_TOP_K,
            RetrievalWeights::default(),
            now(),
        )
        .map_err(db_err)?;
        let relationship = state::get_relationship(&conn).map_err(db_err)?;
        let profile = state::all_profile(&conn).map_err(db_err)?;
        (memories, relationship, profile)
    };

    let base = persona::base_system_prompt();
    Ok(prompt::assemble(&prompt::PromptContext {
        base_persona: &base,
        locale,
        profile: &profile,
        relationship: &relationship,
        memories: &memories,
        history,
        user_input: message,
    }))
}

/// Send a chat message and get Mutsumi's reply (non-streaming).
///
/// Used by background callers that want the whole reply at once; the UI uses
/// [`chat_stream`]. `history` is the recent conversation (frontend-owned for
/// v1); `locale` ("en"/"zh"/"ja") sets the reply language.
#[tauri::command]
pub async fn chat_send(
    qwen: State<'_, QwenState>,
    db: State<'_, Db>,
    message: String,
    locale: Option<String>,
    history: Option<Vec<ChatMessage>>,
) -> Result<String, ChatError> {
    let locale = locale.unwrap_or_else(|| "zh".into());
    let history = history.unwrap_or_default();

    let messages = build_messages(&qwen.0, &db, &message, &locale, &history).await?;
    let completion = qwen.0.chat(&messages, None, ChatOptions::default()).await?;
    Ok(completion.message.content.unwrap_or_default())
}

/// Send a chat message and stream Mutsumi's reply token-by-token over `on_event`.
///
/// Emits a [`ChatEvent::Delta`] per token and a final [`ChatEvent::Done`] with
/// the full reply. Returning `Ok(())` signals normal completion; errors land in
/// the rejected-promise path of the frontend `invoke`.
#[tauri::command]
pub async fn chat_stream(
    qwen: State<'_, QwenState>,
    db: State<'_, Db>,
    message: String,
    locale: Option<String>,
    history: Option<Vec<ChatMessage>>,
    on_event: Channel<ChatEvent>,
) -> Result<(), ChatError> {
    let locale = locale.unwrap_or_else(|| "zh".into());
    let history = history.unwrap_or_default();

    let messages = build_messages(&qwen.0, &db, &message, &locale, &history).await?;

    let completion = qwen
        .0
        .chat_stream(&messages, None, ChatOptions::default(), |delta| {
            // Send failures mean the frontend dropped the channel — ignore and
            // let the stream finish (or the connection will simply be unused).
            let _ = on_event.send(ChatEvent::Delta {
                text: delta.to_string(),
            });
        })
        .await?;

    let _ = on_event.send(ChatEvent::Done {
        content: completion.message.content.unwrap_or_default(),
    });
    Ok(())
}
