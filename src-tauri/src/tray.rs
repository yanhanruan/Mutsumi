//! System tray icon with show/hide/pomodoro/settings/quit menu.
//!
//! Mirrors the Python original's `tray.py`.

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, Runtime, State,
};

use crate::app_state::SharedState;

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let show_item     = MenuItem::with_id(app, "show",      "Show",          true, None::<&str>)?;
    let hide_item     = MenuItem::with_id(app, "hide",      "Hide",          true, None::<&str>)?;
    let sep1          = PredefinedMenuItem::separator(app)?;
    let pom_start     = MenuItem::with_id(app, "pom_start", "Pomodoro: Start", true, None::<&str>)?;
    let pom_pause     = MenuItem::with_id(app, "pom_pause", "Pomodoro: Pause", true, None::<&str>)?;
    let pom_stop      = MenuItem::with_id(app, "pom_stop",  "Pomodoro: Stop",  true, None::<&str>)?;
    let sep2          = PredefinedMenuItem::separator(app)?;
    let settings_item = MenuItem::with_id(app, "settings",  "Settings…",     true, None::<&str>)?;
    let sep3          = PredefinedMenuItem::separator(app)?;
    let quit_item     = MenuItem::with_id(app, "quit",      "Quit",          true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &show_item, &hide_item,
            &sep1,
            &pom_start, &pom_pause, &pom_stop,
            &sep2,
            &settings_item,
            &sep3,
            &quit_item,
        ],
    )?;

    let _tray = TrayIconBuilder::with_id("mutsumi-tray")
        .tooltip("Mutsumi")
        .icon(app.default_window_icon().cloned().expect("no default icon"))
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            "hide" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            "pom_start" => {
                let s: State<SharedState> = app.state();
                s.0.lock().unwrap().pomodoro.start();
            }
            "pom_pause" => {
                let s: State<SharedState> = app.state();
                s.0.lock().unwrap().pomodoro.pause();
            }
            "pom_stop" => {
                let s: State<SharedState> = app.state();
                s.0.lock().unwrap().pomodoro.stop();
            }
            "settings" => {
                if let Some(w) = app.get_webview_window("settings") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
}
