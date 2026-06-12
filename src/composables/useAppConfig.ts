/**
 * useAppConfig — persistent cross-window app configuration.
 *
 * Stores user preferences in localStorage (both windows share the same
 * Tauri webview origin). Changes are broadcast via a Tauri `emit` event
 * so the pet window updates immediately when the settings window changes
 * a value.
 *
 * Singleton pattern: `config` is a single reactive ref initialised once
 * per window's Vue runtime. All `useAppConfig()` calls share it.
 */

import { ref } from 'vue'
import { emit, listen } from '@tauri-apps/api/event'
import type { Locale } from '../i18n'

// ── Types ────────────────────────────────────────────────────────────

export type CharacterSize = 'small' | 'medium' | 'large'

export interface AppConfig {
  characterSize: CharacterSize
  showWeather:   boolean
  /** Show the mini music controller (Lottie speakers badge + transport panel). */
  showMusic:     boolean
  /**
   * Manually-chosen UI language. `null` (the default) means "follow the
   * system" — the app uses navigator.language detection. Setting a locale
   * here overrides detection and persists across restarts.
   */
  language:      Locale | null
}

// ── Size dimensions ──────────────────────────────────────────────────

/**
 * Main-window logical pixel dimensions [width, height] for each size tier.
 * Default (large) matches the initial tauri.conf.json values (200 × 340).
 */
export const CHAR_SIZE_DIMS: Record<CharacterSize, [number, number]> = {
  small:  [140, 238],
  medium: [170, 289],
  large:  [200, 340],
}

// ── Storage ──────────────────────────────────────────────────────────

const STORAGE_KEY  = 'mutsumi_app_config_v2'
const EVENT_NAME   = 'app-config-changed'

const DEFAULT_CONFIG: AppConfig = {
  characterSize: 'medium',
  showWeather:   true,
  showMusic:     true,
  language:      null,   // null → follow system locale
}

function loadConfig(): AppConfig {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) return { ...DEFAULT_CONFIG, ...(JSON.parse(raw) as Partial<AppConfig>) }
  } catch { /* ignore parse errors */ }
  return { ...DEFAULT_CONFIG }
}

// ── Singleton reactive state ─────────────────────────────────────────

const config = ref<AppConfig>(loadConfig())

// Cross-window listener (settings → pet window and vice-versa).
// Started lazily on first composable call to avoid crashing test
// environments that don't mock the Tauri internals.
let _listenerStarted = false
function startListenerOnce(): void {
  if (_listenerStarted) return
  _listenerStarted = true
  listen<AppConfig>(EVENT_NAME, e => {
    config.value = e.payload
    localStorage.setItem(STORAGE_KEY, JSON.stringify(e.payload))
  }).catch(() => { /* no-op in non-Tauri environments */ })
}

// ── Composable ───────────────────────────────────────────────────────

export function useAppConfig() {
  // Start the cross-window listener the first time any window calls the composable.
  startListenerOnce()

  /**
   * Patch one or more config keys.
   * Immediately updates the reactive ref, persists to localStorage,
   * and broadcasts an event so the other window stays in sync.
   */
  async function updateConfig(patch: Partial<AppConfig>): Promise<void> {
    const next: AppConfig = { ...config.value, ...patch }
    config.value = next
    localStorage.setItem(STORAGE_KEY, JSON.stringify(next))
    await emit(EVENT_NAME, next)
  }

  return { config, updateConfig }
}
