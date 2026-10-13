//! window_ops.rs — atomic native window geometry.
//!
//! Tauri's JS API exposes `setSize` and `setPosition` separately, so growing
//! and recentring the main window for tarot Card Mode took two ops — a visible
//! two-stage jump, and a back-to-back pair that could re-enter tao's paint
//! flush and panic (flush_paint_messages assertion). This command sets outer
//! position AND size in one native operation — `SetWindowPos` on Windows and
//! `NSWindow::setFrame:display:` on macOS — so the window lands at its final
//! bounds in one step.
//!
//! Marshaled to the UI thread (where the window was created), matching how
//! Tauri runs its own geometry calls.

use tauri::WebviewWindow;

#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct MacFrame {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// Translate Tauri's physical, top-left coordinates into an AppKit frame.
///
/// AppKit uses logical points with a bottom-left origin. Computing the target
/// relative to the current native frame avoids assuming that the primary
/// display starts at `(0, 0)` and continues to work on displays with negative
/// coordinates. The current window's scale factor is the correct conversion
/// for the card-mode resize performed by the frontend.
#[cfg(any(target_os = "macos", test))]
fn mac_frame_from_physical_delta(
    current: MacFrame,
    current_x: i32,
    current_y: i32,
    target_x: i32,
    target_y: i32,
    target_width: i32,
    target_height: i32,
    scale_factor: f64,
) -> Result<MacFrame, String> {
    if target_width <= 0 || target_height <= 0 {
        return Err("window width and height must be positive".into());
    }
    if !scale_factor.is_finite() || scale_factor <= 0.0 {
        return Err("window scale factor must be positive".into());
    }

    let delta_x = (f64::from(target_x) - f64::from(current_x)) / scale_factor;
    let delta_y = (f64::from(target_y) - f64::from(current_y)) / scale_factor;
    let width = f64::from(target_width) / scale_factor;
    let height = f64::from(target_height) / scale_factor;
    let target_top = current.y + current.height - delta_y;

    Ok(MacFrame {
        x: current.x + delta_x,
        y: target_top - height,
        width,
        height,
    })
}

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
    #[cfg(target_os = "macos")]
    {
        use std::sync::mpsc;

        use objc2_app_kit::NSWindow;

        if width <= 0 || height <= 0 {
            return Err("window width and height must be positive".into());
        }

        let current_position = window.outer_position().map_err(|e| e.to_string())?;
        let scale_factor = window.scale_factor().map_err(|e| e.to_string())?;
        let (tx, rx) = mpsc::channel::<Result<(), String>>();
        let native_window = window.clone();

        window
            .run_on_main_thread(move || {
                let result = (|| {
                    let pointer = native_window.ns_window().map_err(|e| e.to_string())?;
                    if pointer.is_null() {
                        return Err("NSWindow handle is null".into());
                    }

                    // SAFETY: Tauri owns this NSWindow for the lifetime of the
                    // cloned WebviewWindow, and this closure runs on AppKit's
                    // main thread. setFrame:display: is a public AppKit API.
                    let ns_window: &NSWindow = unsafe { &*pointer.cast() };
                    let mut frame = ns_window.frame();
                    let target = mac_frame_from_physical_delta(
                        MacFrame {
                            x: frame.origin.x,
                            y: frame.origin.y,
                            width: frame.size.width,
                            height: frame.size.height,
                        },
                        current_position.x,
                        current_position.y,
                        x,
                        y,
                        width,
                        height,
                        scale_factor,
                    )?;
                    frame.origin.x = target.x;
                    frame.origin.y = target.y;
                    frame.size.width = target.width;
                    frame.size.height = target.height;
                    ns_window.setFrame_display(frame, true);
                    Ok(())
                })();
                let _ = tx.send(result);
            })
            .map_err(|e| e.to_string())?;
        rx.recv()
            .map_err(|_| "macOS window geometry callback was dropped".to_string())?
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (window, x, y, width, height);
        Err("window bounds are not supported on this platform".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_frame_keeps_top_left_delta_while_resizing() {
        let frame = mac_frame_from_physical_delta(
            MacFrame {
                x: 50.0,
                y: 300.0,
                width: 170.0,
                height: 289.0,
            },
            100,
            200,
            80,
            160,
            400,
            600,
            2.0,
        )
        .unwrap();

        assert_eq!(
            frame,
            MacFrame {
                x: 40.0,
                y: 309.0,
                width: 200.0,
                height: 300.0,
            }
        );
    }

    #[test]
    fn mac_frame_supports_negative_display_coordinates() {
        let frame = mac_frame_from_physical_delta(
            MacFrame {
                x: -700.0,
                y: 100.0,
                width: 200.0,
                height: 300.0,
            },
            -1400,
            500,
            -1500,
            500,
            400,
            600,
            2.0,
        )
        .unwrap();

        assert_eq!(frame.x, -750.0);
        assert_eq!(frame.y, 100.0);
    }

    #[test]
    fn mac_frame_rejects_invalid_size_and_scale() {
        let current = MacFrame {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        assert!(mac_frame_from_physical_delta(current, 0, 0, 0, 0, 0, 10, 1.0).is_err());
        assert!(mac_frame_from_physical_delta(current, 0, 0, 0, 0, 10, 10, 0.0).is_err());
    }
}
