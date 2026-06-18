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

#[cfg(test)]
mod eval;
pub(crate) mod extraction;
mod prompt;
pub mod reflection;

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use crate::db::history::{self, StoredMessage};
use crate::db::memory::{self, MemorySubject, RetrievalWeights};
use crate::db::{now, state, Db};
use crate::http::ApiError;
use crate::persona;
use crate::search::{self, trigger, SearchState};
use crate::services::qwen::{ChatMessage, ChatOptions, QwenClient};
use crate::services::QwenState;

use extraction::Exchange;

/// How many user-facts to inject into the prompt per turn.
const MEMORY_TOP_K: usize = 5;
/// How many of Mutsumi's own memories to inject per turn (kept smaller).
const SELF_MEMORY_TOP_K: usize = 3;

/// Buffer this many turns before running one batched extraction pass. Trades a
/// little memory freshness for ~Nx fewer extraction LLM calls. The remainder is
/// flushed when the chat panel closes (`chat_flush_memory`).
const EXTRACT_BATCH_TURNS: usize = 6;

/// Managed state: exchanges accumulated since the last extraction pass.
pub struct ChatBuffer(Mutex<Vec<Exchange>>);

impl ChatBuffer {
    pub fn new() -> Self {
        ChatBuffer(Mutex::new(Vec::new()))
    }
}

impl Default for ChatBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Chat options for the user-facing turn. Enables DashScope's built-in web search
/// so in-world factual questions (band rosters, dates, real events) are grounded
/// rather than hallucinated. It is model-gated — Qwen only actually searches when
/// it judges it needs to — so casual/emotional turns pay no extra latency.
fn turn_options() -> ChatOptions {
    ChatOptions {
        enable_search: false,
        ..Default::default()
    }
}

/// Hard ceiling on the whole search step. If exceeded, the turn proceeds with
/// no web context rather than making the user wait.
const SEARCH_BUDGET: Duration = Duration::from_secs(9);

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
    search: &SearchState,
    message: &str,
    locale: &str,
    history: &[ChatMessage],
) -> Result<Vec<ChatMessage>, ChatError> {
    // Search-enhanced awareness + embedding run concurrently (they are
    // independent), so search latency overlaps the embed call instead of
    // stacking. Search is best-effort and capped by SEARCH_BUDGET so a slow or
    // flaky engine can never stall the turn — on timeout we just proceed
    // without web context.
    let search_fut = async {
        if trigger::needs_search(message) {
            let results = tokio::time::timeout(
                SEARCH_BUDGET,
                search::search(&search.client, search.engine(), message),
            )
            .await
            .unwrap_or_default();
            (!results.is_empty()).then(|| search::format_context(message, &results))
        } else {
            None
        }
    };

    let (search_context, embed_result) = tokio::join!(search_fut, qwen.embed(message));
    let query_embedding = embed_result?;

    // One timestamp for both retrieval scoring and the prompt's recency labels.
    let ts = now();
    let weights = RetrievalWeights::default();
    let (memories, self_memories, relationship, profile) = {
        let conn = db.0.lock().map_err(db_err)?;
        // User-facts and Mutsumi's own memories are retrieved separately so both
        // surface (a turn about the user's job won't crowd out her promises).
        let memories =
            memory::search_subject(&conn, &query_embedding, MemorySubject::User, MEMORY_TOP_K, weights, ts)
                .map_err(db_err)?;
        let self_memories = memory::search_subject(
            &conn,
            &query_embedding,
            MemorySubject::Mutsumi,
            SELF_MEMORY_TOP_K,
            weights,
            ts,
        )
        .map_err(db_err)?;
        let relationship = state::get_relationship(&conn).map_err(db_err)?;
        let profile = state::all_profile(&conn).map_err(db_err)?;
        (memories, self_memories, relationship, profile)
    };

    let base = persona::base_system_prompt();
    Ok(prompt::assemble(&prompt::PromptContext {
        base_persona: &base,
        locale,
        profile: &profile,
        relationship: &relationship,
        memories: &memories,
        self_memories: &self_memories,
        search_context: search_context.as_deref(),
        history,
        user_input: message,
        now: ts,
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
    search: State<'_, SearchState>,
    message: String,
    locale: Option<String>,
    history: Option<Vec<ChatMessage>>,
) -> Result<String, ChatError> {
    let locale = locale.unwrap_or_else(|| "zh".into());
    let history = history.unwrap_or_default();

    let messages = build_messages(&qwen.0, &db, &search, &message, &locale, &history).await?;
    let completion = qwen.0.chat(&messages, None, turn_options()).await?;
    Ok(completion.message.content.unwrap_or_default())
}

/// Tauri command: wipe all learned memory + dynamic user state (the "clear
/// memory" reset). Returns how many memory rows were removed.
#[tauri::command]
pub async fn chat_clear_memory(db: State<'_, Db>) -> Result<usize, ChatError> {
    let conn = db.0.lock().map_err(db_err)?;
    let removed = memory::clear_all(&conn).map_err(db_err)?;
    state::reset(&conn).map_err(db_err)?;
    log::info!("chat memory cleared: {removed} memorie(s) removed + state reset");
    Ok(removed)
}

/// Send a chat message and stream Mutsumi's reply token-by-token over `on_event`.
///
/// Emits a [`ChatEvent::Delta`] per token and a final [`ChatEvent::Done`] with
/// the full reply. Returning `Ok(())` signals normal completion; errors land in
/// the rejected-promise path of the frontend `invoke`.
#[tauri::command]
pub async fn chat_stream(
    app: AppHandle,
    qwen: State<'_, QwenState>,
    db: State<'_, Db>,
    search: State<'_, SearchState>,
    buffer: State<'_, ChatBuffer>,
    message: String,
    locale: Option<String>,
    history: Option<Vec<ChatMessage>>,
    on_event: Channel<ChatEvent>,
) -> Result<(), ChatError> {
    let locale = locale.unwrap_or_else(|| "zh".into());
    let history = history.unwrap_or_default();

    let messages = build_messages(&qwen.0, &db, &search, &message, &locale, &history).await?;

    let completion = qwen
        .0
        .chat_stream(&messages, None, turn_options(), |delta| {
            // Send failures mean the frontend dropped the channel — ignore and
            // let the stream finish (or the connection will simply be unused).
            let _ = on_event.send(ChatEvent::Delta {
                text: delta.to_string(),
            });
        })
        .await?;

    let reply = completion.message.content.unwrap_or_default();
    let _ = on_event.send(ChatEvent::Done {
        content: reply.clone(),
    });

    // Persist both turns to the durable transcript (the IM-style history). This
    // is independent of the LLM `history` window above — it never changes what
    // the model sees. Short synchronous lock, dropped before the spawn below.
    {
        let conn = db.0.lock().map_err(db_err)?;
        let ts = now();
        let _ = history::append_message(&conn, "user", &message, ts);
        let _ = history::append_message(&conn, "assistant", &reply, ts);
    }

    // Pipeline B: buffer the exchange; run one batched extraction per
    // EXTRACT_BATCH_TURNS (the rest is flushed on chat close). The lock is
    // released before any await — we only spawn the (fire-and-forget) extraction.
    let ready = {
        let mut buf = buffer.0.lock().map_err(db_err)?;
        buffer_push(&mut buf, Exchange { user: message, reply }, EXTRACT_BATCH_TURNS)
    };
    if let Some(batch) = ready {
        tauri::async_runtime::spawn(extraction::extract_and_store_batch(app, batch));
    }
    Ok(())
}

/// Push `ex` onto the buffer; return a full batch to extract once the buffer
/// reaches `batch_size`, else `None`. Pure, so the batching trigger is unit-tested.
fn buffer_push(buf: &mut Vec<Exchange>, ex: Exchange, batch_size: usize) -> Option<Vec<Exchange>> {
    buf.push(ex);
    (buf.len() >= batch_size).then(|| buf.drain(..).collect())
}

/// Tauri command: flush any buffered exchanges through extraction now. Called by
/// the frontend when the chat panel closes so a short conversation's memories
/// aren't stranded below the batch threshold.
#[tauri::command]
pub async fn chat_flush_memory(
    app: AppHandle,
    buffer: State<'_, ChatBuffer>,
) -> Result<(), ChatError> {
    let batch: Vec<Exchange> = {
        let mut buf = buffer.0.lock().map_err(db_err)?;
        buf.drain(..).collect()
    };
    if !batch.is_empty() {
        tauri::async_runtime::spawn(extraction::extract_and_store_batch(app, batch));
    }
    Ok(())
}

// ── Chat history (persistent transcript) ──────────────────────────────────

/// Load a page of the transcript for the IM-style thread. `before` is a keyset
/// cursor: pass `None` for the newest page (initial load), or an existing
/// message id to fetch the page *older* than it (upward scroll). Returns
/// ascending (oldest-first) for direct rendering.
#[tauri::command]
pub async fn chat_recent_messages(
    db: State<'_, Db>,
    before: Option<i64>,
    limit: usize,
) -> Result<Vec<StoredMessage>, ChatError> {
    let conn = db.0.lock().map_err(db_err)?;
    history::recent(&conn, before, limit).map_err(db_err)
}

/// Load the page of messages *newer* than `after_id`, ascending. Backs downward
/// scroll after a History jump leaves the user mid-timeline; a short page means
/// the present tail has been reached.
#[tauri::command]
pub async fn chat_messages_after(
    db: State<'_, Db>,
    after_id: i64,
    limit: usize,
) -> Result<Vec<StoredMessage>, ChatError> {
    let conn = db.0.lock().map_err(db_err)?;
    history::after(&conn, after_id, limit).map_err(db_err)
}

/// Search the transcript by keyword and/or date range (epoch seconds), newest
/// first. Backs the History panel.
#[tauri::command]
pub async fn chat_search_history(
    db: State<'_, Db>,
    query: Option<String>,
    start: Option<i64>,
    end: Option<i64>,
    limit: usize,
) -> Result<Vec<StoredMessage>, ChatError> {
    let conn = db.0.lock().map_err(db_err)?;
    history::search(&conn, query.as_deref(), start, end, limit).map_err(db_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ex(u: &str) -> Exchange {
        Exchange { user: u.into(), reply: "……".into() }
    }

    #[test]
    fn buffer_push_drains_only_at_batch_size() {
        let mut buf = Vec::new();
        assert!(buffer_push(&mut buf, ex("1"), 3).is_none());
        assert!(buffer_push(&mut buf, ex("2"), 3).is_none());
        let batch = buffer_push(&mut buf, ex("3"), 3).expect("batch ready at size 3");
        assert_eq!(batch.len(), 3);
        assert!(buf.is_empty(), "buffer drained after a batch");
        // Next cycle starts fresh.
        assert!(buffer_push(&mut buf, ex("4"), 3).is_none());
        assert_eq!(buf.len(), 1);
    }
}
