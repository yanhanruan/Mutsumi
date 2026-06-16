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
#[allow(dead_code)]
pub struct QwenState(pub QwenClient);

impl QwenState {
    /// Construct from static config. Built once at startup and `.manage()`d.
    pub fn new(config: QwenConfig) -> Result<Self, ApiError> {
        Ok(Self(QwenClient::new(config)?))
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
