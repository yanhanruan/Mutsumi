/**
 * Tests for the pure update-scheduling policy (src/config/updatePolicy.ts).
 *
 * "now" is pinned to a fixed epoch so the day math is exact and clock-free.
 */
import { describe, it, expect } from 'vitest'
import {
  CHECK_INTERVAL_HOURS,
  SNOOZE_MAX_DAYS,
  SNOOZE_MIN_DAYS,
  clampSnoozeDays,
  computeSnoozeUntil,
  isSnoozed,
  shouldCheckNow,
} from './updatePolicy'

const HOUR = 60 * 60 * 1000
const DAY = 24 * HOUR
// 2026-07-05T00:00:00.000Z — arbitrary fixed reference point.
const NOW = Date.UTC(2026, 6, 5)

// ── clampSnoozeDays() ──────────────────────────────────────────────

describe('clampSnoozeDays()', () => {
  it('passes through in-range whole days', () => {
    expect(clampSnoozeDays(1)).toBe(1)
    expect(clampSnoozeDays(7)).toBe(7)
    expect(clampSnoozeDays(30)).toBe(30)
  })

  it('clamps below the minimum up to SNOOZE_MIN_DAYS', () => {
    expect(clampSnoozeDays(0)).toBe(SNOOZE_MIN_DAYS)
    expect(clampSnoozeDays(-5)).toBe(SNOOZE_MIN_DAYS)
  })

  it('clamps above the maximum down to SNOOZE_MAX_DAYS', () => {
    expect(clampSnoozeDays(31)).toBe(SNOOZE_MAX_DAYS)
    expect(clampSnoozeDays(9999)).toBe(SNOOZE_MAX_DAYS)
  })

  it('rounds fractional days to whole days', () => {
    expect(clampSnoozeDays(2.4)).toBe(2)
    expect(clampSnoozeDays(2.6)).toBe(3)
  })

  it('falls back to the minimum for non-finite input', () => {
    expect(clampSnoozeDays(NaN)).toBe(SNOOZE_MIN_DAYS)
    expect(clampSnoozeDays(Infinity)).toBe(SNOOZE_MIN_DAYS)
  })
})

// ── shouldCheckNow() ───────────────────────────────────────────────

describe('shouldCheckNow()', () => {
  it('checks when it has never checked before', () => {
    expect(shouldCheckNow(NOW, null)).toBe(true)
  })

  it('does not check again before the interval elapses', () => {
    expect(shouldCheckNow(NOW, NOW - 1 * HOUR)).toBe(false)
    expect(shouldCheckNow(NOW, NOW - 23 * HOUR)).toBe(false)
  })

  it('checks once the interval has elapsed', () => {
    expect(shouldCheckNow(NOW, NOW - CHECK_INTERVAL_HOURS * HOUR)).toBe(true)
    expect(shouldCheckNow(NOW, NOW - 48 * HOUR)).toBe(true)
  })

  it('respects a custom interval', () => {
    expect(shouldCheckNow(NOW, NOW - 2 * HOUR, 1)).toBe(true)
    expect(shouldCheckNow(NOW, NOW - 30 * 60 * 1000, 1)).toBe(false)
  })

  it('waits when the last-check timestamp is in the future (clock skew)', () => {
    expect(shouldCheckNow(NOW, NOW + 5 * HOUR)).toBe(false)
  })
})

// ── isSnoozed() ────────────────────────────────────────────────────

describe('isSnoozed()', () => {
  it('is not snoozed when there is no deadline', () => {
    expect(isSnoozed(NOW, null)).toBe(false)
  })

  it('is snoozed while now is before the deadline', () => {
    expect(isSnoozed(NOW, NOW + 3 * DAY)).toBe(true)
  })

  it('is no longer snoozed once the deadline has passed', () => {
    expect(isSnoozed(NOW, NOW - 1 * DAY)).toBe(false)
  })

  it('is not snoozed exactly at the deadline', () => {
    expect(isSnoozed(NOW, NOW)).toBe(false)
  })
})

// ── computeSnoozeUntil() ───────────────────────────────────────────

describe('computeSnoozeUntil()', () => {
  it('returns an ISO date N days ahead', () => {
    const iso = computeSnoozeUntil(NOW, 7)
    expect(iso).toBe(new Date(NOW + 7 * DAY).toISOString())
  })

  it('clamps the horizon to the allowed range', () => {
    expect(computeSnoozeUntil(NOW, 0)).toBe(new Date(NOW + SNOOZE_MIN_DAYS * DAY).toISOString())
    expect(computeSnoozeUntil(NOW, 999)).toBe(new Date(NOW + SNOOZE_MAX_DAYS * DAY).toISOString())
  })

  it('produces a deadline that reads as snoozed until it passes', () => {
    const until = Date.parse(computeSnoozeUntil(NOW, 5))
    expect(isSnoozed(NOW, until)).toBe(true)
    expect(isSnoozed(until, until)).toBe(false)
  })
})
