//! System tray icon with show/hide/pomodoro/settings/quit menu.
//!
//! The menu is built from localised labels; `rebuild_tray()` rebuilds it
//! when the frontend reports the detected system locale via `set_tray_locale`.
//!
//! The settings window is created on demand (not pre-created at startup) so
//! opening it never silently fails.

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, Runtime, State,
    WebviewUrl, WebviewWindowBuilder,
};

use crate::app_state::SharedState;

// ── Localised label sets ────────────────────────────────────────────

struct TrayLabels {
    show:      &'static str,
    hide:      &'static str,
    pom_start: &'static str,
    pom_pause: &'static str,
    pom_stop:  &'static str,
    settings:  &'static str,
    quit:      &'static str,
}

fn labels_for_locale(locale: &str) -> TrayLabels {
    match locale {
        "zh" => TrayLabels {
            show:      "显示",
            hide:      "隐藏",
            pom_start: "番茄钟: 开始",
            pom_pause: "番茄钟: 暂停",
            pom_stop:  "番茄钟: 停止",
            settings:  "设置…",
            quit:      "退出",
        },
        "ja" => TrayLabels {
            show:      "表示",
            hide:      "非表示",
            pom_start: "ポモドーロ: 開始",
            pom_pause: "ポモドーロ: 一時停止",
            pom_stop:  "ポモドーロ: 停止",
            settings:  "設定…",
            quit:      "終了",
        },
        _ => TrayLabels {
            show:      "Show",
            hide:      "Hide",
            pom_start: "Pomodoro: Start",
            pom_pause: "Pomodoro: Pause",
            pom_stop:  "Pomodoro: Stop",
            settings:  "Settings…",
            quit:      "Quit",
        },
    }
}

// ── Menu construction ───────────────────────────────────────────────

fn build_tray_menu<R: Runtime>(
    app: &AppHandle<R>,
    labels: &TrayLabels,
) -> tauri::Result<Menu<R>> {
    let show_item     = MenuItem::with_id(app, "show",      labels.show,      true, None::<&str>)?;
    let hide_item     = MenuItem::with_id(app, "hide",      labels.hide,      true, None::<&str>)?;
    let sep1          = PredefinedMenuItem::separator(app)?;
    let pom_start     = MenuItem::with_id(app, "pom_start", labels.pom_start, true, None::<&str>)?;
    let pom_pause     = MenuItem::with_id(app, "pom_pause", labels.pom_pause, true, None::<&str>)?;
    let pom_stop      = MenuItem::with_id(app, "pom_stop",  labels.pom_stop,  true, None::<&str>)?;
    let sep2          = PredefinedMenuItem::separator(app)?;
    let settings_item = MenuItem::with_id(app, "settings",  labels.settings,  true, None::<&str>)?;
    let sep3          = PredefinedMenuItem::separator(app)?;
    let quit_item     = MenuItem::with_id(app, "quit",      labels.quit,      true, None::<&str>)?;

    Menu::with_items(
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
    )
}

// ── Public API ──────────────────────────────────────────────────────

/// Build and register the tray icon (English labels by default).
/// Call once from `setup()`. The locale is corrected later by the frontend
/// via `set_tray_locale` once `navigator.language` is known.
pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let labels = labels_for_locale("en");
    let menu   = build_tray_menu(app, &labels)?;

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
                // Show existing window or create on demand — never silently fails.
                if let Some(w) = app.get_webview_window("settings") {
                    let _ = w.show();
                    let _ = w.set_focus();
                } else {
                    let result = WebviewWindowBuilder::new(
                        app,
                        "settings",
                        WebviewUrl::App("index.html?window=settings".into()),
                    )
                    .title("Mutsumi · Settings")
                    .inner_size(360.0, 460.0)
                    .resizable(false)
                    // Custom Vue titlebar takes over — no OS chrome.
                    .decorations(false)
                    .transparent(true)
                    .shadow(false)
                    .center()
                    .build();

                    if let Ok(w) = result {
                        let _ = w.set_focus();
                    }
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
}

/// Rebuild the tray menu with labels for the given locale.
/// Called from the `set_tray_locale` Tauri command after the frontend
/// detects the system language via `navigator.language`.
pub fn rebuild_tray<R: Runtime>(app: &AppHandle<R>, locale: &str) {
    if let Some(tray) = app.tray_by_id("mutsumi-tray") {
        if let Ok(menu) = build_tray_menu(app, &labels_for_locale(locale)) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}
