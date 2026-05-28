/**
 * Tests for the pure helpers in useBalloonFlight.
 *
 * All functions are pure (no DOM/Tauri/RAF), so tests run in Vitest/happy-dom
 * without any mocking. The `randVec` parameter of blendVectors is injected to
 * make direction math fully deterministic.
 */
import { describe, it, expect } from 'vitest'
import {
  SPEED,
  CENTER_WEIGHT,
  RANDOM_WEIGHT,
  SPRITE_W,
  SPRITE_H,
  randomUnitVec,
  normalize,
  blendVectors,
  applyWallForce,
  stepPosition,
} from './useBalloonFlight'

// ── randomUnitVec ──────────────────────────────────────────────────

describe('randomUnitVec — unit-length random direction', () => {
  it('every call returns a vector of length ≈ 1', () => {
    for (let i = 0; i < 20; i++) {
      const { vx, vy } = randomUnitVec()
      expect(Math.sqrt(vx * vx + vy * vy)).toBeCloseTo(1, 5)
    }
  })
})

// ── normalize ──────────────────────────────────────────────────────

describe('normalize — vector normalization', () => {
  it('3-4-5 triangle gives (0.6, 0.8)', () => {
    const { vx, vy } = normalize(3, 4)
    expect(vx).toBeCloseTo(0.6, 5)
    expect(vy).toBeCloseTo(0.8, 5)
  })

  it('negative components are preserved', () => {
    const { vx, vy } = normalize(-3, -4)
    expect(vx).toBeCloseTo(-0.6, 5)
    expect(vy).toBeCloseTo(-0.8, 5)
  })

  it('already-unit vector is unchanged', () => {
    const { vx, vy } = normalize(1, 0)
    expect(vx).toBeCloseTo(1, 5)
    expect(vy).toBeCloseTo(0, 5)
  })

  it('near-zero input returns a unit-length fallback (no NaN)', () => {
    const { vx, vy } = normalize(0, 0)
    const len = Math.sqrt(vx * vx + vy * vy)
    expect(len).toBeCloseTo(1, 5)
    expect(vx).not.toBeNaN()
    expect(vy).not.toBeNaN()
  })

  it('output always has length ≈ 1 for various inputs', () => {
    const cases: [number, number][] = [[1, 1], [5, 0], [0, 3], [-2, 7], [0.001, 0.001]]
    for (const [x, y] of cases) {
      const { vx, vy } = normalize(x, y)
      expect(Math.sqrt(vx * vx + vy * vy)).toBeCloseTo(1, 5)
    }
  })
})

// ── blendVectors ───────────────────────────────────────────────────

describe('blendVectors — center-seek + random blend', () => {
  it('output magnitude is exactly SPEED regardless of direction', () => {
    const positions: [number, number][] = [[0, 0], [100, 170], [199, 319], [50, 50]]
    for (const [px, py] of positions) {
      const v = blendVectors(px, py, 200, 340, { vx: 1, vy: 0 })
      expect(Math.sqrt(v.vx * v.vx + v.vy * v.vy)).toBeCloseTo(SPEED, 5)
    }
  })

  it('when random and center both point right, vx is positive', () => {
    // Sprite at left edge, center is to the right → both forces rightward
    const v = blendVectors(0, 170, 200, 340, { vx: 1, vy: 0 })
    expect(v.vx).toBeGreaterThan(0)
  })

  it('when random points left but sprite is already left, center wins partially', () => {
    // Sprite at x=0 (left), center-seek → right (1, 0) × 0.3
    // Random → left (−1, 0) × 0.7   →  net raw = 0.3 − 0.7 = −0.4 → leftward
    const v = blendVectors(0, 170, 200, 340, { vx: -1, vy: 0 })
    expect(v.vx).toBeLessThan(0)
  })

  it('when random and center-seek align, result is in the same direction', () => {
    // Sprite at right edge → center-seek points left (−1, 0)
    // Random also leftward → both forces agree
    const v = blendVectors(199, 170, 200, 340, { vx: -1, vy: 0 })
    expect(v.vx).toBeLessThan(0)
  })

  it('CENTER_WEIGHT + RANDOM_WEIGHT equal 1 (blend is balanced)', () => {
    expect(CENTER_WEIGHT + RANDOM_WEIGHT).toBeCloseTo(1, 10)
  })
})

// ── applyWallForce ─────────────────────────────────────────────────

describe('applyWallForce — anti-sticking axis forcing', () => {
  it('left wall: negative vx becomes positive', () => {
    const v = applyWallForce(-1.2, 0.5, true, false, false, false)
    expect(v.vx).toBeCloseTo(1.2, 5)   // forced rightward
    expect(v.vy).toBeCloseTo(0.5, 5)   // vy unchanged
  })

  it('right wall: positive vx becomes negative', () => {
    const v = applyWallForce(1.2, 0.5, false, true, false, false)
    expect(v.vx).toBeCloseTo(-1.2, 5)  // forced leftward
    expect(v.vy).toBeCloseTo(0.5, 5)
  })

  it('top wall: negative vy becomes positive (downward)', () => {
    const v = applyWallForce(0.5, -1.0, false, false, true, false)
    expect(v.vy).toBeCloseTo(1.0, 5)   // forced downward
    expect(v.vx).toBeCloseTo(0.5, 5)
  })

  it('bottom wall: positive vy becomes negative (upward)', () => {
    const v = applyWallForce(0.5, 1.0, false, false, false, true)
    expect(v.vy).toBeCloseTo(-1.0, 5)  // forced upward
    expect(v.vx).toBeCloseTo(0.5, 5)
  })

  it('no wall hit: vector is returned unchanged', () => {
    const v = applyWallForce(1.2, -0.8, false, false, false, false)
    expect(v.vx).toBeCloseTo(1.2, 5)
    expect(v.vy).toBeCloseTo(-0.8, 5)
  })

  it('already moving away from left wall stays unchanged in magnitude', () => {
    // vx already positive, hit left → abs(vx) = vx (no change)
    const v = applyWallForce(1.5, 0, true, false, false, false)
    expect(v.vx).toBeCloseTo(1.5, 5)
  })

  it('corner (left + top): both axes forced away from the corner', () => {
    const v = applyWallForce(-0.8, -0.6, true, false, true, false)
    expect(v.vx).toBeGreaterThan(0)   // rightward
    expect(v.vy).toBeGreaterThan(0)   // downward
  })

  it('corner (right + bottom): both axes forced away', () => {
    const v = applyWallForce(0.8, 0.6, false, true, false, true)
    expect(v.vx).toBeLessThan(0)      // leftward
    expect(v.vy).toBeLessThan(0)      // upward
  })
})

// ── stepPosition ──────────────────────────────────────────────────

describe('stepPosition — movement + boundary clamping', () => {
  it('open space: position advances by velocity, no collision', () => {
    const r = stepPosition(50, 50, 2, 3, 200, 340)
    expect(r.nx).toBe(52)
    expect(r.ny).toBe(53)
    expect(r.hitLeft).toBe(false)
    expect(r.hitRight).toBe(false)
    expect(r.hitTop).toBe(false)
    expect(r.hitBot).toBe(false)
  })

  it('moving into left wall: nx clamped to 0, hitLeft set', () => {
    const r = stepPosition(1, 50, -5, 0, 200, 340)
    expect(r.nx).toBe(0)
    expect(r.hitLeft).toBe(true)
    expect(r.hitRight).toBe(false)
  })

  it('moving into right wall: nx clamped, hitRight set', () => {
    const r = stepPosition(195, 50, 10, 0, 200, 340)
    expect(r.nx).toBe(200 - SPRITE_W)
    expect(r.hitRight).toBe(true)
    expect(r.hitLeft).toBe(false)
  })

  it('moving into top wall: ny clamped to 0, hitTop set', () => {
    const r = stepPosition(50, 1, 0, -5, 200, 340)
    expect(r.ny).toBe(0)
    expect(r.hitTop).toBe(true)
    expect(r.hitBot).toBe(false)
  })

  it('moving into bottom wall: ny clamped, hitBot set', () => {
    const r = stepPosition(50, 330, 0, 20, 200, 340)
    expect(r.ny).toBe(340 - SPRITE_H)
    expect(r.hitBot).toBe(true)
    expect(r.hitTop).toBe(false)
  })

  it('sitting exactly at x=0 moving left: still reports hitLeft', () => {
    const r = stepPosition(0, 50, -1, 0, 200, 340)
    expect(r.hitLeft).toBe(true)
    expect(r.nx).toBe(0)
  })

  it('deep interior movement reports no collision', () => {
    // With SPRITE_W=120, SPRITE_H=120, the safe x range is [0, 80] and y range [0, 220].
    // (40, 100) + (1, 1) → (41, 101); right edge = 161 < 200, bottom = 221 < 340.
    const r = stepPosition(40, 100, 1, 1, 200, 340)
    expect(r.hitLeft || r.hitRight || r.hitTop || r.hitBot).toBe(false)
  })

  it('corner collision sets both hit flags', () => {
    const r = stepPosition(1, 1, -5, -5, 200, 340)
    expect(r.hitLeft).toBe(true)
    expect(r.hitTop).toBe(true)
    expect(r.nx).toBe(0)
    expect(r.ny).toBe(0)
  })
})
