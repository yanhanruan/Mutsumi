/**
 * animations.ts — the WHAT of the pet's animation system: data and policy.
 * No Vue, no timers, no DOM — everything here is declarative or pure, so it
 * can be read (and unit-tested) without touching playback machinery.
 *
 *   - AnimationName / AnimationDef / HoldCycle — the vocabulary
 *   - DEFAULT_ANIMATIONS                       — the registry (frames, fps, sequence shapes)
 *   - ANIM_CHAINS                              — what follows what when a clip ends
 *   - IDLE_VARIANTS / MUSIC_MODE_ANIMS         — classification sets
 *   - resolveBaseline                          — the return-to-rest policy
 *   - buildPingPongSequence (+ odd/even)       — pure sequence-shape math used by the registry
 *
 * The HOW — actually playing these at the right time — is the playback
 * engine + state machine in src/composables/useAnimator.ts. Translating
 * backend events into animator inputs is the job of the dedicated
 * composables (useAudioReaction, usePetStatus).
 */

// ── Vocabulary ──────────────────────────────────────────────────────

export type AnimationName =
  | 'idle'
  | 'idle_low_energy'    // low energy only  (placeholder — swap dir when assets arrive)
  | 'idle_low_affection' // low affection only
  | 'idle_exhausted'     // both low
  | 'sleep'              // max-priority rest state (assets in progress)
  | 'flying'             // balloon-mode loop (the window itself glides — flight.rs)
  | 'fly_enter'          // idle→fly morph (shared transform clip, reversed)
  | 'fly_exit'           // fly→idle morph (shared transform clip, forward)
  | 'click'
  | 'pat_head'
  | 'headphones_on'
  | 'headphones_off'
  | 'music1'
  | 'music2'
  | 'music3'
  | 'music4'

export interface AnimationDef {
  /** Frame URLs relative to `/assets/`. */
  dir:       string
  /** Number of source files on disk (frame_001.webp … frame_NNN.webp). */
  count:     number
  fps:       number
  loop:      boolean
  /**
   * Optional post-load transform. When provided, the raw loaded frames are
   * passed in and the returned array becomes the actual playback sequence.
   * Use this to build ping-pong, intro+loop, or other non-linear orderings
   * without changing the tick loop.
   */
  buildSequence?: (frames: HTMLImageElement[]) => HTMLImageElement[]
  /**
   * Anchored-hold ping-pong playback. When set, the clip does NOT run straight
   * through: it drifts between the given `anchors`, freezing on each for a
   * random [holdMinMs, holdMaxMs] before continuing to the next, and reverses
   * direction at the first/last anchor — an endless, restful ping-pong:
   *   anchors[0] → … → anchors[n-1] → … → anchors[0] → …
   * Used by `sleep` (a small shift between long holds). Mutually exclusive with
   * buildSequence.
   */
  holdCycle?: HoldCycle
}

/** Config for anchored-hold ping-pong playback (see AnimationDef.holdCycle). */
export interface HoldCycle {
  /** 1-based frame numbers, ascending. First/last are the turn-around points. */
  anchors:   number[]
  holdMinMs: number
  holdMaxMs: number
}

// ── Classification sets ─────────────────────────────────────────────

/** All animation names that represent an "idle" state (any tier). */
export const IDLE_VARIANTS: ReadonlySet<AnimationName> = new Set([
  'idle', 'idle_low_energy', 'idle_low_affection', 'idle_exhausted',
])

/**
 * Animations that represent "in / entering music mode". headphones_off is
 * intentionally NOT in this set — it's the *exit* animation that leads back
 * to idle. If audio resumes while headphones_off is playing, the reaction
 * DOES need to re-enter via headphones_on, not skip it.
 */
export const MUSIC_MODE_ANIMS: ReadonlySet<AnimationName> = new Set([
  'headphones_on',
  'music1',
  'music2',
  'music3',
  'music4',
])

// ── Chains ──────────────────────────────────────────────────────────

/**
 * What follows what when a clip runs to its natural end (and nothing is
 * pending): the hardwired music cycle from the Python original, plus the
 * fly_enter → flying handoff. Endings NOT listed here either re-loop
 * (loop: true) or fall back through resolveBaseline. fly_exit is the one
 * ending with a side effect (flight mode drops), so it's handled in the
 * state machine, not here.
 */
export const ANIM_CHAINS: Readonly<Partial<Record<AnimationName, AnimationName>>> = {
  fly_enter:      'flying',
  headphones_on:  'music1',
  music1:         'music2',
  music2:         'music3',
  music3:         'music4',
  music4:         'music1',
}

// ── Baseline policy ─────────────────────────────────────────────────

/**
 * Baseline resolver — THE shared "what should she return to?" rule, applied
 * whenever any animation or max-priority state ends (fly_exit landing, waking
 * from sleep, pat_head/click one-shots, headphones_off, …). Instead of each
 * ending hardcoding its own fallback, they all converge here, so state that
 * changed *while* something else was playing (e.g. music kept playing through
 * a whole flight) is never lost.
 *
 * Priority: sleep (max-priority rest) > live audio (enter the music chain via
 * its canonical headphones_on entrance) > the current idle variant.
 *
 * Pure — the animator feeds it the synchronized inputs it tracks
 * (setAudioActive / setIdleVariant mirror backend-owned facts). Unit-tested.
 */
export function resolveBaseline(
  sleeping:    boolean,
  audioActive: boolean,
  idleVariant: AnimationName,
): AnimationName {
  if (sleeping) return 'sleep'
  if (audioActive) return 'headphones_on'
  return idleVariant
}

// ── Generic ping-pong sequence builder ─────────────────────────────
//
// Builds an intro → ping-pong → outro playback sequence from a flat frame
// array. The three boundary parameters fully determine all three regions:
//
//   pingStart — first frame index (0-based, inclusive) of the ping-pong range.
//               Everything before it is the intro, played once.
//   pingEnd   — exclusive end index of the ping-pong range.
//               Everything from pingEnd onward is the outro, played once.
//   passes    — number of directional passes over [pingStart, pingEnd),
//               alternating fwd/rev and always starting fwd.
//               An OddNumber ends the sequence on a forward pass; an
//               EvenNumber ends it on a reverse pass. Construct the value
//               with the odd() / even() helper to make the parity explicit.
//
// Exported so the sequence logic can be unit-tested independently.

declare const _ODD: unique symbol
/** Branded type for odd integers. Construct via odd(). */
export type OddNumber = number & { readonly [_ODD]: true }

declare const _EVEN: unique symbol
/** Branded type for even integers. Construct via even(). */
export type EvenNumber = number & { readonly [_EVEN]: true }

/** Cast n to OddNumber. Throws a RangeError at runtime if n is even. */
export function odd(n: number): OddNumber {
  if (n % 2 === 0) throw new RangeError(`passes must be odd, got ${n}`)
  return n as OddNumber
}

/** Cast n to EvenNumber. Throws a RangeError at runtime if n is odd. */
export function even(n: number): EvenNumber {
  if (n % 2 !== 0) throw new RangeError(`passes must be even, got ${n}`)
  return n as EvenNumber
}

export function buildPingPongSequence(
  frames:    HTMLImageElement[],
  pingStart: number,
  pingEnd:   number,
  passes:    OddNumber | EvenNumber,
): HTMLImageElement[] {
  const intro = frames.slice(0, pingStart)
  const fwd   = frames.slice(pingStart, pingEnd)
  const rev   = fwd.slice().reverse()
  const outro = frames.slice(pingEnd)

  const seq: HTMLImageElement[] = [...intro]
  for (let i = 0; i < passes; i++) {
    seq.push(...(i % 2 === 0 ? fwd : rev))
  }
  seq.push(...outro)
  return seq
}

// ── Registry (matches Python original) ──────────────────────────────

export const DEFAULT_ANIMATIONS: Record<AnimationName, AnimationDef> = {
  idle:                { dir: 'idle',           count: 426, fps: 24, loop: true  },
  // ── Idle-variant placeholders ──────────────────────────────────────────
  // These fall back to the normal `idle` directory until dedicated animation
  // assets are added. To activate: update `dir` (and `count` if different).
  // Kept in sync with `idle` (426) so they loop on the same seamless boundary.
  idle_low_energy:    { dir: 'idle',           count: 426, fps: 24, loop: true  },
  idle_low_affection: { dir: 'idle',           count: 426, fps: 24, loop: true  },
  idle_exhausted:     { dir: 'idle',           count: 426, fps: 24, loop: true  },
  // ─────────────────────────────────────────────────────────────────────
  // ── Sleep ───────────────────────────────────────────────────────────────
  // Anchored-hold ping-pong: drift 1→68→133→144→192 then back, resting on each
  // anchor for ~5 min (+ a little jitter) before the next small shift. `loop` is
  // moot under holdCycle (it never runs to the end) but kept true for safety.
  sleep:               { dir: 'sleep',          count: 192, fps: 24, loop: true,
                         holdCycle: { anchors: [1, 68, 133, 144, 192],
                                      holdMinMs: 5 * 60_000, holdMaxMs: 5 * 60_000 + 45_000 } },
  // ── Flying (balloon mode) ───────────────────────────────────────────────
  // The window itself glides across the screen (src-tauri/src/flight.rs);
  // these clips only animate the sprite. Only LEFT-facing frames exist —
  // rightward flight mirrors the <img> with scaleX(-1) (PetWindow `mirrored`
  // class, driven by `balloon-facing` events from flight.rs).
  // `flying` ping-pongs 1 → 192 → 1 … without duplicating the turn frames.
  flying:              { dir: 'fly_left',       count: 192, fps: 24, loop: true,
                         buildSequence: f => [...f, ...f.slice(1, -1).reverse()] },
  fly_enter:           { dir: 'fly_to_idle',    count: 185, fps: 24, loop: false,
                         buildSequence: f => f.slice().reverse() },
  fly_exit:            { dir: 'fly_to_idle',    count: 185, fps: 24, loop: false },
  // ─────────────────────────────────────────────────────────────────────────
  click:               { dir: 'click_matched',  count: 156, fps: 24, loop: false },
  pat_head:            { dir: 'pat_head',       count: 192, fps: 24, loop: false },
  headphones_on:       { dir: 'headphones_on',  count: 185, fps: 24, loop: false },
  headphones_off:      { dir: 'headphones_off', count: 185, fps: 24, loop: false },
  music1:              { dir: 'music1',         count: 185, fps: 24, loop: true  },
  music2:              { dir: 'music2',         count: 185, fps: 24, loop: false,
                         buildSequence: f => buildPingPongSequence(f,  81, 104, odd(31)) },
  music3:              { dir: 'music3',         count: 139, fps: 18, loop: false,
                         buildSequence: f => buildPingPongSequence(f,  1, 139, even(2)) },
  music4:              { dir: 'music4',         count: 185, fps: 24, loop: false,
                         buildSequence: f => buildPingPongSequence(f, 105, 161, odd(13)) },
}
