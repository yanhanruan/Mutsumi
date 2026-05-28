//! OS-level cursor poller (Windows).
//!
//! When the webview has `setIgnoreCursorEvents(true)`, it stops receiving
//! mouse events — so the only way to detect the cursor returning to a
//! visible pixel is to poll from outside the webview. This module spawns a
//! background thread that calls `GetCursorPos` every ~30 ms, computes the
//! cursor position relative to the main window, and emits a `cursor-pos`
//! event the frontend uses to drive per-pixel hit testing.
//!
//! On non-Windows platforms the spawn is a no-op.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

#[cfg(windows)]
use windows::Win32::Foundation::POINT;
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

const POLL_INTERVAL: Duration = Duration::from_millis(30);   // ~33 Hz

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
struct CursorPos {
    /// Cursor x relative to the main window's top-left (in physical px).
    x:           i32,
    /// Cursor y relative to the main window's top-left.
    y:           i32,
    /// True when the cursor is inside the window's outer rect.
    over_window: bool,
}

/// Pure geometry helper: given the cursor's absolute screen position and
/// the window's outer position + size, compute the cursor relative to the
/// window's top-left plus whether it lies inside the window's outer rect.
///
/// The "inside" check uses inclusive-left/exclusive-right (matches Win32
/// hit testing).
pub fn compute_relative(
    cursor_screen: (i32, i32),
    window_pos:    (i32, i32),
    window_size:   (u32, u32),
) -> (i32, i32, bool) {
    let rel_x = cursor_screen.0 - window_pos.0;
    let rel_y = cursor_screen.1 - window_pos.1;
    let over  = rel_x >= 0
             && rel_y >= 0
             && rel_x < window_size.0 as i32
             && rel_y < window_size.1 as i32;
    (rel_x, rel_y, over)
}

pub fn spawn(app: AppHandle, stop_flag: Arc<AtomicBool>) {
    #[cfg(not(windows))]
    {
        let _ = (app, stop_flag);
        return;
    }

    #[cfg(windows)]
    thread::spawn(move || {
        while !stop_flag.load(Ordering::Relaxed) {
            thread::sleep(POLL_INTERVAL);

            let Some(window) = app.get_webview_window("main") else { continue };
            let Ok(win_pos)  = window.outer_position()       else { continue };
            let Ok(win_size) = window.outer_size()           else { continue };

            let cursor = unsafe {
                let mut p = POINT::default();
                if GetCursorPos(&mut p).is_err() { continue }
                p
            };

            let (rel_x, rel_y, over) = compute_relative(
                (cursor.x, cursor.y),
                (win_pos.x, win_pos.y),
                (win_size.width, win_size.height),
            );

            let _ = app.emit("cursor-pos", CursorPos { x: rel_x, y: rel_y, over_window: over });
        }
    });
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_inside_window_at_origin() {
        let (x, y, over) = compute_relative((50, 100), (0, 0), (200, 340));
        assert_eq!((x, y, over), (50, 100, true));
    }

    #[test]
    fn cursor_inside_window_at_offset_position() {
        let (x, y, over) = compute_relative((150, 250), (100, 200), (200, 340));
        assert_eq!((x, y, over), (50, 50, true));
    }

    #[test]
    fn cursor_at_top_left_corner_is_inside() {
        let (x, y, over) = compute_relative((100, 200), (100, 200), (200, 340));
        assert_eq!((x, y, over), (0, 0, true));
    }

    #[test]
    fn cursor_at_bottom_right_edge_is_outside_exclusive() {
        // Right/bottom edges are exclusive (matches Win32 hit testing).
        let (x, y, over) = compute_relative((300, 540), (100, 200), (200, 340));
        assert_eq!((x, y, over), (200, 340, false));
    }

    #[test]
    fn cursor_one_pixel_inside_bottom_right() {
        let (x, y, over) = compute_relative((299, 539), (100, 200), (200, 340));
        assert_eq!((x, y, over), (199, 339, true));
    }

    #[test]
    fn cursor_above_window_is_outside_with_negative_y() {
        let (x, y, over) = compute_relative((150, 100), (100, 200), (200, 340));
        assert_eq!((x, y, over), (50, -100, false));
    }

    #[test]
    fn cursor_left_of_window_is_outside_with_negative_x() {
        let (x, y, over) = compute_relative((50, 250), (100, 200), (200, 340));
        assert_eq!((x, y, over), (-50, 50, false));
    }

    #[test]
    fn cursor_far_right_outside_window() {
        let (x, y, over) = compute_relative((1000, 250), (100, 200), (200, 340));
        assert_eq!((x, y, over), (900, 50, false));
    }

    #[test]
    fn cursor_far_below_outside_window() {
        let (x, y, over) = compute_relative((150, 5000), (100, 200), (200, 340));
        assert_eq!((x, y, over), (50, 4800, false));
    }

    #[test]
    fn window_at_negative_screen_position_is_handled() {
        // Multi-monitor setups can give windows negative outer positions.
        let (x, y, over) = compute_relative((-50, -100), (-200, -300), (200, 340));
        assert_eq!((x, y, over), (150, 200, true));
    }
}
