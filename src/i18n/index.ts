/**
 * useI18n — lightweight reactive localisation composable.
 *
 * Architecture:
 *   - Module-level `locale` ref is shared across all component instances
 *     (singleton pattern — no provide/inject needed).
 *   - detectLocale() reads navigator.language once at module load time
 *     and maps zh-* → 'zh', ja → 'ja', everything else → 'en'.
 *   - setLocale() allows runtime overrides (settings toggle, tests).
 *   - useI18n() returns a computed `t` that re-evaluates reactively when
 *     locale changes, plus the raw locale ref.
 *
 * Supported locales: 'en' | 'zh' | 'ja'
 * Fallback: 'en' — navigator.language values not matching zh/ja default here.
 */

import { ref, computed } from 'vue'
import type { Translations } from './types'
import { en } from './locales/en'
import { zh } from './locales/zh'
import { ja } from './locales/ja'

// ── Locale type ────────────────────────────────────────────────────

export type Locale = 'en' | 'zh' | 'ja'

// ── Locale registry ────────────────────────────────────────────────

const LOCALES: Record<Locale, Translations> = { en, zh, ja }

// ── Detection ──────────────────────────────────────────────────────

/**
 * Detect the best matching supported locale from navigator.language.
 * Exported for unit testing.
 */
export function detectLocale(navLang: string = navigator.language): Locale {
  const lang = navLang.toLowerCase()
  if (lang.startsWith('zh')) return 'zh'
  if (lang.startsWith('ja')) return 'ja'
  return 'en'
}

// ── Singleton state ────────────────────────────────────────────────

const locale = ref<Locale>(detectLocale())

/**
 * Override the active locale at runtime.
 * Useful for settings language pickers and unit tests.
 */
export function setLocale(l: Locale): void {
  locale.value = l
}

// ── Composable ─────────────────────────────────────────────────────

/**
 * Returns:
 *   t      — computed Translations object for the current locale.
 *   locale — the raw locale ref (readable + writable via setLocale).
 */
export function useI18n() {
  const t = computed<Translations>(() => LOCALES[locale.value])
  return { t, locale }
}

// Re-export types so callers don't need to import from ./types directly.
export type { Translations }
