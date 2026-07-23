/**
 * deform.ts — parameter-driven mesh deformation: the from-scratch core of the
 * puppet renderer. Given a base mesh and a set of parameter→keyform bindings,
 * it produces the deformed vertex positions for the current parameter values.
 * This is the actual "magic" of Live2D-style animation, reimplemented openly.
 *
 * Model (the shape Live2D and Inochi2D both use, our minimal version):
 *   - a Keyform captures the mesh's per-vertex OFFSETS at one normalized
 *     parameter position `at` ∈ [0,1]. ParamEyeLOpen has keyforms at 0 (shut)
 *     and 1 (open); the offsets morph the eyelid vertices between them.
 *   - a ParamBinding is one parameter's ordered list of keyforms.
 *   - deform() interpolates each binding between its two surrounding keyforms at
 *     the current value, then sums every binding ADDITIVELY onto the base mesh —
 *     so blink, breathing, gaze, and lip sync COMPOSE instead of overwriting one
 *     another. (Inochi2D's headline advantage over Live2D, and what the
 *     migration plan means by a defined parameter processing order.)
 *
 * Pure: no WebGL. Uploading these vertices to the GPU is a later slice; this
 * module is just the math, fully unit-tested. Offsets are {x,y} for readability;
 * a Float32Array fast path can slot in behind this same API when perf needs it.
 */
import type { ParameterDef } from './parameters'
import { normalizeParam } from './parameters'

export interface Vec2 { x: number; y: number }

/** The mesh's per-vertex offsets at one normalized parameter position. */
export interface Keyform {
  /** Normalized parameter position in [0,1] this keyform applies at. */
  at:      number
  /** Offset per base vertex; index-aligned with the base mesh. */
  offsets: Vec2[]
}

/** One parameter's deformation. `keyforms` MUST be sorted ascending by `at`. */
export interface ParamBinding {
  paramId:  string
  keyforms: Keyform[]
}

/**
 * Deform `base` by every binding at the current parameter values. Returns a
 * fresh vertex array; `base` is never mutated (a rig's rest pose is reused every
 * frame). Bindings whose parameter is unknown or which have no keyforms are
 * skipped. Offsets are added only where index-aligned, so a keyform with fewer
 * offsets than the mesh silently affects just its leading vertices instead of
 * throwing — a partial rig degrades rather than crashing.
 */
export function deform(
  base:     readonly Vec2[],
  bindings: readonly ParamBinding[],
  defs:     Map<string, ParameterDef>,
  values:   Map<string, number>,
): Vec2[] {
  const out: Vec2[] = base.map(v => ({ x: v.x, y: v.y }))
  for (const binding of bindings) {
    const def = defs.get(binding.paramId)
    if (!def || binding.keyforms.length === 0) continue
    const t = normalizeParam(def, values.get(binding.paramId) ?? def.default)
    addKeyformOffsets(out, binding.keyforms, t)
  }
  return out
}

/**
 * Add the interpolated offsets of one binding at normalized position `t` onto
 * `out`. Below the first / above the last keyform we hold the endpoint (no
 * extrapolation), which keeps a parameter pinned at its extreme from flinging
 * vertices past the artist's intended pose.
 */
function addKeyformOffsets(out: Vec2[], keyforms: Keyform[], t: number): void {
  const last = keyforms.length - 1
  if (t <= keyforms[0].at)    return addOffsets(out, keyforms[0].offsets)
  if (t >= keyforms[last].at) return addOffsets(out, keyforms[last].offsets)

  // Find the bracketing pair [hi-1, hi] with keyforms[hi].at >= t.
  let hi = 1
  while (hi < last && keyforms[hi].at < t) hi++
  const a = keyforms[hi - 1]
  const b = keyforms[hi]
  const span = b.at - a.at
  const f = span <= 0 ? 0 : (t - a.at) / span

  const len = Math.min(out.length, a.offsets.length, b.offsets.length)
  for (let i = 0; i < len; i++) {
    out[i].x += a.offsets[i].x + (b.offsets[i].x - a.offsets[i].x) * f
    out[i].y += a.offsets[i].y + (b.offsets[i].y - a.offsets[i].y) * f
  }
}

/** Accumulate one offset array onto `out`, index-aligned. */
function addOffsets(out: Vec2[], offsets: Vec2[]): void {
  const len = Math.min(out.length, offsets.length)
  for (let i = 0; i < len; i++) {
    out[i].x += offsets[i].x
    out[i].y += offsets[i].y
  }
}
