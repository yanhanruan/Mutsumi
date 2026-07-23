/**
 * part.ts — the scene-graph model: a Puppet is an ordered list of Parts, each a
 * SEPARATE textured mesh (its own layer). This is the fix for "the eye can't
 * fully close and drags the eyebrows": on a single shared mesh a parameter
 * inevitably tugs neighbouring regions, and stretching a flat eye texture can
 * never become skin-over-eye. With independent parts, `ParamEyeOpen` addresses
 * ONLY the eye + eyelid parts — the face/brow part is never touched, and an
 * opaque eyelid part slides down to fully occlude the eye.
 *
 * Draw order is array order (painter's algorithm): parts[0] is the backmost
 * layer, the last part is frontmost. Deformation is per-part and pure; the GPU
 * step lives in webgl/puppetRenderer.ts.
 */
import type { ParameterDef } from './parameters'
import type { ParamBinding, Vec2 } from './deform'
import { deform } from './deform'
import type { Mesh } from './mesh'

/** Geometry + deformation for one layer, without its texture (kept pure/testable). */
export interface PartGeom {
  id:       string
  mesh:     Mesh
  bindings: ParamBinding[]
}

/** A drawable layer: geometry + its own texture image. */
export interface Part extends PartGeom {
  image: TexImageSource
}

export interface Puppet {
  /** Draw order: index 0 = backmost, last = frontmost. */
  parts: Part[]
  defs:  ParameterDef[]
}

/**
 * Deform every part independently at the current parameter values, returning one
 * vertex array per part IN DRAW ORDER. Each part only sees its own bindings, so
 * a parameter bound in one part can never move another — layer independence is
 * structural, not a tuning trick.
 */
export function deformPuppet(
  parts:  readonly PartGeom[],
  defs:   Map<string, ParameterDef>,
  values: Map<string, number>,
): Vec2[][] {
  return parts.map(p => deform(p.mesh.vertices, p.bindings, defs, values))
}

/** True if any part's bindings reference `paramId` (used by tests/tools). */
export function partsBoundTo(parts: readonly PartGeom[], paramId: string): string[] {
  return parts.filter(p => p.bindings.some(b => b.paramId === paramId)).map(p => p.id)
}
