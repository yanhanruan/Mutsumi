/**
 * useTarotJournal — tests for the pure helpers and the daily/history logic.
 *
 * The composable holds a module-level singleton `history` ref, so we
 * reset modules + a stub localStorage between cases that touch storage
 * (same pattern as useWeatherAvailable.test.ts).
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { localDateKey, prependCapped, type JournalEntry } from './useTarotJournal'

// ── Pure helpers ─────────────────────────────────────────────────────

describe('localDateKey', () => {
  it('formats local date as zero-padded YYYY-MM-DD', () => {
    expect(localDateKey(new Date(2026, 0, 5))).toBe('2026-01-05')   // Jan = month 0
    expect(localDateKey(new Date(2026, 11, 31))).toBe('2026-12-31')
  })

  it('uses local time, not UTC', () => {
    // Construct a local date explicitly; the key should match its local fields.
    const d = new Date(2026, 5, 3, 23, 30) // Jun 3 2026, 23:30 local
    expect(localDateKey(d)).toBe('2026-06-03')
  })
})

describe('prependCapped', () => {
  const mk = (id: number): JournalEntry => ({ id, reversed: false, ts: id })

  it('prepends newest first', () => {
    const out = prependCapped([mk(1), mk(2)], mk(3))
    expect(out.map(e => e.id)).toEqual([3, 1, 2])
  })

  it('trims to the cap, dropping the oldest', () => {
    const start = Array.from({ length: 5 }, (_, i) => mk(i))
    const out = prependCapped(start, mk(99), 3)
    expect(out.map(e => e.id)).toEqual([99, 0, 1])
    expect(out).toHaveLength(3)
  })

  it('does not mutate the input list', () => {
    const start = [mk(1)]
    prependCapped(start, mk(2), 10)
    expect(start.map(e => e.id)).toEqual([1])
  })
})

// ── Daily + history via the composable (storage-backed) ──────────────

describe('useTarotJournal (storage-backed)', () => {
  beforeEach(() => {
    vi.resetModules()
    const store = new Map<string, string>()
    vi.stubGlobal('localStorage', {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => { store.set(k, v) },
      removeItem: (k: string) => { store.delete(k) },
      clear: () => store.clear(),
    })
  })

  it('records the daily card only once per day (first wins)', async () => {
    const { useTarotJournal } = await import('./useTarotJournal')
    const j = useTarotJournal()
    expect(j.getToday()).toBeNull()

    j.recordDailyIfAbsent(7, false)
    expect(j.getToday()).toMatchObject({ id: 7, reversed: false })

    // A second card the same day must NOT overwrite the first.
    j.recordDailyIfAbsent(13, true)
    expect(j.getToday()).toMatchObject({ id: 7, reversed: false })
  })

  it('appends to history newest-first and persists', async () => {
    const { useTarotJournal } = await import('./useTarotJournal')
    const j = useTarotJournal()
    j.addEntry(1, false)
    j.addEntry(2, true)
    expect(j.history.value.map(e => e.id)).toEqual([2, 1])
    expect(j.history.value[0]).toMatchObject({ id: 2, reversed: true })
  })

  it('counts draws per day and exposes a per-day cap', async () => {
    const mod = await import('./useTarotJournal')
    const j = mod.useTarotJournal()
    expect(j.drawsToday.value).toBe(0)
    expect(j.bumpDraws()).toBe(1)
    expect(j.bumpDraws()).toBe(2)
    expect(j.drawsToday.value).toBe(2)
    expect(mod.MAX_DRAWS_PER_DAY).toBe(3)
  })
})
