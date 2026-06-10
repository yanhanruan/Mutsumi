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
import { onMounted, onUnmounted, ref, computed } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '../i18n'

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
}

const { t } = useI18n()

const data    = ref<MediaSnapshot | null>(null)
const hovered = ref(false)

// Local progress interpolation between backend updates.
let basePos    = 0          // position_ms at the last update
let baseAt     = 0          // performance.now() at the last update
const displayMs = ref(0)
let raf = 0

const playing  = computed(() => data.value?.status === 'playing')
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

let unlisten: UnlistenFn | null = null

onMounted(async () => {
  try {
    const cached = await invoke<MediaSnapshot | null>('get_media')
    if (cached) applySnapshot(cached)
  } catch { /* ignore — events will populate it */ }

  unlisten = await listen<MediaSnapshot>('media-update', e => applySnapshot(e.payload))
  raf = requestAnimationFrame(tick)
})

onUnmounted(() => {
  unlisten?.()
  cancelAnimationFrame(raf)
})
</script>

<template>
  <div
    v-if="data?.active"
    class="music-anchor pet-ui-overlay"
    @mouseenter="hovered = true"
    @mouseleave="hovered = false"
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
      </div>
    </Transition>

    <!-- Compact equalizer badge -->
    <div class="badge" :class="{ playing }">
      <span class="eq"><i /><i /><i /></span>
    </div>
  </div>
</template>

<style scoped>
/* ── Anchor (bottom-right; panel stacks above the badge) ─────────── */
.music-anchor {
  position: absolute;
  bottom: 8px;
  right: 6px;
  display: flex;
  flex-direction: column;       /* panel on top, badge anchored at the bottom */
  align-items: flex-end;
  gap: 6px;
  pointer-events: auto;
  user-select: none;
  z-index: 1;
}

/* ── Badge ───────────────────────────────────────────────────────── */
.badge {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: 10px;
  background: rgba(255, 255, 255, 0.20);
  backdrop-filter: blur(14px) saturate(180%);
  -webkit-backdrop-filter: blur(14px) saturate(180%);
  border: 1px solid rgba(255, 255, 255, 0.38);
  box-shadow:
    0 3px 10px rgba(40, 70, 40, 0.14),
    inset 0 1px 0 rgba(255, 255, 255, 0.55);
  transition: transform 160ms cubic-bezier(0.34, 1.56, 0.64, 1), background 160ms ease;
  cursor: default;
}
.music-anchor:hover .badge {
  transform: scale(1.10);
  background: rgba(255, 255, 255, 0.28);
}

/* equalizer bars */
.eq { display: flex; align-items: flex-end; gap: 2px; height: 14px; }
.eq i {
  width: 3px;
  height: 40%;
  border-radius: 1px;
  background: #2a6a4a;
}
.badge.playing .eq i { animation: eq 900ms ease-in-out infinite; }
.badge.playing .eq i:nth-child(2) { animation-delay: 150ms; }
.badge.playing .eq i:nth-child(3) { animation-delay: 300ms; }
@keyframes eq {
  0%, 100% { height: 30%; }
  50%      { height: 100%; }
}

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
.controls { display: flex; align-items: center; justify-content: center; gap: 14px; }
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
  width: 32px; height: 32px;
  border-color: transparent;
  background: linear-gradient(135deg, #779977, #5a8060);
  color: #fff;
}
.ctrl.play svg { width: 16px; height: 16px; }
.ctrl:hover:not(:disabled)  { background: #fff; transform: scale(1.08); }
.ctrl.play:hover:not(:disabled) { background: linear-gradient(135deg, #80a880, #5a8060); transform: scale(1.08); }
.ctrl:active:not(:disabled) { transform: scale(0.92); }
.ctrl:disabled { opacity: 0.4; cursor: not-allowed; }

/* ── Panel enter/leave ───────────────────────────────────────────── */
/* Scale from the bottom-right (the badge) and fade — no translate, so the
   controls don't appear to slide when the panel closes. */
.tip-enter-active, .tip-leave-active { transition: opacity 160ms ease, transform 160ms cubic-bezier(0.22, 1, 0.36, 1); }
.tip-enter-from, .tip-leave-to { opacity: 0; transform: scale(0.92); }
</style>
