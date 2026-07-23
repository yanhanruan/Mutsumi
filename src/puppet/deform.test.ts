/**
 * Tests for parameter-driven mesh deformation (src/puppet/deform.ts).
 *
 * A tiny 2-vertex "mesh" is enough to prove the interpolation and additive
 * composition math. Pure — no WebGL.
 */
import { describe, it, expect } from 'vitest'
import { deform, type ParamBinding, type Vec2 } from './deform'
import { indexDefs, type ParameterDef } from './parameters'

const eyeOpen: ParameterDef = { id: 'ParamEyeLOpen', min: 0, max: 1,   default: 1 }
const angleX:  ParameterDef = { id: 'ParamAngleX',  min: -30, max: 30, default: 0 }
const defs = indexDefs([eyeOpen, angleX])

const base: Vec2[] = [{ x: 0, y: 0 }, { x: 10, y: 0 }]

// Eyelid: fully open (t=1) = no offset; fully shut (t=0) = drop vertex 0 by 10.
const eyeBinding: ParamBinding = {
  paramId:  'ParamEyeLOpen',
  keyforms: [
    { at: 0, offsets: [{ x: 0, y: -10 }, { x: 0, y: 0 }] },
    { at: 1, offsets: [{ x: 0, y: 0   }, { x: 0, y: 0 }] },
  ],
}

const vals = (m: Record<string, number>) => new Map(Object.entries(m))

describe('deform()', () => {
  it('returns a fresh array and never mutates the base', () => {
    const out = deform(base, [], defs, vals({}))
    expect(out).not.toBe(base)
    expect(out[0]).not.toBe(base[0])
    expect(out).toEqual(base)
    expect(base).toEqual([{ x: 0, y: 0 }, { x: 10, y: 0 }])
  })

  it('applies no deformation at a keyform whose offsets are zero (eye open)', () => {
    const out = deform(base, [eyeBinding], defs, vals({ ParamEyeLOpen: 1 }))
    expect(out).toEqual(base)
  })

  it('applies the full offset at the opposite keyform (eye shut)', () => {
    const out = deform(base, [eyeBinding], defs, vals({ ParamEyeLOpen: 0 }))
    expect(out[0]).toEqual({ x: 0, y: -10 })
    expect(out[1]).toEqual({ x: 10, y: 0 })
  })

  it('linearly interpolates between surrounding keyforms', () => {
    const out = deform(base, [eyeBinding], defs, vals({ ParamEyeLOpen: 0.5 }))
    expect(out[0]).toEqual({ x: 0, y: -5 })
  })

  it('holds the endpoint below the first / above the last keyform (no extrapolation)', () => {
    // ParamEyeLOpen clamps to [0,1]; there is no way to overshoot, so verify the
    // hold with a binding whose keyforms do not span the full normalized range.
    const partial: ParamBinding = {
      paramId:  'ParamAngleX',
      keyforms: [
        { at: 0.25, offsets: [{ x: -4, y: 0 }, { x: 0, y: 0 }] },
        { at: 0.75, offsets: [{ x: 4,  y: 0 }, { x: 0, y: 0 }] },
      ],
    }
    // t=0 (angleX=-30) is below the first keyform → hold at:0.25's offset.
    expect(deform(base, [partial], defs, vals({ ParamAngleX: -30 }))[0]).toEqual({ x: -4, y: 0 })
    // t=1 (angleX=30) is above the last keyform → hold at:0.75's offset.
    expect(deform(base, [partial], defs, vals({ ParamAngleX: 30 }))[0]).toEqual({ x: 4, y: 0 })
  })

  it('composes multiple bindings additively', () => {
    const angleBinding: ParamBinding = {
      paramId:  'ParamAngleX',
      keyforms: [
        { at: 0,   offsets: [{ x: -6, y: 0 }, { x: 0, y: 0 }] },
        { at: 0.5, offsets: [{ x: 0,  y: 0 }, { x: 0, y: 0 }] },
        { at: 1,   offsets: [{ x: 6,  y: 0 }, { x: 0, y: 0 }] },
      ],
    }
    // Eye half-shut (y −5) AND head turned fully right (x +6) on the same vertex.
    const out = deform(base, [eyeBinding, angleBinding], defs, vals({ ParamEyeLOpen: 0.5, ParamAngleX: 30 }))
    expect(out[0]).toEqual({ x: 6, y: -5 })
  })

  it('uses the parameter rest value when a value is missing', () => {
    // ParamEyeLOpen default = 1 (open) → no offset.
    const out = deform(base, [eyeBinding], defs, vals({}))
    expect(out).toEqual(base)
  })

  it('skips bindings with an unknown parameter or no keyforms', () => {
    const unknown: ParamBinding = { paramId: 'nope', keyforms: [{ at: 0, offsets: [{ x: 99, y: 99 }] }] }
    const empty:   ParamBinding = { paramId: 'ParamAngleX', keyforms: [] }
    expect(deform(base, [unknown, empty], defs, vals({}))).toEqual(base)
  })

  it('collapses a NaN parameter value to the rest value (no NaN reaches the mesh)', () => {
    const out = deform(base, [eyeBinding], defs, vals({ ParamEyeLOpen: NaN }))
    expect(out).toEqual(base)   // NaN → default 1 → eye open → no offset
    expect(out.every(v => Number.isFinite(v.x) && Number.isFinite(v.y))).toBe(true)
  })

  it('tolerates a keyform with fewer offsets than the mesh (partial rig)', () => {
    const shortBinding: ParamBinding = {
      paramId:  'ParamAngleX',
      keyforms: [
        { at: 0, offsets: [{ x: 5, y: 0 }] },   // only vertex 0
        { at: 1, offsets: [{ x: 5, y: 0 }] },
      ],
    }
    const out = deform(base, [shortBinding], defs, vals({ ParamAngleX: 30 }))
    expect(out[0]).toEqual({ x: 5, y: 0 })
    expect(out[1]).toEqual({ x: 10, y: 0 })     // untouched, no crash
  })
})
