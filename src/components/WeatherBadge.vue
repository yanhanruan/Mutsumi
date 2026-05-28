<script setup lang="ts">
/**
 * WeatherBadge — persistent emoji at the top-right of the pet.
 *
 * Subscribes to `weather-update` events emitted by the Rust backend's
 * weather fetcher. Shows the WMO-code emoji plus a tooltip with the
 * temperature and city. Hidden until the first update lands.
 */
import { onMounted, onUnmounted, ref, computed } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

interface WeatherUpdate {
  code:   number
  emoji:  string
  label:  string
  temp_c: number
  city:   string | null
}

const data = ref<WeatherUpdate | null>(null)

const tooltip = computed(() => {
  const d = data.value
  if (!d) return ''
  const t = Math.round(d.temp_c * 10) / 10
  return d.city ? `${d.label} · ${t}°C · ${d.city}` : `${d.label} · ${t}°C`
})

let unlisten: UnlistenFn | null = null

onMounted(async () => {
  unlisten = await listen<WeatherUpdate>('weather-update', e => {
    data.value = e.payload
  })
})

onUnmounted(() => {
  unlisten?.()
})
</script>

<template>
  <div v-if="data" class="weather pet-ui-overlay" :title="tooltip">
    {{ data.emoji }}
  </div>
</template>

<style scoped>
.weather {
  position: absolute;
  top: 6px;
  right: 6px;
  font-size: 18px;
  line-height: 1;
  padding: 2px 4px;
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.35);
  backdrop-filter: blur(8px) saturate(160%);
  -webkit-backdrop-filter: blur(8px) saturate(160%);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.18);
  pointer-events: auto;   /* allow hover tooltip */
  user-select: none;
}
</style>
