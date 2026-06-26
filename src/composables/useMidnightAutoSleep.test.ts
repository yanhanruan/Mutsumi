/**
 * Tests for the pure crossedMidnight helper from useMidnightAutoSleep.ts.
 *
 * The composable itself is a thin setInterval wrapper around this predicate;
 * the day-rollover logic is the only part worth pinning down.
 */
import { describe, it, expect } from 'vitest'
import { crossedMidnight } from './useMidnightAutoSleep'

describe('crossedMidnight', () => {
  it('is false within the same day', () => {
    const a = new Date(2026, 5, 27, 23, 59, 30)
    const b = new Date(2026, 5, 27, 23, 59, 59)
    expect(crossedMidnight(a, b)).toBe(false)
  })

  it('is false for the identical instant', () => {
    const a = new Date(2026, 5, 27, 12, 0, 0)
    expect(crossedMidnight(a, a)).toBe(false)
  })

  it('is true across a midnight rollover', () => {
    const before = new Date(2026, 5, 27, 23, 59, 50)
    const after  = new Date(2026, 5, 28, 0, 0, 10)
    expect(crossedMidnight(before, after)).toBe(true)
  })

  it('is true across a month boundary', () => {
    const before = new Date(2026, 5, 30, 23, 59, 50)
    const after  = new Date(2026, 6, 1, 0, 0, 10)
    expect(crossedMidnight(before, after)).toBe(true)
  })

  it('is true across a year boundary', () => {
    const before = new Date(2026, 11, 31, 23, 59, 50)
    const after  = new Date(2027, 0, 1, 0, 0, 10)
    expect(crossedMidnight(before, after)).toBe(true)
  })

  it('detects the same wall-clock time on a different day', () => {
    const a = new Date(2026, 5, 27, 9, 0, 0)
    const b = new Date(2026, 5, 28, 9, 0, 0)
    expect(crossedMidnight(a, b)).toBe(true)
  })
})
