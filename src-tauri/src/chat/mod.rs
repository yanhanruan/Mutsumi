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
#[cfg(test)]
mod eval_record;
pub(crate) mod extraction;
mod media;
mod prompt;
pub mod reflection;

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use base64::Engine as _;

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

/// Per-image size cap (5 MB) and per-turn count cap for vision input. The
/// frontend pre-checks the count; size/type are (re)validated here.
const IMAGE_MAX_BYTES: usize = 5 * 1024 * 1024;
const IMAGE_MAX_COUNT: usize = 3;
/// Accepted image extensions (lowercased, no dot). HEIC is allowed but stored
/// uncompressed (the `image` crate can't decode it).
const IMAGE_EXT_WHITELIST: [&str; 9] =
    ["bmp", "jpe", "jpeg", "jpg", "png", "tif", "tiff", "webp", "heic"];

/// Map a whitelisted extension to its MIME type for the data URL.
fn mime_for(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "webp" => "image/webp",
        "heic" => "image/heic",
        _ => "image/jpeg", // jpg/jpe/jpeg + safe default
    }
}

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

/// Upper bound on a single reply's generated tokens. Mutsumi is terse (1–2 short
/// lines), so this only guards against a runaway generation — it rarely binds.
const MAX_REPLY_TOKENS: u32 = 512;

/// Chat options for the user-facing turn.
///
/// `enable_thinking: false` skips the Qwen3.x chain-of-thought — for terse,
/// in-character replies the reasoning pass adds latency without improving the
/// answer (world-facts are already grounded by the persona block), so turning it
/// off is the main per-turn latency win. `max_completion_tokens` caps a runaway
/// reply. Web search stays off (grounding comes from the model + persona facts).
fn turn_options() -> ChatOptions {
    ChatOptions {
        enable_search: false,
        enable_thinking: Some(false),
        max_completion_tokens: Some(MAX_REPLY_TOKENS),
        ..Default::default()
    }
}

/// Hard ceiling on the whole search step. If exceeded, the turn proceeds with
/// no web context rather than making the user wait.
const SEARCH_BUDGET: Duration = Duration::from_secs(9);

/// Errors surfaced from the chat pipeline.
///
/// Serializes as a **stable code** (see [`ChatError::code`]) — not a prose
/// message — so the frontend can map each failure class to its own localized,
/// in-character line (network down, key missing, out of credit, …) instead of a
/// single vague "something went wrong".
#[derive(Debug, thiserror::Error)]
pub enum ChatError {
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error("database unavailable: {0}")]
    Db(String),
    /// No API key configured — checked up front so the user gets a "set your
    /// key" hint rather than a raw auth failure.
    #[error("api key not configured")]
    ApiKeyMissing,
    /// Stable codes the frontend maps to localized in-character alerts.
    #[error("image_too_large")]
    ImageTooLarge,
    #[error("image_bad_type")]
    ImageBadType,
}

impl ChatError {
    /// A stable, machine-readable code the frontend maps to a localized message.
    /// HTTP failures are inspected so auth / billing / moderation / rate-limit /
    /// transport problems are told apart.
    pub fn code(&self) -> &'static str {
        match self {
            ChatError::ApiKeyMissing => "api_key_missing",
            ChatError::ImageTooLarge => "image_too_large",
            ChatError::ImageBadType => "image_bad_type",
            ChatError::Db(_) => "db_error",
            ChatError::Api(e) => api_error_code(e),
        }
    }
}

/// Classify a transport/HTTP [`ApiError`] into a stable frontend code.
fn api_error_code(e: &ApiError) -> &'static str {
    match e {
        ApiError::Network(re) if re.is_timeout() => "timeout",
        ApiError::Network(_) => "network",
        ApiError::Api { status, body } => classify_api_status(*status, body),
        // A malformed/undecodable body or a request we couldn't build: no useful
        // category to show the user.
        ApiError::Decode(_) | ApiError::Build(_) => "unknown",
    }
}

/// Bucket a non-2xx DashScope (OpenAI-compatible) response by status code and the
/// provider error code/message in the body. Billing is checked first so a
/// `403 AllocationQuota.FreeTierOnly` lands as "out of credit", not an auth error.
fn classify_api_status(status: u16, body: &str) -> &'static str {
    let b = body.to_ascii_lowercase();
    // Billing / quota: Arrearage, AllocationQuota.FreeTierOnly, quota exceeded, …
    if status == 402
        || b.contains("arrearage")
        || b.contains("insufficient")
        || b.contains("allocationquota")
        || b.contains("freetieronly")
        || b.contains("quota")
    {
        return "insufficient_balance";
    }
    // Auth / API key.
    if status == 401
        || b.contains("invalidapikey")
        || b.contains("invalid_api_key")
        || b.contains("incorrect api key")
        || b.contains("authentication")
    {
        return "api_key_invalid";
    }
    // Rate limiting / throttling.
    if status == 429 || b.contains("throttl") || b.contains("rate limit") || b.contains("requests rate") {
        return "rate_limited";
    }
    // Content moderation — the input itself was rejected ("invalid question").
    if b.contains("datainspectionfailed") || b.contains("data_inspection") || b.contains("inappropriate") {
        return "content_filtered";
    }
    if status >= 500 {
        return "server_error";
    }
    "api_error"
}

impl Serialize for ChatError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.code())
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
        if search.enabled() && trigger::needs_search(message) {
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
    if !qwen.has_key() {
        return Err(ChatError::ApiKeyMissing);
    }
    let locale = locale.unwrap_or_else(|| "zh".into());
    let history = history.unwrap_or_default();

    let client = qwen.client();
    let messages = build_messages(&client, &db, &search, &message, &locale, &history).await?;
    let completion = client.chat(&messages, None, turn_options()).await?;
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
    if !qwen.has_key() {
        return Err(ChatError::ApiKeyMissing);
    }
    let locale = locale.unwrap_or_else(|| "zh".into());
    let history = history.unwrap_or_default();

    let client = qwen.client();
    let messages = build_messages(&client, &db, &search, &message, &locale, &history).await?;

    let completion = client
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

/// Replace each row's relative `image_path` with an absolute one the frontend can
/// hand to `convertFileSrc`. A row whose path can't be resolved drops to `None`
/// (renders as a caption-only bubble) rather than failing the whole page.
fn resolve_image_paths(app: &AppHandle, msgs: &mut [StoredMessage]) {
    for m in msgs.iter_mut() {
        if let Some(rel) = m.image_path.take() {
            m.image_path = media::resolve(app, &rel)
                .ok()
                .map(|p| p.to_string_lossy().into_owned());
        }
    }
}

/// Load a page of the transcript for the IM-style thread. `before` is a keyset
/// cursor: pass `None` for the newest page (initial load), or an existing
/// message id to fetch the page *older* than it (upward scroll). Returns
/// ascending (oldest-first) for direct rendering.
#[tauri::command]
pub async fn chat_recent_messages(
    app: AppHandle,
    db: State<'_, Db>,
    before: Option<i64>,
    limit: usize,
) -> Result<Vec<StoredMessage>, ChatError> {
    let mut msgs = {
        let conn = db.0.lock().map_err(db_err)?;
        history::recent(&conn, before, limit).map_err(db_err)?
    };
    resolve_image_paths(&app, &mut msgs);
    Ok(msgs)
}

/// Load the page of messages *newer* than `after_id`, ascending. Backs downward
/// scroll after a History jump leaves the user mid-timeline; a short page means
/// the present tail has been reached.
#[tauri::command]
pub async fn chat_messages_after(
    app: AppHandle,
    db: State<'_, Db>,
    after_id: i64,
    limit: usize,
) -> Result<Vec<StoredMessage>, ChatError> {
    let mut msgs = {
        let conn = db.0.lock().map_err(db_err)?;
        history::after(&conn, after_id, limit).map_err(db_err)?
    };
    resolve_image_paths(&app, &mut msgs);
    Ok(msgs)
}

/// Search the transcript by keyword and/or date range (epoch seconds), newest
/// first. Backs the History panel.
#[tauri::command]
pub async fn chat_search_history(
    app: AppHandle,
    db: State<'_, Db>,
    query: Option<String>,
    start: Option<i64>,
    end: Option<i64>,
    limit: usize,
) -> Result<Vec<StoredMessage>, ChatError> {
    let mut msgs = {
        let conn = db.0.lock().map_err(db_err)?;
        history::search(&conn, query.as_deref(), start, end, limit).map_err(db_err)?
    };
    resolve_image_paths(&app, &mut msgs);
    Ok(msgs)
}

// ── Vision (image) chat ───────────────────────────────────────────────────

/// A clipboard image staged to a temp file, returned to the frontend so it joins
/// the same path-based send flow as a picked/dropped file.
#[derive(Debug, Clone, Serialize)]
pub struct StagedImage {
    pub path: String,
}

/// Read an image off the system clipboard, validate it, and stage it to a temp
/// file under the media dir; returns its path. Reading at paste-time means a
/// later clipboard change can't swap the image out from under the send.
#[tauri::command]
pub async fn chat_stage_clipboard_image(app: AppHandle) -> Result<StagedImage, ChatError> {
    use tauri_plugin_clipboard_manager::ClipboardExt;

    let img = app
        .clipboard()
        .read_image()
        .map_err(|_| ChatError::ImageBadType)?; // nothing usable on the clipboard
    let (w, h) = (img.width(), img.height());
    let rgba = image::RgbaImage::from_raw(w, h, img.rgba().to_vec())
        .ok_or(ChatError::ImageBadType)?;

    // Encode to PNG (clipboard images are raw RGBA).
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(rgba)
        .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .map_err(|e| ChatError::Db(e.to_string()))?;
    if bytes.len() > IMAGE_MAX_BYTES {
        return Err(ChatError::ImageTooLarge);
    }

    let dir = media::staging_dir(&app).map_err(ChatError::Db)?;
    std::fs::create_dir_all(&dir).map_err(|e| ChatError::Db(e.to_string()))?;
    let path = dir.join(format!("clip-{}.png", now()));
    std::fs::write(&path, &bytes).map_err(|e| ChatError::Db(e.to_string()))?;
    Ok(StagedImage {
        path: path.to_string_lossy().into_owned(),
    })
}

/// Send up to [`IMAGE_MAX_COUNT`] images (by path) with an optional caption and
/// stream Mutsumi's text reply. Images are read + validated in Rust (no bytes
/// over IPC), sent to the vision model, and — only on success — compressed,
/// archived, and persisted to the transcript.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn chat_vision_stream(
    app: AppHandle,
    qwen: State<'_, QwenState>,
    db: State<'_, Db>,
    search: State<'_, SearchState>,
    buffer: State<'_, ChatBuffer>,
    message: Option<String>,
    paths: Vec<String>,
    locale: Option<String>,
    history: Option<Vec<ChatMessage>>,
    on_event: Channel<ChatEvent>,
) -> Result<(), ChatError> {
    if !qwen.has_key() {
        return Err(ChatError::ApiKeyMissing);
    }
    let locale = locale.unwrap_or_else(|| "zh".into());
    let history = history.unwrap_or_default();
    let caption = message.unwrap_or_default().trim().to_string();

    if paths.is_empty() || paths.len() > IMAGE_MAX_COUNT {
        return Err(ChatError::ImageBadType);
    }

    // Read + validate each image (defense in depth; the frontend pre-checks count).
    let mut images: Vec<(Vec<u8>, String)> = Vec::with_capacity(paths.len());
    for p in &paths {
        let ext = std::path::Path::new(p)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !IMAGE_EXT_WHITELIST.contains(&ext.as_str()) {
            return Err(ChatError::ImageBadType);
        }
        let bytes = std::fs::read(p).map_err(|_| ChatError::ImageBadType)?;
        if bytes.len() > IMAGE_MAX_BYTES {
            return Err(ChatError::ImageTooLarge);
        }
        images.push((bytes, ext));
    }

    // base64 data URLs for the VL request (original bytes).
    let data_urls: Vec<String> = images
        .iter()
        .map(|(bytes, ext)| {
            format!(
                "data:{};base64,{}",
                mime_for(ext),
                base64::engine::general_purpose::STANDARD.encode(bytes)
            )
        })
        .collect();

    // Reuse the text-pipeline assembly (persona + dynamic context + memory + history)
    // with the caption as the retrieval query, then drop its trailing user-text
    // turn (the vision turn re-adds it) and append the vision guidance block.
    let query = if caption.is_empty() { "图片".to_string() } else { caption.clone() };
    let client = qwen.client();
    let mut messages = build_messages(&client, &db, &search, &query, &locale, &history).await?;
    messages.pop(); // remove the placeholder user-text message
    messages.push(ChatMessage::system(persona::vision_guidance().to_string()));

    let completion = client
        .chat_vision_stream(&messages, &data_urls, &caption, |delta| {
            let _ = on_event.send(ChatEvent::Delta { text: delta.to_string() });
        })
        .await?;
    let reply = completion.message.content.unwrap_or_default();
    let _ = on_event.send(ChatEvent::Done { content: reply.clone() });

    // Success → compress + archive each image (no lock held across this IO).
    let mut rels: Vec<String> = Vec::with_capacity(images.len());
    for (bytes, ext) in &images {
        match media::store(&app, bytes, ext) {
            Ok(rel) => rels.push(rel),
            Err(e) => log::warn!("media store failed: {e}"),
        }
    }

    // Persist the image row(s) (caption rides on the first) + the assistant reply.
    {
        let conn = db.0.lock().map_err(db_err)?;
        let ts = now();
        for (i, rel) in rels.iter().enumerate() {
            let cap = if i == 0 { caption.as_str() } else { "" };
            let _ = history::append_image_message(&conn, cap, rel, ts);
        }
        let _ = history::append_message(&conn, "assistant", &reply, ts);
    }

    // Best-effort cleanup of any staged clipboard temp files we just consumed.
    if let Ok(staging) = media::staging_dir(&app) {
        for p in &paths {
            if std::path::Path::new(p).starts_with(&staging) {
                let _ = std::fs::remove_file(p);
            }
        }
    }

    // Feed the exchange to batched extraction (caption-grounded; image-only ⇒ no
    // user-fact extraction, which is correct).
    let user_for_mem = if caption.is_empty() { "[图片]".to_string() } else { caption };
    let ready = {
        let mut buf = buffer.0.lock().map_err(db_err)?;
        buffer_push(&mut buf, Exchange { user: user_for_mem, reply }, EXTRACT_BATCH_TURNS)
    };
    if let Some(batch) = ready {
        tauri::async_runtime::spawn(extraction::extract_and_store_batch(app, batch));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ex(u: &str) -> Exchange {
        Exchange { user: u.into(), reply: "……".into() }
    }

    #[test]
    fn classify_api_status_buckets_known_failures() {
        // Billing / quota — including the 403 free-tier allocation gate.
        assert_eq!(classify_api_status(402, ""), "insufficient_balance");
        assert_eq!(
            classify_api_status(403, r#"{"error":{"code":"AllocationQuota.FreeTierOnly"}}"#),
            "insufficient_balance"
        );
        assert_eq!(
            classify_api_status(400, r#"{"code":"Arrearage","message":"insufficient balance"}"#),
            "insufficient_balance"
        );
        // Auth.
        assert_eq!(classify_api_status(401, ""), "api_key_invalid");
        assert_eq!(
            classify_api_status(400, r#"{"code":"InvalidApiKey"}"#),
            "api_key_invalid"
        );
        // Rate limit + moderation + server + generic.
        assert_eq!(classify_api_status(429, ""), "rate_limited");
        assert_eq!(
            classify_api_status(400, r#"{"code":"DataInspectionFailed"}"#),
            "content_filtered"
        );
        assert_eq!(classify_api_status(503, ""), "server_error");
        assert_eq!(classify_api_status(418, "teapot"), "api_error");
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
