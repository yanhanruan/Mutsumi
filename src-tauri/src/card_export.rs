//! card_export.rs — save a tarot card image to the user's Downloads folder.
//!
//! WebView2 ignores the browser `<a download>` flow, and native "Save As"
//! dialogs are unreliable from a Tauri command thread on Windows (COM/STA).
//! So we skip the dialog: the frontend fetches the image bytes and we write
//! them to the Downloads dir with std::fs (no fs-plugin scope needed in Rust).
//! Returns the saved path so the UI can confirm.

use tauri::{AppHandle, Manager};

#[tauri::command]
pub fn save_card_image(app: AppHandle, bytes: Vec<u8>, filename: String) -> Result<String, String> {
    // Strip anything that could escape the Downloads dir.
    let safe = filename.replace(['/', '\\', ':'], "_");
    let dir = app.path().download_dir().map_err(|e| e.to_string())?;
    let path = dir.join(safe);
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

/// Open the folder containing `path` in the OS file manager.
#[tauri::command]
pub fn reveal_in_folder(path: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::path::Path;
        let p = Path::new(&path);
        let dir = p.parent().unwrap_or(p);
        // Pass the directory as a single argument (handles spaces); explorer
        // opens that folder.
        std::process::Command::new("explorer")
            .arg(dir)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Ok(())
    }
}
