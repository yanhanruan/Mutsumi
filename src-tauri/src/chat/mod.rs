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
use tauri::State;

use crate::db::memory::{self, RetrievalWeights};
use crate::db::{now, state, Db};
use crate::http::ApiError;
use crate::persona;
use crate::services::qwen::{ChatMessage, ChatOptions};
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

/// Map a rusqlite error into [`ChatError::Db`].
fn db_err(e: impl std::fmt::Display) -> ChatError {
    ChatError::Db(e.to_string())
}

/// Send a chat message and get Mutsumi's reply (non-streaming).
///
/// `history` is the recent conversation (frontend-owned for v1); `locale`
/// ("en"/"zh"/"ja") sets the reply language.
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

    // 1. Vectorize the user input for semantic retrieval.
    let query_embedding = qwen.0.embed(&message).await?;

    // 2. Retrieve memory + relationship + profile. Short synchronous DB work;
    //    the MutexGuard is confined to this block and dropped before any await.
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

    // 3. Assemble persona + dynamic context + history + input.
    let base = persona::base_system_prompt();
    let messages = prompt::assemble(&prompt::PromptContext {
        base_persona: &base,
        locale: &locale,
        profile: &profile,
        relationship: &relationship,
        memories: &memories,
        history: &history,
        user_input: &message,
    });

    // 4. Generate the character-constrained reply.
    let completion = qwen.0.chat(&messages, None, ChatOptions::default()).await?;
    Ok(completion.message.content.unwrap_or_default())
}
