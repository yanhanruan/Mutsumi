/**
 * mesh.ts — the deformable mesh: base vertices, texture coords, and triangle
 * indices. A puppet part is a textured mesh whose vertices get pushed around by
 * the parameter deformers (deform.ts); this module just builds and describes the
 * geometry. Pure — no WebGL — so the grid math is unit-tested.
 *
 * Coordinate conventions:
 *   - vertices are in model space, y-up, centered on the origin (the renderer
 *     applies an aspect-fit scale to clip space).
 *   - uvs are [0,1] with v=0 at the TOP of the texture (image-space), matching
 *     how a 2D canvas is drawn; the GL renderer flips on upload.
 */
import type { Vec2 } from './deform'

export interface Mesh {
  /** Base (rest-pose) vertex positions, model space, y-up. */
  vertices: Vec2[]
  /** Texture coords [0,1], v=0 at top; index-aligned with `vertices`. */
  uvs:      Vec2[]
  /** Triangle list — indices into `vertices`, three per triangle. */
  indices:  number[]
}

/**
 * Build a `cols`×`rows` quad grid of `width`×`height` in model space, centered
 * at (`cx`,`cy`). (cols+1)·(rows+1) vertices, cols·rows·2 triangles, uvs [0,1]
 * with v=0 at top. A subdivided grid lets a flat texture deform smoothly — more
 * cells, smoother warp, at the cost of vertices. Each puppet PART is one of
 * these placed at the part's location on the character.
 */
export function buildSubMesh(
  cx: number, cy: number, width: number, height: number, cols: number, rows: number,
): Mesh {
  if (cols < 1 || rows < 1) throw new RangeError(`grid needs cols>=1 and rows>=1, got ${cols}×${rows}`)

  const vertices: Vec2[] = []
  const uvs:      Vec2[] = []
  for (let r = 0; r <= rows; r++) {
    for (let c = 0; c <= cols; c++) {
      const u = c / cols
      const v = r / rows
      vertices.push({ x: cx + (u - 0.5) * width, y: cy + (0.5 - v) * height })  // v=0 → top (+y)
      uvs.push({ x: u, y: v })
    }
  }

  const stride = cols + 1
  const indices: number[] = []
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const i = r * stride + c
      // Two triangles per cell: (i, i+1, i+stride) and (i+1, i+stride+1, i+stride).
      indices.push(i, i + 1, i + stride, i + 1, i + stride + 1, i + stride)
    }
  }

  return { vertices, uvs, indices }
}

/** A grid centered on the origin — the whole-canvas convenience case. */
export function buildGridMesh(cols: number, rows: number, width = 1.8, height = 1.8): Mesh {
  return buildSubMesh(0, 0, width, height, cols, rows)
}

/** Flatten a Vec2 list to an interleaved [x0,y0,x1,y1,…] Float32Array for the GPU. */
export function flattenVec2(list: readonly Vec2[]): Float32Array {
  const out = new Float32Array(list.length * 2)
  for (let i = 0; i < list.length; i++) {
    out[i * 2]     = list[i].x
    out[i * 2 + 1] = list[i].y
  }
  return out
}
