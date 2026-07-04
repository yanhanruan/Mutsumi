/**
 * Tests for the pure mapping in useFlightCollision.
 * (The canvas alpha scan is exercised in the running app, not here —
 * happy-dom has no real 2D raster context.)
 */
import { describe, it, expect } from 'vitest'
import {
  boxToWindowInsets,
  insetsDiffer,
  SEND_EPSILON,
  type OpaqueBox,
  type EdgeInsets,
} from './useFlightCollision'

const FULL: OpaqueBox = { x0: 0, y0: 0, x1: 1, y1: 1 }

describe('boxToWindowInsets', () => {
  it('fully-opaque frame at matching aspect gives zero insets', () => {
    const ins = boxToWindowInsets(FULL, 100, 100, 200, 200)
    expect(ins).toEqual({ left: 0, top: 0, right: 0, bottom: 0 })
  })

  it('centered box at matching aspect maps margins through directly', () => {
    const box: OpaqueBox = { x0: 0.25, y0: 0.25, x1: 0.75, y1: 0.75 }
    const ins = boxToWindowInsets(box, 100, 100, 200, 200)
    expect(ins).toEqual({ left: 0.25, top: 0.25, right: 0.25, bottom: 0.25 })
  })

  it('asymmetric box keeps each edge distinct', () => {
    const box: OpaqueBox = { x0: 0.1, y0: 0.2, x1: 0.9, y1: 1.0 }
    const ins = boxToWindowInsets(box, 100, 200, 100, 200)
    expect(ins.left).toBeCloseTo(0.1)
    expect(ins.top).toBeCloseTo(0.2)
    expect(ins.right).toBeCloseTo(0.1)
    expect(ins.bottom).toBeCloseTo(0.0)
  })

  it('wide window letterboxes horizontally (contain): bars join the margins', () => {
    // Square frame in a 400×200 window → drawn 200×200, 100 px bars each side.
    const ins = boxToWindowInsets(FULL, 100, 100, 400, 200)
    expect(ins.left).toBeCloseTo(0.25)
    expect(ins.right).toBeCloseTo(0.25)
    expect(ins.top).toBeCloseTo(0)
    expect(ins.bottom).toBeCloseTo(0)
  })

  it('tall window letterboxes vertically: bars join the margins', () => {
    // Square frame in a 170×289 pet window → drawn 170×170, (289-170)/2 bars.
    const ins = boxToWindowInsets(FULL, 100, 100, 170, 289)
    const bar = (289 - 170) / 2 / 289
    expect(ins.top).toBeCloseTo(bar)
    expect(ins.bottom).toBeCloseTo(bar)
    expect(ins.left).toBeCloseTo(0)
    expect(ins.right).toBeCloseTo(0)
  })

  it('letterbox bars and in-frame margins combine', () => {
    // Square frame, wide window, box with a 10% left margin inside the frame:
    // inset = bar (100px) + 10% of the drawn 200px = 120px of 400 = 0.3.
    const box: OpaqueBox = { x0: 0.1, y0: 0, x1: 1, y1: 1 }
    const ins = boxToWindowInsets(box, 100, 100, 400, 200)
    expect(ins.left).toBeCloseTo(0.3)
    expect(ins.right).toBeCloseTo(0.25)
  })
})

describe('insetsDiffer', () => {
  const BASE: EdgeInsets = { left: 0.1, top: 0.2, right: 0.1, bottom: 0.05 }

  it('identical insets are not worth resending', () => {
    expect(insetsDiffer(BASE, { ...BASE })).toBe(false)
  })

  it('sub-epsilon jitter on every edge is not worth resending', () => {
    const jitter = SEND_EPSILON / 2
    expect(insetsDiffer(BASE, {
      left:   BASE.left + jitter,
      top:    BASE.top - jitter,
      right:  BASE.right + jitter,
      bottom: BASE.bottom - jitter,
    })).toBe(false)
  })

  it('a single edge moving by epsilon triggers a resend', () => {
    expect(insetsDiffer(BASE, { ...BASE, left: BASE.left + SEND_EPSILON })).toBe(true)
    expect(insetsDiffer(BASE, { ...BASE, bottom: BASE.bottom - SEND_EPSILON })).toBe(true)
  })
})
