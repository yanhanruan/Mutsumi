//! System idle detection — feeds the flying-screensaver input of the flight
//! mode controller (flight.rs).
//!
//! "Truly idle" requires ALL of the following to be true:
//!   - OS-wide input idle time ≥ the configurable screensaver wait
//!     (flight::screensaver_wait_secs, set from Settings; default 10 min)
//!   - No active display-sleep-prevention power assertions
//!     (Windows: `ES_DISPLAY_REQUIRED`; macOS:
//!     `PreventUserIdleDisplaySleep` IOKit assertions)
//!
//! # Thread design
//! A background thread sleeps for POLL_INTERVAL_MS per iteration, then samples
//! the two conditions. Reports to `flight::set_idle_active` only on state
//! *transitions*; the flight controller owns the `toggle-balloon-mode` event
//! and folds in the manual (Ctrl+Alt+F) and settings inputs.
//!
//! # Graceful suspend/resume
//! When the OS suspends, `thread::sleep` returns far later than expected.
//! If the actual sleep duration exceeds SUSPEND_OVERSHOOT_MULT × the target
//! duration, we treat the wakeup as a resume event: immediately report
//! not-idle and skip the idle check for that iteration, so balloon mode never
//! engages while the display is off.
//!
//! # Platform support
//! Full implementation is available on Windows and macOS. Other platforms
//! remain a no-op until both input-idle and display-sleep assertion checks are
//! implemented, so an incomplete adapter cannot trigger flight unexpectedly.
//!
//! # Testable pure helper
//! `should_fly(idle_secs, wait_secs, display_prevented)` is exported so unit
//! tests can cover the decision logic without any OS calls.

use std::sync::atomic::AtomicBool;
#[cfg(any(windows, target_os = "macos"))]
use std::sync::atomic::Ordering;
use std::sync::Arc;
#[cfg(any(windows, target_os = "macos"))]
use std::thread;
#[cfg(any(windows, target_os = "macos"))]
use std::time::{Duration, Instant};

use tauri::AppHandle;

// ── Configuration ──────────────────────────────────────────────────

/// Background-thread sleep between polls. Kept at 1 s for near-zero CPU cost.
#[cfg(any(windows, target_os = "macos"))]
const POLL_INTERVAL_MS: u64 = 1_000;

/// If the thread wakes this many times later than expected the system
/// was probably suspended. Report not-idle and skip the poll.
#[cfg(any(windows, target_os = "macos"))]
const SUSPEND_OVERSHOOT_MULT: u64 = 5;

// ── Pure decision helper (unit-testable) ───────────────────────────

/// Returns `true` when the idle screensaver condition is met.
///
/// `idle_secs`          – seconds since last OS-wide mouse/keyboard event.
/// `wait_secs`          – configured screensaver wait (from Settings).
/// `display_prevented`  – true if any process holds the platform's display
///                        sleep-prevention assertion.
///
/// Pure function: no OS calls, safe to unit-test.
#[cfg(any(windows, target_os = "macos", test))]
pub fn should_fly(idle_secs: u64, wait_secs: u64, display_prevented: bool) -> bool {
    idle_secs >= wait_secs && !display_prevented
}

// ── Public entry point ─────────────────────────────────────────────

/// Spawns the idle-monitor thread. The thread exits when `stop_flag` is set.
pub fn spawn(app: AppHandle, stop_flag: Arc<AtomicBool>) {
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (app, stop_flag);
        return;
    }

    #[cfg(any(windows, target_os = "macos"))]
    thread::spawn(move || run_loop(app, stop_flag));
}

// ── Main poll loop ─────────────────────────────────────────────────

#[cfg(any(windows, target_os = "macos"))]
fn run_loop(app: AppHandle, stop_flag: Arc<AtomicBool>) {
    let mut last_active = false;
    let mut sample_error_reported = false;
    let poll_target = Duration::from_millis(POLL_INTERVAL_MS);
    let overshoot_threshold = poll_target * SUSPEND_OVERSHOOT_MULT as u32;

    while !stop_flag.load(Ordering::Relaxed) {
        let sleep_start = Instant::now();
        thread::sleep(poll_target);
        let actual = sleep_start.elapsed();

        // ── Suspend/resume detection ────────────────────────────────
        // If we slept significantly longer than requested, the OS was
        // suspended. Report not-idle and skip this iteration so we don't
        // accidentally activate on the first tick after resume (input idle
        // time may still be high right after wake).
        if actual > overshoot_threshold {
            if last_active {
                last_active = false;
                crate::flight::set_idle_active(&app, false);
            }
            continue;
        }

        // ── Idle check ──────────────────────────────────────────────
        let (idle_secs, display_prevented) = match sample_idle_conditions() {
            Ok(sample) => {
                if sample_error_reported {
                    log::info!("system idle sampling recovered");
                    sample_error_reported = false;
                }
                sample
            }
            Err(error) => {
                if !sample_error_reported {
                    log::warn!(
                        "system idle sampling unavailable; automatic flight paused: {error}"
                    );
                    sample_error_reported = true;
                }
                if last_active {
                    last_active = false;
                    crate::flight::set_idle_active(&app, false);
                }
                continue;
            }
        };
        let want_active = should_fly(
            idle_secs,
            crate::flight::screensaver_wait_secs(),
            display_prevented,
        );

        if want_active != last_active {
            last_active = want_active;
            // The flight controller folds in the manual shortcut and the
            // screensaver settings, then starts/stops the window flight and
            // notifies the frontend.
            crate::flight::set_idle_active(&app, want_active);
        }
    }
}

#[cfg(windows)]
fn sample_idle_conditions() -> Result<(u64, bool), String> {
    Ok((get_input_secs(), is_display_sleep_prevented()))
}

#[cfg(target_os = "macos")]
fn sample_idle_conditions() -> Result<(u64, bool), String> {
    Ok((
        macos::get_input_secs()?,
        macos::is_display_sleep_prevented()?,
    ))
}

// ── OS query helpers (Windows) ─────────────────────────────────────

/// Returns the number of whole seconds since the last OS-wide mouse or
/// keyboard input (`GetLastInputInfo` + `GetTickCount64`).
///
/// Returns 0 on any API failure (conservative: don't activate balloon mode).
#[cfg(windows)]
pub fn get_input_secs() -> u64 {
    use std::mem::size_of;
    use windows::Win32::System::SystemInformation::GetTickCount64;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    let mut lii = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };

    unsafe {
        if !GetLastInputInfo(&mut lii).as_bool() {
            return 0;
        }
        // GetTickCount64 wraps at ~49.7 days but dwTime is a u32 tick count;
        // take only the low 32 bits of now for the subtraction to handle wrapping.
        let now_ms = GetTickCount64();
        let idle_ms = now_ms.saturating_sub(lii.dwTime as u64);
        idle_ms / 1_000
    }
}

/// Returns `true` if the system execution state has `ES_DISPLAY_REQUIRED` set,
/// meaning some process is actively preventing the display from sleeping
/// (e.g. a video player, game, or presentation app).
///
/// Uses `CallNtPowerInformation(SystemExecutionState)`, which aggregates the
/// `SetThreadExecutionState` flags of all running threads system-wide.
///
/// Returns `false` on API failure (conservative: don't block balloon mode).
#[cfg(windows)]
pub fn is_display_sleep_prevented() -> bool {
    use std::mem::size_of;
    use windows::Win32::System::Power::{CallNtPowerInformation, SystemExecutionState};

    // ES_DISPLAY_REQUIRED — keeps the display on while set.
    // Defined in <winbase.h>; no named constant in windows-rs at this feature level.
    const ES_DISPLAY_REQUIRED: u32 = 0x0000_0002;

    let mut state: u32 = 0;
    let result = unsafe {
        CallNtPowerInformation(
            SystemExecutionState,
            None,
            0,
            Some(&mut state as *mut u32 as *mut core::ffi::c_void),
            size_of::<u32>() as u32,
        )
    };

    result.is_ok() && (state & ES_DISPLAY_REQUIRED) != 0
}

// ── OS query helpers (macOS) ───────────────────────────────────────

#[cfg(target_os = "macos")]
mod macos {
    use core::ffi::{c_char, c_void};
    use std::ptr;

    type CfTypeRef = *const c_void;
    type CfDictionaryRef = *const c_void;
    type CfStringRef = *const c_void;

    const CG_EVENT_SOURCE_COMBINED_SESSION_STATE: i32 = 0;
    const CG_ANY_INPUT_EVENT_TYPE: u32 = u32::MAX;
    const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    const CF_NUMBER_INT_TYPE: isize = 9;
    const IOKIT_SUCCESS: i32 = 0;

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(state_id: i32, event_type: u32) -> f64;
    }

    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        fn IOPMCopyAssertionsStatus(assertions_status: *mut CfDictionaryRef) -> i32;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringCreateWithCString(
            allocator: CfTypeRef,
            c_str: *const c_char,
            encoding: u32,
        ) -> CfStringRef;
        fn CFDictionaryGetValue(dictionary: CfDictionaryRef, key: CfTypeRef) -> CfTypeRef;
        fn CFNumberGetValue(number: CfTypeRef, number_type: isize, value: *mut c_void) -> u8;
        fn CFRelease(value: CfTypeRef);
    }

    /// Owns a Core Foundation object returned under the Create/Copy rule.
    struct OwnedCf(CfTypeRef);

    impl OwnedCf {
        fn new(value: CfTypeRef, context: &str) -> Result<Self, String> {
            if value.is_null() {
                Err(format!("{context} returned null"))
            } else {
                Ok(Self(value))
            }
        }
    }

    impl Drop for OwnedCf {
        fn drop(&mut self) {
            // SAFETY: `OwnedCf` is only constructed for non-null objects
            // returned under a Core Foundation Create/Copy ownership rule.
            unsafe { CFRelease(self.0) }
        }
    }

    /// Returns whole seconds since the last keyboard, mouse, or tablet event in
    /// the current login session. This public CoreGraphics query does not
    /// install an event tap and therefore needs no Accessibility permission.
    pub(super) fn get_input_secs() -> Result<u64, String> {
        // SAFETY: Both enum values come directly from CGEventTypes.h and the
        // function has no pointer arguments or caller-owned output.
        let seconds = unsafe {
            CGEventSourceSecondsSinceLastEventType(
                CG_EVENT_SOURCE_COMBINED_SESSION_STATE,
                CG_ANY_INPUT_EVENT_TYPE,
            )
        };

        if !seconds.is_finite() || seconds < 0.0 {
            return Err(format!("CoreGraphics returned invalid idle time {seconds}"));
        }

        Ok(seconds.floor().min(u64::MAX as f64) as u64)
    }

    /// Returns whether any process currently holds the public IOKit
    /// `PreventUserIdleDisplaySleep` assertion used by video players,
    /// presentation tools, and `caffeinate -d`.
    pub(super) fn is_display_sleep_prevented() -> Result<bool, String> {
        let mut dictionary: CfDictionaryRef = ptr::null();
        // SAFETY: IOPM writes one retained CFDictionary reference to the valid
        // out pointer on success; it is released by `OwnedCf` below.
        let status = unsafe { IOPMCopyAssertionsStatus(&mut dictionary) };
        if status != IOKIT_SUCCESS {
            return Err(format!(
                "IOPMCopyAssertionsStatus failed with IOReturn {status:#x}"
            ));
        }
        let dictionary = OwnedCf::new(dictionary, "IOPMCopyAssertionsStatus")?;

        // kIOPMAssertPreventUserIdleDisplaySleep is a CFSTR macro, not an
        // exported symbol, so construct the equivalent key with the public
        // Core Foundation API and release it after the lookup.
        let key = unsafe {
            CFStringCreateWithCString(
                ptr::null(),
                c"PreventUserIdleDisplaySleep".as_ptr(),
                CF_STRING_ENCODING_UTF8,
            )
        };
        let key = OwnedCf::new(key, "CFStringCreateWithCString")?;

        // SAFETY: Both CF objects are live for the duration of the lookup. The
        // returned CFNumber is borrowed from `dictionary` and is not released.
        let number = unsafe { CFDictionaryGetValue(dictionary.0, key.0) };
        if number.is_null() {
            return Err("IOKit assertion dictionary omitted PreventUserIdleDisplaySleep".into());
        }

        let mut level: i32 = 0;
        // SAFETY: `number` is the CFNumber documented by
        // IOPMCopyAssertionsStatus; `level` is valid writable storage for
        // kCFNumberIntType.
        let converted = unsafe {
            CFNumberGetValue(
                number,
                CF_NUMBER_INT_TYPE,
                &mut level as *mut i32 as *mut c_void,
            )
        };
        if converted == 0 {
            return Err("IOKit display assertion level was not a CFNumber".into());
        }

        // IOPMCopyAssertionsStatus reports the system-wide aggregate as a
        // normalized non-zero value (observed as 1 for `caffeinate -d`), not
        // necessarily the 255 value accepted when creating an assertion.
        Ok(level != 0)
    }
}

// ── Unit tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Representative screensaver wait for the tests (the runtime value is
    /// configurable — see flight::screensaver_wait_secs).
    const WAIT: u64 = 600;

    // ── should_fly (pure decision logic) ──────────────────────

    #[test]
    fn not_idle_short_time_no_assertion() {
        // Well below the wait, no assertion → stay off
        assert!(!should_fly(0, WAIT, false));
        assert!(!should_fly(300, WAIT, false));
        assert!(!should_fly(WAIT - 1, WAIT, false));
    }

    #[test]
    fn exactly_at_threshold_no_assertion_activates() {
        assert!(should_fly(WAIT, WAIT, false));
    }

    #[test]
    fn above_threshold_no_assertion_activates() {
        assert!(should_fly(WAIT + 1, WAIT, false));
        assert!(should_fly(3600, WAIT, false));
    }

    #[test]
    fn at_threshold_but_display_prevented_stays_off() {
        // A video is playing — do not enter balloon mode
        assert!(!should_fly(WAIT, WAIT, true));
        assert!(!should_fly(WAIT + 100, WAIT, true));
        assert!(!should_fly(u64::MAX, WAIT, true));
    }

    #[test]
    fn zero_idle_with_assertion_stays_off() {
        assert!(!should_fly(0, WAIT, true));
    }

    #[test]
    fn boundary_one_below_threshold_stays_off() {
        assert!(!should_fly(WAIT - 1, WAIT, false));
    }

    #[test]
    fn boundary_one_above_threshold_activates() {
        assert!(should_fly(WAIT + 1, WAIT, false));
    }

    #[test]
    fn wait_is_respected_when_reconfigured() {
        // Shorter and longer waits from Settings shift the boundary with them.
        assert!(should_fly(360, 360, false));
        assert!(!should_fly(359, 360, false));
        assert!(should_fly(1800, 1800, false));
        assert!(!should_fly(1799, 1800, false));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_public_idle_queries_return_valid_samples() {
        assert!(macos::get_input_secs().is_ok());
        assert!(macos::is_display_sleep_prevented().is_ok());
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires an active PreventUserIdleDisplaySleep assertion, e.g. caffeinate -d"]
    fn macos_detects_active_display_sleep_assertion() {
        assert_eq!(macos::is_display_sleep_prevented(), Ok(true));
    }
}
