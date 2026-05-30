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
 */
import { onMounted, onUnmounted, ref, shallowRef } from 'vue'

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
}

export type AnimationName =
  | 'idle'
  | 'idle_low_energy'    // low energy only  (placeholder — swap dir when assets arrive)
  | 'idle_low_affection' // low affection only
  | 'idle_exhausted'     // both low
  | 'click'
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
//               Must be OddNumber so the sequence ends on a forward pass.
//               Use the odd() helper to construct the value.
//
// Exported so the sequence logic can be unit-tested independently.

declare const _ODD: unique symbol
/** Branded type for odd integers. Construct via odd(). */
export type OddNumber = number & { readonly [_ODD]: true }

/** Cast n to OddNumber. Throws a RangeError at runtime if n is even. */
export function odd(n: number): OddNumber {
  if (n % 2 === 0) throw new RangeError(`passes must be odd, got ${n}`)
  return n as OddNumber
}

export function buildPingPongSequence(
  frames:    HTMLImageElement[],
  pingStart: number,
  pingEnd:   number,
  passes:    OddNumber,
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

// ── Default registry (matches Python original) ──────────────────────

export const DEFAULT_ANIMATIONS: Record<AnimationName, AnimationDef> = {
  idle:                { dir: 'idle',           count: 234, fps: 24, loop: true  },
  // ── Idle-variant placeholders ──────────────────────────────────────────
  // These fall back to the normal `idle` directory until dedicated animation
  // assets are added. To activate: update `dir` (and `count` if different).
  idle_low_energy:    { dir: 'idle',           count: 419, fps: 24, loop: true  },
  idle_low_affection: { dir: 'idle',           count: 419, fps: 24, loop: true  },
  idle_exhausted:     { dir: 'idle',           count: 419, fps: 24, loop: true  },
  // ─────────────────────────────────────────────────────────────────────
  click:               { dir: 'click_matched',  count: 156, fps: 24, loop: false },
  headphones_on:       { dir: 'headphones_on',  count: 185, fps: 24, loop: false },
  headphones_off:      { dir: 'headphones_off', count: 185, fps: 24, loop: false },
  music1:              { dir: 'music1/temp',         count: 185, fps: 24, loop: true  },
  music2:              { dir: 'music2',         count: 185, fps: 24, loop: false,
                         buildSequence: f => buildPingPongSequence(f,  81, 104, odd(31)) },
  music3:              { dir: 'music3',         count: 330, fps: 18, loop: true  },
  music4:              { dir: 'music4',         count: 185, fps: 24, loop: false,
                         buildSequence: f => buildPingPongSequence(f, 105, 161, odd(13)) },
}

// ── Composable ─────────────────────────────────────────────────────

export function useAnimator(registry: Record<AnimationName, AnimationDef> = DEFAULT_ANIMATIONS) {
  // Loaded animations populated by preload(); kept in a shallowRef so Vue
  // doesn't try to deeply proxy the image arrays.
  const loaded = shallowRef<Partial<Record<AnimationName, LoadedAnimation>>>({})

  // Current state
  const currentName  = ref<AnimationName>('idle')
  const currentSrc   = ref<string>('')
  const ready        = ref(false)

  // Internal state (not reactive — touched 60+ times/sec)
  let frameIx      = 0
  let pendingAnim: AnimationName | null = null
  let lastFrameT   = 0
  let rafId        = 0
  // Which idle-variant to play. Updated by setIdleVariant() from pet-status events.
  let idleAnimName: AnimationName = 'idle'

  // ── Preload ────────────────────────────────────────────────────
  function preloadAnim(def: AnimationDef): Promise<HTMLImageElement[]> {
    const frames: HTMLImageElement[] = []
    const promises: Promise<void>[]  = []
    for (let i = 1; i <= def.count; i++) {
      const img = new Image()
      img.src = `/assets/${def.dir}/frame_${String(i).padStart(3, '0')}.webp`
      frames.push(img)
      promises.push(new Promise(res => {
        img.onload  = () => res()
        img.onerror = () => res()
      }))
    }
    return Promise.all(promises).then(() =>
      def.buildSequence ? def.buildSequence(frames) : frames
    )
  }

  async function preloadAll(): Promise<void> {
    const entries = Object.entries(registry) as [AnimationName, AnimationDef][]
    // Preload idle first so we can start showing it before the rest finishes.
    const idleDef = registry.idle
    const idleFrames = await preloadAnim(idleDef)
    loaded.value = { ...loaded.value, idle: { frames: idleFrames, fps: idleDef.fps, loop: idleDef.loop } }
    currentSrc.value = idleFrames[0].src
    ready.value = true

    // Preload the rest in parallel.
    await Promise.all(
      entries
        .filter(([name]) => name !== 'idle')
        .map(async ([name, def]) => {
          const frames = await preloadAnim(def)
          loaded.value = { ...loaded.value, [name]: { frames, fps: def.fps, loop: def.loop } }
        })
    )
  }

  // ── State machine ─────────────────────────────────────────────
  function setAnim(name: AnimationName) {
    const def = loaded.value[name]
    if (!def) return                 // not loaded yet — silently ignore
    currentName.value = name
    frameIx = 0
    currentSrc.value = def.frames[0].src
    lastFrameT = performance.now()
  }

  /**
   * Queue an animation to start at the natural end of the current loop.
   * Mirrors `_pending_anim` from the Python original.
   */
  function queueAnim(name: AnimationName) {
    pendingAnim = name
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

  // ── RAF loop ──────────────────────────────────────────────────
  function tick(t: number) {
    rafId = requestAnimationFrame(tick)

    const def = loaded.value[currentName.value]
    if (!def) return

    const interval = 1000 / def.fps
    const elapsed  = t - lastFrameT
    if (elapsed < interval) return

    // Locked-step + 2× lag clamp (same pattern as Python original).
    if (elapsed > interval * 2) lastFrameT = t
    else                        lastFrameT += interval

    frameIx++
    if (frameIx >= def.frames.length) {
      // End of animation — pick next.
      const nxt = nextAnimAfterEnd()
      setAnim(nxt)
    } else {
      const next = def.frames[frameIx]
      // Same-pixmap guard equivalent: only update src if the image object
      // actually differs (currently never triggers since indexes are
      // unique, but enables future hold-frame patterns).
      if (next.src !== currentSrc.value) currentSrc.value = next.src
    }
  }

  // ── Lifecycle ─────────────────────────────────────────────────
  onMounted(async () => {
    await preloadAll()
    setAnim('music1')
    rafId = requestAnimationFrame(tick)
  })

  onUnmounted(() => {
    cancelAnimationFrame(rafId)
  })

  return {
    currentName,
    currentSrc,
    ready,
    queueAnim,
    setAnim,
    getPending,
    cancelPending,
    getCurrentImage,
    setIdleVariant,
  }
}
