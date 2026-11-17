/**
 * Window-control helpers for the in-pet overlays (chat / history).
 *
 * On Windows the pet ("main") window is created with `skipTaskbar: true`; while
 * a panel is open we expose a taskbar button so minimize has a restore target.
 * macOS intentionally keeps a permanent Dock icon and treats skip-taskbar as a
 * no-op, so the same calls preserve the product baseline there.
 */
import { getCurrentWindow } from '@tauri-apps/api/window'

/** Show (or hide) the main window's taskbar button. */
export async function setTaskbarVisible(visible: boolean): Promise<void> {
  try {
    await getCurrentWindow().setSkipTaskbar(!visible)
  } catch {
    /* best-effort — permission/older API; minimize still works regardless */
  }
}

/** Minimize the main window (its taskbar button is already shown while open). */
export async function minimizeWindow(): Promise<void> {
  try {
    await getCurrentWindow().minimize()
  } catch {
    /* best-effort */
  }
}
