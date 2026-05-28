//! Windows audio detection via WASAPI session enumeration.
//!
//! Detection is **continuity-based**, not edge-based: brief sub-threshold
//! dips during continuous playback (codec sample boundaries, quiet musical
//! passages, buffering) don't trigger a stop, and brief above-threshold
//! blips don't trigger a start.
//!
//! The continuity logic is factored into `ContinuityTracker` so it can be
//! unit-tested without WASAPI or Tauri. The WASAPI polling loop owns the
//! tracker, feeds it `observe(playing)` once per poll, and forwards any
//! resulting `AudioEvent` to the frontend as a Tauri event.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use windows::core::Interface;
use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
use windows::Win32::Media::Audio::{
    eMultimedia, eRender, IAudioSessionEnumerator, IAudioSessionManager2, IMMDevice,
    IMMDeviceEnumerator, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};

const THRESHOLD: f32 = 0.001;
const POLL_INTERVAL: Duration = Duration::from_millis(500);
const START_THRESHOLD_POLLS: u32 = 6;
const STOP_THRESHOLD_POLLS:  u32 = 12;

// ── Pure state machine (testable without WASAPI/Tauri) ─────────────

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AudioEvent {
    Started,
    Stopped,
}

#[derive(Debug)]
pub struct ContinuityTracker {
    emitted_playing:     bool,
    consecutive_playing: u32,
    consecutive_silent:  u32,
    start_threshold:     u32,
    stop_threshold:      u32,
}

impl ContinuityTracker {
    pub fn new(start_threshold: u32, stop_threshold: u32) -> Self {
        Self {
            emitted_playing:     false,
            consecutive_playing: 0,
            consecutive_silent:  0,
            start_threshold,
            stop_threshold,
        }
    }

    pub fn observe(&mut self, playing: bool) -> Option<AudioEvent> {
        if playing {
            self.consecutive_playing += 1;
            self.consecutive_silent = 0;
        } else {
            self.consecutive_silent += 1;
            self.consecutive_playing = 0;
        }

        if !self.emitted_playing && self.consecutive_playing >= self.start_threshold {
            self.emitted_playing = true;
            return Some(AudioEvent::Started);
        }
        if self.emitted_playing && self.consecutive_silent >= self.stop_threshold {
            self.emitted_playing = false;
            return Some(AudioEvent::Stopped);
        }
        None
    }

    pub fn emitted_playing(&self) -> bool { self.emitted_playing }
    pub fn streaks(&self) -> (u32, u32) { (self.consecutive_playing, self.consecutive_silent) }
}

// ── Background thread that owns a tracker + polls WASAPI ───────────

pub fn spawn(app: AppHandle, stop_flag: Arc<AtomicBool>) {
    thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }

        let mut tracker = ContinuityTracker::new(START_THRESHOLD_POLLS, STOP_THRESHOLD_POLLS);
        let mut poll_count: u64 = 0;

        while !stop_flag.load(Ordering::Relaxed) {
            poll_count += 1;
            let now_playing = unsafe { is_audio_playing() }.unwrap_or(false);

            match tracker.observe(now_playing) {
                Some(AudioEvent::Started) => {
                    log::info!("[audio] *** audio-started (poll #{}) ***", poll_count);
                    let _ = app.emit("audio-started", ());
                }
                Some(AudioEvent::Stopped) => {
                    log::info!("[audio] *** audio-stopped (poll #{}) ***", poll_count);
                    let _ = app.emit("audio-stopped", ());
                }
                None => {}
            }

            if poll_count % 20 == 0 {
                let (play_s, silent_s) = tracker.streaks();
                log::info!(
                    "[audio] poll #{}: now={} emitted={} play_streak={} silent_streak={}",
                    poll_count, now_playing, tracker.emitted_playing(), play_s, silent_s
                );
            }

            thread::sleep(POLL_INTERVAL);
        }

        unsafe { CoUninitialize() };
    });
}

unsafe fn is_audio_playing() -> windows::core::Result<bool> {
    let enumerator: IMMDeviceEnumerator =
        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    let device: IMMDevice = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
    let session_mgr: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
    let sessions: IAudioSessionEnumerator = session_mgr.GetSessionEnumerator()?;
    let count = sessions.GetCount()?;

    for i in 0..count {
        let session_ctl = match sessions.GetSession(i) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let meter: IAudioMeterInformation = match session_ctl.cast() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let peak = match meter.GetPeakValue() {
            Ok(p) => p,
            Err(_) => continue,
        };
        if peak > THRESHOLD {
            return Ok(true);
        }
    }
    Ok(false)
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn observe_run(t: &mut ContinuityTracker, playing: bool, n: u32) -> Vec<AudioEvent> {
        (0..n).filter_map(|_| t.observe(playing)).collect()
    }

    #[test]
    fn emits_started_after_threshold_continuous_polls() {
        let mut t = ContinuityTracker::new(6, 12);
        assert_eq!(observe_run(&mut t, true, 5), vec![]);
        assert!(!t.emitted_playing());
        assert_eq!(t.observe(true), Some(AudioEvent::Started));
        assert!(t.emitted_playing());
    }

    #[test]
    fn emits_stopped_after_threshold_continuous_silence() {
        let mut t = ContinuityTracker::new(6, 12);
        observe_run(&mut t, true, 6);
        assert!(t.emitted_playing());
        assert_eq!(observe_run(&mut t, false, 11), vec![]);
        assert!(t.emitted_playing());
        assert_eq!(t.observe(false), Some(AudioEvent::Stopped));
        assert!(!t.emitted_playing());
    }

    #[test]
    fn brief_silence_during_playback_does_not_emit_stopped() {
        let mut t = ContinuityTracker::new(6, 12);
        observe_run(&mut t, true, 6);
        observe_run(&mut t, false, 11);
        let resume_event = t.observe(true);
        assert_eq!(resume_event, None);
        assert!(t.emitted_playing());
        assert_eq!(t.streaks(), (1, 0));
        observe_run(&mut t, false, 11);
        let _ = t.observe(true);
        assert!(t.emitted_playing());
    }

    #[test]
    fn brief_noise_during_silence_does_not_emit_started() {
        let mut t = ContinuityTracker::new(6, 12);
        observe_run(&mut t, true, 5);
        let silence_event = t.observe(false);
        assert_eq!(silence_event, None);
        assert!(!t.emitted_playing());
        assert_eq!(t.streaks(), (0, 1));
    }

    #[test]
    fn no_double_emission_while_playing_streak_continues() {
        let mut t = ContinuityTracker::new(6, 12);
        observe_run(&mut t, true, 6);
        for _ in 0..100 {
            assert_eq!(t.observe(true), None);
        }
        assert!(t.emitted_playing());
    }

    #[test]
    fn no_emission_when_starting_silent_and_staying_silent() {
        let mut t = ContinuityTracker::new(6, 12);
        for _ in 0..100 {
            assert_eq!(t.observe(false), None);
        }
        assert!(!t.emitted_playing());
    }

    #[test]
    fn full_cycle_start_stop_start() {
        let mut t = ContinuityTracker::new(6, 12);
        observe_run(&mut t, true, 5);
        assert_eq!(t.observe(true), Some(AudioEvent::Started));
        observe_run(&mut t, false, 11);
        assert_eq!(t.observe(false), Some(AudioEvent::Stopped));
        observe_run(&mut t, true, 5);
        assert_eq!(t.observe(true), Some(AudioEvent::Started));
    }

    #[test]
    fn streak_resets_correctly_on_state_flip() {
        let mut t = ContinuityTracker::new(6, 12);
        t.observe(true);
        t.observe(true);
        t.observe(true);
        assert_eq!(t.streaks(), (3, 0));
        t.observe(false);
        assert_eq!(t.streaks(), (0, 1));
        t.observe(false);
        assert_eq!(t.streaks(), (0, 2));
        t.observe(true);
        assert_eq!(t.streaks(), (1, 0));
    }

    #[test]
    fn custom_thresholds_are_respected() {
        let mut t = ContinuityTracker::new(2, 3);
        assert_eq!(t.observe(true),  None);
        assert_eq!(t.observe(true),  Some(AudioEvent::Started));
        assert_eq!(t.observe(false), None);
        assert_eq!(t.observe(false), None);
        assert_eq!(t.observe(false), Some(AudioEvent::Stopped));
    }

    #[test]
    fn observation_exactly_at_silence_threshold_after_playing_emits_once() {
        let mut t = ContinuityTracker::new(6, 12);
        observe_run(&mut t, true, 6);
        let events = observe_run(&mut t, false, 12);
        assert_eq!(events, vec![AudioEvent::Stopped]);
    }
}
