/**
 * Tests for pure helpers exported from useAnimator.ts.
 *
 * buildPingPongSequence is tested with the parameters used for music2 and
 * music4 so the tests also serve as regression coverage for those layouts.
 *
 * Frames are represented as plain numbers so the tests run in Node/happy-dom
 * without needing real HTMLImageElement objects. Each number is unique, so
 * reference-equality checks (toBe) correctly verify that the right source
 * frame appears at each position in the sequence.
 *
 * buildPingPongSequence(frames, pingStart, pingEnd, passes):
 *   intro  = frames[0 .. pingStart)              played once
 *   ping   = frames[pingStart .. pingEnd)  × passes  (alternating fwd/rev)
 *   outro  = frames[pingEnd ..]                  played once
 */
import { describe, it, expect } from 'vitest'
import { buildPingPongSequence, odd, randBetween, resolveHoldCycle } from './useAnimator'

// ── helpers ────────────────────────────────────────────────────────

function makeFrames(n: number): HTMLImageElement[] {
  return Array.from({ length: n }, (_, i) => i) as unknown as HTMLImageElement[]
}

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

// ── odd() ──────────────────────────────────────────────────────────

describe('odd()', () => {
  it('returns the value unchanged for odd inputs', () => {
    expect(odd(1)).toBe(1)
    expect(odd(3)).toBe(3)
    expect(odd(7)).toBe(7)
  })

  it('throws RangeError for even inputs', () => {
    expect(() => odd(0)).toThrow(RangeError)
    expect(() => odd(2)).toThrow(RangeError)
    expect(() => odd(14)).toThrow(RangeError)
  })
})

// ── music2: buildPingPongSequence(frames, 81, 159, odd(3)) ─────────
//
//   Intro  : frames[0..80]     →  81 frames   (seq[0..80])
//   Pass 1 fwd: frames[81..158] →  78 frames  (seq[81..158])
//   Pass 2 rev: frames[158..81] →  78 frames  (seq[159..236])
//   Pass 3 fwd: frames[81..158] →  78 frames  (seq[237..314])
//   Outro  : frames[159..184]  →  26 frames   (seq[315..340])
//   Total  : 81 + 3×78 + 26 = 341 frames

describe('buildPingPongSequence — music2 shape', () => {
  const frames = makeFrames(185)
  const seq    = buildPingPongSequence(frames, 81, 159, odd(3))

  it('total length = 81 (intro) + 3×78 (ping-pong) + 26 (outro) = 341', () => {
    expect(seq.length).toBe(341)
  })

  it('does not mutate the source frames array', () => {
    const src = makeFrames(185)
    buildPingPongSequence(src, 81, 159, odd(3))
    for (let i = 0; i < 185; i++) expect(src[i]).toBe(i)
  })
})

describe('buildPingPongSequence — music2 intro (frames 1-81)', () => {
  const frames = makeFrames(185)
  const seq    = buildPingPongSequence(frames, 81, 159, odd(3))

  it('seq[0] is source index 0',   () => expect(seq[0]).toBe(frames[0]))
  it('seq[80] is source index 80', () => expect(seq[80]).toBe(frames[80]))

  it('all 81 intro frames are in ascending order', () => {
    for (let i = 0; i < 81; i++) expect(seq[i]).toBe(frames[i])
  })
})

describe('buildPingPongSequence — music2 ping-pong passes', () => {
  const frames = makeFrames(185)
  const seq    = buildPingPongSequence(frames, 81, 159, odd(3))

  // Pass 0 fwd: seq[81..158] = frames[81..158]
  it('pass 1 fwd: starts at source index 81',  () => expect(seq[81]).toBe(frames[81]))
  it('pass 1 fwd: ends at source index 158',   () => expect(seq[158]).toBe(frames[158]))
  it('pass 1 fwd: all 78 frames ascending', () => {
    for (let i = 0; i < 78; i++) expect(seq[81 + i]).toBe(frames[81 + i])
  })

  // Pass 1 rev: seq[159..236] = frames[158..81]
  it('pass 2 rev: starts at source index 158', () => expect(seq[159]).toBe(frames[158]))
  it('pass 2 rev: ends at source index 81',    () => expect(seq[236]).toBe(frames[81]))
  it('pass 2 rev: all 78 frames descending', () => {
    for (let i = 0; i < 78; i++) expect(seq[159 + i]).toBe(frames[158 - i])
  })

  // Pass 2 fwd: seq[237..314] = frames[81..158]
  it('pass 3 fwd: starts at source index 81',  () => expect(seq[237]).toBe(frames[81]))
  it('pass 3 fwd: ends at source index 158',   () => expect(seq[314]).toBe(frames[158]))
  it('pass 3 fwd: all 78 frames ascending', () => {
    for (let i = 0; i < 78; i++) expect(seq[237 + i]).toBe(frames[81 + i])
  })

  it('last ping-pong frame is source index 158 (ends on fwd pass)', () => {
    expect(seq[314]).toBe(frames[158])
  })
})

describe('buildPingPongSequence — music2 outro (frames after pingEnd)', () => {
  const frames = makeFrames(185)
  const seq    = buildPingPongSequence(frames, 81, 159, odd(3))

  // Outro: seq[315..340] = frames[159..184]
  it('seq[315] is source index 159', () => expect(seq[315]).toBe(frames[159]))
  it('seq[340] is source index 184', () => expect(seq[340]).toBe(frames[184]))
  it('all 26 outro frames are in ascending order', () => {
    for (let i = 0; i < 26; i++) expect(seq[315 + i]).toBe(frames[159 + i])
  })
})

// ── music4: buildPingPongSequence(frames, 105, 161, odd(1)) ────────
//
//   Intro  : frames[0..104]   → 105 frames  (seq[0..104])
//   Pass 1 fwd: frames[105..160] → 56 frames (seq[105..160])
//   Outro  : frames[161..184] →  24 frames  (seq[161..184])
//   Total  : 105 + 1×56 + 24 = 185 frames

describe('buildPingPongSequence — music4 shape', () => {
  const frames = makeFrames(185)
  const seq    = buildPingPongSequence(frames, 105, 161, odd(1))

  it('total length = 105 (intro) + 1×56 (ping-pong) + 24 (outro) = 185', () => {
    expect(seq.length).toBe(185)
  })

  it('does not mutate the source frames array', () => {
    const src = makeFrames(185)
    buildPingPongSequence(src, 105, 161, odd(1))
    for (let i = 0; i < 185; i++) expect(src[i]).toBe(i)
  })
})

describe('buildPingPongSequence — music4 intro (frames 1-105)', () => {
  const frames = makeFrames(185)
  const seq    = buildPingPongSequence(frames, 105, 161, odd(1))

  it('seq[0] is source index 0',     () => expect(seq[0]).toBe(frames[0]))
  it('seq[104] is source index 104', () => expect(seq[104]).toBe(frames[104]))

  it('all 105 intro frames are in ascending order', () => {
    for (let i = 0; i < 105; i++) expect(seq[i]).toBe(frames[i])
  })
})

describe('buildPingPongSequence — music4 ping-pong (1 fwd pass)', () => {
  const frames = makeFrames(185)
  const seq    = buildPingPongSequence(frames, 105, 161, odd(1))

  // Pass 0 fwd: seq[105..160] = frames[105..160]
  it('pass 1 fwd: starts at source index 105', () => expect(seq[105]).toBe(frames[105]))
  it('pass 1 fwd: ends at source index 160',   () => expect(seq[160]).toBe(frames[160]))
  it('pass 1 fwd: all 56 frames ascending', () => {
    for (let i = 0; i < 56; i++) expect(seq[105 + i]).toBe(frames[105 + i])
  })
})

describe('buildPingPongSequence — music4 outro (frames after pingEnd)', () => {
  const frames = makeFrames(185)
  const seq    = buildPingPongSequence(frames, 105, 161, odd(1))

  // Outro: seq[161..184] = frames[161..184]
  it('seq[161] is source index 161', () => expect(seq[161]).toBe(frames[161]))
  it('seq[184] is source index 184', () => expect(seq[184]).toBe(frames[184]))
  it('all 24 outro frames are in ascending order', () => {
    for (let i = 0; i < 24; i++) expect(seq[161 + i]).toBe(frames[161 + i])
  })
})
