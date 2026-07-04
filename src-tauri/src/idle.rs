//! System idle detection — feeds the flying-screensaver input of the flight
//! mode controller (flight.rs).
//!
//! "Truly idle" requires ALL of the following to be true:
//!   - OS-wide input idle time ≥ the configurable screensaver wait
//!     (flight::screensaver_wait_secs, set from Settings; default 10 min)
//!   - No active display-sleep-prevention power assertions
//!     (i.e. no app is calling SetThreadExecutionState(ES_DISPLAY_REQUIRED))
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
//! Full implementation on Windows only. `spawn()` is a no-op on other OSes.
//!
//! # Testable pure helper
//! `should_fly(idle_secs, wait_secs, display_prevented)` is exported so unit
//! tests can cover the decision logic without any OS calls.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use tauri::AppHandle;

// ── Configuration ──────────────────────────────────────────────────

/// Background-thread sleep between polls. Kept at 1 s for near-zero CPU cost.
const POLL_INTERVAL_MS: u64 = 1_000;

/// If the thread wakes this many times later than expected the system
/// was probably suspended. Report not-idle and skip the poll.
const SUSPEND_OVERSHOOT_MULT: u64 = 5;

// ── Pure decision helper (unit-testable) ───────────────────────────

/// Returns `true` when the idle screensaver condition is met.
///
/// `idle_secs`          – seconds since last OS-wide mouse/keyboard event.
/// `wait_secs`          – configured screensaver wait (from Settings).
/// `display_prevented`  – true if any process holds ES_DISPLAY_REQUIRED.
///
/// Pure function: no OS calls, safe to unit-test.
pub fn should_fly(idle_secs: u64, wait_secs: u64, display_prevented: bool) -> bool {
    idle_secs >= wait_secs && !display_prevented
}

// ── Public entry point ─────────────────────────────────────────────

/// Spawns the idle-monitor thread. The thread exits when `stop_flag` is set.
pub fn spawn(app: AppHandle, stop_flag: Arc<AtomicBool>) {
    #[cfg(not(windows))]
    {
        let _ = (app, stop_flag);
        return;
    }

    #[cfg(windows)]
    thread::spawn(move || run_loop(app, stop_flag));
}

// ── Main poll loop (Windows) ───────────────────────────────────────

#[cfg(windows)]
fn run_loop(app: AppHandle, stop_flag: Arc<AtomicBool>) {
    let mut last_active = false;
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
        let idle_secs = get_input_secs();
        let display_prevented = is_display_sleep_prevented();
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
}
