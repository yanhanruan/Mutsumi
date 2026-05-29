<script setup lang="ts">
/**
 * SettingsWindow — Pomodoro durations + reset + launch-on-startup.
 *
 * Glassmorphic design: frosted-card sections over a soft purple gradient
 * background, matching the weather badge's visual language.
 */
import { onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { enable, disable, isEnabled } from '@tauri-apps/plugin-autostart'
import { useI18n } from '../i18n'

interface PetStateSnapshot {
  energy:    number
  affection: number
  mood:      string
}

interface PomodoroSnapshot {
  phase:           string
  focus_mins:      number
  break_mins:      number
  remaining_secs:  number
  running:         boolean
}

interface StateSnapshot {
  pet:      PetStateSnapshot
  pomodoro: PomodoroSnapshot
}

const { t } = useI18n()

const focusMins   = ref(25)
const breakMins   = ref(5)
const energy      = ref(100)
const affection   = ref(50)
const mood        = ref('content')
const status      = ref('')
const autostart   = ref(false)

async function refresh() {
  const s = await invoke<StateSnapshot>('get_state')
  focusMins.value = s.pomodoro.focus_mins
  breakMins.value = s.pomodoro.break_mins
  energy.value    = Math.round(s.pet.energy)
  affection.value = Math.round(s.pet.affection)
  mood.value      = s.pet.mood
}

async function refreshAutostart() {
  try {
    autostart.value = await isEnabled()
  } catch {
    autostart.value = false
  }
}

async function toggleAutostart() {
  try {
    if (autostart.value) {
      await enable()
    } else {
      await disable()
    }
    status.value = autostart.value ? t.value.autostartOnMsg : t.value.autostartOffMsg
    setTimeout(() => { status.value = '' }, 1800)
  } catch (e) {
    console.error('autostart toggle failed', e)
    autostart.value = !autostart.value
  }
}

async function save() {
  await invoke('pom_set_durations', {
    focusMins: Math.max(1, Math.round(focusMins.value)),
    breakMins: Math.max(1, Math.round(breakMins.value)),
  })
  status.value = t.value.savedMsg
  setTimeout(() => { status.value = '' }, 1800)
}

async function reset() {
  await invoke('pet_reset')
  await refresh()
  status.value = t.value.resetMsg
  setTimeout(() => { status.value = '' }, 1800)
}

async function close() {
  await getCurrentWindow().hide()
}

onMounted(async () => {
  await refresh()
  await refreshAutostart()
})
</script>

<template>
  <div class="glass-root">
    <!-- Decorative orbs -->
    <div class="orb orb-1" />
    <div class="orb orb-2" />

    <div class="container">
      <!-- Header -->
      <div class="header">
        <span class="logo">🌸</span>
        <h1>Mutsumi</h1>
        <span class="version">{{ t.settingsTitle }}</span>
      </div>

      <!-- Pomodoro card -->
      <div class="card">
        <h3>{{ t.pomodoro }}</h3>
        <div class="row">
          <label>{{ t.focusLabel }}</label>
          <div class="input-group">
            <input type="number" v-model="focusMins" min="1" max="180" />
            <span class="unit">{{ t.minuteUnit }}</span>
          </div>
        </div>
        <div class="row">
          <label>{{ t.breakLabel }}</label>
          <div class="input-group">
            <input type="number" v-model="breakMins" min="1" max="60" />
            <span class="unit">{{ t.minuteUnit }}</span>
          </div>
        </div>
      </div>

      <!-- Pet status card -->
      <div class="card">
        <h3>{{ t.petStatus }}</h3>
        <div class="stat-grid">
          <div class="stat-item">
            <span class="stat-icon">⚡</span>
            <div class="stat-body">
              <span class="stat-label">{{ t.energy }}</span>
              <div class="bar-track">
                <div class="bar-fill energy" :style="{ width: energy + '%' }" />
              </div>
            </div>
            <span class="stat-val">{{ energy }}</span>
          </div>
          <div class="stat-item">
            <span class="stat-icon">💜</span>
            <div class="stat-body">
              <span class="stat-label">{{ t.affection }}</span>
              <div class="bar-track">
                <div class="bar-fill affection" :style="{ width: affection + '%' }" />
              </div>
            </div>
            <span class="stat-val">{{ affection }}</span>
          </div>
          <div class="stat-item mood-row">
            <span class="stat-icon">✨</span>
            <span class="stat-label">{{ t.mood }}</span>
            <span class="mood-badge">{{ mood }}</span>
          </div>
        </div>
      </div>

      <!-- System card -->
      <div class="card">
        <h3>{{ t.system }}</h3>
        <div class="row">
          <label for="autostart-toggle">{{ t.launchOnStartup }}</label>
          <label class="toggle">
            <input
              id="autostart-toggle"
              type="checkbox"
              v-model="autostart"
              @change="toggleAutostart"
            />
            <span class="slider" />
          </label>
        </div>
      </div>

      <!-- Actions -->
      <div class="actions">
        <button class="btn primary" @click="save">{{ t.save }}</button>
        <button class="btn" @click="reset">{{ t.resetPet }}</button>
        <button class="btn ghost" @click="close">{{ t.close }}</button>
      </div>

      <Transition name="fade">
        <p v-if="status" class="status-toast">{{ status }}</p>
      </Transition>
    </div>
  </div>
</template>

<style scoped>
/* ── Root / background ───────────────────────────────── */
.glass-root {
  position: relative;
  width: 100%;
  min-height: 100vh;
  background: linear-gradient(135deg, #e8e0f5 0%, #d4c8ef 40%, #c9d8f5 100%);
  overflow: hidden;
  font-family: system-ui, "Segoe UI", sans-serif;
  font-size: 13px;
  color: #2a1a3a;
  box-sizing: border-box;
}

/* Decorative blurred orbs */
.orb {
  position: absolute;
  border-radius: 50%;
  filter: blur(60px);
  pointer-events: none;
  z-index: 0;
}
.orb-1 {
  width: 220px; height: 220px;
  background: rgba(167, 139, 250, 0.35);
  top: -60px; right: -60px;
}
.orb-2 {
  width: 180px; height: 180px;
  background: rgba(147, 197, 253, 0.30);
  bottom: -50px; left: -40px;
}

/* ── Content container ───────────────────────────────── */
.container {
  position: relative;
  z-index: 1;
  padding: 18px 16px 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

/* ── Header ──────────────────────────────────────────── */
.header {
  display: flex;
  align-items: center;
  gap: 7px;
  margin-bottom: 2px;
}
.logo { font-size: 18px; line-height: 1; }
h1 {
  margin: 0;
  font-size: 16px;
  font-weight: 700;
  color: #3d1f6a;
  letter-spacing: -0.3px;
}
.version {
  font-size: 10px;
  font-weight: 500;
  color: rgba(61, 31, 106, 0.45);
  text-transform: uppercase;
  letter-spacing: 0.6px;
  margin-top: 1px;
}

/* ── Glass card ──────────────────────────────────────── */
.card {
  background: rgba(255, 255, 255, 0.38);
  backdrop-filter: blur(16px) saturate(180%);
  -webkit-backdrop-filter: blur(16px) saturate(180%);
  border: 1px solid rgba(255, 255, 255, 0.55);
  border-radius: 14px;
  padding: 11px 13px;
  box-shadow:
    0 2px 10px rgba(80, 40, 120, 0.08),
    inset 0 1px 0 rgba(255, 255, 255, 0.65);
}

h3 {
  margin: 0 0 8px;
  font-size: 10px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.7px;
  color: rgba(61, 31, 106, 0.50);
}

/* ── Row ─────────────────────────────────────────────── */
.row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 3px 0;
}
.row + .row { margin-top: 5px; }
.row label { color: #4a2d6a; font-weight: 500; font-size: 12.5px; }

/* ── Number input ────────────────────────────────────── */
.input-group {
  display: flex;
  align-items: center;
  gap: 5px;
}
.input-group input {
  width: 54px;
  padding: 4px 8px;
  font-size: 13px;
  font-weight: 600;
  border: 1.5px solid rgba(167, 139, 250, 0.40);
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.70);
  color: #2a1a3a;
  text-align: center;
  outline: none;
  transition: border-color 150ms ease, box-shadow 150ms ease;
}
.input-group input:focus {
  border-color: #7c5cbf;
  box-shadow: 0 0 0 3px rgba(124, 92, 191, 0.18);
}
.unit {
  font-size: 11px;
  color: rgba(61, 31, 106, 0.45);
  font-weight: 500;
}

/* ── Stats ───────────────────────────────────────────── */
.stat-grid { display: flex; flex-direction: column; gap: 7px; }
.stat-item {
  display: flex;
  align-items: center;
  gap: 7px;
}
.stat-icon { font-size: 13px; line-height: 1; flex-shrink: 0; }
.stat-body { flex: 1; display: flex; flex-direction: column; gap: 2px; }
.stat-label {
  font-size: 11px;
  color: rgba(61, 31, 106, 0.60);
  font-weight: 500;
}
.stat-val {
  font-size: 12px;
  font-weight: 700;
  color: #4a2d6a;
  min-width: 28px;
  text-align: right;
}
.bar-track {
  width: 100%;
  height: 4px;
  background: rgba(0, 0, 0, 0.09);
  border-radius: 2px;
  overflow: hidden;
}
.bar-fill {
  height: 100%;
  border-radius: 2px;
  transition: width 600ms ease;
}
.bar-fill.energy    { background: linear-gradient(90deg, #a78bfa, #7c5cbf); }
.bar-fill.affection { background: linear-gradient(90deg, #f0abfc, #c084fc); }

.mood-row { align-items: center; }
.mood-badge {
  margin-left: auto;
  padding: 2px 9px;
  border-radius: 20px;
  background: rgba(167, 139, 250, 0.22);
  border: 1px solid rgba(167, 139, 250, 0.38);
  font-size: 11px;
  font-weight: 600;
  color: #5b3a9e;
  text-transform: capitalize;
}

/* ── Toggle switch ───────────────────────────────────── */
.toggle {
  position: relative;
  display: inline-block;
  width: 40px;
  height: 22px;
  cursor: pointer;
  flex-shrink: 0;
}
.toggle input { opacity: 0; width: 0; height: 0; }
.slider {
  position: absolute;
  inset: 0;
  border-radius: 11px;
  background: rgba(0, 0, 0, 0.15);
  transition: background 220ms ease;
  border: 1px solid rgba(255, 255, 255, 0.40);
}
.slider::before {
  content: '';
  position: absolute;
  width: 16px; height: 16px;
  border-radius: 50%;
  background: white;
  top: 2px; left: 2px;
  transition: transform 220ms cubic-bezier(0.34, 1.56, 0.64, 1);
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.22);
}
.toggle input:checked + .slider {
  background: linear-gradient(135deg, #8b5cf6, #6d28d9);
}
.toggle input:checked + .slider::before {
  transform: translateX(18px);
}

/* ── Action buttons ──────────────────────────────────── */
.actions {
  display: flex;
  gap: 7px;
  align-items: center;
  margin-top: 2px;
}
.btn {
  padding: 6px 14px;
  font-size: 12px;
  font-weight: 600;
  border-radius: 9px;
  cursor: pointer;
  border: 1.5px solid rgba(255, 255, 255, 0.55);
  background: rgba(255, 255, 255, 0.45);
  backdrop-filter: blur(10px);
  -webkit-backdrop-filter: blur(10px);
  color: #3d1f6a;
  transition: background 150ms ease, transform 100ms ease, box-shadow 150ms ease;
  box-shadow: 0 1px 4px rgba(80, 40, 120, 0.10);
}
.btn:hover {
  background: rgba(255, 255, 255, 0.65);
  box-shadow: 0 2px 8px rgba(80, 40, 120, 0.14);
}
.btn:active { transform: scale(0.96); }

.btn.primary {
  background: linear-gradient(135deg, #8b5cf6, #6d28d9);
  border-color: transparent;
  color: white;
  box-shadow: 0 2px 8px rgba(109, 40, 217, 0.35);
}
.btn.primary:hover {
  background: linear-gradient(135deg, #9d6ffe, #7c3beb);
  box-shadow: 0 3px 12px rgba(109, 40, 217, 0.45);
}
.btn.ghost {
  background: transparent;
  border-color: rgba(61, 31, 106, 0.25);
  color: rgba(61, 31, 106, 0.65);
}
.btn.ghost:hover {
  background: rgba(255, 255, 255, 0.35);
}

/* ── Status toast ────────────────────────────────────── */
.status-toast {
  margin: 0;
  text-align: center;
  font-size: 11px;
  font-weight: 500;
  color: #6d28d9;
  padding: 4px 0;
}
.fade-enter-active, .fade-leave-active {
  transition: opacity 250ms ease;
}
.fade-enter-from, .fade-leave-to {
  opacity: 0;
}
</style>
