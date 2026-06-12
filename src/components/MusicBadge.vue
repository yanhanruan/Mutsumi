<script setup lang="ts">
/**
 * MusicBadge — mini music controller pinned to the bottom-right of the pet.
 *
 * Mirrors WeatherBadge: seed the cached snapshot via `get_media` on mount, then
 * subscribe to `media-update`. A compact equalizer badge shows whenever a media
 * session is active; hovering expands a frosted panel with title / artist, a
 * progress bar, and transport controls (prev / play-pause / next).
 *
 * The backend (SMTC, src-tauri/src/media.rs) drives any app that integrates with
 * Windows media controls — Spotify, browsers, 网易云音乐, etc. The progress bar
 * is interpolated from a local clock between ~1 Hz backend updates so it ticks
 * smoothly.
 */
import { onMounted, onUnmounted, ref, computed, watch, nextTick } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import lottie from 'lottie-web'
import { useI18n } from '../i18n'
import { useAppConfig, type CharacterSize } from '../composables/useAppConfig'
import speakersAnim from '../assets/speakers.json'

interface MediaSnapshot {
  active:      boolean
  title:       string
  artist:      string
  status:      string
  position_ms: number
  duration_ms: number
  can_next:    boolean
  can_prev:    boolean
  can_play:    boolean
  can_pause:   boolean
  muted:       boolean
  volume:      number
}

const { t } = useI18n()
const { config } = useAppConfig()

// Badge size tracks the character-size setting (大中小), mirroring how the rest
// of the pet UI scales with the window. The Lottie itself is an SVG, so it
// fills whatever box we give it crisply at any size.
const BADGE_PX: Record<CharacterSize, number> = {
  small:  34,
  medium: 40,
  large:  46,
}
const badgePx = computed(() => BADGE_PX[config.value.characterSize])

const data    = ref<MediaSnapshot | null>(null)
const hovered = ref(false)
// System-audio activity (from the WASAPI detector). The Lottie speakers spin
// while audio is actually playing and rest when it stops — independent of
// whether a controllable SMTC session exists. Whether the badge is *shown* at
// all is decided in Settings (config.showMusic), not here.
const audioOn = ref(false)

// Volume slider state (local so dragging stays smooth between backend polls).
const vol = ref(0)
let draggingVol = false

// Lottie speakers animation for the badge.
const lottieEl = ref<HTMLElement | null>(null)
let anim: ReturnType<typeof lottie.loadAnimation> | null = null

// Local progress interpolation between backend updates.
let basePos    = 0          // position_ms at the last update
let baseAt     = 0          // performance.now() at the last update
const displayMs = ref(0)
let raf = 0

const playing  = computed(() => data.value?.status === 'playing')
const muted    = computed(() => data.value?.muted ?? false)
const title    = computed(() => data.value?.title?.trim() || t.value.music.unknownTitle)
const artist   = computed(() => data.value?.artist?.trim() || t.value.music.unknownArtist)
const duration = computed(() => data.value?.duration_ms ?? 0)
const pct = computed(() => {
  const d = duration.value
  return d > 0 ? Math.min(100, (displayMs.value / d) * 100) : 0
})

function fmt(ms: number): string {
  const total = Math.max(0, Math.round(ms / 1000))
  const m = Math.floor(total / 60)
  const s = total % 60
  return `${m}:${s.toString().padStart(2, '0')}`
}

function applySnapshot(s: MediaSnapshot): void {
  data.value = s
  basePos = s.position_ms
  baseAt  = performance.now()
  displayMs.value = s.position_ms
  if (!draggingVol) vol.value = s.volume   // don't fight the user mid-drag
}

// Advance the displayed position while playing (rAF; cheap, only updates a ref).
function tick(): void {
  raf = requestAnimationFrame(tick)
  if (!playing.value) { displayMs.value = basePos; return }
  const next = basePos + (performance.now() - baseAt)
  displayMs.value = duration.value > 0 ? Math.min(duration.value, next) : next
}

// ── Controls ───────────────────────────────────────────────────────
function prev()      { void invoke('media_prev').catch(() => {}) }
function next()      { void invoke('media_next').catch(() => {}) }
function playPause() { void invoke('media_play_pause').catch(() => {}) }
function replay()    { void invoke('media_replay').catch(() => {}) }
async function toggleMute() {
  try {
    const m = await invoke<boolean>('media_toggle_mute')
    if (data.value) data.value = { ...data.value, muted: m }   // optimistic; poll re-syncs
  } catch { /* ignore */ }
}
function onVolumeInput(e: Event) {
  const v = Number((e.target as HTMLInputElement).value)
  vol.value = v
  void invoke('media_set_volume', { level: v }).catch(() => {})
}
function onVolStart() { draggingVol = true }
function onVolEnd()   { draggingVol = false }

// ── Lottie badge ───────────────────────────────────────────────────
// Spin while system audio plays; rest (reset to the first frame) when silent.
function syncLottie() {
  if (!anim) return
  if (audioOn.value) anim.play()
  else anim.stop()
}
function ensureLottie() {
  if (anim || !lottieEl.value) return
  anim = lottie.loadAnimation({
    container: lottieEl.value,
    renderer: 'svg',
    loop: true,
    autoplay: false,
    animationData: speakersAnim,
  })
  syncLottie()
}
watch(audioOn, syncLottie)

let unlisten:      UnlistenFn | null = null
let unlistenStart: UnlistenFn | null = null
let unlistenStop:  UnlistenFn | null = null

onMounted(async () => {
  try {
    const cached = await invoke<MediaSnapshot | null>('get_media')
    if (cached) applySnapshot(cached)
  } catch { /* ignore — events will populate it */ }

  // Seed + subscribe to the system-audio detector so the speakers animate in
  // lock-step with real playback (same events as the pet's headphones).
  try { audioOn.value = await invoke<boolean>('get_audio_state') } catch { /* ignore */ }
  unlistenStart = await listen('audio-started', () => { audioOn.value = true  })
  unlistenStop  = await listen('audio-stopped', () => { audioOn.value = false })

  unlisten = await listen<MediaSnapshot>('media-update', e => applySnapshot(e.payload))
  raf = requestAnimationFrame(tick)
  await nextTick()
  ensureLottie()
})

onUnmounted(() => {
  unlisten?.()
  unlistenStart?.()
  unlistenStop?.()
  cancelAnimationFrame(raf)
  anim?.destroy()
  anim = null
})
</script>

<template>
  <div
    class="music-anchor pet-ui-overlay"
    @mouseenter="hovered = true"
    @mouseleave="hovered = false"
    @mousedown.stop
    @mouseup.stop
    @click.stop
  >
    <!-- Hover panel (above the badge) -->
    <Transition name="tip">
      <div v-if="hovered" class="panel">
        <div class="meta">
          <div class="title">{{ title }}</div>
          <div class="artist">{{ artist }}</div>
        </div>
        <div class="progress">
          <div class="bar" :style="{ width: pct + '%' }" />
        </div>
        <div class="times">
          <span>{{ fmt(displayMs) }}</span>
          <span>{{ fmt(duration) }}</span>
        </div>
        <div class="controls">
          <!-- Primary transport -->
          <div class="ctrl-row">
            <button class="ctrl" :data-tip="t.music.prev" :aria-label="t.music.prev" :disabled="!data?.can_prev" @click.stop="prev">
              <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 6v12M9 12l9 6V6z"/></svg>
            </button>
            <button class="ctrl play" :data-tip="playing ? t.music.pause : t.music.play" :aria-label="playing ? t.music.pause : t.music.play" @click.stop="playPause">
              <svg v-if="playing" viewBox="0 0 24 24" aria-hidden="true"><path d="M8 5h3v14H8zM13 5h3v14h-3z"/></svg>
              <svg v-else viewBox="0 0 24 24" aria-hidden="true"><path d="M8 5l11 7-11 7z"/></svg>
            </button>
            <button class="ctrl" :data-tip="t.music.next" :aria-label="t.music.next" :disabled="!data?.can_next" @click.stop="next">
              <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M17 6v12M15 12L6 6v12z"/></svg>
            </button>
          </div>
          <!-- Secondary: replay + mute (with a vertical volume popup on hover) -->
          <div class="ctrl-row">
            <button class="ctrl small" :data-tip="t.music.replay" :aria-label="t.music.replay" @click.stop="replay">
              <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5V1L7 6l5 5V7c3.31 0 6 2.69 6 6s-2.69 6-6 6-6-2.69-6-6H4c0 4.42 3.58 8 8 8s8-3.58 8-8-3.58-8-8-8z"/></svg>
            </button>
            <div class="vol-control">
              <!-- Vertical volume slider, shown only while hovering the mute button -->
              <div class="vol-popup">
                <input
                  type="range" class="vol-slider" min="0" max="1" step="0.01"
                  :value="vol" :style="{ '--v': (vol * 100) + '%' }"
                  :aria-label="t.music.volume"
                  @input="onVolumeInput" @pointerdown="onVolStart" @pointerup="onVolEnd" @click.stop
                />
              </div>
              <button class="ctrl small mute" :class="{ active: muted }" :aria-label="muted ? t.music.unmute : t.music.mute" @click.stop="toggleMute">
                <svg v-if="muted" viewBox="0 0 24 24" aria-hidden="true"><path d="M16.5 12c0-1.77-1.02-3.29-2.5-4.03v2.21l2.45 2.45c.03-.2.05-.41.05-.63zm2.5 0c0 .94-.2 1.82-.54 2.64l1.51 1.51C20.63 14.91 21 13.5 21 12c0-4.28-2.99-7.86-7-8.77v2.06c2.89.86 5 3.54 5 6.71zM4.27 3L3 4.27 7.73 9H3v6h4l5 5v-6.73l4.25 4.25c-.67.52-1.42.93-2.25 1.18v2.06c1.38-.31 2.63-.95 3.69-1.81L19.73 21 21 19.73l-9-9L4.27 3zM12 4L9.91 6.09 12 8.18V4z"/></svg>
                <svg v-else viewBox="0 0 24 24" aria-hidden="true"><path d="M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM14 3.23v2.06c2.89.86 5 3.54 5 6.71s-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77s-2.99-7.86-7-8.77z"/></svg>
              </button>
            </div>
          </div>
        </div>
      </div>
    </Transition>

    <!-- Animated speakers badge (Lottie) — scales with the 大中小 setting -->
    <div class="badge" :style="{ width: badgePx + 'px', height: badgePx + 'px' }">
      <div ref="lottieEl" class="lottie" />
    </div>
  </div>
</template>

<style scoped>
/* ── Anchor (bottom-right; panel stacks above the badge) ─────────── */
.music-anchor {
  position: absolute;
  /* Mirror WeatherBadge's anchor so the two badges share a right edge:
     weather sits top-right, music sits bottom-right, both inset 8px. */
  bottom: 8px;
  right: 8px;
  display: flex;
  flex-direction: column;       /* panel on top, badge anchored at the bottom */
  align-items: flex-end;
  gap: 6px;
  pointer-events: auto;
  user-select: none;
  z-index: 1;
}

/* ── Badge (animated Lottie speakers) ────────────────────────────── */
.badge {
  display: flex;
  align-items: center;
  justify-content: center;
  /* width / height set inline from the 大中小 character-size setting. */
  cursor: default;
  transition: transform 160ms cubic-bezier(0.34, 1.56, 0.64, 1);
  filter: drop-shadow(0 2px 5px rgba(40, 70, 40, 0.18));
}
.music-anchor:hover .badge { transform: scale(1.10); }
.lottie { width: 100%; height: 100%; }

/* ── Panel ───────────────────────────────────────────────────────── */
.panel {
  display: flex;
  flex-direction: column;
  gap: 6px;
  /* Fit within the (small) pet window — 8px margins each side — capped so it
     doesn't get oversized on the large character setting. */
  width: calc(100vw - 12px);
  max-width: 240px;
  box-sizing: border-box;
  padding: 9px 10px;
  border-radius: 14px;
  /* Match the tarot reading panel (frosted green glass). */
  background: rgba(245, 250, 245, 0.94);
  backdrop-filter: blur(20px) saturate(160%);
  -webkit-backdrop-filter: blur(20px) saturate(160%);
  border: 1px solid rgba(148, 185, 148, 0.45);
  /* Light, soft drop shadow. */
  box-shadow: 0 3px 12px rgba(40, 70, 40, 0.10), inset 0 1px 0 rgba(255, 255, 255, 0.65);
  pointer-events: auto;
  /* Collapse toward the badge (bottom-right) on enter/leave. */
  transform-origin: bottom right;
}
.meta { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
.title {
  font-family: system-ui, "Segoe UI", "Noto Sans SC", "Noto Sans JP", sans-serif;
  font-size: 12px; font-weight: 700; color: #1a2e1a;
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}
.artist {
  font-family: system-ui, "Segoe UI", sans-serif;
  font-size: 10px; color: rgba(42, 74, 42, 0.65);
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}

/* ── Progress ────────────────────────────────────────────────────── */
.progress {
  height: 4px;
  border-radius: 999px;
  background: rgba(119, 153, 119, 0.25);
  overflow: hidden;
}
.bar {
  height: 100%;
  border-radius: 999px;
  background: linear-gradient(90deg, #779977, #5a8060);
  transition: width 240ms linear;
}
.times {
  display: flex;
  justify-content: space-between;
  font-size: 9px;
  font-variant-numeric: tabular-nums;
  color: rgba(45, 85, 45, 0.55);
}

/* ── Controls ────────────────────────────────────────────────────── */
.controls { display: flex; flex-direction: column; align-items: center; gap: 8px; }
.ctrl-row { display: flex; align-items: center; justify-content: center; gap: 12px; }
/* Frosted circular buttons — same language as the tarot controls (.ctrl). */
.ctrl {
  position: relative;
  display: flex; align-items: center; justify-content: center;
  width: 26px; height: 26px; padding: 0;
  border-radius: 50%;
  border: 1px solid rgba(119, 153, 119, 0.40);
  background: rgba(245, 250, 245, 0.92);
  color: #2a4a2a;
  cursor: pointer;
  box-shadow: 0 2px 8px rgba(40, 70, 40, 0.14);
  transition: background 150ms ease, transform 120ms ease, opacity 150ms ease;
}

/* Custom frosted tooltip above each button (replaces the native title box). */
.ctrl[data-tip]::after {
  content: attr(data-tip);
  position: absolute;
  bottom: calc(100% + 6px);
  left: 50%;
  transform: translateX(-50%);
  white-space: nowrap;
  padding: 3px 7px;
  border-radius: 7px;
  font-family: system-ui, "Segoe UI", "Noto Sans SC", "Noto Sans JP", sans-serif;
  font-size: 10px;
  font-weight: 600;
  color: #2a4a2a;
  background: rgba(245, 250, 245, 0.97);
  border: 1px solid rgba(148, 185, 148, 0.45);
  box-shadow: 0 2px 8px rgba(40, 70, 40, 0.14);
  opacity: 0;
  visibility: hidden;
  transition: opacity 120ms ease, visibility 120ms ease;
  pointer-events: none;
  z-index: 5;
}
.ctrl[data-tip]:hover::after { opacity: 1; visibility: visible; }
.ctrl svg { width: 14px; height: 14px; fill: currentColor; }
.ctrl.play {
  width: 30px; height: 30px;
  border-color: transparent;
  background: linear-gradient(135deg, #779977, #5a8060);
  color: #fff;
}
.ctrl.play svg { width: 15px; height: 15px; }
/* Secondary row (replay / mute) — slightly smaller. */
.ctrl.small { width: 24px; height: 24px; }
.ctrl.small svg { width: 13px; height: 13px; }
/* Muted state highlight. */
.ctrl.active {
  background: linear-gradient(135deg, #b06a6a, #8a5050);
  border-color: transparent;
  color: #fff;
}
.ctrl.active:hover:not(:disabled) { background: linear-gradient(135deg, #c07a7a, #8a5050); }
.ctrl:hover:not(:disabled)  { background: #fff; transform: scale(1.08); }
.ctrl.play:hover:not(:disabled) { background: linear-gradient(135deg, #80a880, #5a8060); transform: scale(1.08); }
.ctrl:active:not(:disabled) { transform: scale(0.92); }
.ctrl:disabled { opacity: 0.4; cursor: not-allowed; }

/* ── Volume: vertical slider popup, shown on mute-button hover ───── */
.vol-control { position: relative; display: flex; align-items: center; }
.vol-popup {
  position: absolute;
  bottom: calc(100% + 8px);
  left: 50%;
  transform: translateX(-50%);
  width: 26px;
  height: 92px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 10px;
  background: rgba(245, 250, 245, 0.97);
  border: 1px solid rgba(148, 185, 148, 0.45);
  box-shadow: 0 3px 12px rgba(40, 70, 40, 0.16);
  opacity: 0;
  visibility: hidden;
  transition: opacity 140ms ease, visibility 140ms ease;
  z-index: 6;
}
.vol-control:hover .vol-popup { opacity: 1; visibility: visible; }
/* A horizontal range rotated -90° → reliable vertical slider with our custom
   track gradient (left→bottom, so it fills from the bottom up). */
.vol-slider {
  -webkit-appearance: none;
  appearance: none;
  width: 72px;          /* becomes the vertical length once rotated */
  height: 5px;
  border-radius: 999px;
  cursor: pointer;
  transform: rotate(-90deg);
  background: linear-gradient(
    to right,
    #5a8060 var(--v, 0%),
    rgba(119, 153, 119, 0.25) var(--v, 0%)
  );
}
.vol-slider::-webkit-slider-thumb {
  -webkit-appearance: none;
  appearance: none;
  width: 11px;
  height: 11px;
  border-radius: 50%;
  background: #fff;
  border: 1px solid rgba(119, 153, 119, 0.5);
  box-shadow: 0 1px 3px rgba(40, 70, 40, 0.25);
  cursor: pointer;
}

/* ── Panel enter/leave ───────────────────────────────────────────── */
/* Scale from the bottom-right (the badge) and fade — no translate, so the
   controls don't appear to slide when the panel closes. */
.tip-enter-active, .tip-leave-active { transition: opacity 160ms ease, transform 160ms cubic-bezier(0.22, 1, 0.36, 1); }
.tip-enter-from, .tip-leave-to { opacity: 0; transform: scale(0.92); }
</style>
