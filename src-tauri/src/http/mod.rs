//! HTTP infrastructure layer.
//!
//! A small, API-agnostic foundation that the service layer (see
//! [`crate::chat`]) builds on. Nothing in here knows about any specific
//! endpoint — it only knows how to send requests, apply global configuration,
//! intercept responses, and report errors.

pub mod client;
pub mod error;

pub use client::{HttpClient, HttpClientConfig};
pub use error::ApiError;
