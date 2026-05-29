<script setup lang="ts">
/**
 * PomodoroBadge — corner overlay showing pomodoro phase + remaining time.
 *
 * Redesigned with #779977 (sage green) as the primary theme colour.
 * Focus → sage green gradient. Break → complementary teal gradient.
 *
 * On mount, pulls the current state via `get_state` so the badge appears
 * immediately even if the backend ticked before the listener registered.
 * Then subscribes to `pomodoro-tick` (every second) for live updates.
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

const isBreak = computed(() => phase.value === 'break')

const label = computed(() => {
  const total = Math.max(0, remaining.value)
  const m = Math.floor(total / 60)
  const s = total % 60
  return `${m}:${s.toString().padStart(2, '0')}`
})

const phaseLabel = computed(() =>
  isBreak.value ? t.value.pomBreak : t.value.pomFocus
)

function applySnapshot(pom: PomodoroSnapshot) {
  phase.value     = pom.phase
  remaining.value = pom.remaining_secs
  running.value   = pom.running
}

let unlisten: UnlistenFn | null = null

onMounted(async () => {
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
      :class="{ break: isBreak, paused: !running }"
    >
      <!-- Phase dot -->
      <span class="phase-dot" />

      <!-- Phase label -->
      <span class="phase-label">{{ phaseLabel }}</span>

      <!-- Separator -->
      <span class="sep">·</span>

      <!-- Countdown -->
      <span class="time">{{ label }}</span>

      <!-- Pause indicator -->
      <span v-if="!running" class="pause-icon">⏸</span>
    </div>
  </Transition>
</template>

<style scoped>
/* ── Base badge ─────────────────────────────────────────────── */
.badge {
  position: absolute;
  top: 8px;
  left: 8px;

  display: flex;
  align-items: center;
  gap: 5px;
  padding: 4px 10px 4px 8px;
  border-radius: 20px;

  /* Focus: sage green — the #779977 theme */
  background: linear-gradient(
    135deg,
    rgba(105, 140, 105, 0.92) 0%,
    rgba(82,  115, 82,  0.92) 100%
  );

  color: rgba(255, 255, 255, 0.95);
  font-family: system-ui, "Segoe UI", sans-serif;

  /* Frosted glass effect */
  backdrop-filter: blur(14px) saturate(160%);
  -webkit-backdrop-filter: blur(14px) saturate(160%);

  border: 1px solid rgba(180, 220, 180, 0.40);

  pointer-events: none;
  user-select: none;
  white-space: nowrap;

  /* Smooth colour transitions between phases */
  transition: background 600ms ease, border-color 600ms ease;
}

/* ── Break variant ──────────────────────────────────────────── */
.badge.break {
  background: linear-gradient(
    135deg,
    rgba(75, 125, 118, 0.92) 0%,
    rgba(55, 100,  94, 0.92) 100%
  );
  border-color: rgba(160, 215, 210, 0.40);
}

/* ── Paused state ───────────────────────────────────────────── */
.badge.paused {
  opacity: 0.72;
}

/* ── Phase dot ──────────────────────────────────────────────── */
.phase-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.85);
  flex-shrink: 0;
  /* Pulse animation while active */
  animation: pulse-dot 2.4s ease-in-out infinite;
}

.paused .phase-dot {
  animation: none;
  opacity: 0.55;
}

@keyframes pulse-dot {
  0%, 100% { opacity: 0.85; transform: scale(1);    }
  50%       { opacity: 0.50; transform: scale(0.72); }
}

/* ── Phase label ────────────────────────────────────────────── */
.phase-label {
  font-size: 9.5px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  opacity: 0.82;
}

/* ── Separator ──────────────────────────────────────────────── */
.sep {
  font-size: 9px;
  opacity: 0.45;
  margin: 0 -1px;
}

/* ── Countdown ──────────────────────────────────────────────── */
.time {
  font-size: 12px;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
  letter-spacing: 0.3px;
}

/* ── Pause icon ─────────────────────────────────────────────── */
.pause-icon {
  font-size: 9px;
  opacity: 0.75;
  margin-left: -1px;
}

/* ── Enter/leave ────────────────────────────────────────────── */
.badge-enter-active,
.badge-leave-active {
  transition: opacity 220ms ease, transform 220ms ease;
}
.badge-enter-from,
.badge-leave-to {
  opacity: 0;
  transform: translateX(-8px) scale(0.88);
}
</style>
