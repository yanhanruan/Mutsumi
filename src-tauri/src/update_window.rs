//! update_window.rs — the in-app "update available" pop-up and the metadata
//! hand-off between the daily checker (running in the pet window) and that
//! pop-up.
//!
//! The updater plugin's `Update` object lives in the JS heap of whichever
//! webview called `check()` and can't be serialized across windows. So the pet
//! window's daily check passes only *display metadata* (version + release
//! notes) through here; this module stashes it and opens the pop-up, which
//! reads it back via `get_pending_update` and re-runs `check()` itself only if
//! the user actually chooses to install. The manual "Check for updates" button
//! in the About window opens the pop-up with no metadata (`None`), so the
//! pop-up checks for itself.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, Manager, Runtime, State, WebviewUrl, WebviewWindowBuilder,
};

/// Display metadata for an available update, rendered by the pop-up window.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PendingUpdate {
    pub version: String,
    pub notes: String,
}

/// Managed state: the most recent update metadata handed to the pop-up.
/// `None` means "nothing pre-fetched" — the pop-up then checks for itself.
#[derive(Default)]
pub struct PendingUpdateState(pub Mutex<Option<PendingUpdate>>);

/// Show-or-build the update pop-up, stashing optional display metadata for it to
/// read on mount. Passing `None` clears any stale metadata so a manual check
/// starts fresh. If the window already exists, it is refocused and told (via the
/// `pending-update-changed` event) to re-read the freshly-stashed metadata.
///
/// This is `async` on purpose: a synchronous command runs on the main (UI)
/// thread, and building a WebView2 window there while the *calling* webview is
/// blocked awaiting the invoke wedges the message loop mid-creation (the window
/// frame appears, then the app goes "not responding"). Running async moves the
/// command off the main thread; `build()` then marshals to the main thread while
/// it stays free to pump WebView2's initialization messages.
#[tauri::command]
pub async fn open_update_window<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, PendingUpdateState>,
    version: Option<String>,
    notes: Option<String>,
) -> Result<(), String> {
    {
        let mut pending = state.0.lock().map_err(|e| e.to_string())?;
        *pending = version.map(|v| PendingUpdate {
            version: v,
            notes: notes.unwrap_or_default(),
        });
    }

    if let Some(w) = app.get_webview_window("update") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        let _ = app.emit("pending-update-changed", ());
        return Ok(());
    }

    WebviewWindowBuilder::new(
        &app,
        "update",
        WebviewUrl::App("index.html?window=update".into()),
    )
    .title("Mutsumi · Update")
    // Compact by default (checking / up-to-date / error states). The frontend
    // grows it to the full size only for the "update available" view, so the
    // one-line states don't open in a cavernous window.
    .inner_size(400.0, 250.0)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .center()
    .build()
    .map(|w| {
        let _ = w.set_focus();
    })
    .map_err(|e| e.to_string())
}

/// Return the stashed update metadata (or `null`) for the pop-up to render.
#[tauri::command]
pub fn get_pending_update(state: State<'_, PendingUpdateState>) -> Option<PendingUpdate> {
    state.0.lock().ok().and_then(|g| g.clone())
}
