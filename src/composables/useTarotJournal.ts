/**
 * useTarotJournal — localStorage-backed tarot persistence.
 *
 * Two pieces of state, both private to the pet window (no cross-window
 * broadcast needed — only the tarot overlay reads them):
 *
 *   - Daily card: the first reading of each local day is remembered as the
 *     "card of the day" and re-shown when the overlay reopens that day.
 *   - History: a capped, newest-first log of recent draws.
 *
 * Mirrors the localStorage pattern in useAppConfig.ts. Singleton reactive
 * history ref so all overlay instances in a window share one list.
 */
import { ref } from 'vue'

// ── Types ────────────────────────────────────────────────────────────

/** A recorded draw: card id (0–77) + orientation. */
export interface JournalEntry {
  id:       number
  reversed: boolean
  /** Epoch ms when the card was revealed. */
  ts:       number
}

interface DailyRecord {
  /** Local YYYY-MM-DD the card was drawn on. */
  date:     string
  id:       number
  reversed: boolean
}

// ── Storage keys + cap ───────────────────────────────────────────────

const DAILY_KEY   = 'mutsumi_tarot_daily'
const HISTORY_KEY = 'mutsumi_tarot_history'
const HISTORY_CAP = 30

// ── Pure date helper (exported for tests) ────────────────────────────

/**
 * Local calendar day as `YYYY-MM-DD`. Uses local time (not UTC) so the
 * "card of the day" rolls over at the user's midnight.
 */
export function localDateKey(d: Date = new Date()): string {
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  return `${y}-${m}-${day}`
}

// ── Pure history reducer (exported for tests) ────────────────────────

/** Prepend a new entry and trim to the cap. Pure — does not touch storage. */
export function prependCapped(
  list: readonly JournalEntry[],
  entry: JournalEntry,
  cap: number = HISTORY_CAP,
): JournalEntry[] {
  return [entry, ...list].slice(0, cap)
}

// ── Storage I/O (guarded for non-browser test envs) ──────────────────

function readJSON<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key)
    if (raw) return JSON.parse(raw) as T
  } catch { /* ignore parse / access errors */ }
  return fallback
}

function writeJSON(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value))
  } catch { /* ignore quota / access errors */ }
}

// ── Singleton reactive history ───────────────────────────────────────

const history = ref<JournalEntry[]>(readJSON<JournalEntry[]>(HISTORY_KEY, []))

// ── Composable ───────────────────────────────────────────────────────

export function useTarotJournal() {
  /** Today's recorded card, or null if none was drawn today yet. */
  function getToday(): DailyRecord | null {
    const rec = readJSON<DailyRecord | null>(DAILY_KEY, null)
    return rec && rec.date === localDateKey() ? rec : null
  }

  /** Record today's card — only if none has been recorded today (first wins). */
  function recordDailyIfAbsent(id: number, reversed: boolean): void {
    if (getToday()) return
    writeJSON(DAILY_KEY, { date: localDateKey(), id, reversed } satisfies DailyRecord)
  }

  /** Append a reveal to the history log (newest first, capped). */
  function addEntry(id: number, reversed: boolean): void {
    history.value = prependCapped(history.value, { id, reversed, ts: Date.now() })
    writeJSON(HISTORY_KEY, history.value)
  }

  return { history, getToday, recordDailyIfAbsent, addEntry }
}
