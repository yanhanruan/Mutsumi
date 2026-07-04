/**
 * Tests for the animation data + policy module (src/config/animations.ts).
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
import {
  ANIM_CHAINS,
  buildPingPongSequence,
  odd,
  resolveBaseline,
  type AnimationName,
} from './animations'

// ── helpers ────────────────────────────────────────────────────────

function makeFrames(n: number): HTMLImageElement[] {
  return Array.from({ length: n }, (_, i) => i) as unknown as HTMLImageElement[]
}

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

// ── ANIM_CHAINS ─────────────────────────────────────────────────────

describe('ANIM_CHAINS', () => {
  it('the music chain is a closed cycle entered via headphones_on', () => {
    // headphones_on → music1 → music2 → music3 → music4 → music1 → …
    const seen = new Set<AnimationName>()
    let cur: AnimationName | undefined = ANIM_CHAINS.headphones_on
    while (cur && !seen.has(cur)) {
      seen.add(cur)
      cur = ANIM_CHAINS[cur]
    }
    expect([...seen]).toEqual(['music1', 'music2', 'music3', 'music4'])
    expect(cur).toBe('music1')   // the cycle closes back onto music1
  })

  it('fly_enter hands off to the flying loop', () => {
    expect(ANIM_CHAINS.fly_enter).toBe('flying')
  })

  it('fly_exit is NOT chained — landing is handled by the state machine', () => {
    expect(ANIM_CHAINS.fly_exit).toBeUndefined()
  })
})

// ── resolveBaseline ─────────────────────────────────────────────────
// The single "what should she return to?" rule shared by every ending:
// fly_exit landings, exitSleep wakes, and one-shots like pat_head or
// headphones_off. Priority: sleep > live audio > idle variant.

describe('resolveBaseline()', () => {
  it('returns the idle variant when nothing else is going on', () => {
    expect(resolveBaseline(false, false, 'idle')).toBe('idle')
    expect(resolveBaseline(false, false, 'idle_low_energy')).toBe('idle_low_energy')
  })

  it('re-enters music via headphones_on when audio is playing', () => {
    // The fix for: fly/sleep/pat_head during music used to strand her in
    // idle afterwards, because audio events are edge-triggered and had
    // already fired.
    expect(resolveBaseline(false, true, 'idle')).toBe('headphones_on')
  })

  it('sleep outranks audio — waking is the user\'s call, not the music\'s', () => {
    expect(resolveBaseline(true, true, 'idle')).toBe('sleep')
    expect(resolveBaseline(true, false, 'idle')).toBe('sleep')
  })

  it('audio outranks the idle variant regardless of tier', () => {
    expect(resolveBaseline(false, true, 'idle_exhausted')).toBe('headphones_on')
  })
})
