/**
 * parameters.ts — the parameter model at the heart of the from-scratch puppet
 * runtime: our own open, no-SDK take on Live2D-style character deformation.
 *
 * A "parameter" is one named scalar with a range and a rest value — ParamAngleX
 * (head turn), ParamEyeLOpen (left eyelid), ParamMouthOpenY (lip sync),
 * breathing, and so on. EVERYTHING the character does is one or more parameters
 * moving; deformers (see deform.ts) translate those moves into mesh vertex
 * offsets. This is exactly how Live2D and Inochi2D both model a rig — minus any
 * proprietary format or SDK.
 *
 * Pure: no DOM, no WebGL, no timers. Just the value math, so it unit-tests
 * deterministically (same split as config/animations.ts vs useAnimator.ts).
 */

export interface ParameterDef {
  id:  string
  min: number
  max: number
  /** Rest value the rig settles to; expected to lie within [min, max]. */
  default: number
}

/**
 * Clamp a raw value into [min, max]. NaN (a common result of bad audio/gaze
 * input) collapses to the rest value rather than poisoning the mesh — the
 * migration plan's "NaN and invalid input values are normalized" rule, enforced
 * at the parameter boundary so no downstream deformer ever sees a NaN.
 */
export function clampParam(def: ParameterDef, value: number): number {
  if (Number.isNaN(value)) return def.default
  return value < def.min ? def.min : value > def.max ? def.max : value
}

/**
 * Normalize a value to [0, 1] across [min, max] (clamped first). Deformers index
 * their keyforms in this normalized space, so a binding is independent of the
 * parameter's real-world units and range. A zero- or negative-width range
 * collapses to 0.
 */
export function normalizeParam(def: ParameterDef, value: number): number {
  const span = def.max - def.min
  if (span <= 0) return 0
  return (clampParam(def, value) - def.min) / span
}

/** Build the initial value map with every parameter at its rest value. */
export function defaultValues(defs: readonly ParameterDef[]): Map<string, number> {
  return new Map(defs.map(d => [d.id, d.default]))
}

/** Index parameter defs by id for O(1) lookup during deformation. */
export function indexDefs(defs: readonly ParameterDef[]): Map<string, ParameterDef> {
  return new Map(defs.map(d => [d.id, d]))
}
