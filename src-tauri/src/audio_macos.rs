//! macOS system-audio activity via public CoreAudio device properties.
//!
//! This adapter intentionally does not capture or inspect audio samples. It
//! asks the default output device whether any process is running I/O, which is
//! available on the macOS 13 product baseline without a recording permission.
//! That makes this a useful animation signal, but not proof that audible
//! samples are currently playing: a process may keep an output stream alive
//! while silent, and the device may be muted. The platform capability therefore
//! reports this implementation as `degraded`, rather than Windows-equivalent.

use std::ffi::c_void;
use std::fmt;
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

const POLL_INTERVAL: Duration = Duration::from_millis(500);
const START_THRESHOLD_POLLS: u32 = 6;
const STOP_THRESHOLD_POLLS: u32 = 12;
const ERROR_LOG_INTERVAL_POLLS: u64 = 120;

type AudioObjectId = u32;
type AudioObjectPropertySelector = u32;
type AudioObjectPropertyScope = u32;
type AudioObjectPropertyElement = u32;
type OsStatus = i32;

const NO_ERR: OsStatus = 0;
const AUDIO_OBJECT_UNKNOWN: AudioObjectId = 0;
const AUDIO_OBJECT_SYSTEM_OBJECT: AudioObjectId = 1;

const fn fourcc(bytes: [u8; 4]) -> u32 {
    ((bytes[0] as u32) << 24)
        | ((bytes[1] as u32) << 16)
        | ((bytes[2] as u32) << 8)
        | bytes[3] as u32
}

// Values from the public CoreAudio AudioHardware headers.
const AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE: AudioObjectPropertySelector = fourcc(*b"dOut");
const AUDIO_DEVICE_PROPERTY_DEVICE_IS_RUNNING_SOMEWHERE: AudioObjectPropertySelector =
    fourcc(*b"gone");
const AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL: AudioObjectPropertyScope = fourcc(*b"glob");
const AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN: AudioObjectPropertyElement = 0;

#[repr(C)]
#[derive(Clone, Copy)]
struct AudioObjectPropertyAddress {
    selector: AudioObjectPropertySelector,
    scope: AudioObjectPropertyScope,
    element: AudioObjectPropertyElement,
}

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectGetPropertyData(
        object_id: AudioObjectId,
        address: *const AudioObjectPropertyAddress,
        qualifier_data_size: u32,
        qualifier_data: *const c_void,
        data_size: *mut u32,
        data: *mut c_void,
    ) -> OsStatus;
}

#[derive(Debug, PartialEq, Eq)]
enum CoreAudioError {
    PropertyRead {
        operation: &'static str,
        status: OsStatus,
    },
    UnexpectedPropertySize {
        operation: &'static str,
        expected: u32,
        actual: u32,
    },
}

impl fmt::Display for CoreAudioError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PropertyRead { operation, status } => {
                write!(formatter, "{operation} failed with OSStatus {status}")
            }
            Self::UnexpectedPropertySize {
                operation,
                expected,
                actual,
            } => write!(
                formatter,
                "{operation} returned {actual} bytes; expected {expected}"
            ),
        }
    }
}

fn property_address(selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        selector,
        scope: AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        element: AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    }
}

fn read_u32_property(
    object_id: AudioObjectId,
    address: AudioObjectPropertyAddress,
    operation: &'static str,
) -> Result<u32, CoreAudioError> {
    let expected_size = size_of::<u32>() as u32;
    let mut actual_size = expected_size;
    let mut value = 0u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            object_id,
            &address,
            0,
            std::ptr::null(),
            &mut actual_size,
            (&mut value as *mut u32).cast(),
        )
    };

    if status != NO_ERR {
        return Err(CoreAudioError::PropertyRead { operation, status });
    }
    if actual_size != expected_size {
        return Err(CoreAudioError::UnexpectedPropertySize {
            operation,
            expected: expected_size,
            actual: actual_size,
        });
    }
    Ok(value)
}

/// Returns whether the current default output device is running I/O in at
/// least one process. `Ok(false)` also covers the brief interval where macOS
/// has no default output device (for example while hardware is switching).
fn output_io_active() -> Result<bool, CoreAudioError> {
    let device = read_u32_property(
        AUDIO_OBJECT_SYSTEM_OBJECT,
        property_address(AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE),
        "read default output device",
    )?;
    if device == AUDIO_OBJECT_UNKNOWN {
        return Ok(false);
    }

    let running = read_u32_property(
        device,
        property_address(AUDIO_DEVICE_PROPERTY_DEVICE_IS_RUNNING_SOMEWHERE),
        "read output-device I/O state",
    )?;
    Ok(running != 0)
}

// Mirrors the continuity semantics of the Windows adapter without moving or
// changing Windows code in this macOS-only slice.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum AudioEvent {
    Started,
    Stopped,
}

#[derive(Debug)]
struct ContinuityTracker {
    emitted_playing: bool,
    consecutive_playing: u32,
    consecutive_silent: u32,
    start_threshold: u32,
    stop_threshold: u32,
}

impl ContinuityTracker {
    fn new(start_threshold: u32, stop_threshold: u32) -> Self {
        Self {
            emitted_playing: false,
            consecutive_playing: 0,
            consecutive_silent: 0,
            start_threshold,
            stop_threshold,
        }
    }

    fn observe(&mut self, playing: bool) -> Option<AudioEvent> {
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

    fn sampling_unavailable(&mut self) {
        // Keep the last emitted state, but require a fresh continuous streak
        // before the next transition once sampling recovers.
        self.consecutive_playing = 0;
        self.consecutive_silent = 0;
    }
}

pub struct AudioState(pub AtomicBool);

#[tauri::command]
pub fn get_audio_state(state: tauri::State<'_, AudioState>) -> bool {
    state.0.load(Ordering::Relaxed)
}

pub fn spawn(app: AppHandle, stop_flag: Arc<AtomicBool>) {
    let result = thread::Builder::new()
        .name("macos-audio-activity".into())
        .spawn(move || {
            let mut tracker =
                ContinuityTracker::new(START_THRESHOLD_POLLS, STOP_THRESHOLD_POLLS);
            let mut consecutive_errors = 0u64;

            while !stop_flag.load(Ordering::Relaxed) {
                match output_io_active() {
                    Ok(active) => {
                        if consecutive_errors > 0 {
                            log::info!(
                                "[audio] CoreAudio activity sampling recovered after {} failed polls",
                                consecutive_errors
                            );
                            consecutive_errors = 0;
                        }

                        match tracker.observe(active) {
                            Some(AudioEvent::Started) => {
                                if let Some(state) = app.try_state::<AudioState>() {
                                    state.0.store(true, Ordering::Relaxed);
                                }
                                let _ = app.emit("audio-started", ());
                            }
                            Some(AudioEvent::Stopped) => {
                                if let Some(state) = app.try_state::<AudioState>() {
                                    state.0.store(false, Ordering::Relaxed);
                                }
                                let _ = app.emit("audio-stopped", ());
                            }
                            None => {}
                        }
                    }
                    Err(error) => {
                        // A transient CoreAudio failure is not evidence of
                        // silence. Preserve the last state and retry so device
                        // changes cannot synthesize a false stop event.
                        tracker.sampling_unavailable();
                        consecutive_errors += 1;
                        if consecutive_errors == 1
                            || consecutive_errors % ERROR_LOG_INTERVAL_POLLS == 0
                        {
                            log::warn!(
                                "[audio] CoreAudio activity sample unavailable (failure #{}): {}",
                                consecutive_errors,
                                error
                            );
                        }
                    }
                }

                thread::sleep(POLL_INTERVAL);
            }
        });

    if let Err(error) = result {
        log::error!("[audio] failed to start CoreAudio activity thread: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observe_run(tracker: &mut ContinuityTracker, playing: bool, polls: u32) -> Vec<AudioEvent> {
        (0..polls)
            .filter_map(|_| tracker.observe(playing))
            .collect()
    }

    #[test]
    fn starts_and_stops_only_after_continuous_thresholds() {
        let mut tracker = ContinuityTracker::new(3, 4);
        assert!(observe_run(&mut tracker, true, 2).is_empty());
        assert_eq!(tracker.observe(true), Some(AudioEvent::Started));
        assert!(observe_run(&mut tracker, false, 3).is_empty());
        assert_eq!(tracker.observe(false), Some(AudioEvent::Stopped));
    }

    #[test]
    fn opposite_sample_resets_the_pending_streak() {
        let mut tracker = ContinuityTracker::new(3, 4);
        assert!(observe_run(&mut tracker, true, 2).is_empty());
        assert_eq!(tracker.observe(false), None);
        assert!(observe_run(&mut tracker, true, 2).is_empty());
        assert_eq!(tracker.observe(true), Some(AudioEvent::Started));
    }

    #[test]
    fn unavailable_sample_preserves_state_but_resets_transition_streaks() {
        let mut tracker = ContinuityTracker::new(3, 4);
        assert!(observe_run(&mut tracker, true, 2).is_empty());
        tracker.sampling_unavailable();
        assert!(observe_run(&mut tracker, true, 2).is_empty());
        assert_eq!(tracker.observe(true), Some(AudioEvent::Started));

        assert!(observe_run(&mut tracker, false, 3).is_empty());
        tracker.sampling_unavailable();
        assert!(observe_run(&mut tracker, false, 3).is_empty());
        assert_eq!(tracker.observe(false), Some(AudioEvent::Stopped));
    }

    #[test]
    fn coreaudio_fourcc_values_match_the_public_headers() {
        assert_eq!(AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE, 0x644f_7574);
        assert_eq!(
            AUDIO_DEVICE_PROPERTY_DEVICE_IS_RUNNING_SOMEWHERE,
            0x676f_6e65
        );
        assert_eq!(AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL, 0x676c_6f62);
    }

    #[test]
    #[ignore = "requires a live macOS CoreAudio user session"]
    fn live_default_output_activity_property_is_readable() {
        output_io_active().expect("default output activity should be readable");
    }
}
