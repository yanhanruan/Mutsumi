<script setup lang="ts">
/**
 * PomodoroBadge — corner overlay showing pomodoro phase + remaining time.
 *
 * Subscribes to the `pomodoro-tick` event from the Rust backend. Hidden
 * when phase is "idle". Color encodes phase: red for focus, blue for break.
 */
import { onMounted, onUnmounted, ref, computed } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

interface PomodoroSnapshot {
  phase:           string
  focus_mins:      number
  break_mins:      number
  remaining_secs:  number
  running:         boolean
}

const phase     = ref<string>('idle')
const remaining = ref<number>(0)

const visible = computed(() => phase.value !== 'idle')

const label = computed(() => {
  const total = Math.max(0, remaining.value)
  const m = Math.floor(total / 60)
  const s = total % 60
  return `${m}:${s.toString().padStart(2, '0')}`
})

const bgColor = computed(() => phase.value === 'break' ? '#1565C0' : '#D32F2F')

let unlisten: UnlistenFn | null = null

onMounted(async () => {
  unlisten = await listen<PomodoroSnapshot>('pomodoro-tick', e => {
    phase.value     = e.payload.phase
    remaining.value = e.payload.remaining_secs
  })
})

onUnmounted(() => {
  unlisten?.()
})
</script>

<template>
  <div v-if="visible" class="badge pet-ui-overlay" :style="{ backgroundColor: bgColor }">
    {{ label }}
  </div>
</template>

<style scoped>
.badge {
  position: absolute;
  top: 6px;
  left: 6px;
  padding: 2px 7px;
  border-radius: 6px;
  color: white;
  font-family: system-ui, "Segoe UI", monospace;
  font-size: 11px;
  font-weight: 600;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.25);
  pointer-events: none;
  user-select: none;
}
</style>
