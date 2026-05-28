<script setup lang="ts">
/**
 * BalloonPet — floating balloon zombie, active when the user is truly idle.
 *
 * Lifecycle:
 *   - Listens for `toggle-balloon-mode { active: boolean }` from the Rust
 *     idle detector (src-tauri/src/idle.rs).
 *   - When active: starts a requestAnimationFrame loop that moves the sprite
 *     using vector-blended movement (see useBalloonFlight.ts).
 *   - When inactive: cancels the RAF loop and hides the sprite.
 *
 * Movement recap (all math lives in useBalloonFlight.ts):
 *   - Every tick: stepPosition adds the current velocity, clamps to viewport.
 *   - On any wall hit: blendVectors() picks a new direction, applyWallForce()
 *     guarantees the sprite can't stick by forcing the axis away from the wall.
 *   - Sprite image: cycles through /assets/fly_right/ when vx ≥ 0,
 *                   /assets/fly_left/ when vx < 0.
 *
 * Note: fly_right / fly_left frame sequences must be placed at
 *   public/assets/fly_right/frame_001.webp … frame_NNN.webp
 *   public/assets/fly_left/ frame_001.webp … frame_NNN.webp
 * Update FRAMES_RIGHT / FRAMES_LEFT below to match your actual frame counts.
 */
import { ref, onMounted, onUnmounted } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import {
  SPRITE_W, SPRITE_H,
  blendVectors, applyWallForce, stepPosition,
} from '../composables/useBalloonFlight'

// ── Config ──────────────────────────────────────────────────────────
/** Frame rate for the sprite animation cycle (not the movement rate). */
const FRAME_RATE   = 12
const MS_PER_FRAME = 1000 / FRAME_RATE

/** Number of frames in public/assets/fly_right/ and fly_left/. */
const FRAMES_RIGHT = 8
const FRAMES_LEFT  = 8

// ── Reactive render state ───────────────────────────────────────────
const visible = ref(false)
const spriteX = ref(0)
const spriteY = ref(0)
const imgSrc  = ref('')

// ── Internal mutable state (not reactive — updated every RAF tick) ──
let vx = 0
let vy = 0
let frameIndex    = 0
let lastFrameTime = 0
let rafId: number | null = null
let unlisten: UnlistenFn | null = null

// ── Frame path helper ───────────────────────────────────────────────
function frameSrc(dir: 'right' | 'left', idx: number): string {
  const folder = dir === 'right' ? 'fly_right' : 'fly_left'
  const n = String(idx + 1).padStart(3, '0')
  return `/assets/${folder}/frame_${n}.webp`
}

// ── RAF animation loop ───────────────────────────────────────────────
function tick(now: number) {
  if (!visible.value) return

  const w = window.innerWidth
  const h = window.innerHeight

  // Step position and detect wall hits.
  const { nx, ny, hitLeft, hitRight, hitTop, hitBot } =
    stepPosition(spriteX.value, spriteY.value, vx, vy, w, h)

  // On any wall hit: recalculate blended direction, then force axis away.
  if (hitLeft || hitRight || hitTop || hitBot) {
    const blended = blendVectors(nx, ny, w, h)
    const forced  = applyWallForce(
      blended.vx, blended.vy,
      hitLeft, hitRight, hitTop, hitBot,
    )
    vx = forced.vx
    vy = forced.vy
  }

  spriteX.value = nx
  spriteY.value = ny

  // Advance sprite frame at FRAME_RATE fps (independent of movement speed).
  if (now - lastFrameTime >= MS_PER_FRAME) {
    const dir   = vx >= 0 ? 'right' : 'left'
    const count = vx >= 0 ? FRAMES_RIGHT : FRAMES_LEFT
    frameIndex    = (frameIndex + 1) % count
    imgSrc.value  = frameSrc(dir, frameIndex)
    lastFrameTime = now
  }

  rafId = requestAnimationFrame(tick)
}

// ── Activation / deactivation ───────────────────────────────────────
function activate() {
  const w = window.innerWidth
  const h = window.innerHeight

  // Start near the center of the viewport.
  spriteX.value = w / 2 - SPRITE_W / 2
  spriteY.value = h / 2 - SPRITE_H / 2

  // Pick an initial blended velocity.
  const v = blendVectors(spriteX.value, spriteY.value, w, h)
  vx = v.vx
  vy = v.vy

  frameIndex    = 0
  lastFrameTime = 0
  imgSrc.value  = frameSrc(vx >= 0 ? 'right' : 'left', 0)
  visible.value = true

  if (rafId !== null) cancelAnimationFrame(rafId)
  rafId = requestAnimationFrame(tick)
}

function deactivate() {
  visible.value = false
  if (rafId !== null) {
    cancelAnimationFrame(rafId)
    rafId = null
  }
}

// ── Lifecycle ────────────────────────────────────────────────────────
onMounted(async () => {
  unlisten = await listen<{ active: boolean }>('toggle-balloon-mode', e => {
    if (e.payload.active) activate()
    else deactivate()
  })
})

onUnmounted(() => {
  deactivate()
  unlisten?.()
})
</script>

<template>
  <Transition name="balloon">
    <div v-if="visible" class="balloon-stage">
      <img
        :src="imgSrc"
        :style="{
          left:   spriteX + 'px',
          top:    spriteY + 'px',
          width:  SPRITE_W + 'px',
          height: SPRITE_H + 'px',
        }"
        class="balloon-sprite"
        alt=""
        draggable="false"
      />
    </div>
  </Transition>
</template>

<style scoped>
/* Full-viewport transparent layer — pointer-events: none so the pet
   window still receives mouse input for drag/click while balloon is active. */
.balloon-stage {
  position: fixed;
  inset: 0;
  overflow: hidden;
  pointer-events: none;
  z-index: 10;
}

.balloon-sprite {
  position: absolute;
  object-fit: contain;
  pointer-events: none;
  user-select: none;
  -webkit-user-drag: none;
}

/* Fade in/out — the sprite animation itself handles the visual float. */
.balloon-enter-active,
.balloon-leave-active { transition: opacity 400ms ease; }
.balloon-enter-from,
.balloon-leave-to     { opacity: 0; }
</style>
