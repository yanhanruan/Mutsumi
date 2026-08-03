/**
 * Tests for the procedural multi-part face rig (src/puppet/spike/faceRig.ts).
 * Geometry + bindings are pure; the textures are DOM-only and unchecked here.
 */
import { describe, it, expect } from 'vitest'
import { buildFacePartsGeometry, buildLayeredFaceGeometry } from './faceRig'
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

describe('buildLayeredFaceGeometry()', () => {
  const { parts, defs } = buildLayeredFaceGeometry(1)

  it('emits back → lidSkin → lidLash → front in draw order, sharing ONE mesh', () => {
    expect(parts.map(p => p.id)).toEqual(['back', 'lidSkin', 'lidLash', 'front'])
    // Same mesh reference → the layers deform identically and never drift.
    for (let i = 1; i < parts.length; i++) expect(parts[i].mesh).toBe(parts[0].mesh)
  })

  it('binds the blink ONLY to the two lid layers — back and front never move', () => {
    expect(partsBoundTo(parts, 'ParamEyeLOpen').sort()).toEqual(['lidLash', 'lidSkin'])
  })

  it('moves every layer together on head motion (breath/angle)', () => {
    for (const param of ['ParamAngleX', 'ParamAngleY', 'ParamBreath']) {
      expect(partsBoundTo(parts, param).sort()).toEqual(['back', 'front', 'lidLash', 'lidSkin'])
    }
  })

  // One eye centred at uv-x 0.5, half = 6 grid cols (0.075) so the corner lands on
  // a real vertex; v = 1 is below the lash (wv = 1). Skin covers with a high floor;
  // the lash uses a low floor + tip lift for a deeper arc with lifted tips.
  const half = 6 / 80, travel = 0.2
  const shaped = buildLayeredFaceGeometry(1, {
    eyes: [{ cx: 0.5, half, pinV: 0.40, lashV: 0.46, travel }],
    skinFloor: 0.74, lashFloor: 0.20, lashPow: 1.7, tipLift: 0.14,
  })
  const closedOf = (id: string) => {
    const p = shaped.parts.find(q => q.id === id)!
    const kf = p.bindings.find(b => b.paramId === 'ParamEyeLOpen')!.keyforms.find(k => k.at === 0)!
    return (u: number, v: number): number =>
      kf.offsets[p.mesh.uvs.findIndex(uv => Math.abs(uv.x - u) < 1e-6 && Math.abs(uv.y - v) < 1e-6)].y
  }

  it('shuts the skin from a pinned top: above the pin stays, centre covers full', () => {
    const skin = closedOf('lidSkin')
    expect(skin(0.5, 0.375)).toBeCloseTo(0)      // above pinV → pinned
    expect(skin(0.5, 1)).toBeCloseTo(-travel)    // eye centre below lash → full travel (covers iris)
    expect(skin(0.5 - half, 1)).toBeCloseTo(-travel * 0.74)  // corner still drops to the floor (buries it)
    expect(skin(0, 1)).toBeCloseTo(0)                        // far from the eye → no curtain, no drop
  })

  it('closes the lash onto a DEEPER arc than the skin — corners lift, centre meets the lid', () => {
    const skin = closedOf('lidSkin'), lash = closedOf('lidLash')
    expect(lash(0.5, 1)).toBeCloseTo(-travel)                      // centre meets the lower lid, like the skin
    // At the corner the lash sits much higher than the skin (deep arc), and the
    // tip lift pulls it even above its own floor.
    expect(-lash(0.5 - half, 1)).toBeLessThan(-skin(0.5 - half, 1))
    expect(-lash(0.5 - half, 1)).toBeLessThan(travel * 0.20)       // tip lift < plain floor
    // Never horizontal, never upward.
    for (const o of shaped.parts.find(p => p.id === 'lidLash')!
      .bindings.find(b => b.paramId === 'ParamEyeLOpen')!.keyforms.find(k => k.at === 0)!.offsets)
      expect(o.x === 0 && o.y <= 1e-9).toBe(true)
  })

  it('sizes the mesh to the image aspect (no squish)', () => {
    const g = buildLayeredFaceGeometry(720 / 1280)
    const xs = g.parts[0].mesh.vertices.map(v => v.x)
    const ys = g.parts[0].mesh.vertices.map(v => v.y)
    const w = Math.max(...xs) - Math.min(...xs)
    const h = Math.max(...ys) - Math.min(...ys)
    expect(w / h).toBeCloseTo(720 / 1280, 2)
  })

  it('declares each parameter once with default within range', () => {
    const ids = defs.map(d => d.id)
    expect(new Set(ids).size).toBe(ids.length)
    for (const d of defs) {
      expect(d.default).toBeGreaterThanOrEqual(d.min)
      expect(d.default).toBeLessThanOrEqual(d.max)
    }
  })
})
