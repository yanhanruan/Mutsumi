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
use std::sync::{Arc, Mutex, OnceLock};
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

/// A lightweight entry in the source switcher's session list.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct SessionInfo {
    /// SourceAppUserModelId — stable per app; used to pin/select the source.
    pub id:      String,
    pub title:   String,
    pub artist:  String,
    pub playing: bool,
}

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
    /// System render-endpoint master volume, 0.0–1.0.
    pub volume:      f32,
    /// SourceAppUserModelId of the session currently shown / controlled.
    pub app_id:      String,
    /// All current media sessions, for the source switcher.
    pub sessions:    Vec<SessionInfo>,
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

/// Seek relative to the current position by `delta_ms` (negative = backward),
/// clamped to the track's [start, end]. Used by the ±10 s skip buttons.
#[tauri::command]
pub fn media_skip(delta_ms: i64) -> Result<(), String> {
    init_mta();
    let session = current_session().map_err(|_| "no active media session".to_string())?;
    let tl = session.GetTimelineProperties().map_err(|e| e.message())?;
    // Raw timeline ticks (100-ns); positions are absolute, not start-normalized.
    let pos   = tl.Position().map(|t| t.Duration).unwrap_or(0);
    let start = tl.StartTime().map(|t| t.Duration).unwrap_or(0);
    let end   = tl.EndTime().map(|t| t.Duration).unwrap_or(0);
    let target = (pos + delta_ms * 10_000).clamp(start, end.max(start));
    let ok = session
        .TryChangePlaybackPositionAsync(target)
        .map_err(|e| e.message())?
        .get()
        .map_err(|e| e.message())?;
    if ok { Ok(()) } else { Err("seek was rejected".into()) }
}

/// Seek to an absolute position, `position_ms` measured from the track start
/// (i.e. start-normalized, matching the snapshot's `position_ms`). Used by the
/// click/drag-to-seek progress bar.
#[tauri::command]
pub fn media_seek(position_ms: i64) -> Result<(), String> {
    init_mta();
    let session = current_session().map_err(|_| "no active media session".to_string())?;
    let tl = session.GetTimelineProperties().map_err(|e| e.message())?;
    let start = tl.StartTime().map(|t| t.Duration).unwrap_or(0);
    let end   = tl.EndTime().map(|t| t.Duration).unwrap_or(0);
    // Re-base onto the raw timeline and clamp to the track bounds.
    let target = (start + position_ms.max(0) * 10_000).clamp(start, end.max(start));
    // Browser media (Chrome, Edge, …) is reachable only through SMTC, which
    // routes the seek across an IPC chain into the page. While it propagates,
    // the browser's audio output keeps draining the PCM it had already buffered
    // ahead of the playhead, so the pre-seek audio replays for a beat before it
    // catches up. A page's own seekbar avoids this because it sets `currentTime`
    // in-renderer and flushes those buffers synchronously — which we can't do
    // from outside. We mimic that flush with the only controls SMTC gives us:
    // pause the output, seek, then resume. Local players (PotPlayer, …) seek
    // instantly and cleanly, so we skip this for them (it would only add a blip).
    let is_browser = {
        let a = session_id(&session).to_lowercase();
        ["chrome", "msedge", "edge", "firefox", "brave", "opera", "vivaldi"]
            .iter()
            .any(|b| a.contains(b))
    };
    let was_playing = session
        .GetPlaybackInfo()
        .and_then(|pb| pb.PlaybackStatus())
        .map(|s| s == PlaybackStatus::Playing)
        .unwrap_or(false);
    let flush = is_browser && was_playing;

    if flush {
        let _ = session.TryPauseAsync().and_then(|op| op.get());
    }
    let ok = session
        .TryChangePlaybackPositionAsync(target)
        .map_err(|e| e.message())?
        .get()
        .map_err(|e| e.message())?;
    if flush {
        // Give the seek time to land before resuming; otherwise the play command
        // races the still-in-flight seek and the browser drops it (stays paused).
        std::thread::sleep(Duration::from_millis(250));
        let _ = session.TryPlayAsync().and_then(|op| op.get());
    }
    if ok { Ok(()) } else { Err("seek was rejected".into()) }
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

/// Pin a source (by SourceAppUserModelId) for display + control; an empty string
/// clears the pin and returns to auto-follow. A pinned source is kept even while
/// paused, so the user can resume it instead of the controller jumping to
/// whatever else is currently playing.
#[tauri::command]
pub fn media_select(app_id: String) {
    *pinned().lock().unwrap() = if app_id.is_empty() { None } else { Some(app_id) };
}

/// Set the system master volume (0.0–1.0).
#[tauri::command]
pub fn media_set_volume(level: f32) -> Result<(), String> {
    init_mta();
    let l = level.clamp(0.0, 1.0);
    unsafe {
        let vol = endpoint_volume().map_err(|e| e.message())?;
        vol.SetMasterVolumeLevelScalar(l, std::ptr::null())
            .map_err(|e| e.message())?;
    }
    Ok(())
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

/// User-pinned source (SourceAppUserModelId), or None for auto-follow.
fn pinned() -> &'static Mutex<Option<String>> {
    static PINNED: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    PINNED.get_or_init(|| Mutex::new(None))
}

/// Auto-follow target (SourceAppUserModelId): the source that most recently
/// transitioned into Playing. It stays put when that source merely pauses, so it
/// can be resumed without the controller jumping to another playing source.
fn auto_target() -> &'static Mutex<Option<String>> {
    static AUTO: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    AUTO.get_or_init(|| Mutex::new(None))
}

/// Source ids that were playing on the previous poll, for play-start transition
/// detection. Maintained by the poller via `update_auto_target`.
fn prev_playing() -> &'static Mutex<std::collections::HashSet<String>> {
    static PREV: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    PREV.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

/// SourceAppUserModelId of a session (stable per app), or "" if unavailable.
fn session_id(s: &Session) -> String {
    s.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default()
}

/// (priority, last-updated ticks) — a higher tuple wins. Playing > Changing >
/// other; ties broken by the most recently advanced timeline (the audible one).
fn session_score(s: &Session) -> (i32, i64) {
    let pri = match s.GetPlaybackInfo().and_then(|pb| pb.PlaybackStatus()) {
        Ok(PlaybackStatus::Playing) => 2,
        Ok(PlaybackStatus::Changing) => 1,
        _ => 0,
    };
    let updated = s
        .GetTimelineProperties()
        .and_then(|tl| tl.LastUpdatedTime())
        .map(|dt| dt.UniversalTime)
        .unwrap_or(0);
    (pri, updated)
}

/// Collect every current SMTC session.
fn all_sessions(mgr: &SessionManager) -> Vec<Session> {
    let mut items = Vec::new();
    if let Ok(list) = mgr.GetSessions() {
        let size = list.Size().unwrap_or(0);
        for i in 0..size {
            if let Ok(s) = list.GetAt(i) {
                items.push(s);
            }
        }
    }
    items
}

/// Update the auto-follow target from play-start transitions, once per poll. A
/// source that just began playing becomes the focus; a source merely pausing
/// keeps the focus (so it can be resumed) as long as its session still exists.
fn update_auto_target(sessions: &[SessionInfo]) {
    use std::collections::HashSet;
    let current: HashSet<String> = sessions
        .iter()
        .filter(|s| s.playing && !s.id.is_empty())
        .map(|s| s.id.clone())
        .collect();

    let mut prev = prev_playing().lock().unwrap();
    let mut target = auto_target().lock().unwrap();

    if let Some(started) = current.difference(&prev).next() {
        // A source just started playing — follow it.
        *target = Some(started.clone());
    } else {
        // No new start: keep the current target while its session exists,
        // otherwise adopt any currently-playing source (else leave None so the
        // score fallback in choose_session decides).
        let alive = target
            .as_ref()
            .map_or(false, |id| sessions.iter().any(|s| &s.id == id));
        if !alive {
            *target = current.iter().next().cloned();
        }
    }
    *prev = current;
}

/// Choose which session to display / control:
///   1. the user-pinned source, while it still exists — kept even when paused, so
///      it can be resumed instead of the controller jumping to another source;
///   2. else the sticky auto-follow target (the most recently started source),
///      kept while it exists so a just-paused source stays focused;
///   3. else the highest-scoring session.
/// A pinned id that no longer matches any session is cleared (revert to auto).
fn choose_session(items: &[Session]) -> Option<Session> {
    if items.is_empty() {
        return None;
    }
    let pin = pinned().lock().unwrap().clone();
    if let Some(pin_id) = pin {
        if let Some(s) = items.iter().find(|s| session_id(s) == pin_id) {
            return Some(s.clone());
        }
        *pinned().lock().unwrap() = None; // pinned source vanished → auto
    }
    // 2. Auto-follow target: the source most recently started playing, kept even
    //    when paused, so a just-paused source stays focused and can be resumed.
    let auto = auto_target().lock().unwrap().clone();
    if let Some(auto_id) = auto {
        if let Some(s) = items.iter().find(|s| session_id(s) == auto_id) {
            return Some(s.clone());
        }
    }
    // 3. Fallback: the highest-scoring session.
    let mut best: Option<(&Session, (i32, i64))> = None;
    for s in items {
        let sc = session_score(s);
        if best.as_ref().map_or(true, |(_, b)| sc > *b) {
            best = Some((s, sc));
        }
    }
    best.map(|(s, _)| s.clone())
}

/// The session targeted by transport controls — the same one the UI displays.
fn current_session() -> windows::core::Result<Session> {
    let mgr = SessionManager::RequestAsync()?.get()?;
    if let Some(s) = choose_session(&all_sessions(&mgr)) {
        return Ok(s);
    }
    mgr.GetCurrentSession()
}

/// Default render endpoint's volume control (for mute).
unsafe fn endpoint_volume() -> windows::core::Result<IAudioEndpointVolume> {
    let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    let device: IMMDevice = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
    device.Activate(CLSCTX_ALL, None)
}

/// Read the system render-endpoint (muted, volume 0.0–1.0); defaults if unavailable.
fn read_endpoint() -> (bool, f32) {
    unsafe {
        match endpoint_volume() {
            Ok(v) => {
                let muted  = v.GetMute().map(|b| b.as_bool()).unwrap_or(false);
                let volume = v.GetMasterVolumeLevelScalar().unwrap_or(0.0);
                (muted, volume)
            }
            Err(_) => (false, 0.0),
        }
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

/// Read a full snapshot: the chosen session's now-playing state plus the list of
/// all sessions for the source switcher. Returns a default (inactive) snapshot
/// when no session manager is available.
fn read_snapshot() -> MediaSnapshot {
    let mut snap = MediaSnapshot::default();
    let (muted, volume) = read_endpoint();
    snap.muted  = muted;
    snap.volume = volume;

    let mgr = match SessionManager::RequestAsync().and_then(|op| op.get()) {
        Ok(m) => m,
        Err(_) => return snap,
    };
    let items = all_sessions(&mgr);

    // Lightweight list for the source switcher (title/artist/playing per source).
    for s in &items {
        let mut info = SessionInfo { id: session_id(s), ..Default::default() };
        if let Ok(props) = s.TryGetMediaPropertiesAsync().and_then(|op| op.get()) {
            info.title  = props.Title().map(|h| h.to_string()).unwrap_or_default();
            info.artist = props.Artist().map(|h| h.to_string()).unwrap_or_default();
        }
        info.playing = matches!(
            s.GetPlaybackInfo().and_then(|pb| pb.PlaybackStatus()),
            Ok(PlaybackStatus::Playing)
        );
        snap.sessions.push(info);
    }

    // Refresh the sticky auto-follow target from this poll's play-start
    // transitions BEFORE choosing, so the choice reflects it with no lag.
    update_auto_target(&snap.sessions);

    // The session we actually display + control (honors the pin, then the
    // sticky auto-target, then the score fallback).
    let session = match choose_session(&items).or_else(|| mgr.GetCurrentSession().ok()) {
        Some(s) => s,
        None => return snap,
    };
    snap.app_id = session_id(&session);

    if let Ok(props) = session.TryGetMediaPropertiesAsync().and_then(|op| op.get()) {
        snap.title  = props.Title().map(|h| h.to_string()).unwrap_or_default();
        snap.artist = props.Artist().map(|h| h.to_string()).unwrap_or_default();
    }
    if let Ok(pb) = session.GetPlaybackInfo() {
        snap.status = status_str(pb.PlaybackStatus().unwrap_or(PlaybackStatus::Closed)).to_string();
        if let Ok(c) = pb.Controls() {
            snap.can_next  = c.IsNextEnabled().unwrap_or(false);
            snap.can_prev  = c.IsPreviousEnabled().unwrap_or(false);
            snap.can_play  = c.IsPlayEnabled().unwrap_or(false);
            snap.can_pause = c.IsPauseEnabled().unwrap_or(false);
        }
    }
    if let Ok(tl) = session.GetTimelineProperties() {
        let start = tl.StartTime().map(ts_ms).unwrap_or(0);
        let end   = tl.EndTime().map(ts_ms).unwrap_or(0);
        let pos   = tl.Position().map(ts_ms).unwrap_or(0);
        snap.duration_ms = (end - start).max(0);
        snap.position_ms = (pos - start).clamp(0, snap.duration_ms);
    }
    snap.active = matches!(snap.status.as_str(), "playing" | "paused" | "changing");
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
