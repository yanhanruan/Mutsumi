//! macOS media adapter for the Phase-1 compile baseline.
//!
//! macOS has no public API that is a drop-in equivalent of Windows SMTC. Keep
//! the frontend command contract available, seed it with an inactive snapshot,
//! and reject controls explicitly until the public-API media spike selects an
//! implementation. Capability-aware UI will hide these controls on macOS.

use std::sync::Mutex;

const UNAVAILABLE: &str = "media controls are not yet available on macOS";

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub playing: bool,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct MediaSnapshot {
    pub active: bool,
    pub title: String,
    pub artist: String,
    pub status: String,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub can_next: bool,
    pub can_prev: bool,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_seek: bool,
    pub muted: bool,
    pub volume: f32,
    pub app_id: String,
    pub sessions: Vec<SessionInfo>,
}

pub struct MediaState(pub Mutex<MediaSnapshot>);

impl MediaState {
    pub fn new() -> Self {
        Self(Mutex::new(MediaSnapshot::default()))
    }
}

#[tauri::command]
pub fn get_media(state: tauri::State<'_, MediaState>) -> MediaSnapshot {
    state.0.lock().unwrap().clone()
}

fn unavailable<T>() -> Result<T, String> {
    Err(UNAVAILABLE.into())
}

#[tauri::command]
pub fn media_play_pause() -> Result<(), String> {
    unavailable()
}

#[tauri::command]
pub fn media_next() -> Result<(), String> {
    unavailable()
}

#[tauri::command]
pub fn media_prev() -> Result<(), String> {
    unavailable()
}

#[tauri::command]
pub fn media_stop() -> Result<(), String> {
    unavailable()
}

#[tauri::command]
pub fn media_replay() -> Result<(), String> {
    unavailable()
}

#[tauri::command]
pub fn media_skip(_delta_ms: i64) -> Result<(), String> {
    unavailable()
}

#[tauri::command]
pub fn media_seek(_position_ms: i64) -> Result<(), String> {
    unavailable()
}

#[tauri::command]
pub fn media_toggle_mute() -> Result<bool, String> {
    unavailable()
}

#[tauri::command]
pub fn media_select(_app_id: String) -> Result<(), String> {
    unavailable()
}

#[tauri::command]
pub fn media_set_volume(_level: f32) -> Result<(), String> {
    unavailable()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inactive_snapshot_matches_the_frontend_contract() {
        let snapshot = MediaSnapshot::default();
        assert!(!snapshot.active);
        assert!(snapshot.sessions.is_empty());
        assert!(!snapshot.can_play);
        assert!(!snapshot.can_seek);
    }

    #[test]
    fn controls_report_an_explicit_platform_error() {
        assert_eq!(media_play_pause().unwrap_err(), UNAVAILABLE);
        assert_eq!(media_seek(1_000).unwrap_err(), UNAVAILABLE);
    }
}
