//! Unified error type for the HTTP infrastructure layer.
//!
//! Every fallible operation in [`crate::http`] and the service layer built on
//! top of it returns [`ApiError`]. It distinguishes the three failure classes
//! callers actually care about:
//!
//!   * transport / connection problems (DNS, TLS, timeout)  → [`ApiError::Network`]
//!   * the server answered, but with a non-2xx status        → [`ApiError::Api`]
//!   * the body could not be decoded into the expected type  → [`ApiError::Decode`]
//!
//! It implements [`serde::Serialize`] (as its `Display` string) so it can be
//! returned straight out of a `#[tauri::command]`'s `Err` arm.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    /// The request never produced a usable HTTP response (DNS failure, TLS
    /// handshake error, connection refused, timeout, …).
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    /// The server responded, but with a non-success status code. The raw body
    /// is captured so the caller can surface the provider's error message.
    #[error("API error {status}: {body}")]
    Api { status: u16, body: String },

    /// The response body could not be deserialized into the expected type.
    /// Produced by the generic JSON path ([`crate::http::HttpClient::post_json`]).
    #[allow(dead_code)]
    #[error("failed to decode response: {0}")]
    Decode(String),

    /// A request could not be constructed (e.g. an invalid header value).
    #[error("failed to build request: {0}")]
    Build(String),
}

/// Serialize as the human-readable message so the frontend receives a plain
/// string in the rejected-promise path of a Tauri `invoke`.
impl serde::Serialize for ApiError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
