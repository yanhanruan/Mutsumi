/**
 * useWeatherAvailable — reactive weather-service availability flag.
 *
 * Singleton pattern mirrors useAppConfig:
 *  • On first call, fetches the current status via `get_weather_status` and
 *    starts listening to `weather-status` events for live updates.
 *  • `available` is `null` while the initial query is in-flight (treat as
 *    "not yet known" — components should render optimistically).
 *  • Becomes `true` after the Rust thread has a successful fetch, or
 *    `false` after MAX_FAILURES (3) consecutive failures.
 *
 * Both PetWindow (badge visibility) and SettingsWindow (toggle enabled)
 * consume this composable so they stay in sync automatically.
 */

import { ref } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'

// ── Singleton state ──────────────────────────────────────────────────

/** `null` = not yet known; `true` = available; `false` = unavailable */
const available = ref<boolean | null>(null)

let _started = false

function startOnce(): void {
  if (_started) return
  _started = true

  // Live updates from the Rust fetch loop.
  listen<{ available: boolean }>('weather-status', e => {
    available.value = e.payload.available
  }).catch(() => { /* non-Tauri environments */ })

  // Seed with the current persisted state so the UI is correct on mount.
  invoke<boolean>('get_weather_status')
    .then(status => {
      // Only write if no live event has arrived yet.
      if (available.value === null) available.value = status
    })
    .catch(() => { /* non-Tauri environments */ })
}

// ── Composable ───────────────────────────────────────────────────────

export function useWeatherAvailable() {
  startOnce()
  return { weatherAvailable: available }
}
