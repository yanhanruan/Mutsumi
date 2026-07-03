<script setup lang="ts">
/**
 * BalloonPet — flying-mode sprite, shown while the user is truly idle.
 *
 * Movement lives in Rust (src-tauri/src/flight.rs): the whole OS window
 * glides across the monitor work area. Animating the sprite *inside* the
 * webview was the old approach and looked like erratic jitter — the window
 * is only ~170×289 CSS px, so the sprite had almost no room to travel.
 * This component therefore only renders the frame animation; it never moves
 * the sprite.
 *
 * Events (from Rust):
 *   - `toggle-balloon-mode { active }`  (idle.rs)  — show/hide + start/stop RAF.
 *   - `balloon-facing { facing }`       (flight.rs) — "left" | "right", emitted
 *     at flight start and on every horizontal edge bounce.
 *
 * Frames: only a LEFT-facing sequence exists —
 *   public/assets/fly_left/frame_001.webp … frame_192.webp
 * Playback ping-pongs (1 → last → 1 → …). Right-facing flight mirrors the
 * sprite with scaleX(-1); there is no fly_right folder.
 */
import { ref, onMounted, onUnmounted } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

// ── Config ──────────────────────────────────────────────────────────
/** Total frames in public/assets/fly_left/ (frame_001 … frame_192). */
const FRAME_COUNT = 192

/** Sprite playback rate in frames per second. */
const FRAME_RATE   = 12
const MS_PER_FRAME = 1000 / FRAME_RATE

// ── Reactive render state ───────────────────────────────────────────
const visible = ref(false)
const facing  = ref<'left' | 'right'>('left')
const imgSrc  = ref('')

// ── Internal mutable state (not reactive — updated every RAF tick) ──
let frameIndex    = 0
let pingPongStep  = 1   // +1 while playing 1→last, −1 while playing last→1
let lastFrameTime = 0
let rafId: number | null = null
let unlistenMode:   UnlistenFn | null = null
let unlistenFacing: UnlistenFn | null = null

function frameSrc(idx: number): string {
  return `/assets/fly_left/frame_${String(idx + 1).padStart(3, '0')}.webp`
}

// ── RAF animation loop (frame cycling only — no movement) ───────────
function tick(now: number) {
  if (!visible.value) return

  if (now - lastFrameTime >= MS_PER_FRAME) {
    // Ping-pong: bounce the step direction at both ends of the sequence.
    const next = frameIndex + pingPongStep
    if (next < 0 || next >= FRAME_COUNT) pingPongStep = -pingPongStep
    frameIndex   += pingPongStep
    imgSrc.value  = frameSrc(frameIndex)
    lastFrameTime = now
  }

  rafId = requestAnimationFrame(tick)
}

// ── Activation / deactivation ───────────────────────────────────────
function activate() {
  frameIndex    = 0
  pingPongStep  = 1
  lastFrameTime = 0
  imgSrc.value  = frameSrc(0)
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
  unlistenMode = await listen<{ active: boolean }>('toggle-balloon-mode', e => {
    if (e.payload.active) activate()
    else deactivate()
  })
  unlistenFacing = await listen<{ facing: 'left' | 'right' }>('balloon-facing', e => {
    facing.value = e.payload.facing
  })
})

onUnmounted(() => {
  deactivate()
  unlistenMode?.()
  unlistenFacing?.()
})
</script>

<template>
  <Transition name="balloon">
    <div v-if="visible" class="balloon-stage">
      <img
        :src="imgSrc"
        class="balloon-sprite"
        :class="{ mirrored: facing === 'right' }"
        alt=""
        draggable="false"
      />
    </div>
  </Transition>
</template>

<style scoped>
/* Full-window transparent layer — pointer-events: none so the pet window
   still receives mouse input while balloon mode is active. */
.balloon-stage {
  position: fixed;
  inset: 0;
  overflow: hidden;
  pointer-events: none;
  z-index: 10;
}

/* The sprite fills the window and stays put — the window itself flies. */
.balloon-sprite {
  width: 100%;
  height: 100%;
  object-fit: contain;
  pointer-events: none;
  user-select: none;
  -webkit-user-drag: none;
}

/* Only left-facing frames exist; mirror them for rightward flight. */
.balloon-sprite.mirrored {
  transform: scaleX(-1);
}

/* Fade in/out — the frame animation handles the visual float. */
.balloon-enter-active,
.balloon-leave-active { transition: opacity 400ms ease; }
.balloon-enter-from,
.balloon-leave-to     { opacity: 0; }
</style>
