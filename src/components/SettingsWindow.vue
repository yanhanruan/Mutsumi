<script setup lang="ts">
/**
 * SettingsWindow — Pomodoro durations + reset + launch-on-startup.
 *
 * Loaded by App.vue when URL is `?window=settings`. Configured as a separate
 * Tauri window in tauri.conf.json (label "settings", hidden by default).
 *
 * State is fetched on mount via the `get_state` command, saved via
 * `pom_set_durations`. Reset wipes pet energy/affection via `pet_reset`.
 * Launch-on-startup uses @tauri-apps/plugin-autostart.
 */
import { onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { enable, disable, isEnabled } from '@tauri-apps/plugin-autostart'

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
    status.value = autostart.value ? 'Launch on startup enabled.' : 'Launch on startup disabled.'
    setTimeout(() => { status.value = '' }, 1500)
  } catch (e) {
    console.error('autostart toggle failed', e)
    // Revert the checkbox if the OS call failed.
    autostart.value = !autostart.value
  }
}

async function save() {
  await invoke('pom_set_durations', {
    focusMins: Math.max(1, Math.round(focusMins.value)),
    breakMins: Math.max(1, Math.round(breakMins.value)),
  })
  status.value = 'Saved.'
  setTimeout(() => { status.value = '' }, 1500)
}

async function reset() {
  await invoke('pet_reset')
  await refresh()
  status.value = 'Reset.'
  setTimeout(() => { status.value = '' }, 1500)
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
  <div class="settings">
    <h2>Mutsumi Settings</h2>

    <section>
      <h3>Pomodoro</h3>
      <div class="row">
        <label>Focus minutes</label>
        <input type="number" v-model="focusMins" min="1" max="180" />
      </div>
      <div class="row">
        <label>Break minutes</label>
        <input type="number" v-model="breakMins" min="1" max="60" />
      </div>
    </section>

    <section>
      <h3>Pet status</h3>
      <div class="row stat"><label>Energy</label>    <span>{{ energy }}</span></div>
      <div class="row stat"><label>Affection</label> <span>{{ affection }}</span></div>
      <div class="row stat"><label>Mood</label>      <span>{{ mood }}</span></div>
    </section>

    <section>
      <h3>System</h3>
      <div class="row toggle-row">
        <label for="autostart-toggle">Launch on startup</label>
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
    </section>

    <div class="actions">
      <button class="primary" @click="save">Save</button>
      <button @click="reset">Reset pet</button>
      <button @click="close">Close</button>
      <span class="status">{{ status }}</span>
    </div>
  </div>
</template>

<style scoped>
.settings {
  padding: 16px 18px;
  font-family: system-ui, "Segoe UI", sans-serif;
  font-size: 13px;
  color: #2a2533;
  background: #fafaff;
  min-height: 100vh;
  box-sizing: border-box;
}
h2 {
  margin: 0 0 10px;
  font-size: 15px;
  font-weight: 600;
}
h3 {
  margin: 10px 0 6px;
  font-size: 12px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: #7a6e8a;
}
section {
  margin-bottom: 8px;
}
.row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 4px 0;
}
.row label { color: #4a4055; }
.row input[type="number"] {
  width: 80px;
  padding: 4px 8px;
  font-size: 13px;
  border: 1px solid #d4cde0;
  border-radius: 6px;
  background: white;
}
.row.stat span { color: #7a6e8a; }

/* ── Toggle switch ───────────────────────────────── */
.toggle-row {
  padding: 6px 0;
}
.toggle {
  position: relative;
  display: inline-block;
  width: 38px;
  height: 22px;
  cursor: pointer;
}
.toggle input {
  opacity: 0;
  width: 0;
  height: 0;
}
.slider {
  position: absolute;
  inset: 0;
  border-radius: 11px;
  background: #d4cde0;
  transition: background 200ms ease;
}
.slider::before {
  content: '';
  position: absolute;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: white;
  top: 3px;
  left: 3px;
  transition: transform 200ms ease;
  box-shadow: 0 1px 3px rgba(0,0,0,.25);
}
.toggle input:checked + .slider {
  background: #6750a4;
}
.toggle input:checked + .slider::before {
  transform: translateX(16px);
}

/* ── Actions ─────────────────────────────────────── */
.actions {
  display: flex;
  gap: 8px;
  align-items: center;
  margin-top: 12px;
}
button {
  padding: 6px 12px;
  font-size: 12px;
  border: 1px solid #d4cde0;
  border-radius: 6px;
  background: white;
  cursor: pointer;
  color: #2a2533;
}
button:hover { background: #f0ecf6; }
button.primary { background: #6750a4; color: white; border-color: #6750a4; }
button.primary:hover { background: #5a4593; }
.status { font-size: 11px; color: #6750a4; margin-left: auto; }
</style>
