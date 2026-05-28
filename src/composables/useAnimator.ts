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
  /** Number of frames (file_001.webp through file_NNN.webp). */
  count:     number
  fps:       number
  loop:      boolean
}

export type AnimationName =
  | 'idle'
  | 'click'
  | 'headphones_on'
  | 'headphones_off'
  | 'music1'
  | 'music2'
  | 'music3'
  | 'music4'

interface LoadedAnimation {
  frames:    HTMLImageElement[]
  fps:       number
  loop:      boolean
}

// ── Default registry (matches Python original) ──────────────────────

export const DEFAULT_ANIMATIONS: Record<AnimationName, AnimationDef> = {
  idle:           { dir: 'idle',           count: 234, fps: 24, loop: true  },
  click:          { dir: 'click_matched',  count: 156, fps: 24, loop: false },
  headphones_on:  { dir: 'headphones_on',  count: 185, fps: 24, loop: false },
  headphones_off: { dir: 'headphones_off', count: 185, fps: 24, loop: false },
  music1:         { dir: 'music1',         count: 185, fps: 24, loop: true  },
  music2:         { dir: 'music2',         count: 185, fps: 24, loop: true  },
  music3:         { dir: 'music3',         count: 330, fps: 24, loop: true  },
  music4:         { dir: 'music4',         count: 185, fps: 24, loop: true  },
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
    return Promise.all(promises).then(() => frames)
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
    if (cur === 'headphones_off') return 'idle'
    // Loop or fall back to idle for one-shots.
    const def = loaded.value[cur]
    if (def?.loop) return cur
    return 'idle'
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
    setAnim('idle')
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
  }
}
