/**
 * Tests for the pure progress-bar helpers in mediaProgress.
 *
 * Everything here is DOM-free and deterministic (the "clock" is just a plain
 * object and `now` is passed in), so no Tauri / rAF / timers to mock. These
 * cover the seek-guard and track-change logic that keeps the timeline synced
 * with real playback — including the slow-network song-switch case.
 */
import { describe, it, expect } from 'vitest'
import {
  SEEK_SETTLE_TOLERANCE,
  SEEK_GUARD_TIMEOUT,
  DRIFT_TOLERANCE,
  createClock,
  trackKey,
  clampPosition,
  pctOf,
  fmtTime,
  seekPositionFromPointer,
  tickPosition,
  commitSeek,
  reconcile,
  type ProgressClock,
  type TrackSnapshot,
} from './mediaProgress'

// ── Builders ───────────────────────────────────────────────────────

function snap(p: Partial<TrackSnapshot> = {}): TrackSnapshot {
  return {
    title:       'Song',
    artist:      'Artist',
    status:      'playing',
    position_ms: 0,
    duration_ms: 200_000,
    ...p,
  }
}

/** A clock already locked onto `s`, so reconcile won't see a track change. */
function locked(s: TrackSnapshot, over: Partial<ProgressClock> = {}): ProgressClock {
  return { basePos: 0, baseAt: 0, lastTrackKey: trackKey(s), pendingSeek: null, seekAt: 0, ...over }
}

// ── trackKey ───────────────────────────────────────────────────────

describe('trackKey — track identity', () => {
  it('combines title, artist, and whole-second duration', () => {
    expect(trackKey(snap())).toBe('Song|Artist|200')
  })

  it('changes when the title changes', () => {
    expect(trackKey(snap({ title: 'Other' }))).not.toBe(trackKey(snap()))
  })

  it('changes when the artist changes', () => {
    expect(trackKey(snap({ artist: 'Other' }))).not.toBe(trackKey(snap()))
  })

  it('rounds duration to whole seconds so sub-second jitter is ignored', () => {
    expect(trackKey(snap({ duration_ms: 200_400 }))).toBe('Song|Artist|200')
    expect(trackKey(snap({ duration_ms: 199_600 }))).toBe('Song|Artist|200')
  })

  it('distinguishes tracks with a different duration', () => {
    expect(trackKey(snap({ duration_ms: 100_000 }))).not.toBe(trackKey(snap()))
  })
})

// ── clampPosition ──────────────────────────────────────────────────

describe('clampPosition', () => {
  it('passes through a value inside [0, duration]', () => {
    expect(clampPosition(50, 100)).toBe(50)
  })
  it('clamps above the duration', () => {
    expect(clampPosition(150, 100)).toBe(100)
  })
  it('clamps below zero', () => {
    expect(clampPosition(-5, 100)).toBe(0)
  })
  it('with unknown duration only floors at zero', () => {
    expect(clampPosition(150, 0)).toBe(150)
    expect(clampPosition(-5, 0)).toBe(0)
  })
})

// ── pctOf ──────────────────────────────────────────────────────────

describe('pctOf — progress percentage', () => {
  it('computes a normal percentage', () => {
    expect(pctOf(50_000, 200_000)).toBe(25)
  })
  it('is 0 when the duration is unknown', () => {
    expect(pctOf(1000, 0)).toBe(0)
  })
  it('clamps to 100 when position exceeds duration', () => {
    expect(pctOf(300_000, 200_000)).toBe(100)
  })
  it('clamps to 0 for a negative position', () => {
    expect(pctOf(-100, 200_000)).toBe(0)
  })
})

// ── fmtTime ────────────────────────────────────────────────────────

describe('fmtTime — m:ss formatting', () => {
  it('formats zero', () => {
    expect(fmtTime(0)).toBe('0:00')
  })
  it('zero-pads the seconds', () => {
    expect(fmtTime(5_000)).toBe('0:05')
  })
  it('rolls over into minutes', () => {
    expect(fmtTime(65_000)).toBe('1:05')
  })
  it('handles multi-digit minutes', () => {
    expect(fmtTime(600_000)).toBe('10:00')
  })
  it('floors negatives at 0:00', () => {
    expect(fmtTime(-5_000)).toBe('0:00')
  })
})

// ── seekPositionFromPointer ────────────────────────────────────────

describe('seekPositionFromPointer — pointer x to position', () => {
  it('maps the midpoint to half the duration', () => {
    expect(seekPositionFromPointer(150, 100, 100, 200_000)).toBe(100_000)
  })
  it('clamps to 0 left of the track', () => {
    expect(seekPositionFromPointer(50, 100, 100, 200_000)).toBe(0)
  })
  it('clamps to the duration right of the track', () => {
    expect(seekPositionFromPointer(300, 100, 100, 200_000)).toBe(200_000)
  })
  it('returns 0 for an unknown duration', () => {
    expect(seekPositionFromPointer(150, 100, 100, 0)).toBe(0)
  })
  it('returns 0 for a zero-width track', () => {
    expect(seekPositionFromPointer(150, 100, 0, 200_000)).toBe(0)
  })
})

// ── tickPosition ───────────────────────────────────────────────────

describe('tickPosition — per-frame interpolation', () => {
  it('advances by elapsed time while playing', () => {
    const c = locked(snap(), { basePos: 1_000, baseAt: 500 })
    expect(tickPosition(c, 1_500, true, 200_000)).toBe(2_000)
  })
  it('caps at the duration', () => {
    const c = locked(snap(), { basePos: 199_000, baseAt: 0 })
    expect(tickPosition(c, 5_000, true, 200_000)).toBe(200_000)
  })
  it('freezes at basePos while paused', () => {
    const c = locked(snap(), { basePos: 42_000, baseAt: 0 })
    expect(tickPosition(c, 9_999, false, 200_000)).toBe(42_000)
  })
})

// ── commitSeek ─────────────────────────────────────────────────────

describe('commitSeek — arm the optimistic seek guard', () => {
  it('jumps the clock to the target and records the pending seek', () => {
    const c = commitSeek(createClock(), 80_000, 1_234, 200_000)
    expect(c.basePos).toBe(80_000)
    expect(c.baseAt).toBe(1_234)
    expect(c.pendingSeek).toBe(80_000)
    expect(c.seekAt).toBe(1_234)
  })
  it('clamps a target beyond the duration', () => {
    const c = commitSeek(createClock(), 999_000, 0, 200_000)
    expect(c.basePos).toBe(200_000)
    expect(c.pendingSeek).toBe(200_000)
  })
})

// ── reconcile: dragging ────────────────────────────────────────────

describe('reconcile — dragging owns the playhead', () => {
  it('returns the clock unchanged even across a track change', () => {
    const c = locked(snap(), { basePos: 12_345, baseAt: 100 })
    const out = reconcile(c, snap({ title: 'Different' }), 9_999, true)
    expect(out).toBe(c)
  })
})

// ── reconcile: track change (the slow-network fix) ─────────────────

describe('reconcile — track change is a hard resync', () => {
  it('adopts the new position and updates the track key', () => {
    const prev = locked(snap(), { basePos: 120_000, baseAt: 0 })
    const next = snap({ title: 'New Song', position_ms: 0, duration_ms: 180_000 })
    const out = reconcile(prev, next, 5_000, false)
    expect(out.basePos).toBe(0)
    expect(out.baseAt).toBe(5_000)
    expect(out.lastTrackKey).toBe(trackKey(next))
    expect(out.pendingSeek).toBeNull()
  })

  it('clamps a stale cross-track position to the new (shorter) duration', () => {
    // The failure mode: new song's metadata arrives with the OLD elapsed time.
    const prev = locked(snap(), { basePos: 0, baseAt: 0 })
    const next = snap({ title: 'Short Song', position_ms: 180_000, duration_ms: 100_000 })
    const out = reconcile(prev, next, 1_000, false)
    expect(out.basePos).toBe(100_000)        // clamped, not parked past 100%
    expect(out.lastTrackKey).toBe(trackKey(next))
  })

  it('voids a pending seek that belonged to the previous track', () => {
    const prev = locked(snap(), { pendingSeek: 50_000, seekAt: 10 })
    const next = snap({ title: 'New Song' })
    const out = reconcile(prev, next, 2_000, false)
    expect(out.pendingSeek).toBeNull()
  })
})

// ── reconcile: pending seek guard ──────────────────────────────────

describe('reconcile — optimistic seek guard', () => {
  it('ignores a stale poll that still reports the pre-seek position', () => {
    const c = locked(snap(), { basePos: 50_000, baseAt: 1_000, pendingSeek: 50_000, seekAt: 1_000 })
    // Backend still reports the old position 10_000 shortly after the seek.
    const out = reconcile(c, snap({ position_ms: 10_000 }), 1_500, false)
    expect(out).toBe(c)                        // unchanged → bar holds the target
  })

  it('settles once the backend reaches the expected position (advanced while playing)', () => {
    const c = locked(snap(), { basePos: 50_000, baseAt: 1_000, pendingSeek: 50_000, seekAt: 1_000 })
    // 500 ms later, playing → expected ≈ 50_500; backend agrees within tolerance.
    const out = reconcile(c, snap({ position_ms: 50_400 }), 1_500, false)
    expect(out.pendingSeek).toBeNull()
    expect(out.basePos).toBe(50_400)
    expect(out.baseAt).toBe(1_500)
  })

  it('respects SEEK_SETTLE_TOLERANCE at the boundary', () => {
    const c = locked(snap({ status: 'paused' }), { pendingSeek: 50_000, seekAt: 0 })
    const within = reconcile(c, snap({ status: 'paused', position_ms: 50_000 + SEEK_SETTLE_TOLERANCE }), 100, false)
    expect(within.pendingSeek).toBeNull()      // exactly at tolerance → settled
    const beyond = reconcile(c, snap({ status: 'paused', position_ms: 50_000 + SEEK_SETTLE_TOLERANCE + 1 }), 100, false)
    expect(beyond).toBe(c)                      // just beyond → still guarding
  })

  it('gives up guarding after SEEK_GUARD_TIMEOUT even if never settled', () => {
    const c = locked(snap(), { pendingSeek: 50_000, seekAt: 0 })
    const out = reconcile(c, snap({ position_ms: 10_000 }), SEEK_GUARD_TIMEOUT + 1, false)
    expect(out.pendingSeek).toBeNull()
    expect(out.basePos).toBe(10_000)           // adopts reality once the guard expires
  })
})

// ── reconcile: normal sync ─────────────────────────────────────────

describe('reconcile — normal drift-tolerant sync', () => {
  it('ignores small IPC drift while playing (no re-base)', () => {
    const c = locked(snap(), { basePos: 10_000, baseAt: 1_000 })
    // now=1_200 → projected 10_200; backend 10_300 → drift 100 < tolerance.
    const out = reconcile(c, snap({ position_ms: 10_300 }), 1_200, false)
    expect(out).toBe(c)
  })

  it('re-bases on real divergence while playing (external seek)', () => {
    const c = locked(snap(), { basePos: 10_000, baseAt: 1_000 })
    const out = reconcile(c, snap({ position_ms: 30_000 }), 1_200, false)
    expect(out.basePos).toBe(30_000)
    expect(out.baseAt).toBe(1_200)
  })

  it('re-bases at the drift boundary', () => {
    const c = locked(snap(), { basePos: 10_000, baseAt: 1_000 })
    // projected at now=1_000 is 10_000; backend at exactly +DRIFT_TOLERANCE stays put.
    const atEdge = reconcile(c, snap({ position_ms: 10_000 + DRIFT_TOLERANCE }), 1_000, false)
    expect(atEdge).toBe(c)
    const overEdge = reconcile(c, snap({ position_ms: 10_000 + DRIFT_TOLERANCE + 1 }), 1_000, false)
    expect(overEdge.basePos).toBe(10_000 + DRIFT_TOLERANCE + 1)
  })

  it('mirrors the backend exactly while paused', () => {
    const c = locked(snap({ status: 'paused' }), { basePos: 5_000, baseAt: 0 })
    const out = reconcile(c, snap({ status: 'paused', position_ms: 12_345 }), 1_000, false)
    expect(out.basePos).toBe(12_345)
    expect(out.baseAt).toBe(1_000)
  })
})
