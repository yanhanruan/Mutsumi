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
  buildFacePartsGeometry, buildRealFaceGeometry,
  drawFaceBase, drawEye, drawLid, FACE_TUNING_DEFAULTS,
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

// Real-art layer (loaded async). When decoded AND enabled, the puppet becomes a
// single deforming layer showing Mutsumi's actual idle frame (eyes still baked
// in) instead of the procedural placeholder. Its bindings only use head params,
// which the procedural defs already include, so defsIndex/values are unchanged.
let realImg: HTMLImageElement | null = null
let realParts: PartGeom[] = []
let realMode = false

function rebuildPuppet() {
  const useReal = realMode && realImg !== null
  const source: PartGeom[] = useReal ? realParts : geom.parts
  parts = source.map(p => ({ ...p, image: useReal ? realImg! : imageFor(p.id) }))
  renderer.setPuppet(parts as RenderPart[])
}
rebuildPuppet()

// Decode the real frame off the main thread, then swap it in if real mode is on.
{
  const im = new Image()
  im.decoding = 'async'
  im.src = '/assets/idle/frame_001.webp'
  im.decode()
    .then(() => {
      realImg = im
      realParts = buildRealFaceGeometry(im.naturalWidth / im.naturalHeight).parts
      if (realMode) rebuildPuppet()
    })
    .catch((e: unknown) => console.error('real frame failed to load', e))
}

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
const BASE_CLOSE = 70, BASE_HOLD = 40, BASE_OPEN = 130
let blinkStart = -Infinity
let nextBlinkAt = 0
function startBlink(now: number) { blinkStart = now }
function blinkValue(now: number): number {
  const scale = 100 / Math.max(1, blinkSpd.valueAsNumber)
  const close = BASE_CLOSE * scale, hold = BASE_HOLD * scale, open = BASE_OPEN * scale
  const e = now - blinkStart
  if (e < 0 || e > close + hold + open) return 1
  if (e < close)          return 1 - e / close
  if (e < close + hold)   return 0
  const p = (e - close - hold) / open
  return p * p * (3 - 2 * p)
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
  statusOut.textContent = renderer.lost ? 'context lost — restoring…' : 'ok'
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
