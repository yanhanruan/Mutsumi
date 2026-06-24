//! Generic HTTP client wrapper (infrastructure layer).
//!
//! [`HttpClient`] owns a single [`reqwest::Client`] — which is internally an
//! `Arc` over a connection pool, so it is cheap to share and **must** be reused
//! across requests rather than rebuilt per call. Global concerns live here once:
//!
//!   * **base URL**     — prefixed onto every relative path
//!   * **timeout**      — applied to all requests
//!   * **default headers** — auth token, content-type, UA, … sent on every call
//!   * **response interception** — a single chokepoint ([`Self::intercept`])
//!     that turns any non-2xx response into [`ApiError::Api`], so no service-layer
//!     method has to remember to check status codes.
//!
//! The client is deliberately ignorant of any specific API: it only knows how to
//! send a JSON body and hand back either a typed JSON value or the raw bytes.

use std::time::Duration;

use reqwest::header::HeaderMap;
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::Serialize;

use super::error::ApiError;

/// Tunables captured once when the client is created.
pub struct HttpClientConfig {
    /// Scheme + host (+ optional prefix), e.g. `https://api.fish.audio`.
    /// Joined with the per-request `path` via simple concatenation, so `path`
    /// should start with `/`.
    pub base_url: String,
    /// Applied to every request (connect + read).
    pub timeout: Duration,
    /// Sent on every request. Per-request headers can still be layered on top.
    pub default_headers: HeaderMap,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            timeout: Duration::from_secs(30),
            default_headers: HeaderMap::new(),
        }
    }
}

/// Thin, API-agnostic wrapper over [`reqwest::Client`].
#[derive(Clone)]
pub struct HttpClient {
    inner: reqwest::Client,
    base_url: String,
}

impl HttpClient {
    /// Build the client from global config. Fails only if the underlying
    /// `reqwest` builder rejects the configuration (e.g. a malformed default
    /// header), surfaced as [`ApiError::Network`].
    pub fn new(config: HttpClientConfig) -> Result<Self, ApiError> {
        let inner = reqwest::Client::builder()
            .timeout(config.timeout)
            .default_headers(config.default_headers)
            .build()?;
        Ok(Self {
            inner,
            base_url: config.base_url,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// POST a JSON body and deserialize a JSON response into `Res`.
    ///
    /// Generic over both the request (`Req: Serialize`) and the response
    /// (`Res: DeserializeOwned`) so any endpoint can reuse it. The TTS route
    /// returns audio bytes (see [`Self::post_json_bytes`]); this method exists as
    /// the generic JSON path for any future endpoint added on this layer.
    #[allow(dead_code)]
    pub async fn post_json<Req, Res>(&self, path: &str, body: &Req) -> Result<Res, ApiError>
    where
        Req: Serialize + ?Sized,
        Res: DeserializeOwned,
    {
        let resp = self.inner.post(self.url(path)).json(body).send().await?;
        let resp = Self::intercept(resp).await?;
        let bytes = resp.bytes().await?;
        serde_json::from_slice::<Res>(&bytes).map_err(|e| ApiError::Decode(e.to_string()))
    }

    /// POST a JSON body and return the raw response bytes.
    ///
    /// Used for endpoints whose response is **not** JSON — e.g. Fish Audio's TTS
    /// route streams back an audio (`mp3`) byte stream. `reqwest` transparently
    /// reassembles the chunked/streamed transfer; we collect it into one buffer.
    pub async fn post_json_bytes<Req>(&self, path: &str, body: &Req) -> Result<Vec<u8>, ApiError>
    where
        Req: Serialize + ?Sized,
    {
        let resp = self.inner.post(self.url(path)).json(body).send().await?;
        let resp = Self::intercept(resp).await?;
        let bytes = resp.bytes().await?;
        Ok(bytes.to_vec())
    }

    /// POST a JSON body and return the intercepted streaming response for the
    /// caller to consume incrementally (e.g. via `Response::chunk`).
    ///
    /// Status interception runs *before* returning, so a non-2xx is surfaced as
    /// [`ApiError::Api`] up front. Provider-specific framing (Server-Sent Events,
    /// chunked JSON, …) is the caller's concern — e.g. Qwen's SSE parsing lives
    /// in the service layer.
    pub async fn post_json_streaming<Req>(
        &self,
        path: &str,
        body: &Req,
    ) -> Result<reqwest::Response, ApiError>
    where
        Req: Serialize + ?Sized,
    {
        let resp = self.inner.post(self.url(path)).json(body).send().await?;
        Self::intercept(resp).await
    }

    /// Unified response interceptor: pass 2xx through untouched, convert
    /// everything else into [`ApiError::Api`] with the body attached for context.
    /// Centralizing this means individual service calls never repeat status checks.
    async fn intercept(resp: reqwest::Response) -> Result<reqwest::Response, ApiError> {
        let status: StatusCode = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        // Best-effort body capture for diagnostics; ignore decode failures here.
        let body = resp.text().await.unwrap_or_default();
        Err(ApiError::Api {
            status: status.as_u16(),
            body,
        })
    }
}
