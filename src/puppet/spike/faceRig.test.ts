/**
 * Tests for the procedural multi-part face rig (src/puppet/spike/faceRig.ts).
 * Geometry + bindings are pure; the textures are DOM-only and unchecked here.
 */
import { describe, it, expect } from 'vitest'
import { buildFacePartsGeometry } from './faceRig'
import { partsBoundTo } from '../part'

describe('buildFacePartsGeometry()', () => {
  const { parts, defs } = buildFacePartsGeometry()

  it('emits the layers in back-to-front draw order', () => {
    expect(parts.map(p => p.id)).toEqual(['face', 'eyeL', 'lidL', 'eyeR', 'lidR'])
  })

  it('declares each parameter once with default within range', () => {
    const ids = defs.map(d => d.id)
    expect(new Set(ids).size).toBe(ids.length)
    for (const d of defs) {
      expect(d.default).toBeGreaterThanOrEqual(d.min)
      expect(d.default).toBeLessThanOrEqual(d.max)
    }
  })

  // THE structural guarantee: a blink parameter is bound only to eye/eyelid
  // layers, so it can never move the face part (brows/hair/blush live there).
  it('binds the eye-open params ONLY to eye + eyelid layers, never the face', () => {
    expect(partsBoundTo(parts, 'ParamEyeLOpen').sort()).toEqual(['eyeL', 'lidL'])
    expect(partsBoundTo(parts, 'ParamEyeROpen').sort()).toEqual(['eyeR', 'lidR'])
    expect(partsBoundTo(parts, 'ParamEyeLOpen')).not.toContain('face')
    expect(partsBoundTo(parts, 'ParamEyeROpen')).not.toContain('face')
  })

  it('binds head motion to every layer so they move together', () => {
    for (const param of ['ParamAngleX', 'ParamAngleY', 'ParamBreath']) {
      expect(partsBoundTo(parts, param).sort()).toEqual(['eyeL', 'eyeR', 'face', 'lidL', 'lidR'])
    }
  })

  it('keeps keyforms sorted and offset arrays matched to each part mesh', () => {
    for (const p of parts) {
      for (const b of p.bindings) {
        for (let i = 1; i < b.keyforms.length; i++) {
          expect(b.keyforms[i].at).toBeGreaterThanOrEqual(b.keyforms[i - 1].at)
        }
        for (const kf of b.keyforms) expect(kf.offsets).toHaveLength(p.mesh.vertices.length)
      }
    }
  })

  it('lifts the eyelid a meaningful amount between closed and open (full close)', () => {
    const lid = parts.find(p => p.id === 'lidL')!.bindings.find(b => b.paramId === 'ParamEyeLOpen')!
    const openKf = lid.keyforms.find(k => k.at === 1)!   // open = lid retracted
    const maxLift = Math.max(...openKf.offsets.map(o => Math.abs(o.y)))
    expect(maxLift).toBeGreaterThan(0.05)
  })
})
