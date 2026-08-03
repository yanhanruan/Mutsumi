/**
 * faceRig.ts — PROCEDURAL multi-part test rig for the P3 spike. The face is now
 * split into independent layers (parts), which is what makes the eye close fully
 * and stops it dragging the brows:
 *
 *   draw order: face-base (head, hair, BROWS, blush, nose, mouth)
 *               → eyeL, eyeR   (white + iris, their own layer)
 *               → lidL, lidR   (opaque SKIN eyelids, on top)
 *
 * `ParamEyeLOpen/ROpen` are bound ONLY to the eye + eyelid parts, so the
 * face/brow part is structurally untouchable by a blink. A blink collapses the
 * eyelid layer down over the eye (opaque skin → fully closed); opening it lifts
 * the lid to a thin crease line at the top of the eye.
 *
 * Geometry + bindings are pure (buildFacePartsGeometry) and unit-tested; the
 * per-part textures are DOM canvases (drawFaceBase / drawEye / drawLid),
 * attached to the geometry in main.ts.
 */
import type { ParameterDef } from '../parameters'
import type { ParamBinding, Vec2 } from '../deform'
import type { PartGeom } from '../part'
import { buildGridMesh, buildSubMesh, type Mesh } from '../mesh'

export const FACE_SIZE = 1.8
export const FACE_COLS = 20
export const FACE_ROWS = 26

/** Landmark uv positions (v=0 at top), shared by textures and geometry. */
const EYE_L  = { x: 0.36, y: 0.44 }
const EYE_R  = { x: 0.64, y: 0.44 }
const MOUTH  = { x: 0.50, y: 0.68 }

// Model-space rects for the eye / eyelid parts (positioned at the eye centres).
const EYE_W = 0.34, EYE_H = 0.24
const LID_W = 0.40, LID_H = 0.30

/** uv (v=0 top) → model space (y-up), matching buildSubMesh's convention. */
const toModel = (u: number, v: number): Vec2 => ({ x: (u - 0.5) * FACE_SIZE, y: (0.5 - v) * FACE_SIZE })

export interface FaceTuning {
  /** Model-units the open eyelid's crease line sits above the eye top (+ = wider open). */
  openLift?:  number
  /** 0..1 — how much the eye itself narrows as it closes (under the lid). */
  eyeSquash?: number
}
export const FACE_TUNING_DEFAULTS: Required<FaceTuning> = { openLift: 0, eyeSquash: 0.5 }

// ── binding helpers ─────────────────────────────────────────────────

const zeros = (n: number): Vec2[] => Array.from({ length: n }, () => ({ x: 0, y: 0 }))
const offs  = (mesh: Mesh, fn: (uv: Vec2, base: Vec2) => Vec2): Vec2[] => mesh.vertices.map((b, i) => fn(mesh.uvs[i], b))
const mask  = (uv: Vec2, c: Vec2, r: number): number => Math.max(0, 1 - Math.hypot(uv.x - c.x, uv.y - c.y) / r)

const HALF = FACE_SIZE / 2
const SHEAR = 0.16, SLIDE = 0.05

/**
 * Head motion bindings (angleX / angleY / breath) for ANY part's mesh, computed
 * from that mesh's own vertices so every layer moves together and stays aligned.
 */
function headMotion(mesh: Mesh): ParamBinding[] {
  const n = mesh.vertices.length
  return [
    { paramId: 'ParamAngleX', keyforms: [
      { at: 0,   offsets: offs(mesh, (_u, b) => ({ x: -(SHEAR * (b.y / HALF) + SLIDE), y: 0 })) },
      { at: 0.5, offsets: zeros(n) },
      { at: 1,   offsets: offs(mesh, (_u, b) => ({ x:  (SHEAR * (b.y / HALF) + SLIDE), y: 0 })) },
    ] },
    { paramId: 'ParamAngleY', keyforms: [
      { at: 0,   offsets: offs(mesh, (_u, b) => ({ x: 0, y: -(SHEAR * (b.x / HALF) + SLIDE) })) },
      { at: 0.5, offsets: zeros(n) },
      { at: 1,   offsets: offs(mesh, (_u, b) => ({ x: 0, y:  (SHEAR * (b.x / HALF) + SLIDE) })) },
    ] },
    { paramId: 'ParamBreath', keyforms: [
      { at: 0, offsets: zeros(n) },
      { at: 1, offsets: offs(mesh, (_u, b) => ({ x: b.x * 0.03, y: b.y * 0.03 + 0.02 })) },
    ] },
  ]
}

/**
 * Build the parts geometry + parameter defs. Pure — no DOM. The eye/eyelid
 * bindings reference only ParamEye*Open, so a blink cannot reach the face part.
 */
export function buildFacePartsGeometry(tuning: FaceTuning = {}): { parts: PartGeom[]; defs: ParameterDef[] } {
  const tune = { ...FACE_TUNING_DEFAULTS, ...tuning }

  const defs: ParameterDef[] = [
    { id: 'ParamAngleX',     min: -30, max: 30, default: 0 },
    { id: 'ParamAngleY',     min: -30, max: 30, default: 0 },
    { id: 'ParamBreath',     min: 0,   max: 1,  default: 0 },
    { id: 'ParamEyeLOpen',   min: 0,   max: 1,  default: 1 },
    { id: 'ParamEyeROpen',   min: 0,   max: 1,  default: 1 },
    { id: 'ParamMouthOpenY', min: 0,   max: 1,  default: 0 },
  ]

  // ── face base (everything except the eyes) ───────────────────────────
  const faceMesh = buildGridMesh(FACE_COLS, FACE_ROWS, FACE_SIZE, FACE_SIZE)
  const mouthCy  = toModel(MOUTH.x, MOUTH.y).y
  const mouthBinding: ParamBinding = { paramId: 'ParamMouthOpenY', keyforms: [
    { at: 0, offsets: zeros(faceMesh.vertices.length) },
    { at: 1, offsets: offs(faceMesh, (uv, b) => ({ x: 0, y: (b.y - mouthCy) * 0.6 * mask(uv, MOUTH, 0.16) })) },
  ] }
  const parts: PartGeom[] = [
    { id: 'face', mesh: faceMesh, bindings: [...headMotion(faceMesh), mouthBinding] },
  ]

  // ── eye + eyelid parts, per side ─────────────────────────────────────
  const eye = (side: 'L' | 'R', centerUV: Vec2) => {
    const c   = toModel(centerUV.x, centerUV.y)
    const openId = `ParamEye${side}Open`
    const eyeMesh = buildSubMesh(c.x, c.y, EYE_W, EYE_H, 8, 8)
    const lidMesh = buildSubMesh(c.x, c.y, LID_W, LID_H, 6, 6)
    const eyeTop  = c.y + EYE_H / 2

    // Eye layer: narrows slightly toward its centre as it closes.
    const eyeSquash: ParamBinding = { paramId: openId, keyforms: [
      { at: 0, offsets: offs(eyeMesh, (_u, b) => ({ x: 0, y: (c.y - b.y) * tune.eyeSquash })) },
      { at: 1, offsets: zeros(eyeMesh.vertices.length) },
    ] }
    // Eyelid layer: open (1) collapses to a thin crease line at the eye top;
    // closed (0) rests at full height, opaque skin covering the whole eye.
    const lid: ParamBinding = { paramId: openId, keyforms: [
      { at: 0, offsets: zeros(lidMesh.vertices.length) },
      { at: 1, offsets: offs(lidMesh, (_u, b) => ({ x: 0, y: (eyeTop + tune.openLift) - b.y })) },
    ] }

    return [
      { id: `eye${side}`, mesh: eyeMesh, bindings: [...headMotion(eyeMesh), eyeSquash] },
      { id: `lid${side}`, mesh: lidMesh, bindings: [...headMotion(lidMesh), lid] },
    ] as PartGeom[]
  }

  parts.push(...eye('L', EYE_L), ...eye('R', EYE_R))
  return { parts, defs }
}

/** Order the part textures must be attached in — matches buildFacePartsGeometry. */
export type FacePartId = 'face' | 'eyeL' | 'lidL' | 'eyeR' | 'lidR'

// ── real-art single-layer face (migration increment 1) ──────────────────
// Renders an actual character frame (eyes still baked in) as ONE deforming
// layer, to prove real art loads + warps in the puppet before we cut the eyes
// and lids into their own layers. Portrait mesh sized to the image aspect.

// Dense enough (≈7 columns across each eye) that the designed closed-eye CURVE
// resolves smoothly rather than as a chunky few-vertex jump.
const REAL_COLS = 80, REAL_ROWS = 48
const REAL_SWAY = 0.08     // model-units the head slides at full ParamAngle
const REAL_BREATH = 0.02   // model-units the upper body rises at full ParamBreath

const smoothstep = (edge0: number, edge1: number, x: number): number => {
  const t = Math.min(1, Math.max(0, (x - edge0) / (edge1 - edge0)))
  return t * t * (3 - 2 * t)
}

/**
 * Head/breath motion for a full-body portrait: weighted so it's full-strength
 * over the upper body and fades to 0 by the waist — the feet never slide. No
 * shear (that splays a full-body image); just a masked slide + breath rise.
 */
function realBodyMotion(mesh: Mesh, halfH: number): ParamBinding[] {
  const n = mesh.vertices.length
  const w = (b: Vec2) => smoothstep(-halfH, 0, b.y)   // 0 at feet → 1 at mid-body & up
  return [
    { paramId: 'ParamAngleX', keyforms: [
      { at: 0,   offsets: mesh.vertices.map(b => ({ x: -REAL_SWAY * w(b), y: 0 })) },
      { at: 0.5, offsets: zeros(n) },
      { at: 1,   offsets: mesh.vertices.map(b => ({ x:  REAL_SWAY * w(b), y: 0 })) },
    ] },
    { paramId: 'ParamAngleY', keyforms: [
      { at: 0,   offsets: mesh.vertices.map(b => ({ x: 0, y: -REAL_SWAY * w(b) })) },
      { at: 0.5, offsets: zeros(n) },
      { at: 1,   offsets: mesh.vertices.map(b => ({ x: 0, y:  REAL_SWAY * w(b) })) },
    ] },
    { paramId: 'ParamBreath', keyforms: [
      { at: 0, offsets: zeros(n) },
      { at: 1, offsets: mesh.vertices.map(b => ({ x: 0, y: REAL_BREATH * w(b) })) },
    ] },
  ]
}

/**
 * One-layer geometry for a real character frame. `imgAspect` = width/height of
 * the source image; the mesh matches it so the art isn't squished. Only head
 * params are bound (eyes/mouth are still baked into the frame this increment).
 */
export function buildRealFaceGeometry(imgAspect: number): { parts: PartGeom[]; defs: ParameterDef[] } {
  const defs: ParameterDef[] = [
    { id: 'ParamAngleX', min: -30, max: 30, default: 0 },
    { id: 'ParamAngleY', min: -30, max: 30, default: 0 },
    { id: 'ParamBreath', min: 0,   max: 1,  default: 0 },
  ]
  const height = FACE_SIZE
  const width  = height * imgAspect
  const mesh   = buildGridMesh(REAL_COLS, REAL_ROWS, width, height)
  const parts: PartGeom[] = [{ id: 'face', mesh, bindings: realBodyMotion(mesh, height / 2) }]
  return { parts, defs }
}

// ── layered real-art blink rig (migration increment 2: real parametric blink) ──
// Four aligned full-canvas layers, in draw order:
//   back    — everything through the open eyes (body, hair, face-skin, irides)
//   lidSkin — an opaque SKIN curtain that occludes the iris from the top
//   lidLash — the moving eyelash, deformed to a DESIGNED closed-eye curve
//   front   — features that stay above the lid (eyebrows, front hair)
// All share one mesh + head bindings so they deform together and never drift.
//
// The two lid layers both shut on ParamEyeLOpen, but with DIFFERENT closed
// shapes — this is what makes the blink read like a real eyelid rather than a
// panel sliding:
//   · lidSkin closes with a shallow, corner-covering arc: the skin pins at the
//     top and stretches down over the iris (aperture closes from the top), with
//     a high corner floor so the eye is fully occluded across its width.
//   · lidLash closes onto a DEEP, corner-pivoting ‿ curve with lifted outer
//     tips — the centre drops most, the canthi least, so the lash CHANGES
//     CURVATURE as it descends, interpolating toward a designed closed-eye line.
// Opaque skin occludes the iris (no squash, no cross-fade, no ghosting); the
// lash is one texture warped along a curve (no second frame). See
// docs/puppet-eye-layers.md.

export type LayeredPartId = 'back' | 'lidSkin' | 'lidLash' | 'front'

/** Kept for callers/tests referring to the old single-travel default. */
export const EYELID_TRAVEL = 0.1

/** One eye's geometry in the mesh's uv space (v=0 at the top). */
export interface EyeSpan {
  cx: number     // uv-x centre
  half: number   // uv-x half-width (centre → outer corner)
  pinV: number   // uv-v where the curtain top is pinned
  lashV: number  // uv-v of the lash at rest
  travel: number // model-units the eye centre drops to reach the lower lid
}

/** Shape of the eyelid shutter. */
export interface LidShape {
  /** Per-eye geometry; each eye pivots at its own corners. */
  eyes?: EyeSpan[]
  /** Corner drop fraction for the SKIN curtain (high → covers the iris corners). */
  skinFloor?: number
  /** Corner drop fraction for the LASH (low → a deep, pronounced closed arc). */
  lashFloor?: number
  /** Fullness of the lash arc (higher → flatter centre, sharper fall to corners). */
  lashPow?: number
  /** Slight upturn of the outer lash tips (fraction of travel lifted back up). */
  tipLift?: number
}
export const LID_SHAPE_DEFAULTS: Required<LidShape> =
  { eyes: [], skinFloor: 0.74, lashFloor: 0.20, lashPow: 1.7, tipLift: 0.14 }

/**
 * Model-units a lid vertex at (u,v) drops when the eye is fully closed, for the
 * eye that owns column u (0 elsewhere). `wv` pins the skin above the eye and
 * ramps to full at/below the lash; the horizontal profile is `floor` at the
 * corners rising to full (1) at the centre, shaped by `pow`, with an optional
 * `tip` upturn near the outer corners. Skin and lash call this with different
 * floors/pow so they close to different shapes over the same mesh.
 */
function lidDrop(u: number, v: number, eyes: EyeSpan[], floor: number, pow: number, tip: number): number {
  let drop = 0
  for (const e of eyes) {
    const d = Math.abs(u - e.cx) / e.half
    if (d > 1.2) continue                                   // column belongs to another eye
    const dc = Math.min(1, d)
    let s = floor + (1 - floor) * Math.cos((Math.PI / 2) * dc) ** pow   // floor..1
    if (tip > 0 && dc > 0.7) s -= tip * (dc - 0.7) / 0.3    // lift the outer tips
    drop = Math.max(drop, smoothstep(e.pinV, e.lashV, v) * s * e.travel)
  }
  return drop
}

export function buildLayeredFaceGeometry(
  imgAspect: number,
  lid: LidShape = {},
): { parts: PartGeom[]; defs: ParameterDef[] } {
  const { eyes, skinFloor, lashFloor, lashPow, tipLift } = { ...LID_SHAPE_DEFAULTS, ...lid }
  const defs: ParameterDef[] = [
    { id: 'ParamAngleX',   min: -30, max: 30, default: 0 },
    { id: 'ParamAngleY',   min: -30, max: 30, default: 0 },
    { id: 'ParamBreath',   min: 0,   max: 1,  default: 0 },
    { id: 'ParamEyeLOpen', min: 0,   max: 1,  default: 1 },
  ]
  const height   = FACE_SIZE
  const width    = height * imgAspect
  const mesh     = buildGridMesh(REAL_COLS, REAL_ROWS, width, height)
  const body     = realBodyMotion(mesh, height / 2)

  // A ParamEyeLOpen binding whose closed (0) keyform drops each vertex by
  // lidDrop(...) with the given profile, and open (1) is rest. Composes with
  // breath, so the lid still breathes while it blinks.
  const shutter = (floor: number, pow: number, tip: number): ParamBinding => ({
    paramId: 'ParamEyeLOpen',
    keyforms: [
      { at: 0, offsets: offs(mesh, uv => ({ x: 0, y: -lidDrop(uv.x, uv.y, eyes, floor, pow, tip) })) },
      { at: 1, offsets: zeros(mesh.vertices.length) },
    ],
  })

  const parts: PartGeom[] = [
    { id: 'back',    mesh, bindings: body },
    { id: 'lidSkin', mesh, bindings: [...body, shutter(skinFloor, 1.5, 0)] },
    { id: 'lidLash', mesh, bindings: [...body, shutter(lashFloor, lashPow, tipLift)] },
    { id: 'front',   mesh, bindings: body },
  ]
  return { parts, defs }
}

// ── procedural textures (DOM; not unit-tested) ──────────────────────

function ctx(size: number): [HTMLCanvasElement, CanvasRenderingContext2D] {
  const cv = document.createElement('canvas')
  cv.width = cv.height = size
  const g = cv.getContext('2d')
  if (!g) throw new Error('2D canvas context unavailable')
  return [cv, g]
}

const SKIN = '#ffe0cf'

/** Face base: head, hair, brows, blush, nose, mouth — but NO eyes (own layer). */
export function drawFaceBase(size = 512): HTMLCanvasElement {
  const [cv, g] = ctx(size)
  const px = (u: number) => u * size

  g.fillStyle = '#6b4a8f'                                                   // back hair
  g.beginPath(); g.ellipse(px(0.5), px(0.52), px(0.40), px(0.44), 0, 0, Math.PI * 2); g.fill()
  g.fillStyle = SKIN                                                        // head
  g.beginPath(); g.ellipse(px(0.5), px(0.52), px(0.33), px(0.38), 0, 0, Math.PI * 2); g.fill()
  g.fillStyle = '#7d58a6'                                                   // bangs
  g.beginPath(); g.ellipse(px(0.5), px(0.30), px(0.35), px(0.20), 0, Math.PI, Math.PI * 2); g.fill()

  g.fillStyle = 'rgba(255,150,150,0.45)'                                    // blush
  g.beginPath(); g.ellipse(px(0.31), px(0.56), px(0.055), px(0.035), 0, 0, Math.PI * 2); g.fill()
  g.beginPath(); g.ellipse(px(0.69), px(0.56), px(0.055), px(0.035), 0, 0, Math.PI * 2); g.fill()

  g.strokeStyle = '#5b3b82'; g.lineWidth = size * 0.012; g.lineCap = 'round'  // brows
  g.beginPath(); g.moveTo(px(0.29), px(0.35)); g.quadraticCurveTo(px(0.36), px(0.325), px(0.43), px(0.35)); g.stroke()
  g.beginPath(); g.moveTo(px(0.57), px(0.35)); g.quadraticCurveTo(px(0.64), px(0.325), px(0.71), px(0.35)); g.stroke()

  g.strokeStyle = 'rgba(180,120,110,0.6)'; g.lineWidth = size * 0.008        // nose
  g.beginPath(); g.moveTo(px(0.5), px(0.56)); g.lineTo(px(0.485), px(0.60)); g.stroke()
  g.fillStyle = '#c65b6b'                                                    // mouth
  g.beginPath(); g.ellipse(px(MOUTH.x), px(MOUTH.y), px(0.06), px(0.03), 0, 0, Math.PI * 2); g.fill()

  return cv
}

/** One eye (white + iris + pupil + highlight), centred in its own layer. */
export function drawEye(size = 256): HTMLCanvasElement {
  const [cv, g] = ctx(size)
  const c = size / 2
  g.fillStyle = '#ffffff'
  g.beginPath(); g.ellipse(c, c, size * 0.40, size * 0.38, 0, 0, Math.PI * 2); g.fill()
  g.fillStyle = '#5b3b82'
  g.beginPath(); g.ellipse(c, c + size * 0.03, size * 0.24, size * 0.30, 0, 0, Math.PI * 2); g.fill()
  g.fillStyle = '#241033'
  g.beginPath(); g.ellipse(c, c + size * 0.04, size * 0.12, size * 0.16, 0, 0, Math.PI * 2); g.fill()
  g.fillStyle = '#ffffff'
  g.beginPath(); g.ellipse(c - size * 0.08, c - size * 0.07, size * 0.06, size * 0.06, 0, 0, Math.PI * 2); g.fill()
  return cv
}

/** Opaque skin eyelid with a soft lash line along its lower edge. */
export function drawLid(size = 256): HTMLCanvasElement {
  const [cv, g] = ctx(size)
  const c = size / 2
  g.fillStyle = SKIN
  g.beginPath(); g.ellipse(c, c, size * 0.48, size * 0.46, 0, 0, Math.PI * 2); g.fill()
  g.strokeStyle = '#7a5a52'; g.lineWidth = size * 0.02; g.lineCap = 'round'   // lash line
  g.beginPath(); g.ellipse(c, c + size * 0.02, size * 0.42, size * 0.40, 0, Math.PI * 0.15, Math.PI * 0.85); g.stroke()
  return cv
}
