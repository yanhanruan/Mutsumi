/**
 * useHitTest — per-pixel click-through against the current animation frame.
 *
 * The Rust backend (`src-tauri/src/cursor.rs`) polls the OS cursor position
 * at ~33 Hz and emits `cursor-pos` events with the cursor relative to the
 * main window's top-left. This composable:
 *
 *   1. First checks `document.elementsFromPoint` for any element marked
 *      with the `pet-ui-overlay` class (weather badge, pomodoro badge,
 *      chat bubble). If found, the window stays interactive so the user
 *      can hover/read those overlays.
 *   2. Otherwise renders the current frame to an offscreen canvas (matching
 *      the window's CSS dimensions using `object-fit: contain` placement)
 *      and samples alpha in a small region around the cursor.
 *   3. Calls `setIgnoreCursorEvents(true)` when the regional max-alpha is
 *      transparent enough, `(false)` otherwise. State changes only —
 *      redundant Tauri calls are filtered.
 *
 * A two-threshold hysteresis (`ALPHA_OPEN` < `ALPHA_CLOSE`) prevents
 * flicker at soft alpha edges. A 5×5 sample region prevents thin features
 * (character legs after downscaling) from falling into the cursor's gap.
 */
import { onMounted, onUnmounted } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'

interface CursorPos {
  x:           number
  y:           number
  over_window: boolean
}

export const ALPHA_OPEN   = 16    // become click-through below this max-alpha
export const ALPHA_CLOSE  = 48    // become interactive above this max-alpha
export const SAMPLE_SIZE  = 5     // NxN sample region — forgiving on thin features

/**
 * Hysteresis decision: given the current ignore state and the sampled alpha,
 * return the next ignore state. Pure for unit testing.
 */
export function shouldIgnore(currentlyIgnoring: boolean, alpha: number): boolean {
  if (currentlyIgnoring) {
    // Currently click-through; only flip back to interactive once alpha is
    // comfortably opaque, to absorb soft anti-aliased edges.
    return alpha < ALPHA_CLOSE
  }
  // Currently interactive; only become click-through once clearly transparent.
  return alpha < ALPHA_OPEN
}

/**
 * Replicates CSS `object-fit: contain` math.
 * Returns the destination rect (dx, dy, dw, dh) for drawing an image of size
 * (imgW × imgH) into a canvas of size (canvasW × canvasH).
 */
export function computeContainPlacement(
  imgW:    number,
  imgH:    number,
  canvasW: number,
  canvasH: number,
): { dx: number; dy: number; dw: number; dh: number } {
  const aspectImg = imgW / imgH
  const aspectCan = canvasW / canvasH
  let dw: number, dh: number
  if (aspectImg > aspectCan) {
    dw = canvasW
    dh = canvasW / aspectImg
  } else {
    dh = canvasH
    dw = canvasH * aspectImg
  }
  const dx = (canvasW - dw) / 2
  const dy = (canvasH - dh) / 2
  return { dx, dy, dw, dh }
}

/**
 * True if any element in the given list (typically from elementsFromPoint)
 * is marked as a pet UI overlay. Pure — accepts the elements directly so
 * tests can supply synthetic DOM lists.
 */
export function isOverUiOverlay(els: Element[]): boolean {
  for (const el of els) {
    if (el instanceof HTMLElement && el.classList.contains('pet-ui-overlay')) {
      return true
    }
  }
  return false
}

export function useHitTest(
  getCurrentImage: () => HTMLImageElement | null,
  /**
   * Optional predicate: true while a full-window overlay (tarot card, system
   * state panel) is open. In that mode the pet sprite is hidden, so sampling
   * its stale last frame would leave random "ghost" pixels interactive. Skip
   * the alpha sampling entirely — only the overlay's own `pet-ui-overlay`
   * elements (e.g. the panel) are interactive; everything else is click-through.
   */
  isOverlayActive?: () => boolean,
) {
  const canvas = document.createElement('canvas')
  const ctx    = canvas.getContext('2d', { willReadFrequently: true })!
  let unlisten: UnlistenFn | null = null
  let ignoring = false       // current setIgnoreCursorEvents state
  let resizeTimer: number | null = null
  // Cache: skip the canvas redraw + getImageData readback when the
  // animation frame hasn't changed since the last cursor event.
  let lastDrawnImg: HTMLImageElement | null = null

  function syncCanvasSize() {
    const w = window.innerWidth
    const h = window.innerHeight
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width  = w
      canvas.height = h
      lastDrawnImg  = null  // invalidate cache — pixel layout changed
    }
  }

  function redrawCurrentFrame() {
    const img = getCurrentImage()
    if (!img || !img.complete || img.naturalWidth === 0) return
    // Same frame as last time — skip the canvas clear+draw and GPU readback.
    if (img === lastDrawnImg) return
    syncCanvasSize()

    const { dx, dy, dw, dh } = computeContainPlacement(
      img.naturalWidth, img.naturalHeight,
      canvas.width, canvas.height,
    )
    ctx.clearRect(0, 0, canvas.width, canvas.height)
    ctx.drawImage(img, dx, dy, dw, dh)
    lastDrawnImg = img  // mark this frame as cached
  }

  async function setIgnore(ignore: boolean) {
    if (ignore === ignoring) return
    ignoring = ignore
    try {
      await getCurrentWindow().setIgnoreCursorEvents(ignore)
    } catch (e) {
      console.warn('[hit-test] setIgnoreCursorEvents failed', e)
    }
  }

  /**
   * Sample max alpha in a SAMPLE_SIZE × SAMPLE_SIZE region centered on (cx, cy).
   * Returns the maximum alpha (0–255) within the region. Using max instead of
   * a single pixel makes thin sprites (legs, antenna) reliably clickable.
   */
  function sampleMaxAlpha(cx: number, cy: number): number {
    const half = Math.floor(SAMPLE_SIZE / 2)
    let x0 = cx - half
    let y0 = cy - half
    // Clamp inside the canvas — fully outside returns 0.
    if (x0 + SAMPLE_SIZE <= 0 || y0 + SAMPLE_SIZE <= 0) return 0
    if (x0 >= canvas.width   || y0 >= canvas.height)    return 0
    x0 = Math.max(0, Math.min(canvas.width  - SAMPLE_SIZE, x0))
    y0 = Math.max(0, Math.min(canvas.height - SAMPLE_SIZE, y0))

    const data = ctx.getImageData(x0, y0, SAMPLE_SIZE, SAMPLE_SIZE).data
    let max = 0
    for (let i = 3; i < data.length; i += 4) {
      if (data[i] > max) max = data[i]
    }
    return max
  }

  function handleCursor(pos: CursorPos) {
    if (!pos.over_window) {
      // Outside our window — reset to interactive so a return into the
      // window with cursor over a visible pixel is detected immediately.
      void setIgnore(false)
      return
    }

    const dpr  = window.devicePixelRatio || 1
    const cssX = pos.x / dpr
    const cssY = pos.y / dpr

    const overUi = isOverUiOverlay(document.elementsFromPoint(cssX, cssY))

    // Overlay mode: only the overlay's own UI is interactive; the transparent
    // area around it is click-through (don't sample the hidden pet sprite).
    if (isOverlayActive?.()) {
      void setIgnore(!overUi)
      return
    }

    // UI overlays take priority — never click-through over them.
    if (overUi) {
      void setIgnore(false)
      return
    }

    // Otherwise sample the current frame's alpha.
    redrawCurrentFrame()
    const alpha = sampleMaxAlpha(Math.floor(cssX), Math.floor(cssY))
    const next  = shouldIgnore(ignoring, alpha)
    void setIgnore(next)
  }

  function onResize() {
    if (resizeTimer !== null) window.clearTimeout(resizeTimer)
    resizeTimer = window.setTimeout(() => {
      syncCanvasSize()
      redrawCurrentFrame()
    }, 100)
  }

  onMounted(async () => {
    syncCanvasSize()
    window.addEventListener('resize', onResize)
    unlisten = await listen<CursorPos>('cursor-pos', e => handleCursor(e.payload))
  })

  onUnmounted(() => {
    if (resizeTimer !== null) window.clearTimeout(resizeTimer)
    window.removeEventListener('resize', onResize)
    unlisten?.()
    void getCurrentWindow().setIgnoreCursorEvents(false).catch(() => {})
  })
}
