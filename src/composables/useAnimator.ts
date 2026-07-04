/**
 * useAnimator — the HOW of the animation system: playback engine + state
 * machine. The WHAT (registry, chains, classification sets, baseline policy)
 * is data in src/config/animations.ts.
 *
 * Playback engine (non-reactive hot path, mirrors the Python original):
 *   - preloads every frame, then advances on a locked-step clock with a 2×
 *     lag clamp — the pattern that fixed smoothness/catch-up in the original
 *   - paints by writing <img>.src directly via the optional imgRef, bypassing
 *     Vue's reactive proxy so the scheduler never runs on a frame advance
 *   - anchored-hold ping-pong playback for holdCycle clips (sleep)
 *
 * State machine (reactive surface):
 *   - current animation + a single "pending" slot consumed at loop end
 *   - two max-priority modes — sleep and flying — that gate ordinary requests
 *   - synchronized inputs mirroring backend-owned facts (setIdleVariant ←
 *     pet status, setAudioActive ← audio state)
 *   - every natural ending resolves the same way: pending slot first, then
 *     ANIM_CHAINS, then re-loop, then the shared resolveBaseline rule
 */
import { onMounted, onUnmounted, ref, shallowRef, type Ref } from 'vue'
import {
  ANIM_CHAINS,
  DEFAULT_ANIMATIONS,
  IDLE_VARIANTS,
  resolveBaseline,
  type AnimationDef,
  type AnimationName,
  type HoldCycle,
} from '../config/animations'

// ── Runtime representations ─────────────────────────────────────────

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

// ── Frame preloading ────────────────────────────────────────────────
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

  // ── Reactive state ─────────────────────────────────────────────
  const currentName = ref<AnimationName>('idle')
  const ready       = ref(false)
  // Max-priority rest state. While true, ordinary animation requests
  // (audio reaction, click, idle-variant) are ignored — only exitSleep()
  // can leave it. See enterSleep / exitSleep and the guards on setAnim /
  // queueAnim below.
  const sleeping    = ref(false)
  // Flying (balloon) mode. Like sleep, a max-priority state: while true,
  // ordinary animation requests are ignored. Set by enterFlight(); drops
  // when the fly_exit morph finishes (see nextAnimAfterEnd).
  const flying = ref(false)
  // Sprite opacity, driving the idle↔sleep crossfade. The animator owns this
  // transition since it owns the <img>'s frame swaps; PetWindow just binds it.
  // Written only on sleep enter/exit (not the hot path), so plain reactivity
  // is fine.
  const spriteOpacity = ref(1)

  // ── Non-reactive state (touched 60+ times/sec) ─────────────────
  let frameIx     = 0
  let pendingAnim: AnimationName | null = null
  let lastFrameT  = 0
  let rafId       = 0
  // Anchored-hold playback (holdCycle animations, e.g. sleep):
  //   holdUntil > t → frozen on the current anchor until then.
  //   holdDir       → ping-pong direction (+1 forward / -1 reverse).
  let holdUntil   = 0
  let holdDir     = 1

  // ── Synchronized inputs (backend-owned facts, mirrored here) ───
  // Which idle-variant to play. Updated by setIdleVariant() from pet-status events.
  let idleAnimName: AnimationName = 'idle'
  // Whether the backend reports audio playing. Updated by setAudioActive()
  // (useAudioReaction mirrors the Rust AudioState here). Unlike the
  // event-driven queueAnim calls — which are gated while sleeping/flying —
  // this mirror is ALWAYS kept fresh, so the baseline resolver can restore
  // music mode after any state that swallowed the transition events.
  let audioActive = false

  // ════════════════════════════════════════════════════════════════
  // Playback engine
  // ════════════════════════════════════════════════════════════════

  // ── Direct DOM paint (hot path) ────────────────────────────────
  // Writing .src directly on the element is invisible to Vue's scheduler —
  // no proxy trap, no dependency tracking, no queued re-render.
  //
  // Frames that failed to load report naturalWidth === 0; we skip painting
  // them so a missing asset never replaces a good frame with the browser's
  // broken-image glyph. This is what lets a not-yet-authored animation
  // degrade gracefully (it just holds the last drawn pose).
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
      applyAnim(nextAnimAfterEnd())
    } else {
      paintFrame(def.frames[frameIx])  // direct DOM write, no Vue overhead
    }
  }

  // ════════════════════════════════════════════════════════════════
  // State machine
  // ════════════════════════════════════════════════════════════════

  // Unconditional switch. Used internally by the tick loop and by the
  // sleep/flight transitions, which must be able to set animations the
  // public setAnim guard would otherwise block.
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

  /** The shared return-to-rest rule (see resolveBaseline). */
  function baselineAnim(): AnimationName {
    return resolveBaseline(sleeping.value, audioActive, idleAnimName)
  }

  /**
   * Resolve the next animation when the current one ends (StopIteration
   * equivalent — the iterator is exhausted). One uniform cascade:
   * pending slot → fly_exit landing → declared chain → re-loop → baseline.
   */
  function nextAnimAfterEnd(): AnimationName {
    if (pendingAnim) {
      const nxt = pendingAnim
      pendingAnim = null
      return nxt
    }
    const cur = currentName.value
    // Landing is the one ending with a side effect: flight mode drops here.
    // pendingAnim is always null while flying — queueAnim is gated and
    // enterFlight/exitFlight clear it.
    if (cur === 'fly_exit') {
      flying.value = false
      return baselineAnim()
    }
    // Declared chains (music cycle, fly_enter → flying).
    const chained = ANIM_CHAINS[cur]
    if (chained) return chained
    // Loops re-arm; everything else — one-shots like pat_head, click,
    // headphones_off — falls back through the shared baseline rule.
    if (loaded.value[cur]?.loop) return cur
    return baselineAnim()
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

  /** Read the currently-pending animation (if any). */
  function getPending(): AnimationName | null {
    return pendingAnim
  }

  /** Clear the pending animation. No-op if already null. */
  function cancelPending() {
    pendingAnim = null
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
   * Leave the rest state and fall back to the baseline (music mode if audio
   * kept playing while she slept, else the current idle variant), with the
   * same dip-to-transparent crossfade. No-op if not sleeping.
   */
  async function exitSleep() {
    if (!sleeping.value || flying.value) return
    spriteOpacity.value = 0
    await waitMs(SLEEP_FADE_MS)
    sleeping.value = false
    pendingAnim = null
    applyAnim(baselineAnim())
    spriteOpacity.value = 1
  }

  // ── Flying (balloon mode, max-priority like sleep) ────────────
  /**
   * Enter flying mode: play the idle→fly morph (`fly_enter` — the shared
   * transform clip reversed), which chains into the `flying` ping-pong loop.
   * Works from sleep too — `sleeping` stays set through the flight (enter/
   * exitSleep are gated while flying), so the landing baseline returns there.
   * No-op if already flying.
   */
  function enterFlight() {
    if (flying.value) return
    flying.value = true
    pendingAnim = null
    applyAnim('fly_enter')
  }

  /**
   * Leave flying mode: play the fly→idle morph (`fly_exit`) once. When it
   * ends, the machine returns to the baseline (sleep if she slept before
   * takeoff, music if audio is playing, else the current idle variant) and
   * `flying` drops — see nextAnimAfterEnd. No-op if not flying.
   */
  function exitFlight() {
    if (!flying.value) return
    pendingAnim = null
    applyAnim('fly_exit')
  }

  // ── Synchronized inputs ────────────────────────────────────────
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

  /**
   * Mirror of the backend's AudioState (Rust audio.rs owns the truth).
   * Called by useAudioReaction on every audio-started / audio-stopped event
   * and once at bootstrap — never gated, unlike the animation requests those
   * events also make, so it stays fresh through flight/sleep/one-shots.
   *
   * Pure input: it only records the fact. Reacting to it stays where it
   * always was — live transitions enter/exit via useAudioReaction's queued
   * animations; endings consult the baseline resolver, which reads this.
   */
  function setAudioActive(active: boolean) {
    audioActive = active
  }

  // ── Queries (for hit-testing / flight collision) ───────────────
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
    setAudioActive,
  }
}
