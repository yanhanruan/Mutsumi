/**
 * Tests for the pure helpers extracted from useHitTest.
 *
 * The composable itself (canvas, listeners, Tauri calls, DOM hit testing)
 * is integration-tested by running the app; the pure logic that decides
 * the click-through state is covered here without DOM/OS dependencies.
 */
import { describe, expect, it } from 'vitest'
import {
  ALPHA_OPEN,
  ALPHA_CLOSE,
  computeContainPlacement,
  isOverUiOverlay,
  shouldIgnore,
} from './useHitTest'

// ── shouldIgnore (hysteresis) ──────────────────────────────────────

describe('shouldIgnore — hysteresis decision', () => {
  it('starts non-ignoring, becomes click-through when alpha < ALPHA_OPEN', () => {
    expect(shouldIgnore(false, 0)).toBe(true)
    expect(shouldIgnore(false, ALPHA_OPEN - 1)).toBe(true)
  })

  it('starts non-ignoring, stays interactive at ALPHA_OPEN (boundary)', () => {
    expect(shouldIgnore(false, ALPHA_OPEN)).toBe(false)
  })

  it('starts non-ignoring, stays interactive above ALPHA_OPEN', () => {
    expect(shouldIgnore(false, ALPHA_OPEN + 1)).toBe(false)
    expect(shouldIgnore(false, ALPHA_CLOSE)).toBe(false)
    expect(shouldIgnore(false, 255)).toBe(false)
  })

  it('starts ignoring, stays click-through below ALPHA_CLOSE', () => {
    expect(shouldIgnore(true, 0)).toBe(true)
    expect(shouldIgnore(true, ALPHA_OPEN)).toBe(true)
    expect(shouldIgnore(true, ALPHA_CLOSE - 1)).toBe(true)
  })

  it('starts ignoring, becomes interactive at ALPHA_CLOSE', () => {
    expect(shouldIgnore(true, ALPHA_CLOSE)).toBe(false)
  })

  it('starts ignoring, becomes interactive above ALPHA_CLOSE', () => {
    expect(shouldIgnore(true, ALPHA_CLOSE + 1)).toBe(false)
    expect(shouldIgnore(true, 255)).toBe(false)
  })

  it('hysteresis: a value in the dead-zone preserves current state', () => {
    // The dead zone is [ALPHA_OPEN, ALPHA_CLOSE). In it:
    //   - if currently interactive, stay interactive
    //   - if currently ignoring,    stay ignoring
    const deadMid = Math.floor((ALPHA_OPEN + ALPHA_CLOSE) / 2)
    expect(shouldIgnore(false, deadMid)).toBe(false)
    expect(shouldIgnore(true,  deadMid)).toBe(true)
  })

  it('thresholds are configured: ALPHA_OPEN strictly less than ALPHA_CLOSE', () => {
    expect(ALPHA_OPEN).toBeLessThan(ALPHA_CLOSE)
  })
})

// ── computeContainPlacement (object-fit: contain math) ─────────────

describe('computeContainPlacement — CSS object-fit: contain math', () => {
  it('portrait image into landscape canvas: limited by height', () => {
    // 720×1280 image into 200×340 canvas:
    //   aspectImg = 0.5625, aspectCan = 0.588
    //   aspectImg < aspectCan → height-limited
    //   dh = 340, dw = 340 * 0.5625 = 191.25
    const r = computeContainPlacement(720, 1280, 200, 340)
    expect(r.dh).toBe(340)
    expect(r.dw).toBeCloseTo(191.25, 2)
    expect(r.dx).toBeCloseTo((200 - 191.25) / 2, 2)
    expect(r.dy).toBe(0)
  })

  it('landscape image into portrait canvas: limited by width', () => {
    // 1280×720 image into 200×340 canvas:
    //   aspectImg = 1.778, aspectCan = 0.588
    //   aspectImg > aspectCan → width-limited
    //   dw = 200, dh = 200 / 1.778 = 112.5
    const r = computeContainPlacement(1280, 720, 200, 340)
    expect(r.dw).toBe(200)
    expect(r.dh).toBeCloseTo(112.5, 2)
    expect(r.dx).toBe(0)
    expect(r.dy).toBeCloseTo((340 - 112.5) / 2, 2)
  })

  it('equal aspect ratios produce no letterboxing', () => {
    // 100×100 into 200×200 → fills the canvas
    const r = computeContainPlacement(100, 100, 200, 200)
    expect(r.dw).toBe(200)
    expect(r.dh).toBe(200)
    expect(r.dx).toBe(0)
    expect(r.dy).toBe(0)
  })

  it('exactly-matching aspect on differing dimensions still fills', () => {
    // 2:1 image, 4:2 canvas → fills entirely
    const r = computeContainPlacement(800, 400, 1600, 800)
    expect(r.dw).toBe(1600)
    expect(r.dh).toBe(800)
    expect(r.dx).toBe(0)
    expect(r.dy).toBe(0)
  })

  it('output is always centered', () => {
    const r = computeContainPlacement(1, 2, 100, 100)
    expect(r.dx + r.dw / 2).toBeCloseTo(50, 5)
    expect(r.dy + r.dh / 2).toBeCloseTo(50, 5)
  })
})

// ── isOverUiOverlay ────────────────────────────────────────────────

function createDiv(className: string): HTMLDivElement {
  const d = document.createElement('div')
  d.className = className
  return d
}

describe('isOverUiOverlay — UI overlay detection', () => {
  it('empty element list returns false', () => {
    expect(isOverUiOverlay([])).toBe(false)
  })

  it('returns true when any element has the pet-ui-overlay class', () => {
    const els = [createDiv('weather'), createDiv('pet-ui-overlay')]
    expect(isOverUiOverlay(els)).toBe(true)
  })

  it('returns true when class is one of several', () => {
    const els = [createDiv('weather badge pet-ui-overlay other')]
    expect(isOverUiOverlay(els)).toBe(true)
  })

  it('returns false when no elements have the class', () => {
    const els = [createDiv('a'), createDiv('b'), createDiv('c')]
    expect(isOverUiOverlay(els)).toBe(false)
  })

  it('ignores partial class-name matches (e.g. pet-ui-overlayer)', () => {
    const els = [createDiv('pet-ui-overlayer')]
    expect(isOverUiOverlay(els)).toBe(false)
  })

  it('finds the class anywhere in the list, not just first', () => {
    const els = [
      createDiv('a'),
      createDiv('b'),
      createDiv('c'),
      createDiv('pet-ui-overlay'),
    ]
    expect(isOverUiOverlay(els)).toBe(true)
  })
})
