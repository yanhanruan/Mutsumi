//! macOS audio-activity adapter.
//!
//! Phase 1 deliberately exposes the same command contract as the Windows
//! WASAPI backend while reporting an inactive state. The frontend can use
//! `get_platform_capabilities` to distinguish this explicit platform gap from
//! ordinary silence. A public-API CoreAudio implementation will replace this
//! adapter after the permission/OS-version spike is complete.

use std::sync::atomic::{AtomicBool, Ordering};

pub struct AudioState(pub AtomicBool);

#[tauri::command]
pub fn get_audio_state(state: tauri::State<'_, AudioState>) -> bool {
    state.0.load(Ordering::Relaxed)
}
