/**
 * useFlightCollision — pixel-perfect edge collision for flying mode.
 *
 * The Rust flight controller (src-tauri/src/flight.rs) bounces the pet
 * window off the monitor work-area edges. The window rect includes the
 * transparent padding around the drawn sprite, so rect-based bounces make
 * her appear to turn around in mid-air.
 *
 * Collision therefore tracks the CURRENT frame: every flying frame is
 * alpha-scanned once (chunked so the scan never blocks a paint, cached for
 * the app lifetime), and while flying a light poller watches which frame is
 * displayed and reports that frame's margins to Rust (`flight_set_insets`)
 * whenever they change materially. Rust re-reads the margins every movement
 * tick, so the collision envelope follows the wing-flap cycle.
 *
 * Note: a single union box across all frames is NOT good enough — the flying
 * clip has up to ~26% of the frame width of baked-in horizontal travel, so a
 * union hugs the window rect and bounces fire while the visible pixels are
 * still far from the edge.
 */
import { invoke } from '@tauri-apps/api/core'

// ── Types ───────────────────────────────────────────────────────────

/** Opaque bounding box as fractions of the frame image (x1/y1 exclusive). */
export interface OpaqueBox { x0: number; y0: number; x1: number; y1: number }

/** Per-edge transparent margins as fractions of the window size. */
export interface EdgeInsets { left: number; top: number; right: number; bottom: number }

// ── Configuration ───────────────────────────────────────────────────

/** Frames are scanned downscaled to this width — margin accuracy of a few
 *  device pixels, ~200× cheaper than scanning at native resolution. */
export const SCAN_WIDTH = 160

/** Alpha (0-255) at or above which a pixel counts as visible. */
export const ALPHA_MIN = 16

/** How often the sync poller checks which frame is displayed (ms). Frames
 *  advance every ~42 ms at 24 fps, so this lags at most ~1 px of drift at
 *  flight speed. */
export const SYNC_INTERVAL_MS = 30

/** Minimum per-edge change (fraction of the window) worth an IPC message. */
export const SEND_EPSILON = 0.003

/** Frames scanned per chunk before yielding back to the event loop. */
const SCAN_CHUNK = 8

// ── Per-frame alpha scan (canvas; cached) ───────────────────────────

const boxCache = new Map<HTMLImageElement, OpaqueBox | null>()
let scanCtx: CanvasRenderingContext2D | null = null

function opaqueBoxOf(img: HTMLImageElement): OpaqueBox | null {
  if (img.naturalWidth === 0) return null
  const sw = SCAN_WIDTH
  const sh = Math.max(1, Math.round(img.naturalHeight * (sw / img.naturalWidth)))
  if (!scanCtx || scanCtx.canvas.width !== sw || scanCtx.canvas.height !== sh) {
    const canvas = document.createElement('canvas')
    canvas.width  = sw
    canvas.height = sh
    scanCtx = canvas.getContext('2d', { willReadFrequently: true })
    if (!scanCtx) return null
  }
  scanCtx.clearRect(0, 0, sw, sh)
  scanCtx.drawImage(img, 0, 0, sw, sh)
  const a = scanCtx.getImageData(0, 0, sw, sh).data
  let minX = sw, minY = sh, maxX = -1, maxY = -1
  for (let y = 0; y < sh; y++) {
    for (let x = 0; x < sw; x++) {
      if (a[(y * sw + x) * 4 + 3] >= ALPHA_MIN) {
        if (x < minX) minX = x
        if (x > maxX) maxX = x
        if (y < minY) minY = y
        if (y > maxY) maxY = y
      }
    }
  }
  if (maxX < 0) return null
  // +1: max indices are inclusive pixel coordinates.
  return { x0: minX / sw, y0: minY / sh, x1: (maxX + 1) / sw, y1: (maxY + 1) / sh }
}

let scanPromise: Promise<void> | null = null

/**
 * Alpha-scan all (unique, not yet cached) frames, a few per event-loop turn
 * so a flight start never hitches the enter morph. Idempotent; safe to call
 * on every flight activation. Until a frame is scanned the poller simply
 * keeps the previous margins (first flight: window-rect fallback in Rust).
 */
export function prepareFlightCollision(frames: readonly HTMLImageElement[] | null): Promise<void> {
  if (scanPromise) return scanPromise
  const pending = frames ? [...new Set(frames)].filter(f => !boxCache.has(f)) : []
  if (pending.length === 0) return Promise.resolve()
  scanPromise = (async () => {
    for (let i = 0; i < pending.length; i++) {
      boxCache.set(pending[i], opaqueBoxOf(pending[i]))
      if ((i + 1) % SCAN_CHUNK === 0) await new Promise<void>(r => setTimeout(r))
    }
    scanPromise = null
  })()
  return scanPromise
}

// ── Pure mapping (unit-tested) ──────────────────────────────────────

/**
 * Map an opaque box (fractions of the frame image) to window-edge margins
 * (fractions of the window). The pet <img> fills the window with
 * object-fit: contain, so the frame is scaled to fit and centred — the
 * letterboxing bars are part of the transparent margin.
 */
export function boxToWindowInsets(
  box: OpaqueBox,
  naturalW: number, naturalH: number,
  winW: number, winH: number,
): EdgeInsets {
  const s  = Math.min(winW / naturalW, winH / naturalH)
  const dw = naturalW * s
  const dh = naturalH * s
  const ox = (winW - dw) / 2
  const oy = (winH - dh) / 2
  return {
    left:   (ox + box.x0 * dw) / winW,
    top:    (oy + box.y0 * dh) / winH,
    right:  (winW - (ox + box.x1 * dw)) / winW,
    bottom: (winH - (oy + box.y1 * dh)) / winH,
  }
}

/** True when any edge differs by at least `eps` — i.e. worth resending. */
export function insetsDiffer(a: EdgeInsets, b: EdgeInsets, eps: number = SEND_EPSILON): boolean {
  return (
    Math.abs(a.left - b.left)     >= eps ||
    Math.abs(a.top - b.top)       >= eps ||
    Math.abs(a.right - b.right)   >= eps ||
    Math.abs(a.bottom - b.bottom) >= eps
  )
}

// ── Sync poller (runs only while flying) ────────────────────────────

/**
 * Start reporting the displayed frame's margins to Rust. `getImage` is the
 * animator's current-frame accessor. Returns a stop function — call it when
 * flight ends. Frames without a cached box (still scanning, or a morph
 * clip's frames) keep the previous margins.
 */
export function startFlightInsetsSync(
  getImage: () => HTMLImageElement | null,
): () => void {
  let lastImg: HTMLImageElement | null = null
  let lastSent: EdgeInsets | null = null
  const timer = setInterval(() => {
    const img = getImage()
    if (!img || img === lastImg) return
    lastImg = img
    const box = boxCache.get(img)
    if (!box) return
    const ins = boxToWindowInsets(
      box,
      img.naturalWidth, img.naturalHeight,
      window.innerWidth, window.innerHeight,
    )
    if (lastSent && !insetsDiffer(ins, lastSent)) return
    lastSent = ins
    void invoke('flight_set_insets', { ...ins }).catch(() => {})
  }, SYNC_INTERVAL_MS)
  return () => clearInterval(timer)
}
