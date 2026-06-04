<script setup lang="ts">
/**
 * TarotCard — in-pet tarot overlay (full 78-card draw).
 *
 * Rendered inside the main pet window (not a separate OS window) so it feels
 * integrated, like the right-click bubble menu. PetWindow grows the window to
 * a card-suitable size (scaled by the 大中小 character setting) and hides the
 * pet sprite while this is open, then restores both on close.
 *
 * Styling matches the app's frosted-green glass palette (see ChatBubble /
 * ContextMenu). Card art is resolved via TAROT_ASSETS (classic deck or
 * generated SVG placeholders).
 *
 * Flow: a face-down card → click/Enter to flip (3D Y-axis) → "interpreting"
 * delay (1–2 s, tap or Enter to skip) → reveal name + fortune with glow +
 * particles. The first reveal of each local day is remembered as the
 * "card of the day" (re-shown on reopen); every reveal is logged to history.
 *
 * Keyboard: Esc closes (or leaves the history view); Enter/Space flips or
 * skips the delay. The root carries `pet-ui-overlay` so the per-pixel
 * hit-test keeps the window interactive while the reading is open.
 */
import { ref, computed, watch, onUnmounted } from 'vue'
import {
  TAROT_DECK, TAROT_ASSETS, TAROT_TIMINGS, type TarotCard, type LocalizedText,
} from '../config/tarot'
import { useI18n } from '../i18n'
import { useTarotJournal, type JournalEntry } from '../composables/useTarotJournal'
import { playDrawSound, playFlipSound } from '../composables/useTarotSound'

const emit = defineEmits<{ close: [] }>()

const { t, locale } = useI18n()
/** Pick the active-locale string from a LocalizedText field. */
const L = (txt: LocalizedText): string => txt[locale.value]

const { history, getToday, recordDailyIfAbsent, addEntry } = useTarotJournal()

// ── State ───────────────────────────────────────────────────────────────
const visible     = ref(false)
const card        = ref<TarotCard>(TAROT_DECK[0])
const reversed    = ref(false)   // true = card drawn inverted (50 % chance)
const flipped     = ref(false)
const isFlipping  = ref(false)
const loading     = ref(false)
const revealed    = ref(false)
const drawKey     = ref(0)
const isToday     = ref(false)   // the shown card is today's recorded card
const showHistory = ref(false)   // history panel is open
const skipLeave   = ref(false)   // skip the leave fade on dismiss (see dismiss())

let loadingTimer: ReturnType<typeof setTimeout> | null = null

// ── Derived text ─────────────────────────────────────────────────────────
const fortuneText = computed(() =>
  L(reversed.value ? card.value.reversed_text : card.value.fortune_text),
)
const cardName = computed(() => L(card.value.card_name))

/** Look up a card by id (deck is id-ordered, but find() is safe regardless). */
function cardById(id: number): TarotCard {
  return TAROT_DECK.find(c => c.id === id) ?? TAROT_DECK[0]
}
/** Localized name for a history entry. */
function entryName(e: JournalEntry): string {
  return L(cardById(e.id).card_name)
}
function entryDate(ts: number): string {
  return new Date(ts).toLocaleDateString()
}

// ── Card visuals (image with gradient fallback) ──────────────────────────
function frontImage(c: TarotCard): string {
  return c.image_placeholder || TAROT_ASSETS.cardFrontImage(c.id)
}

const frontStyle = computed(() => {
  const img = frontImage(card.value)
  const base = img
    ? { backgroundImage: `url("${img}")` }
    : (() => {
        const h = card.value.hue
        return { backgroundImage: `linear-gradient(150deg, hsl(${h} 70% 62%), hsl(${(h + 40) % 360} 65% 42%))` }
      })()
  // Reversed cards are displayed rotated 180° on the front face.
  return reversed.value ? { ...base, transform: 'rotateY(180deg) rotate(180deg)' } : base
})

const backStyle = computed(() =>
  TAROT_ASSETS.cardBackImage
    ? { backgroundImage: `url("${TAROT_ASSETS.cardBackImage}")` }
    : { backgroundImage: 'linear-gradient(160deg, #dff0df, #a9cda9)' },
)

// ── Particles (pure code, no textures) ──────────────────────────────────
interface Particle { tx: number; ty: number; delay: number; size: number; hue: number }
const particles = ref<Particle[]>([])

function spawnParticles(): void {
  const n = 16
  const out: Particle[] = []
  for (let i = 0; i < n; i++) {
    const angle = (Math.PI * 2 * i) / n + Math.random() * 0.4
    const dist  = 60 + Math.random() * 70
    out.push({
      tx:    Math.cos(angle) * dist,
      ty:    Math.sin(angle) * dist,
      delay: Math.random() * 0.22,
      size:  3 + Math.random() * 4,
      hue:   card.value.hue + (Math.random() * 40 - 20),
    })
  }
  particles.value = out
}

// ── Draw / reset ────────────────────────────────────────────────────────
function pickRandomCard(): TarotCard {
  return TAROT_DECK[Math.floor(Math.random() * TAROT_DECK.length)]
}

function resetToFaceDown(): void {
  if (loadingTimer) { clearTimeout(loadingTimer); loadingTimer = null }
  showHistory.value = false
  flipped.value     = false
  isFlipping.value  = false
  loading.value     = false
  revealed.value    = false
  isToday.value     = false
  particles.value   = []
  card.value        = pickRandomCard()
  reversed.value    = Math.random() < 0.5   // 50 % chance of reversed orientation
  drawKey.value++          // remount stage → entrance animation replays
  playDrawSound()
}

// ── Reveal ────────────────────────────────────────────────────────────────
/** Finish a reading: stop loading, reveal, record to the journal. */
function revealNow(): void {
  if (loadingTimer) { clearTimeout(loadingTimer); loadingTimer = null }
  loading.value    = false
  revealed.value   = true
  isFlipping.value = false
  spawnParticles()
  // First reveal of the day becomes the "card of the day".
  const wasFirstToday = getToday() === null
  recordDailyIfAbsent(card.value.id, reversed.value)
  isToday.value = wasFirstToday
  addEntry(card.value.id, reversed.value)
}

// ── Flip interaction ────────────────────────────────────────────────────
function onCardClick(): void {
  if (loading.value) { revealNow(); return }       // tap-to-skip the delay
  if (isFlipping.value || flipped.value) return     // ignore mid-flip / when shown

  playFlipSound()
  isFlipping.value = true
  flipped.value    = true
  loading.value    = true

  const { loadingMinMs, loadingMaxMs } = TAROT_TIMINGS
  const delay = loadingMinMs + Math.random() * (loadingMaxMs - loadingMinMs)
  loadingTimer = setTimeout(revealNow, delay)
}

// ── Keyboard (active only while the overlay is open) ─────────────────────
function onKeydown(e: KeyboardEvent): void {
  if (!visible.value) return
  if (e.key === 'Escape') {
    e.preventDefault()
    if (showHistory.value) showHistory.value = false   // Esc leaves history first
    else requestClose()
    return
  }
  if (showHistory.value) return
  if (e.key === 'Enter' || e.key === ' ') {
    e.preventDefault()
    if (loading.value) revealNow()
    else if (!flipped.value) onCardClick()
  }
}

watch(visible, v => {
  if (v) window.addEventListener('keydown', onKeydown)
  else   window.removeEventListener('keydown', onKeydown)
})
onUnmounted(() => window.removeEventListener('keydown', onKeydown))

// ── Public API (called by PetWindow) ────────────────────────────────────
function open(): void {
  skipLeave.value   = false   // fade the overlay IN on open
  showHistory.value = false
  visible.value     = true

  const todays = getToday()
  if (todays) {
    // Re-show the card of the day, already revealed (no flip / delay).
    if (loadingTimer) { clearTimeout(loadingTimer); loadingTimer = null }
    card.value       = cardById(todays.id)
    reversed.value   = todays.reversed
    isToday.value    = true
    flipped.value    = true
    isFlipping.value = false
    loading.value    = false
    revealed.value   = true
    particles.value  = []     // calm re-show — no burst
    drawKey.value++
    playDrawSound()
  } else {
    resetToFaceDown()
  }
}
/** User-initiated close request — PetWindow orchestrates the shrink + fade. */
function requestClose(): void {
  emit('close')
}
/**
 * Unmount the overlay instantly (no leave fade). PetWindow calls this AFTER it
 * has shrunk the window back, so there is no card lingering/fading at the pet's
 * original position.
 */
function dismiss(): void {
  if (loadingTimer) { clearTimeout(loadingTimer); loadingTimer = null }
  skipLeave.value = true
  visible.value = false
}
defineExpose({ open, dismiss })
</script>

<template>
  <Transition name="tarot-fade" :css="!skipLeave">
    <!-- No backdrop-click-to-close: the overlay fills the whole window, so
         clicking near the card was closing the reading by accident. Close via
         the × control or Esc. -->
    <div v-if="visible" class="tarot-overlay pet-ui-overlay">
      <!-- Controls -->
      <div class="tarot-controls">
        <button v-if="!showHistory" class="ctrl" :disabled="isFlipping" :title="t.tarot.redraw" @click.stop="resetToFaceDown">↻</button>
        <button class="ctrl" :class="{ active: showHistory }" :title="t.tarot.history" @click.stop="showHistory = !showHistory">📜</button>
        <button class="ctrl" :title="t.tarot.close" @click.stop="requestClose">×</button>
      </div>

      <!-- ── Card view ─────────────────────────────────────────────── -->
      <template v-if="!showHistory">
        <!-- Card stage (keyed → entrance animation replays on each draw) -->
        <div :key="drawKey" class="stage">
          <div
            class="card"
            :class="{ 'is-flipped': flipped, 'is-glowing': revealed, 'is-locked': isFlipping }"
            :style="{ '--hue': card.hue }"
            role="button"
            @click="onCardClick"
          >
            <div class="card-inner">
              <div class="face back"  :style="backStyle" />
              <div class="face front" :style="frontStyle" />
            </div>
            <span
              v-for="(p, i) in particles"
              :key="i"
              class="particle"
              :style="{
                '--tx': p.tx + 'px', '--ty': p.ty + 'px',
                '--delay': p.delay + 's', '--size': p.size + 'px',
                background: `hsl(${p.hue} 90% 68%)`,
              }"
            />
          </div>
        </div>

        <!-- Readout -->
        <div class="readout">
          <Transition name="tarot-fade" mode="out-in">
            <div v-if="loading" key="load" class="loading" @click="revealNow">
              <span class="dot" /><span class="dot" /><span class="dot" />
              <span class="loading-text">{{ t.tarot.interpreting }}</span>
            </div>
            <div v-else-if="revealed" key="res" class="result">
              <div class="result-title">
                {{ cardName }}
                <span v-if="isToday" class="result-today">{{ t.tarot.today }}</span>
                <span class="result-orientation" :class="reversed ? 'is-reversed' : 'is-upright'">
                  {{ reversed ? t.tarot.reversed : t.tarot.upright }}
                </span>
              </div>
              <p class="result-text">{{ fortuneText }}</p>
            </div>
            <div v-else key="hint" class="hint">{{ t.tarot.hint }}</div>
          </Transition>
        </div>
      </template>

      <!-- ── History view ──────────────────────────────────────────── -->
      <div v-else class="history-panel">
        <div class="history-head">{{ t.tarot.history }}</div>
        <p v-if="history.length === 0" class="history-empty">{{ t.tarot.empty }}</p>
        <ul v-else class="history-list">
          <li v-for="(e, i) in history" :key="i" class="history-row">
            <span class="hist-dot" :class="e.reversed ? 'is-reversed' : 'is-upright'" />
            <span class="hist-name">{{ entryName(e) }}</span>
            <span class="hist-orient">{{ e.reversed ? t.tarot.reversed : t.tarot.upright }}</span>
            <span class="hist-date">{{ entryDate(e.ts) }}</span>
          </li>
        </ul>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
/* ── Overlay (transparent backdrop; floats over the desktop) ─────────── */
.tarot-overlay {
  position: fixed;
  inset: 0;
  z-index: 1000;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 14px 12px 12px;
  box-sizing: border-box;
  font-family: system-ui, "Segoe UI", "Noto Sans SC", "Noto Sans JP", sans-serif;
  user-select: none;
}

/* ── Controls ───────────────────────────────────────────────────────── */
.tarot-controls {
  position: absolute;
  top: 8px;
  right: 8px;
  display: flex;
  gap: 6px;
  z-index: 2;
}
.ctrl {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  padding: 0;
  border-radius: 50%;
  border: 1px solid rgba(119, 153, 119, 0.40);
  background: rgba(245, 250, 245, 0.92);
  backdrop-filter: blur(16px) saturate(150%);
  -webkit-backdrop-filter: blur(16px) saturate(150%);
  color: #2a4a2a;
  font-size: 14px;
  line-height: 1;
  text-align: center;
  cursor: pointer;
  box-shadow: 0 2px 8px rgba(40, 70, 40, 0.14);
  transition: background 150ms ease, transform 120ms ease, opacity 150ms ease;
}
.ctrl:hover:not(:disabled)  { background: #fff; transform: scale(1.08); }
.ctrl:active:not(:disabled) { transform: scale(0.92); }
.ctrl:disabled { opacity: 0.4; cursor: not-allowed; }
.ctrl.active {
  background: linear-gradient(135deg, #779977, #5a8060);
  border-color: transparent;
  color: #fff;
}

/* ── Card stage + entrance animation ────────────────────────────────── */
.stage {
  perspective: 1100px;
  animation: card-enter 560ms cubic-bezier(0.22, 1, 0.36, 1) both;
}
@keyframes card-enter {
  from { opacity: 0; transform: translateY(60px) scale(0.82); }
  to   { opacity: 1; transform: translateY(0)    scale(1);    }
}

.card {
  position: relative;
  height: 56vh;
  aspect-ratio: 300 / 500;
  cursor: pointer;
  transition: transform 200ms ease;
}
.card:not(.is-flipped):hover { transform: translateY(-5px); }
.card.is-flipped { cursor: default; }
.card.is-locked  { pointer-events: none; }

.card-inner {
  position: relative;
  width: 100%;
  height: 100%;
  transform-style: preserve-3d;
  transition: transform v-bind('TAROT_TIMINGS.flipMs + "ms"') cubic-bezier(0.45, 0, 0.25, 1);
  border-radius: 16px;
}
.card.is-flipped .card-inner { transform: rotateY(180deg); }

.face {
  position: absolute;
  inset: 0;
  border-radius: 16px;
  overflow: hidden;                 /* clip the tap-hint sheen to the card */
  backface-visibility: hidden;
  -webkit-backface-visibility: hidden;
  background-size: cover;
  background-position: center;
  box-shadow: 0 10px 28px rgba(40, 70, 40, 0.30);
  border: 1px solid rgba(255, 255, 255, 0.5);
}
.back  { transform: rotateY(0deg); }
.front { transform: rotateY(180deg); }

/* ── Tap-hint sheen on the face-down card (signals it's interactive) ─── */
.card:not(.is-flipped) .back::after {
  content: '';
  position: absolute;
  inset: 0;
  background: linear-gradient(115deg, transparent 42%, rgba(255, 255, 255, 0.38) 50%, transparent 58%);
  transform: translateX(-100%);
  animation: tap-sheen 3.2s ease-in-out infinite;
  pointer-events: none;
}
@keyframes tap-sheen {
  0%        { transform: translateX(-100%); }
  55%, 100% { transform: translateX(100%); }
}

/* ── Glow VFX ────────────────────────────────────────────────────────
   Rendered as a STATIC halo on a dedicated ::before layer; only its
   `opacity` animates. Opacity is GPU-composited, so the breathing is
   smooth — unlike animating box-shadow values, which repaints every
   frame and stutters (especially on a transparent window). A one-shot
   `glow-in` eases it up to the pulse's base opacity before the infinite
   pulse takes over, so it never pops in. */
.card.is-glowing::before {
  content: '';
  position: absolute;
  inset: -2px;
  border-radius: 18px;
  box-shadow: 0 0 10px 1px hsl(var(--hue) 80% 60% / 0.6);
  opacity: 0;
  pointer-events: none;
  will-change: opacity;
  animation:
    glow-in    0.6s ease-out forwards,
    glow-pulse 2.8s ease-in-out 0.6s infinite;
}
@keyframes glow-in    { to { opacity: 0.5; } }
@keyframes glow-pulse {
  0%, 100% { opacity: 0.5; }
  50%      { opacity: 0.85; }
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
  box-shadow: 0 0 6px 1px currentColor;
  animation: particle-fly 1000ms ease-out var(--delay) forwards;
}
@keyframes particle-fly {
  0%   { opacity: 0;   transform: translate(-50%, -50%) scale(0.4); }
  20%  { opacity: 1; }
  100% { opacity: 0;   transform: translate(calc(-50% + var(--tx)), calc(-50% + var(--ty))) scale(1); }
}

/* ── Readout (frosted-green, matches ChatBubble) ────────────────────── */
.readout {
  width: min(94%, 340px);
  min-height: 54px;
  display: flex;
  align-items: center;
  justify-content: center;
}
.hint { color: rgba(42, 74, 42, 0.7); font-size: 12px; text-align: center; }

.loading {
  display: flex; align-items: center; gap: 6px;
  padding: 7px 14px;
  background: rgba(245, 250, 245, 0.92);
  border: 1px solid rgba(148, 185, 148, 0.45);
  border-radius: 999px;
  color: #2a4a2a; font-size: 12.5px;
  cursor: pointer;                  /* tap to skip the delay */
  box-shadow: 0 2px 8px rgba(40, 70, 40, 0.12);
}
.dot { width: 6px; height: 6px; border-radius: 50%; background: currentColor; animation: dot-bounce 1s ease-in-out infinite; }
.dot:nth-child(2) { animation-delay: 0.15s; }
.dot:nth-child(3) { animation-delay: 0.30s; }
@keyframes dot-bounce {
  0%, 80%, 100% { transform: translateY(0);    opacity: 0.4; }
  40%           { transform: translateY(-5px); opacity: 1; }
}

.result {
  width: 100%;
  box-sizing: border-box;
  max-height: 26vh;
  overflow-y: auto;
  background: rgba(245, 250, 245, 0.94);
  border: 1px solid rgba(148, 185, 148, 0.45);
  border-radius: 14px;
  padding: 11px 14px;
  backdrop-filter: blur(20px) saturate(160%);
  -webkit-backdrop-filter: blur(20px) saturate(160%);
  box-shadow: 0 4px 14px rgba(40, 70, 40, 0.16), inset 0 1px 0 rgba(255, 255, 255, 0.8);
}
.result-title {
  font-family: Georgia, "Times New Roman", serif;
  font-size: 15px; font-weight: 700; color: #1a2e1a;
  margin-bottom: 5px; text-align: center;
  display: flex; align-items: center; justify-content: center; gap: 7px; flex-wrap: wrap;
}
.result-today {
  font-family: system-ui, "Segoe UI", sans-serif;
  font-size: 10px; font-weight: 600; letter-spacing: 0.04em;
  padding: 2px 7px; border-radius: 999px;
  background: rgba(86, 153, 86, 0.18); color: #2a6a2a;
  border: 1px solid rgba(86, 153, 86, 0.40);
}
.result-orientation {
  font-family: system-ui, "Segoe UI", sans-serif;
  font-size: 10px; font-weight: 600; letter-spacing: 0.06em;
  padding: 2px 7px; border-radius: 999px;
}
.result-orientation.is-upright {
  background: rgba(86, 153, 86, 0.15); color: #2a6a2a;
  border: 1px solid rgba(86, 153, 86, 0.35);
}
.result-orientation.is-reversed {
  background: rgba(153, 86, 86, 0.13); color: #6a2a2a;
  border: 1px solid rgba(153, 86, 86, 0.30);
}
.result-text {
  margin: 0;
  font-size: 12px; line-height: 1.55; color: #1a2e1a;
  word-break: break-word; overflow-wrap: anywhere;
}

/* ── History view ───────────────────────────────────────────────────── */
.history-panel {
  width: min(94%, 340px);
  max-height: 72vh;
  display: flex;
  flex-direction: column;
  background: rgba(245, 250, 245, 0.94);
  border: 1px solid rgba(148, 185, 148, 0.45);
  border-radius: 14px;
  padding: 10px 12px;
  backdrop-filter: blur(20px) saturate(160%);
  -webkit-backdrop-filter: blur(20px) saturate(160%);
  box-shadow: 0 4px 14px rgba(40, 70, 40, 0.16), inset 0 1px 0 rgba(255, 255, 255, 0.8);
}
.history-head {
  font-size: 9.5px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.7px;
  color: rgba(45, 85, 45, 0.55); margin-bottom: 7px;
}
.history-empty { margin: 6px 0; font-size: 12px; color: rgba(42, 74, 42, 0.6); text-align: center; }
.history-list {
  list-style: none; margin: 0; padding: 0;
  overflow-y: auto;
  scrollbar-width: thin;
  scrollbar-color: rgba(119, 153, 119, 0.30) transparent;
}
.history-list::-webkit-scrollbar       { width: 4px; }
.history-list::-webkit-scrollbar-thumb { background: rgba(119, 153, 119, 0.30); border-radius: 2px; }
.history-row {
  display: flex; align-items: center; gap: 7px;
  padding: 5px 2px;
  border-bottom: 1px solid rgba(119, 153, 119, 0.16);
  font-size: 12px; color: #1a2e1a;
}
.history-row:last-child { border-bottom: none; }
.hist-dot { width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0; }
.hist-dot.is-upright  { background: #5a9960; }
.hist-dot.is-reversed { background: #a86a6a; }
.hist-name   { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 600; }
.hist-orient { font-size: 10px; color: rgba(45, 85, 45, 0.6); flex-shrink: 0; }
.hist-date   { font-size: 10px; color: rgba(45, 85, 45, 0.45); flex-shrink: 0; }

/* ── Transitions ────────────────────────────────────────────────────── */
.tarot-fade-enter-active, .tarot-fade-leave-active { transition: opacity 220ms ease; }
.tarot-fade-enter-from,  .tarot-fade-leave-to      { opacity: 0; }
</style>
