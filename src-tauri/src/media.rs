//! Mini music controller backend — Windows System Media Transport Controls (SMTC).
//!
//! SMTC (`GlobalSystemMediaTransportControlsSessionManager`, the API behind the
//! Windows media flyout) exposes the *current* media session for any app that
//! integrates with it — Spotify, Chrome/Edge browser media, Groove, 网易云音乐,
//! etc. We read now-playing metadata + timeline and forward transport controls
//! (play/pause, next, previous, stop), so the pet can control whatever the user
//! is actually playing without per-app integrations.
//!
//! A background thread polls the current session ~1 Hz, mirrors the latest
//! snapshot into managed `MediaState`, and emits `media-update` on change. The
//! frontend interpolates the progress bar between updates from its own clock.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use windows::Foundation::TimeSpan;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    eMultimedia, eRender, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};

const POLL_INTERVAL: Duration = Duration::from_millis(1000);

// ── Snapshot + managed state ───────────────────────────────────────

/// A point-in-time view of the current media session, sent to the frontend.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct MediaSnapshot {
    /// True when a session is loaded (playing / paused / changing).
    pub active:      bool,
    pub title:       String,
    pub artist:      String,
    /// "playing" | "paused" | "stopped" | "changing" | "opened" | "closed" | "unknown"
    pub status:      String,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub can_next:    bool,
    pub can_prev:    bool,
    pub can_play:    bool,
    pub can_pause:   bool,
    /// System render-endpoint mute state (not part of SMTC — see endpoint volume).
    pub muted:       bool,
}

pub struct MediaState(pub Mutex<MediaSnapshot>);

impl MediaState {
    pub fn new() -> Self {
        Self(Mutex::new(MediaSnapshot::default()))
    }
}

/// Tauri command: latest cached snapshot (so the UI seeds instantly on mount,
/// even if the first `media-update` fired before the listener registered).
#[tauri::command]
pub fn get_media(state: tauri::State<MediaState>) -> MediaSnapshot {
    state.0.lock().unwrap().clone()
}

// ── Transport control commands ─────────────────────────────────────

#[tauri::command]
pub fn media_play_pause() -> Result<(), String> {
    control(|s| s.TryTogglePlayPauseAsync()?.get())
}
#[tauri::command]
pub fn media_next() -> Result<(), String> {
    control(|s| s.TrySkipNextAsync()?.get())
}
#[tauri::command]
pub fn media_prev() -> Result<(), String> {
    control(|s| s.TrySkipPreviousAsync()?.get())
}
#[tauri::command]
pub fn media_stop() -> Result<(), String> {
    control(|s| s.TryStopAsync()?.get())
}
/// Restart the current track (seek to position 0).
#[tauri::command]
pub fn media_replay() -> Result<(), String> {
    control(|s| s.TryChangePlaybackPositionAsync(0)?.get())
}

/// Toggle the system render endpoint mute; returns the new muted state.
#[tauri::command]
pub fn media_toggle_mute() -> Result<bool, String> {
    init_mta();
    unsafe {
        let vol = endpoint_volume().map_err(|e| e.message())?;
        let now = !vol.GetMute().map_err(|e| e.message())?.as_bool();
        vol.SetMute(now, std::ptr::null()).map_err(|e| e.message())?;
        Ok(now)
    }
}

/// Run a transport control against the current session (the closure issues the
/// `Try*Async` call and awaits it via `.get()`, returning whether it succeeded).
fn control<F>(f: F) -> Result<(), String>
where
    F: FnOnce(&Session) -> windows::core::Result<bool>,
{
    init_mta();
    let session = current_session().map_err(|_| "no active media session".to_string())?;
    let ok = f(&session).map_err(|e| e.message())?;
    if ok { Ok(()) } else { Err("media control was rejected".into()) }
}

// ── SMTC reads ─────────────────────────────────────────────────────

fn current_session() -> windows::core::Result<Session> {
    let mgr = SessionManager::RequestAsync()?.get()?;
    mgr.GetCurrentSession()
}

/// Default render endpoint's volume control (for mute).
unsafe fn endpoint_volume() -> windows::core::Result<IAudioEndpointVolume> {
    let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    let device: IMMDevice = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
    device.Activate(CLSCTX_ALL, None)
}

/// Read the system render-endpoint mute state (false if unavailable).
fn read_muted() -> bool {
    unsafe {
        endpoint_volume()
            .and_then(|v| v.GetMute())
            .map(|b| b.as_bool())
            .unwrap_or(false)
    }
}

/// TimeSpan is in 100-ns ticks → milliseconds.
fn ts_ms(t: TimeSpan) -> i64 {
    t.Duration / 10_000
}

fn status_str(s: PlaybackStatus) -> &'static str {
    if s == PlaybackStatus::Playing { "playing" }
    else if s == PlaybackStatus::Paused { "paused" }
    else if s == PlaybackStatus::Stopped { "stopped" }
    else if s == PlaybackStatus::Changing { "changing" }
    else if s == PlaybackStatus::Opened { "opened" }
    else if s == PlaybackStatus::Closed { "closed" }
    else { "unknown" }
}

/// Read a full snapshot of the current session, or a default (inactive) one.
fn read_snapshot() -> MediaSnapshot {
    let session = match current_session() {
        Ok(s) => s,
        Err(_) => return MediaSnapshot::default(),
    };
    let mut snap = MediaSnapshot::default();

    // Metadata (async).
    if let Ok(props) = session.TryGetMediaPropertiesAsync().and_then(|op| op.get()) {
        snap.title  = props.Title().map(|h| h.to_string()).unwrap_or_default();
        snap.artist = props.Artist().map(|h| h.to_string()).unwrap_or_default();
    }

    // Playback status + available controls.
    if let Ok(pb) = session.GetPlaybackInfo() {
        snap.status = status_str(pb.PlaybackStatus().unwrap_or(PlaybackStatus::Closed)).to_string();
        if let Ok(c) = pb.Controls() {
            snap.can_next  = c.IsNextEnabled().unwrap_or(false);
            snap.can_prev  = c.IsPreviousEnabled().unwrap_or(false);
            snap.can_play  = c.IsPlayEnabled().unwrap_or(false);
            snap.can_pause = c.IsPauseEnabled().unwrap_or(false);
        }
    }

    // Timeline → position / duration (normalized so position starts at 0).
    if let Ok(tl) = session.GetTimelineProperties() {
        let start = tl.StartTime().map(ts_ms).unwrap_or(0);
        let end   = tl.EndTime().map(ts_ms).unwrap_or(0);
        let pos   = tl.Position().map(ts_ms).unwrap_or(0);
        snap.duration_ms = (end - start).max(0);
        snap.position_ms = (pos - start).clamp(0, snap.duration_ms);
    }

    snap.active = matches!(snap.status.as_str(), "playing" | "paused" | "changing");
    snap.muted  = read_muted();
    snap
}

// ── Apartment init ─────────────────────────────────────────────────

fn init_mta() {
    // WinRT async `.get()` needs the calling thread in an apartment; MTA lets
    // us block on the operation. Harmless if already initialized.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

// ── Background poller ──────────────────────────────────────────────

pub fn spawn(app: AppHandle, stop_flag: Arc<AtomicBool>) {
    thread::spawn(move || {
        init_mta();
        let mut last: Option<MediaSnapshot> = None;

        while !stop_flag.load(Ordering::Relaxed) {
            let snap = read_snapshot();

            if let Some(s) = app.try_state::<MediaState>() {
                *s.0.lock().unwrap() = snap.clone();
            }
            // Emit only on change (position advances each second while playing,
            // so this naturally re-syncs ~1 Hz during playback and stays quiet
            // when paused/idle).
            if last.as_ref() != Some(&snap) {
                let _ = app.emit("media-update", &snap);
                last = Some(snap);
            }

            thread::sleep(POLL_INTERVAL);
        }

        unsafe { CoUninitialize() };
    });
}
