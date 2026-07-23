/**
 * Tests for the deformable mesh geometry (src/puppet/mesh.ts). Pure — no WebGL.
 */
import { describe, it, expect } from 'vitest'
import { buildGridMesh, buildSubMesh, flattenVec2 } from './mesh'

describe('buildSubMesh()', () => {
  it('centers the grid at (cx,cy) in model space', () => {
    const m = buildSubMesh(1, 2, 2, 2, 1, 1)   // 1×1 quad, 2×2 around (1,2)
    expect(m.vertices[0]).toEqual({ x: 0, y: 3 })                 // top-left: cx-1, cy+1
    expect(m.vertices[m.vertices.length - 1]).toEqual({ x: 2, y: 1 })  // bottom-right: cx+1, cy-1
  })
  it('lays uvs [0,1] independent of position', () => {
    const m = buildSubMesh(9, 9, 0.5, 0.5, 1, 1)
    expect(m.uvs[0]).toEqual({ x: 0, y: 0 })
    expect(m.uvs[m.uvs.length - 1]).toEqual({ x: 1, y: 1 })
  })
})

describe('buildGridMesh()', () => {
  it('produces (cols+1)·(rows+1) vertices and matching uvs', () => {
    const m = buildGridMesh(2, 3)
    expect(m.vertices).toHaveLength(3 * 4)
    expect(m.uvs).toHaveLength(3 * 4)
  })

  it('emits cols·rows·2 triangles (·3 indices)', () => {
    const m = buildGridMesh(4, 5)
    expect(m.indices).toHaveLength(4 * 5 * 6)
  })

  it('keeps every index within the vertex range', () => {
    const m = buildGridMesh(3, 3)
    const max = m.vertices.length - 1
    expect(Math.min(...m.indices)).toBe(0)
    expect(Math.max(...m.indices)).toBeLessThanOrEqual(max)
  })

  it('spans width×height centered on the origin, y-up', () => {
    const m = buildGridMesh(2, 2, 2, 2)
    // First vertex is top-left corner: u=0 → x=-1, v=0 → y=+1.
    expect(m.vertices[0]).toEqual({ x: -1, y: 1 })
    // Last vertex is bottom-right: u=1 → x=+1, v=1 → y=-1.
    expect(m.vertices[m.vertices.length - 1]).toEqual({ x: 1, y: -1 })
  })

  it('lays uvs [0,1] with v=0 at the top', () => {
    const m = buildGridMesh(1, 1)
    expect(m.uvs[0]).toEqual({ x: 0, y: 0 })                 // top-left
    expect(m.uvs[m.uvs.length - 1]).toEqual({ x: 1, y: 1 })  // bottom-right
  })

  it('rejects a degenerate grid', () => {
    expect(() => buildGridMesh(0, 3)).toThrow(RangeError)
    expect(() => buildGridMesh(3, 0)).toThrow(RangeError)
  })
})

describe('flattenVec2()', () => {
  it('interleaves x,y into a Float32Array', () => {
    const out = flattenVec2([{ x: 1, y: 2 }, { x: 3, y: 4 }])
    expect(out).toBeInstanceOf(Float32Array)
    expect(Array.from(out)).toEqual([1, 2, 3, 4])
  })
})
