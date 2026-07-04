/**
 * useFlightCollision — pixel-perfect edge collision for flying mode.
 *
 * The Rust flight controller (src-tauri/src/flight.rs) bounces the pet
 * window off the monitor work-area edges. The window rect includes the
 * transparent padding around the drawn sprite, so rect-based bounces make
 * her appear to turn around in mid-air. This module alpha-scans the flying
 * frames once, takes the union opaque bounding box across every frame,
 * converts it to per-edge margins (fractions of the window, accounting for
 * the <img>'s object-fit: contain letterboxing) and reports them to Rust
 * via `flight_set_insets`, so bounces fire exactly when a visible pixel
 * reaches the edge.
 *
 * The union across frames (rather than per-frame boxes) keeps the physics
 * stable: the collision envelope doesn't pulse with the wing-flap cycle.
 */
import { invoke } from '@tauri-apps/api/core'

// ── Types ───────────────────────────────────────────────────────────

/** Opaque bounding box as fractions of the frame image (x1/y1 exclusive). */
export interface OpaqueBox { x0: number; y0: number; x1: number; y1: number }

/** Per-edge transparent margins as fractions of the window size. */
export interface EdgeInsets { left: number; top: number; right: number; bottom: number }

// ── Scan configuration ──────────────────────────────────────────────

/** Frames are scanned downscaled to this width — margin accuracy of a few
 *  device pixels, ~200× cheaper than scanning at native resolution. */
export const SCAN_WIDTH = 160

/** Alpha (0-255) at or above which a pixel counts as visible. */
export const ALPHA_MIN = 16

// ── Alpha scan (canvas — not unit-tested, kept minimal) ─────────────

/**
 * Union opaque bounding box across all distinct frames, as fractions of the
 * frame image. Returns null when nothing is loaded or everything is
 * transparent. One-time cost per frame set; callers should cache.
 */
export function unionOpaqueBox(frames: readonly HTMLImageElement[]): OpaqueBox | null {
  const unique = [...new Set(frames)].filter(f => f.naturalWidth > 0)
  const first = unique[0]
  if (!first) return null

  const sw = SCAN_WIDTH
  const sh = Math.max(1, Math.round(first.naturalHeight * (sw / first.naturalWidth)))
  const canvas = document.createElement('canvas')
  canvas.width  = sw
  canvas.height = sh
  const ctx = canvas.getContext('2d', { willReadFrequently: true })
  if (!ctx) return null

  let minX = sw, minY = sh, maxX = -1, maxY = -1
  for (const f of unique) {
    ctx.clearRect(0, 0, sw, sh)
    ctx.drawImage(f, 0, 0, sw, sh)
    const a = ctx.getImageData(0, 0, sw, sh).data
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
  }
  if (maxX < 0) return null
  // +1: max indices are inclusive pixel coordinates.
  return { x0: minX / sw, y0: minY / sh, x1: (maxX + 1) / sw, y1: (maxY + 1) / sh }
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

// ── Orchestrator ────────────────────────────────────────────────────

let cachedBox: OpaqueBox | null = null
let cachedFrames: readonly HTMLImageElement[] | null = null

/**
 * Scan (once, cached per frame set) and report the flying sprite's margins
 * to the Rust flight controller. Call on each flight activation — the
 * mapping is recomputed against the current window size. Silently no-ops
 * when frames aren't loaded yet; Rust then falls back to window-rect
 * collision for that flight.
 */
export async function sendFlightInsets(frames: readonly HTMLImageElement[] | null): Promise<void> {
  if (!frames || frames.length === 0) return
  if (cachedFrames !== frames) {
    cachedBox = unionOpaqueBox(frames)
    cachedFrames = frames
  }
  const first = frames.find(f => f.naturalWidth > 0)
  if (!cachedBox || !first) return
  const insets = boxToWindowInsets(
    cachedBox,
    first.naturalWidth, first.naturalHeight,
    window.innerWidth, window.innerHeight,
  )
  await invoke('flight_set_insets', { ...insets }).catch(() => {})
}
