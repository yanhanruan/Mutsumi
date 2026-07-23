/**
 * main.ts — entry for the P3 puppet spike (puppet-spike.html). Dev-only: wires
 * the multi-PART runtime end to end so a human can eyeball it —
 *   procedural per-layer textures + per-part grid meshes + parameter bindings
 *     → per-part deform() → layered WebGL2 draw on a transparent canvas.
 *
 * The eyes + eyelids are their own layers, so a blink closes them fully without
 * touching the brows. Automatic breathing + random blink + idle sway; live
 * sliders for head/mouth/eye and eye-shape tuning; background toggle; a
 * "simulate GPU loss" button. NOT part of the production app or its build.
 */
import {
  buildFacePartsGeometry, drawFaceBase, drawEye, drawLid, FACE_TUNING_DEFAULTS,
} from './faceRig'
import { createPuppetRenderer, type RenderPart } from '../webgl/puppetRenderer'
import { deformPuppet, type Part } from '../part'
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

function rebuildPuppet() {
  parts = geom.parts.map(p => ({ ...p, image: imageFor(p.id) }))
  renderer.setPuppet(parts as RenderPart[])
}
rebuildPuppet()

// ── controls ────────────────────────────────────────────────────────
const angleX = el<HTMLInputElement>('angleX')
const angleY = el<HTMLInputElement>('angleY')
const mouth  = el<HTMLInputElement>('mouth')
const eye    = el<HTMLInputElement>('eye')
const autoBlink = el<HTMLInputElement>('autoBlink')
const sway      = el<HTMLInputElement>('sway')
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

  const swayX = sway.checked ? 7 * Math.sin(now * 0.0007) : 0
  const swayY = sway.checked ? 4 * Math.sin(now * 0.0009) : 0
  values.set('ParamAngleX', angleX.valueAsNumber + swayX)
  values.set('ParamAngleY', angleY.valueAsNumber + swayY)
  values.set('ParamMouthOpenY', mouth.valueAsNumber / 100)

  renderer.draw(deformPuppet(parts, defsIndex, values))

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
