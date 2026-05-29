<script setup lang="ts">
/**
 * PomodoroBadge — corner overlay showing pomodoro phase + remaining time.
 *
 * On mount, pulls the current state via `get_state` so the badge appears
 * immediately even if the backend ticked before the listener registered.
 * Then subscribes to `pomodoro-tick` (every second) for live updates.
 *
 * Hidden when phase is "idle". Color encodes phase: red for focus, blue for break.
 */
import { onMounted, onUnmounted, ref, computed } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '../i18n'

interface PomodoroSnapshot {
  phase:           string
  focus_mins:      number
  break_mins:      number
  remaining_secs:  number
  running:         boolean
}

interface StateSnapshot {
  pet:      unknown
  pomodoro: PomodoroSnapshot
}

const { t } = useI18n()

const phase     = ref<string>('idle')
const remaining = ref<number>(0)
const running   = ref<boolean>(false)

const visible = computed(() => phase.value !== 'idle')

const label = computed(() => {
  const total = Math.max(0, remaining.value)
  const m = Math.floor(total / 60)
  const s = total % 60
  return `${m}:${s.toString().padStart(2, '0')}`
})

const phaseLabel = computed(() =>
  phase.value === 'break' ? t.value.pomBreak : t.value.pomFocus
)

const bgColor = computed(() =>
  phase.value === 'break'
    ? 'rgba(21, 101, 192, 0.88)'
    : 'rgba(211, 47, 47, 0.88)'
)

function applySnapshot(pom: PomodoroSnapshot) {
  phase.value     = pom.phase
  remaining.value = pom.remaining_secs
  running.value   = pom.running
}

let unlisten: UnlistenFn | null = null

onMounted(async () => {
  // Pull cached state immediately so badge shows without waiting for next tick.
  try {
    const s = await invoke<StateSnapshot>('get_state')
    applySnapshot(s.pomodoro)
  } catch { /* ignore — live ticks will update */ }

  unlisten = await listen<PomodoroSnapshot>('pomodoro-tick', e => {
    applySnapshot(e.payload)
  })
})

onUnmounted(() => {
  unlisten?.()
})
</script>

<template>
  <Transition name="badge">
    <div
      v-if="visible"
      class="badge pet-ui-overlay"
      :style="{ background: bgColor }"
      :class="{ paused: !running }"
    >
      <span class="phase-label">{{ phaseLabel }}</span>
      <span class="time">{{ label }}</span>
      <span v-if="!running" class="pause-dot">⏸</span>
    </div>
  </Transition>
</template>

<style scoped>
.badge {
  position: absolute;
  top: 6px;
  left: 6px;
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 3px 8px;
  border-radius: 8px;
  color: white;
  font-family: system-ui, "Segoe UI", sans-serif;
  font-size: 11px;
  font-weight: 600;
  box-shadow: 0 1px 6px rgba(0, 0, 0, 0.28);
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
  border: 1px solid rgba(255, 255, 255, 0.20);
  pointer-events: none;
  user-select: none;
  white-space: nowrap;
}

.phase-label {
  font-size: 9px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.4px;
  opacity: 0.80;
}

.time {
  font-variant-numeric: tabular-nums;
  letter-spacing: 0.5px;
}

.pause-dot {
  font-size: 9px;
  opacity: 0.75;
}

.paused {
  opacity: 0.65;
}

/* Slide-in from top-left */
.badge-enter-active,
.badge-leave-active {
  transition: opacity 200ms ease, transform 200ms ease;
}
.badge-enter-from,
.badge-leave-to {
  opacity: 0;
  transform: translateX(-6px) scale(0.92);
}
</style>
