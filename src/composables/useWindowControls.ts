/**
 * Window-control helpers for the in-pet overlays (chat / history).
 *
 * The pet ("main") window is created with `skipTaskbar: true` — it floats as a
 * borderless companion with no taskbar button. While a panel (chat / history) is
 * open we expose a taskbar button so the window behaves like a normal app window:
 * it shows on the taskbar and a minimize parks it there (instead of stranding a
 * skip-taskbar window with no way to restore it). When the panel closes we hide
 * the taskbar button again so the pet goes back to floating cleanly.
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
