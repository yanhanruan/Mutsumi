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
use std::collections::HashSet;
use std::thread;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use windows::Foundation::TimeSpan;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
    GlobalSystemMediaTransportControlsSessionTimelineProperties as TimelineProperties,
};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    eMultimedia, eRender, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
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
    /// True when the app honors seek/position changes (the progress bar, ±10 s
    /// skip and replay). Some apps (notably 网易云音乐) integrate play/pause/next
    /// but bind no position handler, so position changes are silently rejected;
    /// the UI greys those controls out when this is false.
    pub can_seek:    bool,
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
    let session = current_session().ok_or_else(|| "no active media session".to_string())?;
    let tl = session.GetTimelineProperties().map_err(|e| e.message())?;
    let playing = session
        .GetPlaybackInfo()
        .and_then(|pb| pb.PlaybackStatus())
        .map(|s| s == PlaybackStatus::Playing)
        .unwrap_or(false);
    // Raw timeline ticks (100-ns); positions are absolute, not start-normalized.
    // Position is a snapshot at LastUpdatedTime, so advance it to now while
    // playing — otherwise a ±10 s skip is relative to a stale base and lands in
    // the wrong place.
    let pos   = live_position_ticks(&tl, playing);
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
    let session = current_session().ok_or_else(|| "no active media session".to_string())?;
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
    let session = current_session().ok_or_else(|| "no active media session".to_string())?;
    let ok = f(&session).map_err(|e| e.message())?;
    if ok { Ok(()) } else { Err("media control was rejected".into()) }
}

// ── SMTC reads ─────────────────────────────────────────────────────

// ── Selection state ────────────────────────────────────────────────
//
// One source of truth for "which session do we show + control". It is written
// ONLY by the poller (`drive_selection`); transport commands are pure readers
// (`current_session` just looks up `selected.id` in the live list). That single
// writer is what keeps the behavior predictable — no two code paths racing to
// reassign the source.

/// How long to keep showing the current source after its SMTC session drops out
/// of the list, before concluding it's really gone and switching away. Covers
/// the gap when an app tears down + recreates its session across a prev/next
/// track change. Browsers re-register quickly, so this is about session churn,
/// not audio buffering — it need not scale with network speed. It also bounds
/// how long after closing a source the panel takes to reveal the next one.
const GAP_DEBOUNCE: Duration = Duration::from_millis(2000);

/// User-pinned source (SourceAppUserModelId), or None to auto-select. Set by the
/// `media_select` command; read by `drive_selection`.
fn pinned() -> &'static Mutex<Option<String>> {
    static PINNED: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    PINNED.get_or_init(|| Mutex::new(None))
}

/// Ids that were Playing on the previous poll, to detect play-start transitions.
fn prev_playing() -> &'static Mutex<HashSet<String>> {
    static PREV: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    PREV.get_or_init(|| Mutex::new(HashSet::new()))
}

/// The session we present + control, and when it last went missing from the list
/// (`None` while present). Only `drive_selection` mutates this.
#[derive(Default)]
struct Selected {
    id: Option<String>,
    missing_since: Option<Instant>,
}
fn selected() -> &'static Mutex<Selected> {
    static SELECTED: OnceLock<Mutex<Selected>> = OnceLock::new();
    SELECTED.get_or_init(|| Mutex::new(Selected::default()))
}

/// The outcome of one selection step.
#[derive(Debug, PartialEq, Eq)]
enum Decision {
    /// Present + control this source (guaranteed to be in the live list).
    Show(String),
    /// The selected source is briefly absent (a track-change gap) — freeze the
    /// panel on the last good snapshot rather than switch or blank.
    Hold,
    /// Nothing to show.
    Inactive,
}

/// Pure selection core. Decides the next source from the current world only —
/// see the numbered rules. `newly_started` is a present source that transitioned
/// into Playing this poll (if any); `selected_alive` is whether the (absent)
/// selection's process is still running; `best_available` is the highest-scoring
/// present source (the replacement when the selection is really gone). Keeping
/// this free of WinRT + clocks makes the whole policy unit-testable.
fn decide(
    present: &HashSet<String>,
    newly_started: Option<&String>,
    pin: &Option<String>,
    selected: &Option<String>,
    selected_alive: Option<bool>,
    missing_elapsed: Option<Duration>,
    debounce: Duration,
    best_available: Option<&String>,
) -> Decision {
    // 1. An explicit pin wins while its source exists.
    if let Some(p) = pin {
        if present.contains(p) {
            return Decision::Show(p.clone());
        }
    }
    // 2. A source that just started playing grabs focus (follow real intent).
    if let Some(started) = newly_started {
        return Decision::Show(started.clone());
    }
    // 3. Stick with the current source while it's present (don't chase others
    //    around just because they're playing, and don't drop it when it pauses).
    if let Some(cur) = selected {
        if present.contains(cur) {
            return Decision::Show(cur.clone());
        }
        // 4. It's gone from the list. Decide whether this is a transient
        //    track-change teardown (hold) or the source really closing (switch),
        //    using process liveness rather than a clock — a slow track load can
        //    outlast any fixed window, but the app's process stays alive across
        //    it and dies on a real close:
        let held = missing_elapsed.unwrap_or_default();
        match selected_alive {
            // App still running → track-change gap; hold until its session
            // returns (no time limit), bounded only by the MAX_HOLD safety cap.
            Some(true) if held < MAX_HOLD => return Decision::Hold,
            // App gone → it really closed; switch away now (rule 5).
            Some(false) => {}
            // Liveness unknown (non-exe id / snapshot failed) → short debounce.
            None if held < debounce => return Decision::Hold,
            _ => {}
        }
    }
    // 5. Adopt the best remaining source (reveals B when A closes), else nothing.
    match best_available {
        Some(id) => Decision::Show(id.clone()),
        None => Decision::Inactive,
    }
}

/// SourceAppUserModelId of a session (stable per app), or "" if unavailable.
fn session_id(s: &Session) -> String {
    s.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default()
}

/// Whether a session is currently in the Playing state (actively making sound).
fn is_playing(s: &Session) -> bool {
    matches!(
        s.GetPlaybackInfo().and_then(|pb| pb.PlaybackStatus()),
        Ok(PlaybackStatus::Playing)
    )
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

/// The highest-scoring session (Playing > Changing > other, then most recently
/// updated). The replacement adopted when the current selection is really gone.
fn best_session(items: &[Session]) -> Option<&Session> {
    items.iter().max_by_key(|s| session_score(*s))
}

/// Cap on holding an absent-but-alive source. Process liveness tells a *whole-app*
/// close (process gone → switch instantly) apart from a track change, but it can't
/// tell a closed browser *tab* from a track change — both leave the process alive
/// with the session simply gone, and nothing Windows exposes says whether it will
/// return. So this is the one irreducible knob: long enough to clear a real
/// track-change gap (≈5 s on slow mobile data) so we never flash to another source
/// mid-change, short enough that closing a tab updates the panel promptly.
const MAX_HOLD: Duration = Duration::from_secs(7);

/// Lowercased exe names of all running processes, or None if the snapshot failed
/// (so callers treat "unknown" rather than "nothing is running").
fn running_exe_names() -> Option<HashSet<String>> {
    let mut names = HashSet::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut ok = Process32FirstW(snap, &mut entry).is_ok();
        while ok {
            let len = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase();
            if !name.is_empty() {
                names.insert(name);
            }
            ok = Process32NextW(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    (!names.is_empty()).then_some(names)
}

/// Whether the app behind a SourceAppUserModelId is still running.
///   `Some(true)`  — a matching process exists (a missing session is a track-change gap);
///   `Some(false)` — exe-shaped id, no matching process (the app has closed);
///   `None`        — can't tell (id isn't an exe name, or the snapshot failed) → caller
///                   falls back to the time debounce.
/// SMTC reports these ids as bare exe names (e.g. `chrome.exe`, `cloudmusic.exe`),
/// so a name match against the process list is exact for the common players.
fn source_alive(id: &str) -> Option<bool> {
    let lc = id.to_lowercase();
    if !lc.ends_with(".exe") {
        return None;
    }
    let exe = lc.rsplit(['\\', '/']).next().unwrap_or(&lc).to_string();
    Some(running_exe_names()?.contains(&exe))
}

/// One step of the selection state machine, run once per poll. Gathers the live
/// world, runs the pure `decide`, and commits the result to `selected` (the only
/// place that state is written). Returns what the snapshot builder should do.
fn drive_selection(items: &[Session]) -> Decision {
    let present: HashSet<String> = items.iter().map(session_id).collect();
    let playing_now: HashSet<String> =
        items.iter().filter(|s| is_playing(s)).map(session_id).collect();
    // A pin is an explicit, sticky user choice — keep it until the user changes
    // it (via `media_select`). While its source is absent, `decide`'s rule 1
    // simply doesn't match and selection falls through; if the source returns,
    // the pin takes effect again.
    let pin = pinned().lock().unwrap().clone();

    let mut sel = selected().lock().unwrap();
    // Track how long the current selection has been missing from the list, and
    // whether its app is still running (only meaningful while it's absent).
    let sel_id = sel.id.clone();
    let (missing_elapsed, selected_alive) = match &sel_id {
        Some(id) if !present.contains(id) => {
            let elapsed = sel.missing_since.get_or_insert_with(Instant::now).elapsed();
            (Some(elapsed), source_alive(id))
        }
        _ => {
            sel.missing_since = None;
            (None, None)
        }
    };

    // A source that transitioned into Playing since the last poll.
    let prev = prev_playing().lock().unwrap().clone();
    let newly_started = playing_now
        .difference(&prev)
        .find(|id| present.contains(*id))
        .cloned();
    *prev_playing().lock().unwrap() = playing_now;

    let best = best_session(items).map(session_id);
    let decision = decide(
        &present,
        newly_started.as_ref(),
        &pin,
        &sel.id,
        selected_alive,
        missing_elapsed,
        GAP_DEBOUNCE,
        best.as_ref(),
    );

    match &decision {
        Decision::Show(id) => {
            sel.id = Some(id.clone());
            sel.missing_since = None;
        }
        // Hold: leave `selected` as-is (and keep `missing_since` counting).
        Decision::Hold => {}
        Decision::Inactive => {
            sel.id = None;
            sel.missing_since = None;
        }
    }
    decision
}

/// The session transport controls act on — exactly the one the UI shows. A pure
/// reader of `selected`: it looks the id up in the live list, so a control during
/// a Hold gap (selection momentarily absent) is a harmless no-op.
fn current_session() -> Option<Session> {
    let id = selected().lock().unwrap().id.clone()?;
    let mgr = SessionManager::RequestAsync().and_then(|op| op.get()).ok()?;
    all_sessions(&mgr).into_iter().find(|s| session_id(s) == id)
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

/// 100-ns ticks between 1601-01-01 (the Windows `DateTime`/FILETIME epoch) and
/// 1970-01-01 (the Unix epoch).
const FILETIME_UNIX_OFFSET: i64 = 116_444_736_000_000_000;

/// Current wall-clock time as 100-ns ticks since 1601 — directly comparable to
/// a `DateTime::UniversalTime`, which is what SMTC's `LastUpdatedTime` reports.
fn now_universal_ticks() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| (d.as_nanos() / 100) as i64 + FILETIME_UNIX_OFFSET)
        .unwrap_or(0)
}

/// Live playback position in raw 100-ns ticks (absolute, not start-normalized).
///
/// SMTC's `TimelineProperties::Position` is a *snapshot* captured at
/// `LastUpdatedTime`, not a continuously-advancing value: apps push timeline
/// updates at their own (often coarse, event-driven) cadence, so between pushes
/// `Position` is frozen. Reading it raw therefore reports a position that lags
/// real playback by however long ago the last push was — and because an
/// unchanged snapshot is never re-emitted, the staleness only surfaces when
/// some *other* field forces a `media-update` (e.g. a volume change), snapping
/// the UI back to the frozen spot. While playing, advance `Position` by the
/// wall-clock elapsed since `LastUpdatedTime` so it tracks actual playback.
/// (Assumes 1× playback rate, matching the frontend's interpolation.)
fn live_position_ticks(tl: &TimelineProperties, playing: bool) -> i64 {
    let pos = tl.Position().map(|t| t.Duration).unwrap_or(0);
    if !playing {
        return pos;
    }
    let last_updated = tl.LastUpdatedTime().map(|dt| dt.UniversalTime).unwrap_or(0);
    if last_updated <= 0 {
        return pos;
    }
    let elapsed = now_universal_ticks() - last_updated;
    if elapsed > 0 { pos + elapsed } else { pos }
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

/// Read a full snapshot: the selected session's now-playing state plus the list
/// of all sessions for the source switcher. The bool is `hold` — true when the
/// selection is briefly absent (a track-change gap) and the poller should keep
/// the previous snapshot rather than show this (inactive) one.
fn read_snapshot() -> (MediaSnapshot, bool) {
    let mut snap = MediaSnapshot::default();
    let (muted, volume) = read_endpoint();
    snap.muted  = muted;
    snap.volume = volume;

    let mgr = match SessionManager::RequestAsync().and_then(|op| op.get()) {
        Ok(m) => m,
        Err(_) => return (snap, false),
    };
    let items = all_sessions(&mgr);

    // Lightweight list for the source switcher (title/artist/playing per source).
    for s in &items {
        let mut info = SessionInfo { id: session_id(s), ..Default::default() };
        if let Ok(props) = s.TryGetMediaPropertiesAsync().and_then(|op| op.get()) {
            info.title  = props.Title().map(|h| h.to_string()).unwrap_or_default();
            info.artist = props.Artist().map(|h| h.to_string()).unwrap_or_default();
        }
        info.playing = is_playing(s);
        snap.sessions.push(info);
    }

    // Advance the selection state machine and act on its decision.
    let session = match drive_selection(&items) {
        Decision::Show(id) => match items.iter().find(|s| session_id(s) == id) {
            Some(s) => s.clone(),
            None => return (snap, false), // shouldn't happen; treat as inactive
        },
        Decision::Hold => return (snap, true), // freeze the panel on the last snapshot
        Decision::Inactive => return (snap, false),
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
            snap.can_seek  = c.IsPlaybackPositionEnabled().unwrap_or(false);
        }
    }
    if let Ok(tl) = session.GetTimelineProperties() {
        let start = tl.StartTime().map(ts_ms).unwrap_or(0);
        let end   = tl.EndTime().map(ts_ms).unwrap_or(0);
        // Position is only a snapshot at LastUpdatedTime; advance it to now while
        // playing so the reported position tracks real playback instead of the
        // (often stale) last value the app pushed to SMTC.
        let pos   = live_position_ticks(&tl, snap.status == "playing") / 10_000;
        snap.duration_ms = (end - start).max(0);
        snap.position_ms = (pos - start).clamp(0, snap.duration_ms);
    }
    snap.active = matches!(snap.status.as_str(), "playing" | "paused" | "changing");
    (snap, false)
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
            let (mut snap, hold) = read_snapshot();

            // During a track-change gap the selection is briefly absent; keep
            // presenting the last good snapshot so the panel doesn't blink to
            // inactive (or flash another source). The selection state machine
            // bounds the hold, so a genuine close still resolves afterward.
            if hold {
                if let Some(prev) = last.as_ref().filter(|p| p.active) {
                    snap = prev.clone();
                }
            }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn set(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }
    const DEBOUNCE: Duration = Duration::from_millis(2000);

    // Convenience: no pin, no newly-started. `alive` is the liveness of the
    // (absent) selection; None means "unknown" → the time-debounce fallback.
    fn decide_simple(
        present: &[&str],
        selected: Option<&str>,
        alive: Option<bool>,
        missing_elapsed: Option<Duration>,
        best: Option<&str>,
    ) -> Decision {
        let present = set(present);
        let sel = selected.map(|s| s.to_string());
        let best = best.map(|s| s.to_string());
        decide(&present, None, &None, &sel, alive, missing_elapsed, DEBOUNCE, best.as_ref())
    }

    #[test]
    fn pin_present_wins() {
        let present = set(&["a", "b"]);
        let pin = Some("b".to_string());
        let d = decide(&present, None, &pin, &Some("a".into()), None, None, DEBOUNCE, Some(&"a".into()));
        assert_eq!(d, Decision::Show("b".into()));
    }

    #[test]
    fn newly_started_grabs_focus() {
        let present = set(&["a", "b"]);
        let started = "b".to_string();
        let d = decide(&present, Some(&started), &None, &Some("a".into()), None, None, DEBOUNCE, Some(&"a".into()));
        assert_eq!(d, Decision::Show("b".into()));
    }

    #[test]
    fn sticks_with_present_selection() {
        // No pin / no new start → keep showing the current source even though
        // another (b) is the higher-scoring "best".
        assert_eq!(
            decide_simple(&["a", "b"], Some("a"), None, None, Some("b")),
            Decision::Show("a".into())
        );
    }

    #[test]
    fn holds_an_alive_source_even_past_the_debounce() {
        // The slow-4G case: a's session has been gone 5 s (longer than the
        // debounce) but a's process is alive → it's a track change, keep holding.
        assert_eq!(
            decide_simple(&["b"], Some("a"), Some(true), Some(Duration::from_secs(5)), Some("b")),
            Decision::Hold
        );
    }

    #[test]
    fn switches_immediately_when_the_process_is_gone() {
        // a closed (its process is gone) → reveal b at once, no debounce wait,
        // even though b is only paused.
        assert_eq!(
            decide_simple(&["b"], Some("a"), Some(false), Some(Duration::from_millis(100)), Some("b")),
            Decision::Show("b".into())
        );
    }

    #[test]
    fn safety_cap_releases_a_lingering_alive_source() {
        // Process alive but the session never came back (e.g. a closed browser
        // tab) → after MAX_HOLD, switch away so the panel can't freeze forever.
        assert_eq!(
            decide_simple(&["b"], Some("a"), Some(true), Some(MAX_HOLD + Duration::from_secs(1)), Some("b")),
            Decision::Show("b".into())
        );
    }

    #[test]
    fn unknown_liveness_falls_back_to_the_debounce() {
        // Non-exe id / failed snapshot → behave as before: hold briefly…
        assert_eq!(
            decide_simple(&["b"], Some("a"), None, Some(Duration::from_millis(500)), Some("b")),
            Decision::Hold
        );
        // …then switch once the debounce elapses.
        assert_eq!(
            decide_simple(&["b"], Some("a"), None, Some(Duration::from_millis(2500)), Some("b")),
            Decision::Show("b".into())
        );
    }

    #[test]
    fn inactive_when_nothing_remains() {
        // a closed and there is no other source at all.
        assert_eq!(
            decide_simple(&[], Some("a"), Some(false), Some(Duration::from_millis(100)), None),
            Decision::Inactive
        );
    }

    #[test]
    fn picks_best_on_cold_start() {
        // No prior selection → adopt the best available source.
        assert_eq!(
            decide_simple(&["a", "b"], None, None, None, Some("a")),
            Decision::Show("a".into())
        );
    }
}
