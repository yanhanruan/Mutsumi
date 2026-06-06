//! window_ops.rs — atomic window geometry (Windows).
//!
//! Tauri's JS API exposes `setSize` and `setPosition` separately, so growing
//! and recentring the main window for tarot Card Mode took two ops — a visible
//! two-stage jump, and a back-to-back pair that could re-enter tao's paint
//! flush and panic (flush_paint_messages assertion). This command sets outer
//! position AND size in a single native `SetWindowPos` call so the window
//! lands at its final bounds in one step.
//!
//! Marshaled to the UI thread (where the window was created), matching how
//! Tauri runs its own geometry calls.

use tauri::WebviewWindow;

/// Set the main window's outer position and size atomically (physical pixels).
#[tauri::command]
pub fn set_window_bounds(
    window: WebviewWindow,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::sync::mpsc;
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
        };
        // Block until SetWindowPos has actually executed on the UI thread.
        // run_on_main_thread only *queues* the closure, so without this the
        // command would return before the window moved — letting the frontend
        // swap content while the window is still at the old geometry (the
        // pet/bubble flashing at the new centre).
        let (tx, rx) = mpsc::channel::<()>();
        let win = window.clone();
        window
            .run_on_main_thread(move || {
                if let Ok(hwnd) = win.hwnd() {
                    // SAFETY: hwnd is valid for the window's lifetime and this
                    // runs on the UI thread that owns it.
                    unsafe {
                        let _ = SetWindowPos(
                            hwnd,
                            None,
                            x,
                            y,
                            width,
                            height,
                            SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                    }
                }
                let _ = tx.send(());
            })
            .map_err(|e| e.to_string())?;
        let _ = rx.recv(); // wait for the resize to land before returning
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (window, x, y, width, height);
        Ok(())
    }
}
