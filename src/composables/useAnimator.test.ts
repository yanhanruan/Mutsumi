/**
 * Tests for pure helpers exported from useAnimator.ts.
 *
 * buildMusic4Sequence is the only pure export today; the composable itself
 * requires a DOM + RAF loop and is covered via integration with
 * useAudioReaction.test.ts.
 *
 * Frames are represented as plain numbers so the tests run in Node/happy-dom
 * without needing real HTMLImageElement objects. Each number is unique, so
 * reference-equality checks (toBe) correctly verify that the right source
 * frame appears at each position in the sequence.
 */
import { describe, it, expect } from 'vitest'
import { buildMusic4Sequence } from './useAnimator'

// ── helpers ────────────────────────────────────────────────────────

/** 185 unique "frames" (plain numbers 0-184, cast to the expected type). */
function makeFrames(): HTMLImageElement[] {
  return Array.from({ length: 185 }, (_, i) => i) as unknown as HTMLImageElement[]
}

// ── buildMusic4Sequence ────────────────────────────────────────────
//
// Expected layout:
//   Intro    : frames[0..104]   → 105 virtual frames
//   Ping-pong: 7 × { frames[105..160] fwd + frames[160..105] rev }
//            = 7 × 112          → 784 virtual frames
//   Outro    : frames[106..184] →  79 virtual frames
//   Total                       → 968 virtual frames

describe('buildMusic4Sequence — shape', () => {
  const frames = makeFrames()
  const seq    = buildMusic4Sequence(frames)

  it('total length = 105 (intro) + 7 × 112 (ping-pong) + 79 (outro) = 968', () => {
    expect(seq.length).toBe(968)
  })

  it('does not mutate the source frames array', () => {
    const src = makeFrames()
    buildMusic4Sequence(src)
    for (let i = 0; i < 185; i++) {
      expect(src[i]).toBe(i)
    }
  })
})

describe('buildMusic4Sequence — intro (frames 1-105)', () => {
  const frames = makeFrames()
  const seq    = buildMusic4Sequence(frames)

  it('seq[0] is frame_001 (source index 0)', () => expect(seq[0]).toBe(frames[0]))
  it('seq[104] is frame_105 (source index 104)', () => expect(seq[104]).toBe(frames[104]))

  it('all 105 intro frames are in ascending order', () => {
    for (let i = 0; i < 105; i++) {
      expect(seq[i]).toBe(frames[i])
    }
  })
})

describe('buildMusic4Sequence — 7 ping-pong cycles (frames 106-161 ↔ 161-106)', () => {
  const frames = makeFrames()
  const seq    = buildMusic4Sequence(frames)

  // Cycle k (0-indexed) starts at seq[105 + k * 112].
  // Forward half  [+0  .. +55 ]: frames[105..160]  (frame_106 … frame_161)
  // Reverse half  [+56 .. +111]: frames[160..105]  (frame_161 … frame_106)

  for (let k = 0; k < 7; k++) {
    const base = 105 + k * 112

    it(`cycle ${k + 1} forward: starts with frame_106`, () => {
      expect(seq[base]).toBe(frames[105])
    })

    it(`cycle ${k + 1} forward: ends with frame_161`, () => {
      expect(seq[base + 55]).toBe(frames[160])
    })

    it(`cycle ${k + 1} forward: all 56 frames in ascending order`, () => {
      for (let i = 0; i < 56; i++) {
        expect(seq[base + i]).toBe(frames[105 + i])
      }
    })

    it(`cycle ${k + 1} reverse: starts with frame_161`, () => {
      expect(seq[base + 56]).toBe(frames[160])
    })

    it(`cycle ${k + 1} reverse: ends with frame_106`, () => {
      expect(seq[base + 111]).toBe(frames[105])
    })

    it(`cycle ${k + 1} reverse: all 56 frames in descending order`, () => {
      for (let i = 0; i < 56; i++) {
        expect(seq[base + 56 + i]).toBe(frames[160 - i])
      }
    })
  }

  it('last frame of ping-pong section is frame_106 (→ outro begins at 107)', () => {
    // ping-pong ends at seq[105 + 7*112 - 1] = seq[888]
    expect(seq[888]).toBe(frames[105])
  })
})

describe('buildMusic4Sequence — outro (frames 107-185)', () => {
  const frames = makeFrames()
  const seq    = buildMusic4Sequence(frames)

  // Outro occupies seq[889..967].

  it('seq[889] is frame_107 (source index 106)', () => {
    expect(seq[889]).toBe(frames[106])
  })

  it('seq[967] is frame_185 (source index 184)', () => {
    expect(seq[967]).toBe(frames[184])
  })

  it('all 79 outro frames are in ascending order', () => {
    for (let i = 0; i < 79; i++) {
      expect(seq[889 + i]).toBe(frames[106 + i])
    }
  })
})
