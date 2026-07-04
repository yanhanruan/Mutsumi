/**
 * Tests for the pure playback helpers exported from useAnimator.ts.
 * The animation data + policy tests (sequence shapes, chains, baseline)
 * live with their module in src/config/animations.test.ts.
 */
import { describe, it, expect } from 'vitest'
import { randBetween, resolveHoldCycle } from './useAnimator'

// ── randBetween() ──────────────────────────────────────────────────

describe('randBetween()', () => {
  it('maps the rng endpoints to [min, max)', () => {
    expect(randBetween(2000, 3000, () => 0)).toBe(2000)
    expect(randBetween(2000, 3000, () => 0.5)).toBe(2500)
    expect(randBetween(4000, 9000, () => 0.999)).toBeCloseTo(8995, 0)
  })

  it('stays within range for arbitrary rng values', () => {
    for (const r of [0, 0.1, 0.37, 0.5, 0.83, 0.9999]) {
      const v = randBetween(2000, 3000, () => r)
      expect(v).toBeGreaterThanOrEqual(2000)
      expect(v).toBeLessThan(3000)
    }
  })
})

// ── resolveHoldCycle() ─────────────────────────────────────────────

describe('resolveHoldCycle()', () => {
  it('converts 1-based anchors to a 0-based set with first/last', () => {
    const r = resolveHoldCycle({ anchors: [1, 68, 133, 144, 192], holdMinMs: 1, holdMaxMs: 2 }, 192)
    expect(r).toBeDefined()
    expect([...r!.anchors].sort((a, b) => a - b)).toEqual([0, 67, 132, 143, 191])
    expect(r!.first).toBe(0)
    expect(r!.last).toBe(191)
    expect(r!.holdMinMs).toBe(1)
    expect(r!.holdMaxMs).toBe(2)
  })

  it('drops anchors outside [1, frameCount]', () => {
    const r = resolveHoldCycle({ anchors: [0, 1, 50, 300], holdMinMs: 1, holdMaxMs: 2 }, 192)
    // 0 → -1 (dropped), 300 → 299 (dropped); keep 1→0 and 50→49.
    expect([...r!.anchors].sort((a, b) => a - b)).toEqual([0, 49])
    expect(r!.last).toBe(49)
  })

  it('returns undefined when there is nothing to hold on', () => {
    expect(resolveHoldCycle(undefined, 192)).toBeUndefined()
    expect(resolveHoldCycle({ anchors: [], holdMinMs: 1, holdMaxMs: 2 }, 192)).toBeUndefined()
    expect(resolveHoldCycle({ anchors: [500], holdMinMs: 1, holdMaxMs: 2 }, 192)).toBeUndefined()
  })
})
