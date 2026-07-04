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

/** Search engine key — kebab-case, matches the Rust `SearchEngine::from_str`. */
export type SearchEngineKey = 'duckduckgo' | 'bing-cn' | 'bing' | 'google' | 'baidu'

/** Selectable chat model — the exact DashScope model id sent as `chat_model`. */
export type ChatModelKey = 'qwen3.7-max' | 'qwen3.7-plus' | 'qwen3.6-flash'

export interface AppConfig {
  characterSize: CharacterSize
  showWeather:   boolean
  /** Show the mini music controller (Lottie speakers badge + transport panel). */
  showMusic:     boolean
  /** Show the drifting "zzz" effect while she sleeps. */
  showZzz:       boolean
  /** Flying-mode screensaver: fly after `flyingWaitMins` of system idle. */
  flyingScreensaver: boolean
  /** Idle wait before the flying screensaver starts, in minutes (6–30). */
  flyingWaitMins:    number
  /**
   * Manually-chosen UI language. `null` (the default) means "follow the
   * system" — the app uses navigator.language detection. Setting a locale
   * here overrides detection and persists across restarts.
   */
  language:      Locale | null
  /** Primary web-search engine for chat search-enhancement. */
  searchEngine:  SearchEngineKey
  /** Master on/off for chat web search (applied to the backend at startup). */
  searchEnabled: boolean
  /** Active chat model (applied to the backend at startup + on change). */
  chatModel:     ChatModelKey
}

// ── Flying screensaver bounds ────────────────────────────────────────

/**
 * Allowed flying-screensaver wait range in minutes. Kept in sync with the
 * clamp in src-tauri/src/flight.rs (flight_set_screensaver).
 */
export const FLYING_WAIT_MIN_MINS = 6
export const FLYING_WAIT_MAX_MINS = 30

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

/**
 * Main-window logical pixel dimensions [width, height] while the system state
 * overlay is open. This matches the logic of TAROT_WINDOW_DIMS to prevent
 * clipping. The panel stays the height of the live-status page on both tabs;
 * the taller Hardware spec sheet scrolls inside that box rather than growing
 * the panel, so these dimensions only need to fit the status layout.
 */
export const SYS_WINDOW_DIMS: Record<CharacterSize, [number, number]> = {
  small:  [280, 380],
  medium: [320, 430],
  large:  [360, 480],
}

// ── Storage ──────────────────────────────────────────────────────────

const STORAGE_KEY  = 'mutsumi_app_config_v2'
const EVENT_NAME   = 'app-config-changed'

const DEFAULT_CONFIG: AppConfig = {
  characterSize: 'medium',
  showWeather:   true,
  showMusic:     true,
  showZzz:       true,
  flyingScreensaver: true,
  flyingWaitMins:    10,   // matches flight::DEFAULT_WAIT_SECS (600 s)
  language:      null,          // null → follow system locale
  searchEngine:  'duckduckgo',  // fast + scrape-friendly default (matches Rust)
  searchEnabled: true,          // web search on by default (matches prior behavior)
  chatModel:     'qwen3.7-plus',// balanced default
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
