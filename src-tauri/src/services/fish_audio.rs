//! Fish Audio text-to-speech service (service layer).
//!
//! Refactored from this `curl` (POST `https://api.fish.audio/task`, a streaming
//! TTS endpoint that answers with an `mp3` byte stream):
//!
//! ```text
//! curl -H 'authorization: Bearer <token>' -H 'content-type: application/json' \
//!      -H 'x-team-id: …' -H 'x-workspace-id: …' -H 'x-fish-amp-device-id: …' \
//!      -H 'x-fish-amp-session-id: …' -H 'origin: https://fish.audio' \
//!      -X POST https://api.fish.audio/task \
//!      -d '{"type":"tts","stream":true,"model":"…","latency":"balanced",
//!           "parameters":{"text":"早上好","model_id":"…","format":"mp3",
//!                         "latency":"balanced","backend":"s2-pro","normalize":false},
//!           "recaptcha":"…","format":"mp3","backend":"s2-pro","group_id":"…"}'
//! ```
//!
//! ## Parameter-handling strategy
//!
//! | Class                | Fields                                                              | Where it lives                          |
//! |----------------------|--------------------------------------------------------------------|-----------------------------------------|
//! | **Static config**    | auth token, team/workspace/device/session ids → request headers; `model_id`, `latency`, `format`, `backend`, `group_id` → payload | captured in the constructor             |
//! | **Time-sensitive**   | `recaptcha`                                                        | `Mutex<String>`, seeded in the ctor, refreshable via [`FishAudioTtsService::set_recaptcha`] without rebuilding the client |
//! | **Dynamic input**    | `text`                                                            | argument to [`FishAudioTtsService::synthesize`] |

use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, ORIGIN, REFERER,
};
use serde::Serialize;

use crate::http::{ApiError, HttpClient, HttpClientConfig};

const BASE_URL: &str = "https://api.fish.audio";
const TASK_PATH: &str = "/task";
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

// Fixed routing/identity values from the sample request. Unlike the auth token
// and reCAPTCHA (see `config_from_env`), these are not secret and rarely change,
// so they stay inline as defaults.
const DEFAULT_TEAM_ID: &str = "283df3d1f4eb44b89d9a58a213942667";
const DEFAULT_WORKSPACE_ID: &str = "283df3d1f4eb44b89d9a58a213942667";
const DEFAULT_DEVICE_ID: &str = "0d324fba-7003-4ee2-8571-d04796f19f4b";
const DEFAULT_SESSION_ID: &str = "1781376507994";
const DEFAULT_MODEL_ID: &str = "8eca9d8ddda147599ded13abd00b49a2";
const DEFAULT_LATENCY: &str = "balanced";
const DEFAULT_FORMAT: &str = "mp3";
const DEFAULT_BACKEND: &str = "s2-pro";
const DEFAULT_GROUP_ID: &str = "56f38a45-3663-4162-9201-678182487e39";

// Environment variables holding the time-sensitive secrets. Populated from a
// `.env` file loaded at startup (see `crate::run`); template in `src-tauri/.env.example`.
const ENV_AUTH_TOKEN: &str = "FISH_AUDIO_AUTH_TOKEN";
const ENV_RECAPTCHA: &str = "FISH_AUDIO_RECAPTCHA";

/// Build a [`TtsConfig`] from the environment.
///
/// The two secrets — `auth_token` and `recaptcha` — are read from the
/// environment (`FISH_AUDIO_AUTH_TOKEN` / `FISH_AUDIO_RECAPTCHA`), which is
/// populated from `src-tauri/.env` at startup. Missing vars fall back to an empty
/// string so the app still boots; calls then fail with an auth error until the
/// values are supplied (refresh `recaptcha` at runtime via
/// [`FishAudioTtsService::set_recaptcha`]). All other fields use the inline
/// defaults above.
pub fn config_from_env() -> TtsConfig {
    TtsConfig {
        auth_token: std::env::var(ENV_AUTH_TOKEN).unwrap_or_default(),
        recaptcha: std::env::var(ENV_RECAPTCHA).unwrap_or_default(),
        team_id: DEFAULT_TEAM_ID.into(),
        workspace_id: DEFAULT_WORKSPACE_ID.into(),
        device_id: DEFAULT_DEVICE_ID.into(),
        session_id: DEFAULT_SESSION_ID.into(),
        model_id: DEFAULT_MODEL_ID.into(),
        latency: DEFAULT_LATENCY.into(),
        format: DEFAULT_FORMAT.into(),
        backend: DEFAULT_BACKEND.into(),
        group_id: DEFAULT_GROUP_ID.into(),
    }
}

// ── Static configuration ────────────────────────────────────────────
//
// Everything that is fixed for the lifetime of a service instance. Headers
// become the client's default headers; payload fields are stored on the struct
// and stamped onto every request body.

/// Immutable configuration captured by [`FishAudioTtsService::new`].
///
/// `recaptcha` is included here for convenience (it seeds the refreshable slot),
/// but it is the one field that is *not* truly static — see [`FishAudioTtsService`].
#[derive(Clone)]
pub struct TtsConfig {
    // ── auth / routing headers ──
    pub auth_token: String,
    pub team_id: String,
    pub workspace_id: String,
    pub device_id: String,
    pub session_id: String,
    // ── static payload fields ──
    pub model_id: String,
    pub latency: String,
    pub format: String,
    pub backend: String,
    pub group_id: String,
    // ── time-sensitive (seeds the refreshable slot) ──
    pub recaptcha: String,
}

// ── Strongly-typed request payload ──────────────────────────────────
//
// Mirrors the JSON body exactly so we never hand-concatenate strings. `Serialize`
// only — we don't model the response because it's a raw audio byte stream.

#[derive(Debug, Serialize)]
struct TtsRequest {
    // `type` is a Rust keyword; `r#type` serializes as `"type"` (serde strips the
    // raw-identifier prefix), so no `#[serde(rename)]` is needed.
    r#type: &'static str,
    stream: bool,
    model: String,
    latency: String,
    parameters: TtsParameters,
    recaptcha: String,
    format: String,
    backend: String,
    group_id: String,
}

#[derive(Debug, Serialize)]
struct TtsParameters {
    text: String,
    model_id: String,
    format: String,
    latency: String,
    backend: String,
    normalize: bool,
}

// ── Service ─────────────────────────────────────────────────────────

/// API-specific client for Fish Audio TTS, layered over [`HttpClient`].
///
/// Cheap to wrap in an `Arc` and share (e.g. as Tauri managed state): the
/// underlying `reqwest` connection pool is reused across all calls, and the only
/// mutable state is the `recaptcha` token guarded by a [`Mutex`].
pub struct FishAudioTtsService {
    http: Arc<HttpClient>,

    // static payload fields
    model_id: String,
    latency: String,
    format: String,
    backend: String,
    group_id: String,

    // Time-sensitive token. Behind a Mutex so it can be swapped at runtime via
    // `set_recaptcha` without recreating the HttpClient (and thus without
    // dropping the warm connection pool).
    recaptcha: Mutex<String>,
}

impl FishAudioTtsService {
    /// Build the service from its static configuration. Constructs the
    /// underlying [`HttpClient`] with all fixed headers baked in as defaults.
    pub fn new(config: TtsConfig) -> Result<Self, ApiError> {
        let headers = Self::build_headers(&config)?;

        let http = HttpClient::new(HttpClientConfig {
            base_url: BASE_URL.to_string(),
            timeout: HTTP_TIMEOUT,
            default_headers: headers,
        })?;

        Ok(Self {
            http: Arc::new(http),
            model_id: config.model_id,
            latency: config.latency,
            format: config.format,
            backend: config.backend,
            group_id: config.group_id,
            recaptcha: Mutex::new(config.recaptcha),
        })
    }

    /// Assemble the fixed request headers from static config.
    fn build_headers(config: &TtsConfig) -> Result<HeaderMap, ApiError> {
        // Small helper: build a HeaderValue or surface a Build error.
        fn val(s: &str) -> Result<HeaderValue, ApiError> {
            HeaderValue::from_str(s).map_err(|e| ApiError::Build(e.to_string()))
        }

        let mut h = HeaderMap::new();
        h.insert(
            AUTHORIZATION,
            val(&format!("Bearer {}", config.auth_token))?,
        );
        h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        h.insert(ACCEPT, HeaderValue::from_static("*/*"));
        h.insert(ORIGIN, HeaderValue::from_static("https://fish.audio"));
        h.insert(REFERER, HeaderValue::from_static("https://fish.audio/"));
        h.insert(HeaderName::from_static("x-team-id"), val(&config.team_id)?);
        h.insert(
            HeaderName::from_static("x-workspace-id"),
            val(&config.workspace_id)?,
        );
        h.insert(
            HeaderName::from_static("x-fish-amp-device-id"),
            val(&config.device_id)?,
        );
        h.insert(
            HeaderName::from_static("x-fish-amp-session-id"),
            val(&config.session_id)?,
        );
        Ok(h)
    }

    /// Refresh the time-sensitive reCAPTCHA token in place.
    ///
    /// Cheap and non-blocking for callers of [`Self::synthesize`]: it only swaps
    /// the guarded `String`, leaving the HTTP client and its connection pool
    /// untouched.
    pub fn set_recaptcha(&self, token: impl Into<String>) {
        *self.recaptcha.lock().expect("recaptcha mutex poisoned") = token.into();
    }

    /// Synthesize speech for `text` and return the raw `mp3` bytes.
    ///
    /// `text` is the only dynamic per-call input; every other field is drawn from
    /// the static config plus the current reCAPTCHA token.
    pub async fn synthesize(&self, text: impl Into<String>) -> Result<Vec<u8>, ApiError> {
        let recaptcha = self
            .recaptcha
            .lock()
            .expect("recaptcha mutex poisoned")
            .clone();

        let request = TtsRequest {
            r#type: "tts",
            stream: true,
            model: self.model_id.clone(),
            latency: self.latency.clone(),
            parameters: TtsParameters {
                text: text.into(),
                model_id: self.model_id.clone(),
                format: self.format.clone(),
                latency: self.latency.clone(),
                backend: self.backend.clone(),
                normalize: false,
            },
            recaptcha,
            format: self.format.clone(),
            backend: self.backend.clone(),
            group_id: self.group_id.clone(),
        };

        self.http.post_json_bytes(TASK_PATH, &request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> TtsConfig {
        TtsConfig {
            auth_token: "token".into(),
            team_id: "team".into(),
            workspace_id: "ws".into(),
            device_id: "dev".into(),
            session_id: "sess".into(),
            model_id: "model123".into(),
            latency: "balanced".into(),
            format: "mp3".into(),
            backend: "s2-pro".into(),
            group_id: "group123".into(),
            recaptcha: "initial".into(),
        }
    }

    #[test]
    fn builds_without_panicking() {
        assert!(FishAudioTtsService::new(sample()).is_ok());
    }

    #[test]
    fn recaptcha_is_refreshable() {
        let svc = FishAudioTtsService::new(sample()).unwrap();
        assert_eq!(*svc.recaptcha.lock().unwrap(), "initial");
        svc.set_recaptcha("refreshed");
        assert_eq!(*svc.recaptcha.lock().unwrap(), "refreshed");
    }

    #[test]
    fn request_payload_serializes_with_type_key() {
        // Guards the `r#type` → `"type"` rename and overall JSON shape.
        let req = TtsRequest {
            r#type: "tts",
            stream: true,
            model: "m".into(),
            latency: "balanced".into(),
            parameters: TtsParameters {
                text: "hi".into(),
                model_id: "m".into(),
                format: "mp3".into(),
                latency: "balanced".into(),
                backend: "s2-pro".into(),
                normalize: false,
            },
            recaptcha: "r".into(),
            format: "mp3".into(),
            backend: "s2-pro".into(),
            group_id: "g".into(),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["type"], "tts");
        assert_eq!(json["parameters"]["text"], "hi");
        assert_eq!(json["parameters"]["normalize"], false);
        assert_eq!(json["group_id"], "g");
    }
}
