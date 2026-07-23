/**
 * Tests for the scene-graph model (src/puppet/part.ts). Pure — no WebGL.
 */
import { describe, it, expect } from 'vitest'
import { deformPuppet, partsBoundTo, type PartGeom } from './part'
import { indexDefs, type ParameterDef } from './parameters'
import type { Mesh } from './mesh'

const defs: ParameterDef[] = [{ id: 'P', min: 0, max: 1, default: 0 }]
const index = indexDefs(defs)

const meshAt = (x: number, y: number): Mesh => ({ vertices: [{ x, y }], uvs: [{ x: 0, y: 0 }], indices: [] })

// Part A shifts by +10x at P=1; part B has no bindings.
const partA: PartGeom = {
  id: 'A', mesh: meshAt(0, 0),
  bindings: [{ paramId: 'P', keyforms: [{ at: 0, offsets: [{ x: 0, y: 0 }] }, { at: 1, offsets: [{ x: 10, y: 0 }] }] }],
}
const partB: PartGeom = { id: 'B', mesh: meshAt(5, 5), bindings: [] }

describe('deformPuppet()', () => {
  it('returns one vertex array per part, in draw order', () => {
    const out = deformPuppet([partA, partB], index, new Map([['P', 0]]))
    expect(out).toHaveLength(2)
    expect(out[0]).toEqual([{ x: 0, y: 0 }])
    expect(out[1]).toEqual([{ x: 5, y: 5 }])
  })

  it('deforms each layer INDEPENDENTLY — a param in one part cannot move another', () => {
    const out = deformPuppet([partA, partB], index, new Map([['P', 1]]))
    expect(out[0]).toEqual([{ x: 10, y: 0 }])   // A moved
    expect(out[1]).toEqual([{ x: 5, y: 5 }])    // B untouched
  })
})

describe('partsBoundTo()', () => {
  it('lists only parts whose bindings reference the parameter', () => {
    expect(partsBoundTo([partA, partB], 'P')).toEqual(['A'])
    expect(partsBoundTo([partA, partB], 'Q')).toEqual([])
  })
})
