//! macOS application lifecycle integration.
//!
//! Mutsumi intentionally uses the regular activation policy so its Dock icon
//! stays visible alongside the tray/menu-bar entry. Clicking the Dock icon
//! emits `RunEvent::Reopen`; if every window was hidden, restore the pet. If an
//! auxiliary window is already visible, focus the most actionable one instead.

use objc2_app_kit::NSWindow;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use std::time::Duration;

const REOPEN_FOCUS_PRIORITY: [&str; 4] = ["update", "settings", "about", "main"];
const SINGLE_INSTANCE_REFOCUS_DELAY: Duration = Duration::from_millis(1_000);
const SINGLE_INSTANCE_INPUT_TOLERANCE: Duration = Duration::from_millis(50);
const SINGLE_INSTANCE_REFOCUS_MAX_ELAPSED: Duration = Duration::from_millis(2_000);

#[repr(C)]
struct MachTimebaseInfo {
    numer: u32,
    denom: u32,
}

#[link(name = "System")]
unsafe extern "C" {
    fn mach_continuous_time() -> u64;
    fn mach_timebase_info(info: *mut MachTimebaseInfo) -> i32;
}

fn duration_from_mach_ticks(ticks: u64, numer: u32, denom: u32) -> Option<Duration> {
    if denom == 0 {
        return None;
    }

    let nanoseconds = (ticks as u128)
        .checked_mul(numer as u128)?
        .checked_div(denom as u128)?;
    let seconds = nanoseconds / 1_000_000_000;
    if seconds > u64::MAX as u128 {
        return None;
    }

    Some(Duration::new(
        seconds as u64,
        (nanoseconds % 1_000_000_000) as u32,
    ))
}

fn continuous_now() -> Result<Duration, String> {
    let mut timebase = MachTimebaseInfo { numer: 0, denom: 0 };
    // SAFETY: `timebase` is a valid writable struct, and both public Mach APIs
    // have no caller-owned pointers beyond this out parameter.
    let status = unsafe { mach_timebase_info(&mut timebase) };
    if status != 0 {
        return Err(format!(
            "mach_timebase_info failed with kern_return_t {status}"
        ));
    }

    // SAFETY: `mach_continuous_time` has no arguments or caller-owned state and
    // advances across system sleep, unlike `Instant` on Darwin.
    let ticks = unsafe { mach_continuous_time() };
    duration_from_mach_ticks(ticks, timebase.numer, timebase.denom)
        .ok_or_else(|| "mach continuous time could not be represented".to_string())
}

fn should_reassert_single_instance_focus(elapsed: Duration, input_idle: Option<Duration>) -> bool {
    if elapsed > SINGLE_INSTANCE_REFOCUS_MAX_ELAPSED {
        return false;
    }

    input_idle.is_none_or(|idle| idle.saturating_add(SINGLE_INSTANCE_INPUT_TOLERANCE) >= elapsed)
}

fn preferred_visible_label(mut is_visible: impl FnMut(&str) -> bool) -> Option<&'static str> {
    REOPEN_FOCUS_PRIORITY
        .iter()
        .copied()
        .find(|label| is_visible(label))
}

/// Fail startup when the configured transparent main window is still opaque.
///
/// Tauri builds configured windows before `setup`, so AppKit's native state is
/// final by the time this runs. This turns transparency from an assumed config
/// flag into a runtime contract and catches a missing feature/config merge.
pub fn validate_main_window_contract<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<(), Box<dyn std::error::Error>> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| std::io::Error::other("main window was not created"))?;
    let pointer = window.ns_window()?;
    if pointer.is_null() {
        return Err(std::io::Error::other("main NSWindow handle is null").into());
    }

    // SAFETY: setup runs on AppKit's main thread and Tauri owns this NSWindow
    // for at least as long as the WebviewWindow handle above.
    let ns_window: &NSWindow = unsafe { &*pointer.cast() };
    if ns_window.isOpaque() {
        return Err(std::io::Error::other(
            "main NSWindow is opaque; macOS transparency contract failed",
        )
        .into());
    }

    log::info!("[macos] main window transparency contract passed");
    Ok(())
}

/// Restore a useful window after the user clicks the Dock icon.
pub fn handle_reopen<R: Runtime>(app: &AppHandle<R>, has_visible_windows: bool) {
    log::info!("[macos] Dock reopen requested (has_visible_windows={has_visible_windows})");
    if has_visible_windows {
        let label = preferred_visible_label(|label| {
            app.get_webview_window(label)
                .and_then(|window| window.is_visible().ok())
                .unwrap_or(false)
        });

        if let Some(label) = label {
            log::info!("[macos] focusing visible '{label}' window after Dock reopen");
            if let Some(window) = app.get_webview_window(label) {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
                if label == "main" {
                    let _ = app.emit("pet-show", ());
                }
                return;
            }
        }
    }

    log::info!("[macos] restoring hidden main window after Dock reopen");
    crate::tray::show_main_faded(app);
}

/// Reassert focus after a bounded delay intended to outlast the second process.
///
/// The single-instance plugin notifies the existing process and immediately
/// exits the new one. LaunchServices can restore the previously active app as
/// that process disappears, racing the callback's first `set_focus`. Keeping
/// the immediate focus attempt and conditionally repeating it about one second
/// later makes activation deterministic in the native and Rosetta smoke matrix.
/// A mouse-button, keyboard or scroll event after scheduling and beyond the
/// 50 ms sampling tolerance means the user has made a newer focus choice, so
/// the delayed attempt yields instead.
pub fn schedule_single_instance_refocus<R: Runtime>(app: AppHandle<R>) {
    let scheduled_at = match continuous_now() {
        Ok(timestamp) => timestamp,
        Err(error) => {
            log::warn!("[macos] not scheduling delayed refocus: {error}");
            return;
        }
    };
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SINGLE_INSTANCE_REFOCUS_DELAY).await;

        let elapsed = match continuous_now().and_then(|current| {
            current
                .checked_sub(scheduled_at)
                .ok_or_else(|| "mach continuous time moved backwards".to_string())
        }) {
            Ok(elapsed) => elapsed,
            Err(error) => {
                log::warn!("[macos] skipping delayed refocus: {error}");
                return;
            }
        };
        let input_idle = match crate::idle::macos_focus_input_idle_seconds().and_then(|seconds| {
            Duration::try_from_secs_f64(seconds)
                .map_err(|error| format!("invalid CoreGraphics input age {seconds}: {error}"))
        }) {
            Ok(duration) => Some(duration),
            Err(error) => {
                log::warn!("[macos] could not sample focus input before delayed refocus: {error}");
                None
            }
        };
        if !should_reassert_single_instance_focus(elapsed, input_idle) {
            log::info!(
                "[macos] skipping delayed refocus after focus-changing input or a stale delay"
            );
            return;
        }

        log::info!("[macos] reasserting focus after second-instance handoff delay");
        crate::tray::show_main_faded(&app);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_actionable_auxiliary_windows_before_the_pet() {
        let visible = ["main", "about", "settings"];
        assert_eq!(
            preferred_visible_label(|label| visible.contains(&label)),
            Some("settings")
        );
    }

    #[test]
    fn update_window_has_highest_reopen_priority() {
        assert_eq!(preferred_visible_label(|_| true), Some("update"));
    }

    #[test]
    fn returns_none_when_no_known_window_is_visible() {
        assert_eq!(preferred_visible_label(|_| false), None);
    }

    #[test]
    fn delayed_refocus_continues_when_no_new_input_was_observed() {
        let elapsed = Duration::from_millis(1_000);
        assert!(should_reassert_single_instance_focus(
            elapsed,
            Some(Duration::from_millis(1_000)),
        ));
        assert!(should_reassert_single_instance_focus(
            elapsed,
            Some(Duration::from_millis(1_100)),
        ));
        assert!(should_reassert_single_instance_focus(elapsed, None));
    }

    #[test]
    fn delayed_refocus_respects_input_after_it_was_scheduled() {
        let elapsed = Duration::from_millis(1_000);
        assert!(!should_reassert_single_instance_focus(
            elapsed,
            Some(Duration::from_millis(400)),
        ));
        assert!(!should_reassert_single_instance_focus(
            elapsed,
            Some(Duration::from_millis(949)),
        ));
        assert!(should_reassert_single_instance_focus(
            elapsed,
            Some(Duration::from_millis(950)),
        ));
    }

    #[test]
    fn delayed_refocus_expires_instead_of_running_after_a_stall() {
        assert!(!should_reassert_single_instance_focus(
            Duration::from_millis(2_001),
            Some(Duration::from_secs(10)),
        ));
        assert!(!should_reassert_single_instance_focus(
            Duration::from_secs(300),
            Some(Duration::from_secs(300)),
        ));
    }

    #[test]
    fn mach_ticks_convert_without_overflow_or_zero_division() {
        assert_eq!(
            duration_from_mach_ticks(3, 125, 3),
            Some(Duration::from_nanos(125)),
        );
        assert_eq!(duration_from_mach_ticks(1, 1, 0), None);
        assert_eq!(duration_from_mach_ticks(u64::MAX, u32::MAX, 1), None);
    }

    #[test]
    fn public_continuous_clock_returns_a_live_sample() {
        assert!(continuous_now().is_ok());
    }
}
