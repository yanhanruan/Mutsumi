//! Service layer + Tauri command surface.
//!
//! One module per external provider (currently [`fish_audio`]; future: `qwen`,
//! `deepseek`, …). Each provider module owns its API-specific client and config;
//! this file wires those clients into Tauri as managed state and exposes the
//! `#[tauri::command]` surface the frontend calls.
//!
//! Layering:
//!
//! ```text
//!   frontend ─invoke─▶ tts_synthesize / tts_set_recaptcha      (this module)
//!                              │
//!                              ▼
//!                 FishAudioTtsService   (fish_audio.rs — API-specific)
//!                              │
//!                              ▼
//!                 HttpClient            (crate::http — generic)
//! ```

pub mod fish_audio;
// Qwen LLM client. Consumed by the chat/memory pipelines (Phases 2–5); the
// service API lands ahead of those callers, so allow dead_code until wired.
#[allow(dead_code)]
pub mod qwen;

pub use fish_audio::{FishAudioTtsService, TtsConfig};
pub use qwen::{QwenClient, QwenConfig};

use crate::http::ApiError;

/// Tauri-managed handle to the Fish Audio TTS service.
///
/// One state type per provider, so future services follow the same shape
/// (`QwenState`, `DeepseekState`, …).
pub struct FishAudioState(pub FishAudioTtsService);

impl FishAudioState {
    /// Construct from static config. Built once in `setup()` and `.manage()`d.
    pub fn new(config: TtsConfig) -> Result<Self, ApiError> {
        Ok(Self(FishAudioTtsService::new(config)?))
    }
}

/// Tauri-managed handle to the Qwen LLM client.
///
/// Interior-mutable so the user can set their own 百炼 (DashScope) API key from
/// the Settings window at runtime: [`set_api_key`](QwenState::set_api_key)
/// rebuilds the underlying client (the key is baked into the HTTP auth header,
/// so a new key means a new client) while call sites keep reading a cheap,
/// Arc-backed [`client`](QwenState::client) clone.
pub struct QwenState {
    client: std::sync::RwLock<QwenClient>,
    /// Kept so a key change can rebuild the client with the same base URL/models.
    config: std::sync::RwLock<QwenConfig>,
}

impl QwenState {
    /// Construct from static config. Built once at startup and `.manage()`d.
    pub fn new(config: QwenConfig) -> Result<Self, ApiError> {
        let client = QwenClient::new(config.clone())?;
        Ok(Self {
            client: std::sync::RwLock::new(client),
            config: std::sync::RwLock::new(config),
        })
    }

    /// A cheap (Arc-backed) clone of the live client, safe to hold across awaits.
    pub fn client(&self) -> QwenClient {
        self.client.read().unwrap().clone()
    }

    /// Whether a non-empty API key is currently configured.
    pub fn has_key(&self) -> bool {
        !self.config.read().unwrap().api_key.trim().is_empty()
    }

    /// Replace the API key and rebuild the live client. An empty string clears it
    /// (subsequent calls then fail with an auth error until a key is set again).
    pub fn set_api_key(&self, key: &str) -> Result<(), ApiError> {
        let new_client = {
            let mut cfg = self.config.write().unwrap();
            cfg.api_key = key.trim().to_string();
            QwenClient::new(cfg.clone())?
        };
        *self.client.write().unwrap() = new_client;
        Ok(())
    }

    /// Switch the chat model and rebuild the live client. The current API key and
    /// other config are preserved (we rebuild from the stored `QwenConfig`). A
    /// blank model is ignored. Only the text chat model changes; vision/embeddings
    /// keep their configured models.
    pub fn set_chat_model(&self, model: &str) -> Result<(), ApiError> {
        let model = model.trim();
        if model.is_empty() {
            return Ok(());
        }
        let new_client = {
            let mut cfg = self.config.write().unwrap();
            if cfg.chat_model == model {
                return Ok(()); // no-op; avoid a needless client rebuild
            }
            cfg.chat_model = model.to_string();
            QwenClient::new(cfg.clone())?
        };
        *self.client.write().unwrap() = new_client;
        Ok(())
    }
}

// ── Persisted Qwen API key (set via Settings; overrides the .env default) ──
//
// Stored in the OS credential store via `keyring` (Windows Credential Manager /
// macOS Keychain / Linux Secret Service) — never plaintext on disk.

const KEYRING_SERVICE: &str = "com.mutsumi.app";
const KEYRING_USER: &str = "dashscope_api_key";

fn qwen_key_entry() -> keyring::Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
}

/// Load a previously-saved key from the OS credential store, if any (called at
/// startup to override the `.env` default).
pub fn load_persisted_qwen_key() -> Option<String> {
    match qwen_key_entry().ok()?.get_password() {
        Ok(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        _ => None, // no entry, empty, or backend error → fall back to .env
    }
}

/// Persist (or, for an empty key, delete) the key in the OS credential store.
fn save_persisted_qwen_key(key: &str) -> keyring::Result<()> {
    let entry = qwen_key_entry()?;
    let key = key.trim();
    if key.is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e),
        }
    } else {
        entry.set_password(key)
    }
}

// ── Tauri commands ──────────────────────────────────────────────────

/// Synthesize `text` to speech and return the raw `mp3` bytes.
///
/// `text` is the only dynamic input. Returned as a [`tauri::ipc::Response`] so
/// the audio travels to the webview as a binary payload rather than a JSON
/// number array.
#[tauri::command]
pub async fn tts_synthesize(
    state: tauri::State<'_, FishAudioState>,
    text: String,
) -> Result<tauri::ipc::Response, ApiError> {
    let bytes = state.0.synthesize(text).await?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Refresh the time-sensitive reCAPTCHA token without rebuilding the client.
#[tauri::command]
pub fn tts_set_recaptcha(state: tauri::State<'_, FishAudioState>, token: String) {
    state.0.set_recaptcha(token);
}

/// Whether a Qwen / 百炼 (DashScope) API key is currently configured.
#[tauri::command]
pub fn qwen_key_status(qwen: tauri::State<'_, QwenState>) -> bool {
    qwen.has_key()
}

/// Set the user's Qwen / 百炼 (DashScope) API key: applies it to the live client
/// immediately (no restart) and persists it for next launch. Pass an empty
/// string to clear it. Returns whether a key is configured afterwards.
#[tauri::command]
pub fn qwen_set_api_key(
    qwen: tauri::State<'_, QwenState>,
    key: String,
) -> Result<bool, String> {
    qwen.set_api_key(&key).map_err(|e| e.to_string())?;
    save_persisted_qwen_key(&key).map_err(|e| e.to_string())?;
    Ok(qwen.has_key())
}

/// Switch the active chat model at runtime (Settings UI). The choice is persisted
/// in the frontend config and re-applied on startup, so no backend file is needed.
#[tauri::command]
pub fn qwen_set_chat_model(
    qwen: tauri::State<'_, QwenState>,
    model: String,
) -> Result<(), String> {
    qwen.set_chat_model(&model).map_err(|e| e.to_string())
}
