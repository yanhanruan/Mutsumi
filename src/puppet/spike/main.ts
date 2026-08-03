/**
 * main.ts — entry for the P3 puppet spike (puppet-spike.html). Dev-only: wires
 * the multi-PART runtime end to end so a human can eyeball it —
 *   procedural per-layer textures + per-part grid meshes + parameter bindings
 *     → per-part deform() → layered WebGL2 draw on a transparent canvas.
 *
 * The eyes + eyelids are their own layers, so a blink closes them fully without
 * touching the brows. Automatic breathing + random blink; live sliders for
 * head/mouth/eye and eye-shape tuning; a "Real Mutsumi" toggle that swaps the
 * placeholder for her actual idle frame; background toggle; a "simulate GPU
 * loss" button. NOT part of the production app or its build.
 */
import {
  buildFacePartsGeometry, buildRealFaceGeometry, buildLayeredFaceGeometry,
  drawFaceBase, drawEye, drawLid, FACE_TUNING_DEFAULTS, FACE_SIZE,
  type LidShape, type EyeSpan,
} from './faceRig'
import { createPuppetRenderer, type RenderPart } from '../webgl/puppetRenderer'
import { deformPuppet, type Part, type PartGeom } from '../part'
import { defaultValues, indexDefs } from '../parameters'

function el<T extends HTMLElement>(id: string): T {
  const node = document.getElementById(id)
  if (!node) throw new Error(`#${id} not found`)
  return node as T
}

const canvas = el<HTMLCanvasElement>('stage')
const renderer = createPuppetRenderer(canvas)

// Per-layer textures (drawn once; geometry is rebuilt live as tuning changes).
const images = { face: drawFaceBase(512), eye: drawEye(256), lid: drawLid(256) }
const imageFor = (id: string): HTMLCanvasElement =>
  id.startsWith('lid') ? images.lid : id.startsWith('eye') ? images.eye : images.face

let geom = buildFacePartsGeometry()
const defsIndex = indexDefs(geom.defs)
const values = defaultValues(geom.defs)
let parts: Part[] = []

// Real-art layers. Mutsumi is authored as many aligned PSD layers (see
// docs/puppet-eye-layers.md). At load we composite them into three deforming
// groups — back (through the open eyes) → lid (skin curtain + moving lash) →
// front (eyebrows, bangs) — and rig the lid to shut for a real blink. The lid is
// SYNTHESISED: an opaque skin patch (sampled from her face) under her lash, so
// the source needs no dedicated eyelid layer. If the layer set is absent, fall
// back to the single baked idle frame (breathing only).
const MUTSUMI_DIR  = '/assets/mutsumi_layers'
const BACK_LAYERS  = ['back_hair', 'leg_left', 'leg_right', 'bottomwear', 'topwear',
                      'hand_left', 'hand_right', 'face', 'ears', 'nose', 'mouth', 'irides']
const FRONT_LAYERS = ['eyebrow', 'hair_front']
// The eyelash is split: only the MAIN lash rides the moving lid; the OUTER
// CORNER is a fixed anchor (the eye's tail stays put as the lid sweeps down).
// Both are optional splits — fall back to the un-split `eyelash` if absent.
const LASH_MOVING  = 'eyelash_original'
const LASH_ANCHOR  = 'eyelash_outer_corner'
const FRAME_SRC    = '/assets/idle/frame_001.webp'

let realImages: Record<string, TexImageSource> | null = null
let realParts: PartGeom[] = []
let realMode = false
let realIsLayered = false

function rebuildPuppet() {
  const useReal = realMode && realImages !== null
  const source: PartGeom[] = useReal ? realParts : geom.parts
  parts = source.map(p => ({ ...p, image: useReal ? realImages![p.id] : imageFor(p.id) }))
  renderer.setPuppet(parts as RenderPart[])
}
rebuildPuppet()

function decode(src: string): Promise<HTMLImageElement> {
  const im = new Image()
  im.decoding = 'async'
  im.src = src
  return im.decode().then(() => im)
}

/** Decode, or resolve null if the asset is missing (for optional split layers). */
const tryDecode = (src: string): Promise<HTMLImageElement | null> =>
  decode(src).then(im => im, () => null)

function makeCanvas(w: number, h: number): [HTMLCanvasElement, CanvasRenderingContext2D] {
  const cv = document.createElement('canvas')
  cv.width = w; cv.height = h
  const g = cv.getContext('2d', { willReadFrequently: true })
  if (!g) throw new Error('2D context unavailable')
  return [cv, g]
}

const imageCtx = (im: HTMLImageElement): CanvasRenderingContext2D => {
  const [, g] = makeCanvas(im.naturalWidth, im.naturalHeight)
  g.drawImage(im, 0, 0)
  return g
}

/** Columns (x) that hold any alpha across the union of the given images. */
function presentColumns(imgs: HTMLImageElement[], W: number, H: number): boolean[] {
  const present = new Array<boolean>(W).fill(false)
  for (const im of imgs) {
    const { data } = imageCtx(im).getImageData(0, 0, W, H)
    for (let x = 0; x < W; x++)
      for (let y = 0; y < H; y++)
        if (data[(y * W + x) * 4 + 3] > 10) { present[x] = true; break }
  }
  return present
}

/** Alpha bbox of pre-fetched pixels restricted to columns [xlo, xhi). */
function bboxInColumns(data: Uint8ClampedArray, W: number, H: number, xlo: number, xhi: number) {
  let x0 = W, y0 = H, x1 = -1, y1 = -1
  for (let y = 0; y < H; y++)
    for (let x = xlo; x < xhi; x++)
      if (data[(y * W + x) * 4 + 3] > 10) {
        if (x < x0) x0 = x; if (x > x1) x1 = x
        if (y < y0) y0 = y; if (y > y1) y1 = y
      }
  return { x0, y0, x1, y1 }
}

/** Median of bright skin pixels in a small patch just above an eye (avoids the
 *  lash/outline), used to tint that eye's curtain to its LOCAL skin tone. */
function localSkin(faceData: Uint8ClampedArray, W: number, cx: number, eyeTop: number): [number, number, number] {
  const rs: number[] = [], gs: number[] = [], bs: number[] = []
  for (let y = eyeTop - 22; y < eyeTop - 8; y++)
    for (let x = cx - 9; x <= cx + 9; x++) {
      const i = (y * W + x) * 4
      if (faceData[i] > 150 && faceData[i + 3] > 200) { rs.push(faceData[i]); gs.push(faceData[i + 1]); bs.push(faceData[i + 2]) }
    }
  const med = (a: number[], d: number): number => (a.length ? a.sort((p, q) => p - q)[a.length >> 1] : d)
  return [med(rs, 245), med(gs, 227), med(bs, 218)]
}

/** One eye's pixel geometry + the curtain rectangle/skin used to synthesise it. */
interface EyeGeom { span: EyeSpan; curtain: { x0: number; x1: number; top: number; bot: number; skin: [number, number, number] } }

/**
 * Per-eye geometry from the real art. The irides layer is split into two eyes at
 * the empty nose-bridge column (robust — no width-sorting that a stray speck
 * could fool); each eye's horizontal extent comes from the union of iris + lash
 * + corner on that side, and its vertical band from that side's iris box. The
 * curtain is tinted with the eye's LOCAL skin so a shaded brow doesn't patch.
 */
function computeEyes(
  imgs: { irides: HTMLImageElement; face: HTMLImageElement }, lashMoving: HTMLImageElement,
  lashAnchor: HTMLImageElement | null, W: number, H: number,
): EyeGeom[] {
  const irisData = imageCtx(imgs.irides).getImageData(0, 0, W, H).data
  const faceData = imageCtx(imgs.face).getImageData(0, 0, W, H).data
  const irisCols = presentColumns([imgs.irides], W, H)
  const xs = irisCols.map((p, i) => (p ? i : -1)).filter(i => i >= 0)
  const lo = xs[0], hi = xs[xs.length - 1]
  let gapS = -1, gapE = -1
  for (let x = lo; x <= hi; x++) if (!irisCols[x]) { if (gapS < 0) gapS = x; gapE = x }
  const bridge = gapS >= 0 ? (gapS + gapE) >> 1 : (lo + hi) >> 1

  const unionCols = presentColumns([imgs.irides, lashMoving, ...(lashAnchor ? [lashAnchor] : [])], W, H)
  const side = (xlo: number, xhi: number): EyeGeom => {
    const iris = bboxInColumns(irisData, W, H, xlo, xhi)
    let ex0 = xhi, ex1 = xlo
    for (let x = xlo; x < xhi; x++) if (unionCols[x]) { if (x < ex0) ex0 = x; if (x > ex1) ex1 = x }
    const cx = (ex0 + ex1) / 2
    return {
      span: {
        cx: cx / W,
        half: Math.max(1, (ex1 - ex0) / 2) / W,
        pinV: (iris.y0 - 38) / H,
        lashV: iris.y0 / H,
        travel: ((iris.y1 - iris.y0) / H) * FACE_SIZE,
      },
      curtain: { x0: ex0 - 2, x1: ex1 + 2, top: iris.y0 - 38, bot: iris.y0 + 3, skin: localSkin(faceData, W, Math.round(cx), iris.y0) },
    }
  }
  return [side(0, bridge), side(bridge, W)]
}

function compositeLayers(imgs: HTMLImageElement[], w: number, h: number): HTMLCanvasElement {
  const [cv, g] = makeCanvas(w, h)
  for (const im of imgs) g.drawImage(im, 0, 0)
  return cv
}

async function loadMutsumiLayers(): Promise<{
  back: HTMLCanvasElement; lidSkin: HTMLCanvasElement; lidLash: HTMLCanvasElement
  front: HTMLCanvasElement; aspect: number; lidShape: LidShape
}> {
  const names = [...new Set([...BACK_LAYERS, ...FRONT_LAYERS, 'irides', 'face'])]
  const imgs: Record<string, HTMLImageElement> = {}
  await Promise.all(names.map(n => decode(`${MUTSUMI_DIR}/${n}.png`).then(im => { imgs[n] = im })))
  const W = imgs.face.naturalWidth, H = imgs.face.naturalHeight

  // The main lash rides the lid; prefer the split-out original, else the un-split
  // eyelash. The outer corner is a fixed pivot buried in BACK (optional).
  const lashMoving = await tryDecode(`${MUTSUMI_DIR}/${LASH_MOVING}.png`)
    ?? await decode(`${MUTSUMI_DIR}/eyelash.png`)
  const lashAnchor = await tryDecode(`${MUTSUMI_DIR}/${LASH_ANCHOR}.png`)

  // The outer-corner lash is the eye's fixed pivot. It rides in BACK (over the
  // irides, under the lid), so when the lid shuts the descending skin BURIES it —
  // rather than leaving it floating on top of the closed eye.
  const back  = compositeLayers(
    [...BACK_LAYERS.map(n => imgs[n]), ...(lashAnchor ? [lashAnchor] : [])], W, H)
  const front = compositeLayers(FRONT_LAYERS.map(n => imgs[n]), W, H)

  // Per-eye geometry from the real art (see computeEyes).
  const eyeGeoms = computeEyes({ irides: imgs.irides, face: imgs.face }, lashMoving, lashAnchor, W, H)

  // lidSkin: an opaque skin curtain per eye, each tinted to that eye's LOCAL skin
  // so a shaded brow doesn't leave a mismatched patch. The rects are then FEATHERED
  // (soft-blurred edges) so the curtain blends into the surrounding skin instead of
  // reading as a hard-edged block. lidLash: her moving lash, deformed to the
  // designed closed curve on top of the skin.
  const [rects, rg] = makeCanvas(W, H)
  for (const { curtain: c } of eyeGeoms) {
    rg.fillStyle = `rgb(${c.skin[0]},${c.skin[1]},${c.skin[2]})`
    rg.fillRect(c.x0, c.top, c.x1 - c.x0, c.bot - c.top)
  }
  const [lidSkin, sg] = makeCanvas(W, H)
  sg.filter = 'blur(3px)'
  sg.drawImage(rects, 0, 0)
  const lidLash = compositeLayers([lashMoving], W, H)

  const lidShape: LidShape = { eyes: eyeGeoms.map(e => e.span) }
  return { back, lidSkin, lidLash, front, aspect: W / H, lidShape }
}

// Prefer the layered blink; fall back to the single baked frame if absent.
loadMutsumiLayers()
  .then(({ back, lidSkin, lidLash, front, aspect, lidShape }) => {
    realImages = { back, lidSkin, lidLash, front }
    realParts = buildLayeredFaceGeometry(aspect, lidShape).parts
    realIsLayered = true
    if (realMode) rebuildPuppet()
  })
  .catch(() =>
    decode(FRAME_SRC)
      .then(im => {
        realImages = { face: im }
        realParts = buildRealFaceGeometry(im.naturalWidth / im.naturalHeight).parts
        realIsLayered = false
        if (realMode) rebuildPuppet()
      })
      .catch((e: unknown) => console.error('real art failed to load', e)),
  )

// ── controls ────────────────────────────────────────────────────────
const angleX = el<HTMLInputElement>('angleX')
const angleY = el<HTMLInputElement>('angleY')
const mouth  = el<HTMLInputElement>('mouth')
const eye    = el<HTMLInputElement>('eye')
const autoBlink = el<HTMLInputElement>('autoBlink')
const openLift  = el<HTMLInputElement>('openLift')    // eye-open height (×1000)
const eyeSquash = el<HTMLInputElement>('eyeSquash')   // eye narrowing on close (%)
const blinkSpd  = el<HTMLInputElement>('blinkSpd')    // blink speed (%)
const fpsOut = el<HTMLSpanElement>('fps')
const statusOut = el<HTMLSpanElement>('status')

openLift.value  = String(Math.round(FACE_TUNING_DEFAULTS.openLift * 1000))
eyeSquash.value = String(Math.round(FACE_TUNING_DEFAULTS.eyeSquash * 100))

function rebuildGeom() {
  geom = buildFacePartsGeometry({
    openLift:  openLift.valueAsNumber / 1000,
    eyeSquash: eyeSquash.valueAsNumber / 100,
  })
  rebuildPuppet()
}
openLift.addEventListener('input', rebuildGeom)
eyeSquash.addEventListener('input', rebuildGeom)

const realArt = el<HTMLInputElement>('realArt')
realArt.addEventListener('change', () => { realMode = realArt.checked; rebuildPuppet() })

el<HTMLButtonElement>('blinkNow').addEventListener('click', () => startBlink(performance.now()))
el<HTMLButtonElement>('loseCtx').addEventListener('click', () => {
  const ext = (canvas.getContext('webgl2') as WebGL2RenderingContext | null)?.getExtension('WEBGL_lose_context')
  if (ext) { ext.loseContext(); setTimeout(() => ext.restoreContext(), 900) }
})
for (const bg of ['checker', 'dark', 'light'] as const) {
  el<HTMLButtonElement>(`bg-${bg}`).addEventListener('click', () => { document.body.className = `bg-${bg}` })
}

// ── blink state machine ─────────────────────────────────────────────
// Human blink kinematics: a fast, ballistic downstroke (~75ms) and a markedly
// slower, smoother reopen (~180ms), with a brief hold shut. Close uses ease-OUT
// (snaps then settles onto the lid); open uses smoothstep (slow, gentle lift).
const BASE_CLOSE = 75, BASE_HOLD = 30, BASE_OPEN = 180
let blinkStart = -Infinity
let nextBlinkAt = 0
function startBlink(now: number) { blinkStart = now }
function blinkValue(now: number): number {
  const scale = 100 / Math.max(1, blinkSpd.valueAsNumber)
  const close = BASE_CLOSE * scale, hold = BASE_HOLD * scale, open = BASE_OPEN * scale
  const e = now - blinkStart
  if (e < 0 || e > close + hold + open) return 1
  if (e < close)        { const p = e / close;         return (1 - p) * (1 - p) }  // ballistic snap shut
  if (e < close + hold)   return 0
  const p = (e - close - hold) / open
  return p * p * (3 - 2 * p)                                                        // gentle reopen
}

// ── eyelid opacity (kills the open-eye crease sliver) ───────────────
// The lid is fully opaque until the eye is LID_FADE from fully open, then
// ramps to 0 exactly at open. Keyed by part id so it survives geometry rebuilds.
const LID_FADE = 0.15
const lidOpacity = (open: number): number => Math.min(Math.max((1 - open) / LID_FADE, 0), 1)
function partOpacities(openL: number, openR: number): number[] {
  return parts.map(p =>
    p.id === 'lidL' ? lidOpacity(openL) :
    p.id === 'lidR' ? lidOpacity(openR) : 1)
}

// ── render loop ─────────────────────────────────────────────────────
let raf = 0
let last = performance.now()
let fpsEma = 60

function frame(now: number) { raf = requestAnimationFrame(frame); renderOnce(now) }

function renderOnce(now: number) {
  const dt = now - last
  last = now
  if (dt > 0) fpsEma += ((1000 / dt) - fpsEma) * 0.1

  values.set('ParamBreath', 0.5 + 0.5 * Math.sin(now * 0.0016))

  if (autoBlink.checked) {
    if (now >= nextBlinkAt) { startBlink(now); nextBlinkAt = now + 2000 + Math.random() * 4000 }
    const b = blinkValue(now)
    values.set('ParamEyeLOpen', b)
    values.set('ParamEyeROpen', b)
  } else {
    const e = eye.valueAsNumber / 100
    values.set('ParamEyeLOpen', e)
    values.set('ParamEyeROpen', e)
  }

  // No ambient sway: her idle art has none, and an ambient horizontal slide on a
  // full-body layer bends the torso (the "PVZ sunflower" wobble) — structural,
  // not tunable, until the head is its own part rotating about a neck. Breath
  // alone carries the idle life; head params stay slider-driven for testing.
  values.set('ParamAngleX', angleX.valueAsNumber)
  values.set('ParamAngleY', angleY.valueAsNumber)
  values.set('ParamMouthOpenY', mouth.valueAsNumber / 100)

  // Fade each eyelid out over the last sliver of opening. When open, the lid
  // collapses to a line at the eye top; breath perturbs that line just enough
  // for its lash texel to show as a faint crease. Fading it to zero as
  // the eye nears fully open removes that artifact, while the lid stays fully
  // opaque through the whole close (open < 1 - LID_FADE), so a blink still
  // reads as solid skin over the eye.
  const opacities = partOpacities(
    values.get('ParamEyeLOpen') ?? 1,
    values.get('ParamEyeROpen') ?? 1,
  )
  renderer.draw(deformPuppet(parts, defsIndex, values), opacities)

  fpsOut.textContent = fpsEma.toFixed(0)
  statusOut.textContent = renderer.lost ? 'context lost — restoring…'
    : realMode ? (realIsLayered ? 'layered blink' : 'baked frame (no layers found)')
    : 'ok'
}

// ── sizing + lifecycle ──────────────────────────────────────────────
function fit() {
  const rect = canvas.getBoundingClientRect()
  renderer.resize(rect.width, rect.height, window.devicePixelRatio || 1)
}
const ro = new ResizeObserver(fit)
ro.observe(canvas)
fit()
renderOnce(performance.now())
raf = requestAnimationFrame(frame)

function teardown() {
  cancelAnimationFrame(raf)
  ro.disconnect()
  renderer.destroy()
}
window.addEventListener('pagehide', teardown)
