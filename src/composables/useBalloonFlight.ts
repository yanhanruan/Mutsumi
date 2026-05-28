/**
 * useBalloonFlight — pure movement helpers for BalloonPet.
 *
 * Movement is driven by blending two vectors each frame:
 *   - 30% center-seeking (normalized vector from position → screen center)
 *   - 70% random unit vector (injected for deterministic testing)
 * The blend is re-normalized and scaled by SPEED (px / RAF frame).
 *
 * Anti-sticking on wall collision: after recalculating the blended vector,
 * the axis perpendicular to the hit wall is forced away from that wall
 * (abs / negate). This prevents the sprite from getting pinned to a corner.
 *
 * All exported functions are pure (no DOM / Tauri calls) and fully unit-testable.
 */

// ── Constants ──────────────────────────────────────────────────────

/** Movement speed in CSS pixels per requestAnimationFrame tick (~60 fps). */
export const SPEED         = 1.5

/** Weight of the center-seeking vector in the blend (0–1). */
export const CENTER_WEIGHT = 0.3

/** Weight of the random vector in the blend (must sum to 1 with CENTER_WEIGHT). */
export const RANDOM_WEIGHT = 0.7

/** Sprite bounding-box width used for right/bottom collision edges. */
export const SPRITE_W = 120

/** Sprite bounding-box height used for right/bottom collision edges. */
export const SPRITE_H = 120

// ── Types ──────────────────────────────────────────────────────────

export interface Vec2 { vx: number; vy: number }

// ── Pure helpers ───────────────────────────────────────────────────

/**
 * Returns a random unit vector with uniform angular distribution.
 * Used as the default `randVec` argument in `blendVectors`.
 */
export function randomUnitVec(): Vec2 {
  const a = Math.random() * Math.PI * 2
  return { vx: Math.cos(a), vy: Math.sin(a) }
}

/**
 * Normalizes (vx, vy) to length 1.
 * Returns a random unit vector when the input is near-zero to avoid NaN.
 */
export function normalize(vx: number, vy: number): Vec2 {
  const len = Math.sqrt(vx * vx + vy * vy)
  if (len < 1e-6) return randomUnitVec()
  return { vx: vx / len, vy: vy / len }
}

/**
 * Computes a blended velocity vector:
 *   result = normalize( CENTER_WEIGHT * centerUnit + RANDOM_WEIGHT * randVec )
 *            * SPEED
 *
 * @param px      Sprite left-edge position (CSS px)
 * @param py      Sprite top-edge position (CSS px)
 * @param screenW Viewport width
 * @param screenH Viewport height
 * @param randVec Optional pre-supplied unit vector (injectable for tests).
 *                Defaults to a fresh random unit vector.
 */
export function blendVectors(
  px: number,
  py: number,
  screenW: number,
  screenH: number,
  randVec: Vec2 = randomUnitVec(),
): Vec2 {
  // Center-seeking: unit vector from sprite position toward screen center.
  const cx = screenW / 2 - px
  const cy = screenH / 2 - py
  const c = normalize(cx, cy)

  // Blend and normalize.
  const bx = CENTER_WEIGHT * c.vx + RANDOM_WEIGHT * randVec.vx
  const by = CENTER_WEIGHT * c.vy + RANDOM_WEIGHT * randVec.vy
  const n = normalize(bx, by)

  return { vx: n.vx * SPEED, vy: n.vy * SPEED }
}

/**
 * Forces the velocity component perpendicular to each hit wall away from
 * that wall. Apply this AFTER `blendVectors` on every collision frame.
 *
 * Rules:
 *   hit left  → vx =  |vx|   (must travel right)
 *   hit right → vx = −|vx|   (must travel left)
 *   hit top   → vy =  |vy|   (must travel downward)
 *   hit bot   → vy = −|vy|   (must travel upward)
 */
export function applyWallForce(
  vx: number, vy: number,
  hitLeft:  boolean,
  hitRight: boolean,
  hitTop:   boolean,
  hitBot:   boolean,
): Vec2 {
  let nx = vx
  let ny = vy
  if (hitLeft)  nx =  Math.abs(nx)
  if (hitRight) nx = -Math.abs(nx)
  if (hitTop)   ny =  Math.abs(ny)
  if (hitBot)   ny = -Math.abs(ny)
  return { vx: nx, vy: ny }
}

/**
 * Advances the sprite by (vx, vy), clamps it within the viewport, and
 * returns which walls were hit.
 *
 * Collision is inclusive on the near edge (x ≤ 0 → hitLeft) and
 * exclusive on the far edge (x + SPRITE_W ≥ screenW → hitRight), which
 * matches typical Win32 / CSS hit-test conventions.
 */
export function stepPosition(
  x:  number, y:  number,
  vx: number, vy: number,
  screenW: number,
  screenH: number,
): {
  nx: number; ny: number
  hitLeft: boolean; hitRight: boolean
  hitTop:  boolean; hitBot:   boolean
} {
  const nx = x + vx
  const ny = y + vy

  const hitLeft  = nx <= 0
  const hitRight = nx + SPRITE_W >= screenW
  const hitTop   = ny <= 0
  const hitBot   = ny + SPRITE_H >= screenH

  return {
    nx: Math.max(0, Math.min(screenW - SPRITE_W, nx)),
    ny: Math.max(0, Math.min(screenH - SPRITE_H, ny)),
    hitLeft,
    hitRight,
    hitTop,
    hitBot,
  }
}
