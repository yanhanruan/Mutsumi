//! Global-hotkey registry.
//!
//! Every global shortcut the app owns is declared once in [`DEFS`] and
//! registered in isolation: a collision with another app (the OS rejecting
//! `RegisterHotKey`) can only ever disable that one hotkey — never another
//! entry, and never startup. v1.5.1 hardened the single Ctrl+Alt+F binding
//! ad hoc; this generalizes the rule so any future hotkey inherits both the
//! isolation and the status reporting for free.
//!
//! Per-hotkey outcomes land in [`HotkeysState`] (managed) and reach the
//! frontend through the [`get_hotkey_status`] command, so Settings can tell
//! the user *why* a shortcut is dead instead of it failing silently.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Manager};

/// One hotkey the app wants bound.
struct HotkeyDef {
    /// Stable identifier the frontend keys off (never shown to users).
    id: &'static str,
    /// Accelerator in the plugin's parse syntax.
    accelerator: &'static str,
    /// User-facing form for UI text.
    display: &'static str,
    /// Called on every press of the registered shortcut.
    on_press: fn(&AppHandle),
}

/// The full set of global hotkeys this app ever binds. Add new hotkeys HERE —
/// isolated registration and Settings-visible status come for free.
const DEFS: &[HotkeyDef] = &[HotkeyDef {
    id: "toggle-flight",
    accelerator: "ctrl+alt+f",
    display: "Ctrl+Alt+F",
    on_press: |app| crate::flight::toggle_manual(app),
}];

/// Runtime status of one hotkey, as exposed to the frontend.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyStatus {
    pub id: String,
    /// User-facing accelerator ("Ctrl+Alt+F").
    pub accelerator: String,
    /// False when registration failed (another app owns the combination).
    pub active: bool,
}

/// Managed state: what happened to each hotkey at startup.
pub struct HotkeysState(Mutex<Vec<HotkeyStatus>>);

/// Tauri command: per-hotkey registration outcome, so Settings can explain
/// why a shortcut is unavailable this session.
#[tauri::command]
pub fn get_hotkey_status(state: tauri::State<HotkeysState>) -> Vec<HotkeyStatus> {
    state.0.lock().unwrap().clone()
}

/// Register the global-shortcut plugin and every hotkey in [`DEFS`].
///
/// The plugin is initialized with NO pre-bound shortcuts, so its init cannot
/// fail on a collision; each hotkey is then bound on its own, and a failure
/// is recorded + logged instead of propagated. Hotkeys are a nice-to-have,
/// never a launch requirement — startup always succeeds.
pub fn init(app: &AppHandle) -> tauri::Result<()> {
    #[cfg(not(desktop))]
    let statuses: Vec<HotkeyStatus> = Vec::new();

    #[cfg(desktop)]
    let statuses = {
        use tauri_plugin_global_shortcut::{
            Builder as ShortcutBuilder, GlobalShortcutExt, ShortcutState,
        };
        app.plugin(ShortcutBuilder::new().build())?;

        DEFS.iter()
            .map(|def| {
                let on_press = def.on_press;
                let result = app.global_shortcut().on_shortcut(
                    def.accelerator,
                    move |app, _shortcut, event| {
                        if event.state() == ShortcutState::Pressed {
                            on_press(app);
                        }
                    },
                );
                if let Err(e) = &result {
                    log::warn!(
                        "global shortcut {} ({}) unavailable (likely already in use \
                         by another app); disabled for this session: {e}",
                        def.display,
                        def.id,
                    );
                }
                HotkeyStatus {
                    id: def.id.into(),
                    accelerator: def.display.into(),
                    active: result.is_ok(),
                }
            })
            .collect()
    };

    app.manage(HotkeysState(Mutex::new(statuses)));
    Ok(())
}
