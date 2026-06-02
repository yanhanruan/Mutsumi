<script setup lang="ts">
/**
 * TarotDraw — interactive Major Arcana single-card draw.
 *
 * Phases (see config/tarot.ts for all tunable paths + timings):
 *   1. Entrance: a face-down card slides/fades/scales in. A random card is
 *      pre-selected but hidden.
 *   2. Flip: clicking the face-down card triggers a 3D Y-axis flip. Repeat
 *      clicks are ignored while the flip (and subsequent loading) runs.
 *   3. Interpret: a "reading" loading state shows for a randomized 1–2 s
 *      (setTimeout) to simulate computation, then the card name + fortune
 *      text render in a readable panel and a pure-CSS glow + particle VFX
 *      plays around the card.
 *   4. Redraw: resets all state, picks a new random card, replays entrance.
 *
 * No backend, no network, no AI: cards come from the local MAJOR_ARCANA table
 * and the "reading" is a timer. No external VFX textures: the glow is a CSS
 * box-shadow animation and the particles are code-positioned <span>s.
 */
import { ref, computed, onMounted } from 'vue'
import {
  MAJOR_ARCANA, TAROT_ASSETS, TAROT_TIMINGS, type TarotCard,
} from '../config/tarot'
import { playDrawSound, playFlipSound } from '../composables/useTarotSound'

// ── State ───────────────────────────────────────────────────────────────
const card      = ref<TarotCard>(MAJOR_ARCANA[0])
const flipped   = ref(false)   // card is face-up
const isFlipping = ref(false)  // flip/loading in progress → clicks ignored
const loading   = ref(false)   // "interpreting" state visible
const revealed  = ref(false)   // fortune text + VFX visible
const drawKey   = ref(0)       // bump to remount the stage and replay entrance

let loadingTimer: ReturnType<typeof setTimeout> | null = null

// ── Front-face visuals (image with gradient fallback) ───────────────────
/** Resolve a card's front image, honoring per-card override then global builder. */
function frontImage(c: TarotCard): string {
  return c.image_placeholder || TAROT_ASSETS.cardFrontImage(c.id)
}

const frontStyle = computed(() => {
  const img = frontImage(card.value)
  if (img) {
    return { backgroundImage: `url("${img}")`, backgroundSize: 'cover', backgroundPosition: 'center' }
  }
  // Code-generated color block: a two-stop gradient derived from the card hue.
  const h = card.value.hue
  return {
    backgroundImage:
      `linear-gradient(150deg, hsl(${h} 70% 62%) 0%, hsl(${(h + 40) % 360} 65% 42%) 100%)`,
  }
})

const backStyle = computed(() => {
  if (TAROT_ASSETS.cardBackImage) {
    return { backgroundImage: `url("${TAROT_ASSETS.cardBackImage}")`, backgroundSize: 'cover', backgroundPosition: 'center' }
  }
  // Placeholder card back: deep indigo gradient with a faint star motif.
  return {
    backgroundImage:
      'radial-gradient(circle at 50% 30%, rgba(255,255,255,0.10), transparent 60%), linear-gradient(160deg, #2a2350 0%, #15113a 100%)',
  }
})

// ── Particle VFX (pure code, no textures) ───────────────────────────────
interface Particle { tx: number; ty: number; delay: number; size: number; hue: number }
const particles = ref<Particle[]>([])

function spawnParticles(): void {
  const n = 18
  const out: Particle[] = []
  for (let i = 0; i < n; i++) {
    const angle = (Math.PI * 2 * i) / n + Math.random() * 0.4
    const dist  = 70 + Math.random() * 90
    out.push({
      tx:    Math.cos(angle) * dist,
      ty:    Math.sin(angle) * dist,
      delay: Math.random() * 0.25,
      size:  4 + Math.random() * 5,
      hue:   card.value.hue + (Math.random() * 40 - 20),
    })
  }
  particles.value = out
}

// ── Draw / redraw ───────────────────────────────────────────────────────
function pickRandomCard(): TarotCard {
  return MAJOR_ARCANA[Math.floor(Math.random() * MAJOR_ARCANA.length)]
}

function deal(): void {
  if (loadingTimer) { clearTimeout(loadingTimer); loadingTimer = null }
  flipped.value    = false
  isFlipping.value = false
  loading.value    = false
  revealed.value   = false
  particles.value  = []
  card.value       = pickRandomCard()
  drawKey.value++          // remount stage → entrance animation replays
  playDrawSound()
}

// ── Flip interaction ────────────────────────────────────────────────────
function onCardClick(): void {
  // Guard: ignore while flipping/loading or once already revealed.
  if (isFlipping.value || flipped.value) return

  playFlipSound()
  isFlipping.value = true
  flipped.value    = true
  loading.value    = true   // "Interpreting the stars..."

  // Simulated reading delay: random within the configured 1–2 s window.
  const { loadingMinMs, loadingMaxMs } = TAROT_TIMINGS
  const delay = loadingMinMs + Math.random() * (loadingMaxMs - loadingMinMs)

  loadingTimer = setTimeout(() => {
    loading.value    = false
    revealed.value   = true
    isFlipping.value = false   // re-enable interaction (redraw)
    spawnParticles()
    loadingTimer = null
  }, delay)
}

onMounted(() => {
  deal()
})
</script>

<template>
  <div class="tarot-root">
    <!-- Stage is keyed so a redraw remounts it and replays the entrance anim. -->
    <div :key="drawKey" class="tarot-stage">
      <div
        class="tarot-card"
        :class="{ 'is-flipped': flipped, 'is-glowing': revealed, 'is-locked': isFlipping }"
        :style="{ '--card-hue': card.hue }"
        role="button"
        :aria-label="flipped ? card.card_name : 'Face-down tarot card. Click to reveal.'"
        @click="onCardClick"
      >
        <div class="tarot-card-inner">
          <div class="tarot-face tarot-back" :style="backStyle">
            <span class="back-emblem">✦</span>
          </div>
          <div class="tarot-face tarot-front" :style="frontStyle">
            <span class="front-roman">{{ card.id }}</span>
            <span class="front-name">{{ card.card_name }}</span>
          </div>
        </div>

        <!-- Particle scatter — only while revealed. -->
        <span
          v-for="(p, i) in particles"
          :key="i"
          class="particle"
          :style="{
            '--tx': p.tx + 'px',
            '--ty': p.ty + 'px',
            '--delay': p.delay + 's',
            '--size': p.size + 'px',
            background: `hsl(${p.hue} 90% 70%)`,
          }"
        />
      </div>
    </div>

    <!-- Reading / result panel -->
    <div class="tarot-readout">
      <Transition name="fade" mode="out-in">
        <div v-if="loading" key="loading" class="loading">
          <span class="loading-dot" /><span class="loading-dot" /><span class="loading-dot" />
          <span class="loading-text">Interpreting the stars…</span>
        </div>

        <div v-else-if="revealed" key="result" class="result">
          <h2 class="result-title">{{ card.card_name }}</h2>
          <p class="result-text">{{ card.fortune_text }}</p>
        </div>

        <div v-else key="hint" class="hint">
          Click the card to reveal your fortune.
        </div>
      </Transition>
    </div>

    <button class="redraw-btn" :disabled="isFlipping" @click="deal">↻ Redraw</button>
  </div>
</template>

<style scoped>
/* ── Root / layout ──────────────────────────────────────────────────── */
.tarot-root {
  position: fixed;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 22px;
  padding: 24px;
  box-sizing: border-box;
  background:
    radial-gradient(circle at 50% 18%, #2c2660 0%, #181436 45%, #0d0a24 100%);
  font-family: system-ui, "Segoe UI", "Noto Sans SC", "Noto Sans JP", sans-serif;
  color: #ece8ff;
  user-select: none;
  overflow: hidden;
}

/* ── Card stage + entrance animation ────────────────────────────────── */
/* The keyframe runs once on (re)mount — keying .tarot-stage replays it. */
.tarot-stage {
  perspective: 1200px;
  animation: card-enter 620ms cubic-bezier(0.22, 1, 0.36, 1) both;
}
@keyframes card-enter {
  from { opacity: 0; transform: translateY(80px) scale(0.82); }
  to   { opacity: 1; transform: translateY(0)    scale(1);    }
}

.tarot-card {
  position: relative;
  width: 200px;
  height: 320px;
  cursor: pointer;
  transition: transform 220ms ease;
}
.tarot-card:not(.is-flipped):hover { transform: translateY(-6px); }
.tarot-card.is-flipped { cursor: default; }
.tarot-card.is-locked  { pointer-events: none; }

.tarot-card-inner {
  position: relative;
  width: 100%;
  height: 100%;
  transform-style: preserve-3d;
  transition: transform v-bind('TAROT_TIMINGS.flipMs + "ms"') cubic-bezier(0.45, 0, 0.25, 1);
}
.tarot-card.is-flipped .tarot-card-inner { transform: rotateY(180deg); }

/* ── Faces ──────────────────────────────────────────────────────────── */
.tarot-face {
  position: absolute;
  inset: 0;
  border-radius: 16px;
  backface-visibility: hidden;
  -webkit-backface-visibility: hidden;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  box-shadow: 0 12px 34px rgba(0, 0, 0, 0.45);
  border: 1px solid rgba(255, 255, 255, 0.14);
}
.tarot-back { transform: rotateY(0deg); }
.tarot-front {
  transform: rotateY(180deg);
  color: #fff;
  text-shadow: 0 2px 10px rgba(0, 0, 0, 0.45);
  gap: 10px;
}
.back-emblem  { font-size: 48px; color: rgba(255, 255, 255, 0.55); }
.front-roman  { font-size: 40px; font-weight: 700; opacity: 0.9; }
.front-name   { font-size: 20px; font-weight: 600; letter-spacing: 0.04em; text-align: center; padding: 0 12px; }

/* ── Glow VFX (pure box-shadow, no texture) ─────────────────────────── */
.tarot-card.is-glowing .tarot-card-inner {
  animation: glow-pulse 2.4s ease-in-out infinite;
}
@keyframes glow-pulse {
  0%, 100% {
    box-shadow:
      0 0 12px 2px hsl(var(--card-hue) 90% 65% / 0.45),
      0 0 28px 6px hsl(var(--card-hue) 90% 60% / 0.25);
  }
  50% {
    box-shadow:
      0 0 22px 5px hsl(var(--card-hue) 95% 70% / 0.70),
      0 0 50px 14px hsl(var(--card-hue) 90% 62% / 0.40);
  }
}

/* ── Particles ──────────────────────────────────────────────────────── */
.particle {
  position: absolute;
  top: 50%;
  left: 50%;
  width: var(--size);
  height: var(--size);
  border-radius: 50%;
  pointer-events: none;
  opacity: 0;
  animation: particle-fly 1100ms ease-out var(--delay) forwards;
  box-shadow: 0 0 6px 1px currentColor;
}
@keyframes particle-fly {
  0%   { opacity: 0;   transform: translate(-50%, -50%) scale(0.4); }
  20%  { opacity: 1; }
  100% { opacity: 0;   transform: translate(calc(-50% + var(--tx)), calc(-50% + var(--ty))) scale(1); }
}

/* ── Readout panel ──────────────────────────────────────────────────── */
.tarot-readout {
  width: min(420px, 86vw);
  min-height: 96px;
  display: flex;
  align-items: center;
  justify-content: center;
}
.hint { color: rgba(236, 232, 255, 0.6); font-size: 14px; }

.loading { display: flex; align-items: center; gap: 8px; color: rgba(236, 232, 255, 0.85); font-size: 15px; }
.loading-text { margin-left: 4px; letter-spacing: 0.02em; }
.loading-dot {
  width: 8px; height: 8px; border-radius: 50%;
  background: currentColor;
  animation: dot-bounce 1s ease-in-out infinite;
}
.loading-dot:nth-child(2) { animation-delay: 0.15s; }
.loading-dot:nth-child(3) { animation-delay: 0.30s; }
@keyframes dot-bounce {
  0%, 80%, 100% { transform: translateY(0); opacity: 0.4; }
  40%           { transform: translateY(-6px); opacity: 1; }
}

.result {
  width: 100%;
  box-sizing: border-box;
  background: rgba(255, 255, 255, 0.07);
  border: 1px solid rgba(255, 255, 255, 0.14);
  border-radius: 14px;
  padding: 18px 20px;
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.30);
}
.result-title {
  margin: 0 0 8px;
  font-size: 19px;
  font-weight: 700;
  color: #fff;
}
.result-text {
  margin: 0;
  font-size: 14.5px;
  line-height: 1.6;
  color: rgba(236, 232, 255, 0.92);
  word-break: break-word;        /* support wrapping */
  overflow-wrap: anywhere;
}

/* ── Redraw button ──────────────────────────────────────────────────── */
.redraw-btn {
  padding: 9px 22px;
  border-radius: 999px;
  border: 1px solid rgba(255, 255, 255, 0.22);
  background: rgba(255, 255, 255, 0.10);
  color: #fff;
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
  transition: background 160ms ease, transform 120ms ease, opacity 160ms ease;
}
.redraw-btn:hover:not(:disabled)  { background: rgba(255, 255, 255, 0.20); transform: translateY(-1px); }
.redraw-btn:active:not(:disabled) { transform: translateY(0) scale(0.97); }
.redraw-btn:disabled { opacity: 0.45; cursor: not-allowed; }

/* ── Transitions ────────────────────────────────────────────────────── */
.fade-enter-active, .fade-leave-active { transition: opacity 220ms ease; }
.fade-enter-from, .fade-leave-to       { opacity: 0; }
</style>
