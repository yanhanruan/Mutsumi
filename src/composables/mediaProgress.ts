/**
 * mediaProgress — pure helpers for the music controller's progress bar.
 *
 * The bar shows a smooth playhead interpolated from a local clock between the
 * backend's ~1 Hz SMTC polls. All the tricky behaviour — re-basing the clock,
 * the optimistic seek guard, drift tolerance, and resyncing on a track change —
 * lives here as pure functions over an explicit `ProgressClock` so it can be
 * unit-tested without a DOM, Tauri, or requestAnimationFrame.
 *
 * MusicBadge.vue is a thin shell: it owns the refs / rAF / pointer events and
 * delegates every decision to `reconcile` (on each media-update), `tickPosition`
 * (each frame), and `commitSeek` (on a user seek).
 */

// ── Tunables ───────────────────────────────────────────────────────

/** A poll within this many ms of the expected post-seek position = "settled". */
export const SEEK_SETTLE_TOLERANCE = 1500
/** Give up guarding a seek after this long regardless (seek was likely rejected). */
export const SEEK_GUARD_TIMEOUT = 3000
/** While playing, only re-base when the backend diverges from our projection by this much. */
export const DRIFT_TOLERANCE = 400

// ── Types ──────────────────────────────────────────────────────────

/** The minimal slice of a MediaSnapshot the progress clock reasons about. */
export interface TrackSnapshot {
  title:       string
  artist:      string
  status:      string
  position_ms: number
  duration_ms: number
}

/**
 * Interpolation anchor + guard state.
 *   displayed position at time `now` (while playing) = basePos + (now - baseAt)
 */
export interface ProgressClock {
  /** Authoritative position (ms) at the last re-base. */
  basePos:      number
  /** `performance.now()` timestamp of the last re-base. */
  baseAt:       number
  /** Identity of the track the clock is currently tracking. */
  lastTrackKey: string
  /** Target (ms) of an in-flight user seek, or null when none is pending. */
  pendingSeek:  number | null
  /** `performance.now()` timestamp of the pending seek. */
  seekAt:       number
}

/** A fresh, empty clock (position 0, no track, no pending seek). */
export function createClock(): ProgressClock {
  return { basePos: 0, baseAt: 0, lastTrackKey: '', pendingSeek: null, seekAt: 0 }
}

// ── Small pure utilities ───────────────────────────────────────────

/**
 * Stable identity for a track. SMTC can briefly report a *new* track's metadata
 * with the *old* position, so keying on title/artist/duration lets us detect a
 * genuine song change (duration rounded to whole seconds to ignore jitter).
 */
export function trackKey(s: TrackSnapshot): string {
  return `${s.title}|${s.artist}|${Math.round(s.duration_ms / 1000)}`
}

/** Clamp a position to [0, duration]; with an unknown duration just floor at 0. */
export function clampPosition(posMs: number, durationMs: number): number {
  if (durationMs > 0) return Math.min(Math.max(0, posMs), durationMs)
  return Math.max(0, posMs)
}

/** Progress as a 0–100 percentage (0 when the duration is unknown). */
export function pctOf(displayMs: number, durationMs: number): number {
  if (durationMs <= 0) return 0
  return Math.min(100, Math.max(0, (displayMs / durationMs) * 100))
}

/** Format milliseconds as `m:ss`. */
export function fmtTime(ms: number): string {
  const total = Math.max(0, Math.round(ms / 1000))
  const m = Math.floor(total / 60)
  const s = total % 60
  return `${m}:${s.toString().padStart(2, '0')}`
}

/**
 * Map a pointer x (relative to the progress track's bounding rect) to a
 * start-normalized position in ms, clamped to the track. Returns 0 when the
 * track has no known duration or zero width.
 */
export function seekPositionFromPointer(
  clientX: number,
  rectLeft: number,
  rectWidth: number,
  durationMs: number,
): number {
  if (durationMs <= 0 || rectWidth <= 0) return 0
  const frac = Math.min(1, Math.max(0, (clientX - rectLeft) / rectWidth))
  return frac * durationMs
}

/**
 * The position to render at time `now`, given the clock.
 *   playing → basePos + elapsed, capped at the duration
 *   else    → basePos (frozen)
 */
export function tickPosition(
  clock: ProgressClock,
  now: number,
  playing: boolean,
  durationMs: number,
): number {
  if (!playing) return clock.basePos
  const next = clock.basePos + (now - clock.baseAt)
  return durationMs > 0 ? Math.min(durationMs, next) : next
}

// ── State transitions ──────────────────────────────────────────────

/** Re-base the clock onto an authoritative position (clamped to the track). */
function rebase(clock: ProgressClock, posMs: number, now: number, durationMs: number): ProgressClock {
  return {
    ...clock,
    basePos: clampPosition(posMs, durationMs),
    baseAt:  now,
  }
}

/**
 * Begin an optimistic seek to `targetMs`: jump the clock to the target and arm
 * the guard so subsequent stale polls are ignored until the backend catches up.
 */
export function commitSeek(
  clock: ProgressClock,
  targetMs: number,
  now: number,
  durationMs: number,
): ProgressClock {
  const pos = clampPosition(targetMs, durationMs)
  return { ...clock, basePos: pos, baseAt: now, pendingSeek: pos, seekAt: now }
}

/**
 * Fold an incoming backend snapshot into the clock at time `now`, deciding
 * whether (and to what) the clock re-bases. Pure: returns the next clock and
 * never mutates the input.
 *
 * Priority of rules:
 *   1. `dragging` — the pointer owns the playhead; ignore the snapshot entirely.
 *   2. track change — hard resync (void any pending seek; adopt the new position).
 *   3. pending seek — hold the target; adopt only once the backend has settled
 *      near it (or the guard times out), so stale polls can't snap the bar back.
 *   4. normal — while playing, re-base only on real drift; otherwise mirror
 *      the backend exactly.
 */
export function reconcile(
  clock: ProgressClock,
  s: TrackSnapshot,
  now: number,
  dragging: boolean,
): ProgressClock {
  if (dragging) return clock

  const key = trackKey(s)
  if (key !== clock.lastTrackKey) {
    return {
      basePos:      clampPosition(s.position_ms, s.duration_ms),
      baseAt:       now,
      lastTrackKey: key,
      pendingSeek:  null,
      seekAt:       0,
    }
  }

  if (clock.pendingSeek !== null) {
    const since    = now - clock.seekAt
    const expected = clock.pendingSeek + (s.status === 'playing' ? since : 0)
    const settled  = Math.abs(s.position_ms - expected) <= SEEK_SETTLE_TOLERANCE
    if (settled || since > SEEK_GUARD_TIMEOUT) {
      return { ...rebase(clock, s.position_ms, now, s.duration_ms), pendingSeek: null, seekAt: 0 }
    }
    return clock
  }

  if (s.status === 'playing') {
    const projected = clock.basePos + (now - clock.baseAt)
    if (Math.abs(projected - s.position_ms) > DRIFT_TOLERANCE) {
      return rebase(clock, s.position_ms, now, s.duration_ms)
    }
    return clock
  }

  return rebase(clock, s.position_ms, now, s.duration_ms)
}
