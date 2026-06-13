<script setup lang="ts">
/**
 * WeatherBadge — SVG weather icon pinned to the top-right of the pet.
 *
 * On mount, pulls the already-cached weather via `get_weather` so the icon
 * appears immediately even if the Rust thread emitted before the listener
 * was registered. Then subscribes to `weather-update` for 30-min refreshes.
 */
import { onMounted, onUnmounted, ref, computed } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import WeatherIcon from './WeatherIcon.vue'
import { useAppConfig, type CharacterSize } from '../composables/useAppConfig'

interface WeatherUpdate {
  code:   number
  label:  string
  temp_c: number
  city:   string | null
}

const { config } = useAppConfig()

// Badge size mirrors MusicBadge: scales with the 大中小 character-size setting.
const BADGE_PX: Record<CharacterSize, number> = {
  small:  18,
  medium: 24,
  large:  30,
}
const badgePx = computed(() => BADGE_PX[config.value.characterSize])
// Icon fills ~71 % of the badge (mirrors the original 20 px icon inside the
// original 28 px badge).  Rounded to the nearest integer for crisp rendering.
const iconPx  = computed(() => Math.round(badgePx.value * 20 / 28))

const data    = ref<WeatherUpdate | null>(null)
const hovered = ref(false)

const tempLabel = computed(() => {
  if (!data.value) return ''
  return `${Math.round(data.value.temp_c * 10) / 10}°C`
})

let unlisten: UnlistenFn | null = null

onMounted(async () => {
  // Pull cached value immediately — avoids missing the event when the
  // Rust thread fetches faster than the frontend registers its listener.
  const cached = await invoke<WeatherUpdate | null>('get_weather')
  if (cached) data.value = cached

  // Keep listening for the next 30-min refresh.
  unlisten = await listen<WeatherUpdate>('weather-update', e => {
    data.value = e.payload
  })
})

onUnmounted(() => {
  unlisten?.()
})
</script>

<template>
  <div
    class="weather-anchor pet-ui-overlay"
    @mouseenter="hovered = true"
    @mouseleave="hovered = false"
  >
    <!-- Badge pill — always visible; pulsing dot while fetch is pending -->
    <div class="badge" :style="{ width: badgePx + 'px', height: badgePx + 'px' }">
      <WeatherIcon v-if="data" :code="data.code" :size="iconPx" />
      <span v-else class="loading-dot" />
    </div>

    <!-- Custom glass tooltip — only shown once weather data has loaded -->
    <Transition name="tip">
      <div v-if="hovered && data" class="tooltip" role="tooltip">
        <span class="tip-temp">{{ tempLabel }}</span>
        <span class="tip-divider" />
        <span class="tip-label">{{ data.label }}</span>
        <span v-if="data.city" class="tip-city">{{ data.city }}</span>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
/* ── Anchor (positions badge + tooltip together) ─────── */
.weather-anchor {
  position: absolute;
  top: 8px;
  right: 8px;
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 5px;
  pointer-events: auto;
  user-select: none;
  z-index: 1;
}

/* ── Badge ───────────────────────────────────────────── */
.badge {
  display: flex;
  align-items: center;
  justify-content: center;
  /* width / height set inline from the 大中小 character-size setting. */
  border-radius: 10px;
  background: rgba(255, 255, 255, 0.20);
  backdrop-filter: blur(14px) saturate(180%);
  -webkit-backdrop-filter: blur(14px) saturate(180%);
  border: 1px solid rgba(255, 255, 255, 0.38);
  box-shadow:
    0 2px 8px rgba(0, 0, 0, 0.10),
    inset 0 1px 0 rgba(255, 255, 255, 0.50);
  transition:
    transform  160ms cubic-bezier(0.34, 1.56, 0.64, 1),
    box-shadow 160ms ease,
    background 160ms ease;
  cursor: default;
}

.weather-anchor:hover .badge {
  transform: scale(1.10);
  background: rgba(255, 255, 255, 0.28);
  box-shadow:
    0 4px 14px rgba(0, 0, 0, 0.14),
    inset 0 1px 0 rgba(255, 255, 255, 0.60);
}

/* ── Tooltip ─────────────────────────────────────────── */
.tooltip {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 3px;
  padding: 8px 10px;
  border-radius: 11px;
  background: rgba(255, 255, 255, 0.20);
  backdrop-filter: blur(14px) saturate(180%);
  -webkit-backdrop-filter: blur(14px) saturate(180%);
  border: 1px solid rgba(255, 255, 255, 0.38);
  box-shadow:
    0 4px 16px rgba(0, 0, 0, 0.12),
    inset 0 1px 0 rgba(255, 255, 255, 0.50);
  pointer-events: none;
  white-space: nowrap;
}

.tip-temp {
  font-family: system-ui, "Segoe UI", sans-serif;
  font-size: 13px;
  font-weight: 650;
  color: #2a1a3a;
  letter-spacing: 0.01em;
}

.tip-divider {
  width: 100%;
  height: 1px;
  background: rgba(42, 26, 58, 0.12);
  border-radius: 1px;
}

.tip-label {
  font-family: system-ui, "Segoe UI", sans-serif;
  font-size: 10px;
  color: rgba(42, 26, 58, 0.65);
  letter-spacing: 0.02em;
}

.tip-city {
  font-family: system-ui, "Segoe UI", sans-serif;
  font-size: 9px;
  color: rgba(42, 26, 58, 0.42);
  letter-spacing: 0.03em;
}

/* ── Loading state (data not yet arrived) ────────────── */
.loading-dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.50);
  animation: pulse-dot 1.6s ease-in-out infinite;
}
@keyframes pulse-dot {
  0%, 100% { opacity: 0.25; transform: scale(0.85); }
  50%       { opacity: 0.75; transform: scale(1.10); }
}

/* ── Tooltip enter/leave ─────────────────────────────── */
.tip-enter-active,
.tip-leave-active {
  transition: opacity 140ms ease, transform 140ms ease;
}
.tip-enter-from,
.tip-leave-to {
  opacity: 0;
  transform: translateY(-3px) scale(0.97);
}
</style>
