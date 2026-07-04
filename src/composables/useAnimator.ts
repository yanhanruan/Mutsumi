/**
 * useAnimator — animation state machine.
 *
 * Mirrors the Python original's design in pet.py:
 *   - A registry of named animations (frames, fps, loop)
 *   - A current animation + frame index
 *   - A "pending" slot that's consumed at the natural end of the current loop
 *   - Hardwired chains: headphones_on → music1 → music2 → music3 → music4 → music1 → …
 *
 * Timing uses a locked-step clock with a 2× lag clamp, the same pattern that
 * fixed the smoothness/catch-up issues in the Python original.
 *
 * Rendering writes directly to the <img> element's .src property via an
 * optional imgRef parameter — bypasses Vue's reactive proxy on the hot path
 * so the scheduler never runs on a frame advance.
 */
import { onMounted, onUnmounted, ref, shallowRef, type Ref } from 'vue'

// ── Types ───────────────────────────────────────────────────────────

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

/** All animation names that represent an "idle" state (any tier). */
export const IDLE_VARIANTS: ReadonlySet<AnimationName> = new Set([
  'idle', 'idle_low_energy', 'idle_low_affection', 'idle_exhausted',
])

interface LoadedAnimation {
  frames:    HTMLImageElement[]
  fps:       number
  loop:      boolean
  holdCycle?: ResolvedHoldCycle
}

/** A HoldCycle resolved to 0-based frame indices, computed once at preload. */
export interface ResolvedHoldCycle {
  anchors:   ReadonlySet<number>   // 0-based frame indices to hold on
  first:     number                // lowest anchor index (reverses to fwd here)
  last:      number                // highest anchor index (reverses to rev here)
  holdMinMs: number
  holdMaxMs: number
}

/** Uniform random number in [min, max). `rng` is injectable for tests. */
export function randBetween(min: number, max: number, rng: () => number = Math.random): number {
  return min + rng() * (max - min)
}

/**
 * Convert a HoldCycle (1-based anchors) to ResolvedHoldCycle (0-based), dropping
 * out-of-range anchors. Returns undefined when there is nothing usable to hold on.
 */
export function resolveHoldCycle(hc: HoldCycle | undefined, frameCount: number): ResolvedHoldCycle | undefined {
  if (!hc) return undefined
  const zero = hc.anchors.map(a => a - 1).filter(i => i >= 0 && i < frameCount)
  if (zero.length === 0) return undefined
  return {
    anchors:   new Set(zero),
    first:     Math.min(...zero),
    last:      Math.max(...zero),
    holdMinMs: hc.holdMinMs,
    holdMaxMs: hc.holdMaxMs,
  }
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

// ── Flying transform clip ────────────────────────────────────────────
/**
 * Frames in public/assets/fly_to_idle/ — the fly→idle morph, left-facing.
 * ONE shared file set serves both transitions: `fly_exit` plays it forward
 * (fly→idle) and `fly_enter` plays it reversed (idle→fly). Placeholder 1
 * until the assets land — update to the real count when the files are added
 * (a missing folder degrades gracefully: broken frames are never painted).
 */
export const FLY_TRANSFORM_COUNT = 1

// ── Default registry (matches Python original) ──────────────────────

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
  fly_enter:           { dir: 'fly_to_idle',    count: FLY_TRANSFORM_COUNT, fps: 24, loop: false,
                         buildSequence: f => f.slice().reverse() },
  fly_exit:            { dir: 'fly_to_idle',    count: FLY_TRANSFORM_COUNT, fps: 24, loop: false },
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

// ── Frame preloading (shared) ───────────────────────────────────────
/**
 * Load every frame of an animation directory
 * (`/assets/<dir>/frame_001.webp … frame_NNN.webp`) into Image objects.
 * Failed frames resolve too (they report naturalWidth === 0) so a missing
 * file never blocks playback. Swapping an <img>.src to a preloaded frame's
 * URL is a cache hit, which is what keeps 24 fps playback stutter-free.
 */
export function preloadFrames(dir: string, count: number): Promise<HTMLImageElement[]> {
  const frames: HTMLImageElement[] = []
  const promises: Promise<void>[]  = []
  for (let i = 1; i <= count; i++) {
    const img = new Image()
    img.src = `/assets/${dir}/frame_${String(i).padStart(3, '0')}.webp`
    frames.push(img)
    promises.push(new Promise(res => {
      img.onload  = () => res()
      img.onerror = () => res()
    }))
  }
  return Promise.all(promises).then(() => frames)
}

// ── Sleep crossfade ─────────────────────────────────────────────────
/**
 * Duration of the idle↔sleep sprite crossfade, in ms. enterSleep/exitSleep
 * dip `spriteOpacity` to 0, swap the frame set, then raise it back to 1. The
 * matching CSS `transition: opacity` lives on the <img> in PetWindow — keep
 * the two values in sync.
 */
export const SLEEP_FADE_MS = 220

const waitMs = (ms: number) => new Promise<void>(r => setTimeout(r, ms))

// ── Composable ─────────────────────────────────────────────────────

/**
 * @param registry  Animation definitions (defaults to DEFAULT_ANIMATIONS).
 * @param imgRef    Optional ref to the <img> element. When provided, frame
 *                  advances are applied as direct `.src` writes — bypassing
 *                  Vue's reactive scheduler entirely on the 60 Hz hot path.
 */
export function useAnimator(
  registry: Record<AnimationName, AnimationDef> = DEFAULT_ANIMATIONS,
  imgRef?:  Ref<HTMLImageElement | null>,
) {
  // Loaded animations populated by preload(); kept in a shallowRef so Vue
  // doesn't try to deeply proxy the image arrays.
  const loaded = shallowRef<Partial<Record<AnimationName, LoadedAnimation>>>({})

  // Current state
  const currentName = ref<AnimationName>('idle')
  const ready       = ref(false)
  // Max-priority rest state. While true, ordinary animation requests
  // (audio reaction, click, idle-variant) are ignored — only exitSleep()
  // can leave it. See enterSleep / exitSleep and the guards on setAnim /
  // queueAnim below.
  const sleeping    = ref(false)
  // Sprite opacity, driving the idle↔sleep crossfade. The animator owns this
  // transition since it owns the <img>'s frame swaps; PetWindow just binds it.
  // Written only on sleep enter/exit (not the hot path), so plain reactivity
  // is fine.
  const spriteOpacity = ref(1)
  // Flying (balloon) mode. Like sleep, a max-priority state: while true,
  // ordinary animation requests are ignored. Set by enterFlight(); drops
  // when the fly_exit morph finishes (see nextAnimAfterEnd).
  const flying = ref(false)

  // Internal state (not reactive — touched 60+ times/sec)
  let frameIx     = 0
  let pendingAnim: AnimationName | null = null
  let lastFrameT  = 0
  let rafId       = 0
  // Anchored-hold playback (holdCycle animations, e.g. sleep):
  //   holdUntil > t → frozen on the current anchor until then.
  //   holdDir       → ping-pong direction (+1 forward / -1 reverse).
  let holdUntil   = 0
  let holdDir     = 1
  // Which idle-variant to play. Updated by setIdleVariant() from pet-status events.
  let idleAnimName: AnimationName = 'idle'
  // Whether she was sleeping when flight started — fly_exit returns there.
  let wasSleepingBeforeFlight = false

  // ── Direct DOM paint (hot path) ────────────────────────────────
  // Writing .src directly on the element is invisible to Vue's scheduler —
  // no proxy trap, no dependency tracking, no queued re-render.
  //
  // Frames that failed to load report naturalWidth === 0; we skip painting
  // them so a missing asset never replaces a good frame with the browser's
  // broken-image glyph. This is what lets the not-yet-authored `sleep`
  // animation degrade gracefully (it just holds the last drawn pose).
  function paintFrame(img: HTMLImageElement): void {
    if (imgRef?.value && img.naturalWidth > 0) imgRef.value.src = img.src
  }

  // ── Preload ────────────────────────────────────────────────────
  function preloadAnim(def: AnimationDef): Promise<HTMLImageElement[]> {
    return preloadFrames(def.dir, def.count).then(frames =>
      def.buildSequence ? def.buildSequence(frames) : frames
    )
  }

  async function preloadAll(): Promise<void> {
    const entries = Object.entries(registry) as [AnimationName, AnimationDef][]
    const idleDef = registry.idle

    // ── Step 1: show the very first frame as soon as it has loaded ────────
    // Load frame_001 alone so the pet appears immediately instead of waiting
    // for all idle frames to download.
    const firstFrame = new Image()
    firstFrame.src = `/assets/${idleDef.dir}/frame_001.webp`
    await new Promise<void>(res => {
      firstFrame.onload  = () => res()
      firstFrame.onerror = () => res()
    })
    // Seed the idle animation with a single-frame array so the RAF loop can
    // start. `ready` gates the <img v-show> — set it true then paint directly
    // (imgRef.value is always available with v-show; no nextTick needed).
    loaded.value = { idle: { frames: [firstFrame], fps: idleDef.fps, loop: idleDef.loop } }
    ready.value = true
    paintFrame(firstFrame)

    // ── Step 2: load all remaining frames concurrently ────────────────────
    await Promise.all([
      // Full idle array — overwrites the single-frame seed when complete.
      preloadAnim(idleDef).then(frames => {
        loaded.value = { ...loaded.value, idle: { frames, fps: idleDef.fps, loop: idleDef.loop } }
      }),
      // All other animations.
      ...entries
        .filter(([name]) => name !== 'idle')
        .map(async ([name, def]) => {
          const frames = await preloadAnim(def)
          loaded.value = { ...loaded.value, [name]: {
            frames, fps: def.fps, loop: def.loop,
            holdCycle: resolveHoldCycle(def.holdCycle, frames.length),
          } }
        }),
    ])
  }

  // ── State machine ─────────────────────────────────────────────
  // Unconditional switch. Used internally by the tick loop and by the sleep
  // transitions, which must be able to set animations the public setAnim guard
  // would otherwise block.
  function applyAnim(name: AnimationName) {
    const def = loaded.value[name]
    if (!def) return                 // not loaded yet — silently ignore
    currentName.value = name
    frameIx = 0
    const now = performance.now()
    lastFrameT = now
    // Arm anchored-hold playback: frame 0 is the first anchor, so start by
    // resting there. Cleared (holdUntil = 0) for ordinary animations.
    holdDir   = 1
    holdUntil = def.holdCycle ? now + randBetween(def.holdCycle.holdMinMs, def.holdCycle.holdMaxMs) : 0
    paintFrame(def.frames[0])        // direct DOM write — no reactive overhead
  }

  /**
   * Public animation switch. No-op while sleeping or flying so clicks,
   * pat-head, and other ad-hoc requests cannot interrupt those states.
   * Use exitSleep() / exitFlight() to leave them.
   */
  function setAnim(name: AnimationName) {
    if (sleeping.value || flying.value) return
    applyAnim(name)
  }

  /**
   * Queue an animation to start at the natural end of the current loop.
   * Mirrors `_pending_anim` from the Python original. Ignored while sleeping
   * or flying — the audio reaction must not schedule headphones/music behind
   * the sleep or flying loop.
   */
  function queueAnim(name: AnimationName) {
    if (sleeping.value || flying.value) return
    pendingAnim = name
  }

  // ── Sleep (max-priority rest state) ───────────────────────────
  /**
   * Enter the rest state. Fades the sprite out, swaps to the sleep loop, then
   * fades back in — so the idle→sleep switch dips through transparent instead
   * of hard-cutting. Drops any pending animation. Once set, only exitSleep()
   * can leave it — see the guards on setAnim / queueAnim. No-op if already
   * sleeping.
   */
  async function enterSleep() {
    if (sleeping.value || flying.value) return
    spriteOpacity.value = 0
    await waitMs(SLEEP_FADE_MS)
    sleeping.value = true
    pendingAnim = null
    applyAnim('sleep')
    spriteOpacity.value = 1
  }

  /**
   * Leave the rest state and fall back to the current idle variant, with the
   * same dip-to-transparent crossfade. No-op if not sleeping.
   */
  async function exitSleep() {
    if (!sleeping.value || flying.value) return
    spriteOpacity.value = 0
    await waitMs(SLEEP_FADE_MS)
    sleeping.value = false
    pendingAnim = null
    applyAnim(idleAnimName)
    spriteOpacity.value = 1
  }

  // ── Flying (balloon mode, max-priority like sleep) ────────────
  /**
   * Enter flying mode: play the idle→fly morph (`fly_enter` — the shared
   * transform clip reversed), which chains into the `flying` ping-pong loop.
   * Works from sleep too; the pre-flight sleep state is remembered and
   * restored when flight ends. No-op if already flying.
   */
  function enterFlight() {
    if (flying.value) return
    flying.value = true
    wasSleepingBeforeFlight = sleeping.value
    pendingAnim = null
    applyAnim('fly_enter')
  }

  /**
   * Leave flying mode: play the fly→idle morph (`fly_exit`) once. When it
   * ends, the machine returns to sleep (if she was sleeping when flight
   * started) or the current idle variant, and `flying` drops — see
   * nextAnimAfterEnd. No-op if not flying.
   */
  function exitFlight() {
    if (!flying.value) return
    pendingAnim = null
    applyAnim('fly_exit')
  }

  /** Read the currently-pending animation (if any). */
  function getPending(): AnimationName | null {
    return pendingAnim
  }

  /** Clear the pending animation. No-op if already null. */
  function cancelPending() {
    pendingAnim = null
  }

  /**
   * Return the Image element currently being displayed, if loaded.
   * Used by hit testing to sample alpha at the cursor position.
   */
  function getCurrentImage(): HTMLImageElement | null {
    const def = loaded.value[currentName.value]
    if (!def) return null
    return def.frames[frameIx] ?? null
  }

  /**
   * All loaded frames of an animation (the playback sequence — ping-pong
   * sequences repeat Image objects). Null until preloaded. Used by the
   * flight-collision alpha scan (useFlightCollision.ts).
   */
  function getAnimFrames(name: AnimationName): HTMLImageElement[] | null {
    return loaded.value[name]?.frames ?? null
  }

  /**
   * Resolve the next animation when the current one ends (StopIteration
   * equivalent — the iterator is exhausted).
   */
  function nextAnimAfterEnd(): AnimationName {
    if (pendingAnim) {
      const nxt = pendingAnim
      pendingAnim = null
      return nxt
    }
    const cur = currentName.value
    // Flying chains. pendingAnim is always null here while flying —
    // queueAnim is gated and enterFlight/exitFlight clear it.
    if (cur === 'fly_enter')      return 'flying'
    if (cur === 'fly_exit') {
      flying.value = false
      return wasSleepingBeforeFlight ? 'sleep' : idleAnimName
    }
    if (cur === 'headphones_on')  return 'music1'
    if (cur === 'music1')         return 'music2'
    if (cur === 'music2')         return 'music3'
    if (cur === 'music3')         return 'music4'
    if (cur === 'music4')         return 'music1'
    // Exit animations and one-shots fall back to the current idle variant.
    if (cur === 'headphones_off') return idleAnimName
    const def = loaded.value[cur]
    if (def?.loop) return cur
    return idleAnimName
  }

  /**
   * Update which idle-variant animation to play.
   * Called by usePetStatus when the backend reports a pet-state-update.
   *
   * If the pet is currently in any idle variant, immediately switches to
   * the new variant. Does nothing while in music/click/headphones animations
   * — idleAnimName is simply remembered and used when those end.
   */
  function setIdleVariant(name: AnimationName) {
    idleAnimName = name
    if (IDLE_VARIANTS.has(currentName.value) && currentName.value !== name) {
      setAnim(name)
    }
  }

  // ── Anchored-hold ping-pong (holdCycle animations) ────────────
  // Steps one frame per interval in the current direction; when it lands on an
  // anchor it reverses at the ends and freezes for a random hold. Holds the
  // clock while frozen so resuming never fires a catch-up burst.
  function tickHoldCycle(def: LoadedAnimation, hc: ResolvedHoldCycle, t: number): void {
    if (holdUntil > 0) {
      if (t < holdUntil) { lastFrameT = t; return }   // still resting on the anchor
      holdUntil = 0
      lastFrameT = t
      return                                          // resume stepping next tick
    }

    const interval = 1000 / def.fps
    const elapsed  = t - lastFrameT
    if (elapsed < interval) return
    if (elapsed > interval * 2) lastFrameT = t
    else                        lastFrameT += interval

    frameIx += holdDir
    if (frameIx < 0)                       frameIx = 0
    else if (frameIx >= def.frames.length) frameIx = def.frames.length - 1
    paintFrame(def.frames[frameIx])

    if (hc.anchors.has(frameIx)) {
      // Reverse at the turn-around anchors, then rest here.
      if (frameIx >= hc.last)       holdDir = -1
      else if (frameIx <= hc.first) holdDir = 1
      holdUntil = t + randBetween(hc.holdMinMs, hc.holdMaxMs)
    }
  }

  // ── RAF loop ──────────────────────────────────────────────────
  function tick(t: number) {
    rafId = requestAnimationFrame(tick)

    const def = loaded.value[currentName.value]
    if (!def) return

    if (def.holdCycle) { tickHoldCycle(def, def.holdCycle, t); return }

    const interval = 1000 / def.fps
    const elapsed  = t - lastFrameT
    if (elapsed < interval) return

    // Locked-step + 2× lag clamp (same pattern as Python original).
    if (elapsed > interval * 2) lastFrameT = t
    else                        lastFrameT += interval

    frameIx++
    if (frameIx >= def.frames.length) {
      // End of animation — pick next. Use applyAnim (not setAnim) so the sleep
      // loop can re-arm itself; setAnim is gated while sleeping.
      const nxt = nextAnimAfterEnd()
      applyAnim(nxt)
    } else {
      paintFrame(def.frames[frameIx])  // direct DOM write, no Vue overhead
    }
  }

  // ── Lifecycle ─────────────────────────────────────────────────
  onMounted(async () => {
    await preloadAll()
    setAnim('idle')
    rafId = requestAnimationFrame(tick)
  })

  onUnmounted(() => {
    cancelAnimationFrame(rafId)
  })

  return {
    currentName,
    ready,
    sleeping,
    flying,
    spriteOpacity,
    queueAnim,
    setAnim,
    enterSleep,
    exitSleep,
    enterFlight,
    exitFlight,
    getPending,
    cancelPending,
    getCurrentImage,
    getAnimFrames,
    setIdleVariant,
  }
}
