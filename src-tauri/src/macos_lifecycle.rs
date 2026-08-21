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
/// the immediate focus attempt and repeating it about one second later makes
/// activation deterministic in the native and Rosetta smoke matrix.
pub fn schedule_single_instance_refocus<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SINGLE_INSTANCE_REFOCUS_DELAY).await;
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
}
