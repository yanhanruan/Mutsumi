<script setup lang="ts">
/**
 * SettingsWindow — Pomodoro durations, pet status, system settings.
 *
 * Frameless (decorations:false, transparent:true) — the entire UI is
 * painted by Vue. A custom titlebar at the top carries the drag region
 * and OS window-control buttons (minimize / maximize / close).
 *
 * Primary theme: #779977 (sage green).
 */
import { onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { enable, disable, isEnabled } from '@tauri-apps/plugin-autostart'
import { useI18n, setLocale, type Locale } from '../i18n'
import { useAppConfig, type CharacterSize } from '../composables/useAppConfig'
import { useWeatherAvailable } from '../composables/useWeatherAvailable'

// ── Types ──────────────────────────────────────────────────────────

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

// ── State ──────────────────────────────────────────────────────────

const { t, locale } = useI18n()
const { config, updateConfig } = useAppConfig()

// ── Language (manual override) ─────────────────────────────────────
// Each language is shown in its own name (endonym) regardless of the
// current UI locale, so the picker is recognisable in any language.
const LANG_OPTIONS: { key: Locale; label: string }[] = [
  { key: 'en', label: 'English' },
  { key: 'zh', label: '中文' },
  { key: 'ja', label: '日本語' },
]

async function setLanguage(l: Locale) {
  setLocale(l)                              // switch the UI immediately
  await updateConfig({ language: l })       // persist + broadcast to the pet window
  try {
    await invoke('set_tray_locale', { locale: l })   // relocalise the tray menu
  } catch { /* tray update is best-effort */ }
}

const focusMins    = ref(25)
const breakMins    = ref(5)
const energy       = ref(100)
const affection    = ref(50)
const mood         = ref('content')
const status       = ref('')
const autostart    = ref(false)

// ── Character size (Task 3) ────────────────────────────────────────
const SIZE_OPTIONS: { key: CharacterSize; labelKey: 'charSizeSmall' | 'charSizeMedium' | 'charSizeLarge' }[] = [
  { key: 'small',  labelKey: 'charSizeSmall'  },
  { key: 'medium', labelKey: 'charSizeMedium' },
  { key: 'large',  labelKey: 'charSizeLarge'  },
]

const localSize = ref<CharacterSize>(config.value.characterSize)

// Keep in sync if config changes from another source (e.g. future presets).
watch(() => config.value.characterSize, v => { localSize.value = v })

async function setSize(s: CharacterSize) {
  localSize.value = s
  await updateConfig({ characterSize: s })
}

// ── Weather visibility ────────────────────────────────────────────
const { weatherAvailable } = useWeatherAvailable()
const localShowWeather = ref<boolean>(config.value.showWeather)

watch(() => config.value.showWeather, v => { localShowWeather.value = v })

async function toggleWeather() {
  await updateConfig({ showWeather: localShowWeather.value })
}

// ── Window controls ────────────────────────────────────────────────

const win = getCurrentWindow()

function minimize()    { win.minimize() }
function maximize()    { win.toggleMaximize() }
function closeWindow() { win.close() }

// ── Data actions ───────────────────────────────────────────────────

async function refresh() {
  const s = await invoke<StateSnapshot>('get_state')
  focusMins.value = s.pomodoro.focus_mins
  breakMins.value = s.pomodoro.break_mins
  energy.value    = Math.round(s.pet.energy)
  affection.value = Math.round(s.pet.affection)
  mood.value      = s.pet.mood
}

async function refreshAutostart() {
  try { autostart.value = await isEnabled() }
  catch { autostart.value = false }
}

function showStatus(msg: string) {
  status.value = msg
  setTimeout(() => { status.value = '' }, 1800)
}

async function toggleAutostart() {
  try {
    if (autostart.value) await enable()
    else                 await disable()
    showStatus(autostart.value ? t.value.autostartOnMsg : t.value.autostartOffMsg)
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
  showStatus(t.value.savedMsg)
}

async function reset() {
  await invoke('pet_reset')
  await refresh()
  showStatus(t.value.resetMsg)
}

// "Close" button inside the page body also destroys the window —
// it is recreated on demand from the tray the next time.
async function closeContent() {
  await win.close()
}

onMounted(async () => {
  await refresh()
  await refreshAutostart()
})
</script>

<template>
  <div class="shell">

    <!-- ── Decorative orbs (behind everything) ─────────────── -->
    <div class="orb orb-a" />
    <div class="orb orb-b" />
    <div class="orb orb-c" />

    <!-- ── Custom titlebar ─────────────────────────────────── -->
    <!--
      data-tauri-drag-region: the whole bar is a drag surface.
      win-controls has @mousedown.stop so clicks don't bleed into drag.
    -->
    <header class="titlebar" data-tauri-drag-region>
      <div class="title-identity" data-tauri-drag-region>
        <span class="title-logo">🥒</span>
        <span class="title-name">Mutsumi</span>
        <span class="title-sep">·</span>
        <span class="title-sub">{{ t.settingsTitle }}</span>
      </div>

      <div class="win-controls" @mousedown.stop>
        <button class="wbtn wbtn-min"   @click="minimize"    title="Minimize">
          <svg width="10" height="2" viewBox="0 0 10 2"><rect width="10" height="1.5" rx="0.75" fill="currentColor"/></svg>
        </button>
        <button class="wbtn wbtn-max"   @click="maximize"    title="Maximize">
          <svg width="10" height="10" viewBox="0 0 10 10"><rect x="0.75" y="0.75" width="8.5" height="8.5" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.5"/></svg>
        </button>
        <button class="wbtn wbtn-close" @click="closeWindow" title="Close">
          <svg width="10" height="10" viewBox="0 0 10 10"><line x1="1" y1="1" x2="9" y2="9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><line x1="9" y1="1" x2="1" y2="9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>
        </button>
      </div>
    </header>

    <!-- ── Scrollable content ──────────────────────────────── -->
    <div class="content">

      <!-- Pomodoro -->
      <section class="card">
        <h2 class="card-title">
          <span class="card-icon">🍅</span>{{ t.pomodoro }}
        </h2>

        <div class="field-row">
          <label>{{ t.focusLabel }}</label>
          <div class="num-input">
            <input type="number" v-model="focusMins" min="1" max="180" />
            <span class="unit">{{ t.minuteUnit }}</span>
          </div>
        </div>

        <div class="field-row">
          <label>{{ t.breakLabel }}</label>
          <div class="num-input">
            <input type="number" v-model="breakMins" min="1" max="60" />
            <span class="unit">{{ t.minuteUnit }}</span>
          </div>
        </div>
      </section>

      <!-- Pet status -->
      <section class="card">
        <h2 class="card-title">
          <span class="card-icon">🌸</span>{{ t.petStatus }}
        </h2>

        <div class="stat-row">
          <span class="stat-icon">⚡</span>
          <span class="stat-name">{{ t.energy }}</span>
          <div class="bar-track"><div class="bar-fill energy-fill" :style="{ width: energy + '%' }" /></div>
          <span class="stat-val">{{ energy }}</span>
        </div>

        <div class="stat-row">
          <span class="stat-icon">💚</span>
          <span class="stat-name">{{ t.affection }}</span>
          <div class="bar-track"><div class="bar-fill affection-fill" :style="{ width: affection + '%' }" /></div>
          <span class="stat-val">{{ affection }}</span>
        </div>

        <div class="stat-row mood-row">
          <span class="stat-icon">✨</span>
          <span class="stat-name">{{ t.mood }}</span>
          <span class="mood-chip">{{ mood }}</span>
        </div>
      </section>

      <!-- Language (manual override) -->
      <section class="card">
        <h2 class="card-title">
          <span class="card-icon">🌐</span>{{ t.language }}
        </h2>
        <div class="size-group">
          <button
            v-for="opt in LANG_OPTIONS"
            :key="opt.key"
            class="size-btn"
            :class="{ active: locale === opt.key }"
            @click="setLanguage(opt.key)"
          >
            {{ opt.label }}
          </button>
        </div>
      </section>

      <!-- Character size (Task 3) -->
      <section class="card">
        <h2 class="card-title">
          <span class="card-icon">🪟</span>{{ t.characterSize }}
        </h2>
        <div class="size-group">
          <button
            v-for="opt in SIZE_OPTIONS"
            :key="opt.key"
            class="size-btn"
            :class="{ active: localSize === opt.key }"
            @click="setSize(opt.key)"
          >
            {{ t[opt.labelKey] }}
          </button>
        </div>
      </section>

      <!-- System -->
      <section class="card">
        <h2 class="card-title">
          <span class="card-icon">⚙️</span>{{ t.system }}
        </h2>

        <div class="field-row">
          <label for="autostart-toggle">{{ t.launchOnStartup }}</label>
          <label class="toggle">
            <input
              id="autostart-toggle"
              type="checkbox"
              v-model="autostart"
              @change="toggleAutostart"
            />
            <span class="thumb" />
          </label>
        </div>

        <!-- Weather visibility — disabled when service is unavailable -->
        <div class="field-row" style="margin-top: 6px;">
          <label
            for="weather-toggle"
            :style="weatherAvailable === false ? 'opacity: 0.4' : ''"
          >{{ t.showWeather }}</label>
          <label class="toggle" :style="weatherAvailable === false ? 'opacity: 0.4; cursor: not-allowed' : ''">
            <input
              id="weather-toggle"
              type="checkbox"
              v-model="localShowWeather"
              :disabled="weatherAvailable === false"
              @change="toggleWeather"
            />
            <span class="thumb" />
          </label>
        </div>
      </section>

      <!-- Actions + inline status toast -->
      <div class="action-row">
        <div class="actions">
          <button class="btn btn-primary" @click="save">{{ t.save }}</button>
          <button class="btn btn-ghost"   @click="reset">{{ t.resetPet }}</button>
          <button class="btn btn-subtle"  @click="closeContent">{{ t.close }}</button>
        </div>

        <Transition name="toast">
          <p v-if="status" class="toast">{{ status }}</p>
        </Transition>
      </div>

    </div>
  </div>
</template>

<style scoped>
/* ═══════════════════════════════════════════════════════════════
   Shell — the actual window surface.
   Transparent background + blur = genuine glassmorphism over desktop.
   Border-radius clips the corners; the OS sees transparent pixels there.
══════════════════════════════════════════════════════════════ */
.shell {
  position: relative;
  width: 100%;
  height: 100vh;
  overflow: hidden;

  /* Semi-transparent sage-tinted glass */
  background: rgba(218, 238, 218, 0.78);
  backdrop-filter: blur(44px) saturate(190%) brightness(1.04);
  -webkit-backdrop-filter: blur(44px) saturate(190%) brightness(1.04);

  border-radius: 12px;

  font-family: system-ui, "Segoe UI", "Noto Sans SC", "Noto Sans JP", sans-serif;
  font-size: 13px;
  color: #1a2e1a;
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
}

/* ── Decorative orbs (behind content) ─────────────────────── */
.orb {
  position: absolute;
  border-radius: 50%;
  pointer-events: none;
  z-index: 0;
}
.orb-a {
  width: 260px; height: 260px;
  background: rgba(119, 153, 119, 0.30);
  filter: blur(72px);
  top: -90px; right: -80px;
}
.orb-b {
  width: 200px; height: 200px;
  background: rgba(80, 160, 130, 0.22);
  filter: blur(66px);
  bottom: -60px; left: -55px;
}
.orb-c {
  width: 140px; height: 140px;
  background: rgba(155, 200, 110, 0.18);
  filter: blur(54px);
  top: 44%; left: 48%;
  transform: translate(-50%, -50%);
}

/* ═══════════════════════════════════════════════════════════════
   Custom titlebar
══════════════════════════════════════════════════════════════ */
.titlebar {
  position: relative;
  z-index: 10;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 40px;
  padding: 0 10px 0 14px;

  background: rgba(180, 220, 180, 0.28);
  border-bottom: 1px solid rgba(119, 153, 119, 0.22);

  /* macOS-style inset highlight */
  box-shadow: inset 0 -1px 0 rgba(255, 255, 255, 0.18);

  user-select: none;
  -webkit-user-select: none;
  cursor: default;
}

.title-identity {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: 1;
  min-width: 0;
}

.title-logo { font-size: 16px; line-height: 1; }

.title-name {
  font-size: 13px;
  font-weight: 700;
  color: #1a3a1a;
  letter-spacing: -0.2px;
}

.title-sep {
  font-size: 11px;
  color: rgba(40, 80, 40, 0.35);
}

.title-sub {
  font-size: 10px;
  font-weight: 500;
  color: rgba(40, 80, 40, 0.50);
  text-transform: uppercase;
  letter-spacing: 0.6px;
}

/* ── Window control buttons ──────────────────────────────── */
.win-controls {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.wbtn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 22px;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  background: rgba(0, 0, 0, 0.06);
  color: rgba(40, 70, 40, 0.55);
  transition: background 100ms ease, color 100ms ease, transform 80ms ease;
}

.wbtn:hover  { background: rgba(0, 0, 0, 0.12); color: rgba(20, 50, 20, 0.85); }
.wbtn:active { transform: scale(0.90); }

/* Close — red on hover */
.wbtn-close:hover {
  background: rgba(220, 60, 60, 0.82);
  color: white;
}

/* Minimize — amber on hover */
.wbtn-min:hover {
  background: rgba(200, 145, 30, 0.82);
  color: white;
}

/* Maximize — green on hover */
.wbtn-max:hover {
  background: rgba(60, 170, 60, 0.82);
  color: white;
}

/* ═══════════════════════════════════════════════════════════════
   Scrollable main content
══════════════════════════════════════════════════════════════ */
.content {
  position: relative;
  z-index: 1;
  flex: 1;
  overflow-y: auto;
  padding: 12px 14px 14px;
  display: flex;
  flex-direction: column;
  gap: 9px;

  /* Hide scrollbar visually on Windows */
  scrollbar-width: thin;
  scrollbar-color: rgba(119, 153, 119, 0.30) transparent;
}
.content::-webkit-scrollbar       { width: 4px; }
.content::-webkit-scrollbar-track { background: transparent; }
.content::-webkit-scrollbar-thumb { background: rgba(119, 153, 119, 0.30); border-radius: 2px; }

/* ── Glass card ──────────────────────────────────────────── */
.card {
  background: rgba(255, 255, 255, 0.42);
  backdrop-filter: blur(20px) saturate(170%);
  -webkit-backdrop-filter: blur(20px) saturate(170%);
  border: 1px solid rgba(145, 190, 145, 0.35);
  border-radius: 12px;
  padding: 11px 12px 10px;
  box-shadow:
    0 1px 8px rgba(50, 90, 50, 0.07),
    inset 0 1px 0 rgba(255, 255, 255, 0.80);
}

/* ── Card title ──────────────────────────────────────────── */
.card-title {
  display: flex;
  align-items: center;
  gap: 5px;
  margin: 0 0 9px;
  font-size: 9.5px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.7px;
  color: rgba(45, 85, 45, 0.55);
}
.card-icon { font-size: 11px; }

/* ── Field row ───────────────────────────────────────────── */
.field-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 2px 0;
}
.field-row + .field-row { margin-top: 6px; }
.field-row > label {
  color: #2c4e2c;
  font-weight: 500;
  font-size: 12.5px;
}

/* ── Number input ────────────────────────────────────────── */
.num-input { display: flex; align-items: center; gap: 5px; }
.num-input input {
  width: 50px;
  padding: 4px 7px;
  font-size: 13px;
  font-weight: 600;
  border: 1.5px solid rgba(119, 153, 119, 0.40);
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.72);
  color: #1a2e1a;
  text-align: center;
  outline: none;
  transition: border-color 140ms ease, box-shadow 140ms ease;
}
.num-input input:focus {
  border-color: #779977;
  box-shadow: 0 0 0 3px rgba(119, 153, 119, 0.18);
}
.unit { font-size: 11px; color: rgba(45, 85, 45, 0.48); font-weight: 500; }

/* ── Stat rows ───────────────────────────────────────────── */
.stat-row          { display: flex; align-items: center; gap: 7px; padding: 2px 0; }
.stat-row + .stat-row { margin-top: 6px; }
.stat-icon         { font-size: 12px; flex-shrink: 0; }
.stat-name         {
  font-size: 11.5px; color: rgba(38, 76, 38, 0.68);
  font-weight: 500; width: 66px; flex-shrink: 0;
}
.bar-track {
  flex: 1; height: 5px;
  background: rgba(0, 0, 0, 0.07);
  border-radius: 3px; overflow: hidden;
}
.bar-fill          { height: 100%; border-radius: 3px; transition: width 700ms ease; }
.energy-fill       { background: linear-gradient(90deg, #779977, #5a8060); }
.affection-fill    { background: linear-gradient(90deg, #88bb88, #6a9b70); }
.stat-val          { font-size: 12px; font-weight: 700; color: #2c4e2c; min-width: 26px; text-align: right; }

/* Mood row */
.mood-row .stat-name { width: auto; flex: 1; }
.mood-chip {
  padding: 2px 9px; border-radius: 20px;
  background: rgba(119, 153, 119, 0.16);
  border: 1px solid rgba(119, 153, 119, 0.35);
  font-size: 11px; font-weight: 600; color: #365636;
  text-transform: capitalize;
}

/* ── Toggle switch ───────────────────────────────────────── */
.toggle        { position: relative; display: inline-block; width: 40px; height: 22px; cursor: pointer; flex-shrink: 0; }
.toggle input  { opacity: 0; width: 0; height: 0; }
.thumb {
  position: absolute; inset: 0; border-radius: 11px;
  background: rgba(0, 0, 0, 0.13); transition: background 220ms ease;
  border: 1px solid rgba(255, 255, 255, 0.38);
}
.thumb::before {
  content: ''; position: absolute;
  width: 16px; height: 16px; border-radius: 50%;
  background: white; top: 2px; left: 2px;
  transition: transform 220ms cubic-bezier(0.34, 1.56, 0.64, 1);
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.20);
}
.toggle input:checked + .thumb                  { background: linear-gradient(135deg, #779977, #5a8060); border-color: transparent; }
.toggle input:checked + .thumb::before          { transform: translateX(18px); }

/* ── Action buttons ──────────────────────────────────────── */
/* Outer row: buttons on the left, toast fills the remaining space */
.action-row {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 2px;
}

.actions { display: flex; gap: 7px; align-items: center; flex-shrink: 0; }
.btn {
  padding: 7px 14px; font-size: 12px; font-weight: 600;
  border-radius: 9px; cursor: pointer;
  transition: background 130ms ease, transform 80ms ease, box-shadow 130ms ease;
}
.btn:active { transform: scale(0.95); }

.btn-primary {
  background: linear-gradient(135deg, #779977, #5a8060);
  border: none; color: white;
  box-shadow: 0 2px 10px rgba(80, 120, 80, 0.34);
}
.btn-primary:hover {
  background: linear-gradient(135deg, #88aa88, #6a9470);
  box-shadow: 0 3px 14px rgba(80, 120, 80, 0.42);
}

.btn-ghost {
  background: rgba(255, 255, 255, 0.45);
  border: 1.5px solid rgba(119, 153, 119, 0.32);
  color: #2c4e2c;
  backdrop-filter: blur(10px);
  -webkit-backdrop-filter: blur(10px);
}
.btn-ghost:hover { background: rgba(255, 255, 255, 0.65); }

.btn-subtle {
  background: transparent;
  border: 1.5px solid rgba(55, 88, 55, 0.20);
  color: rgba(55, 88, 55, 0.56);
}
.btn-subtle:hover { background: rgba(255, 255, 255, 0.30); }

/* ── Status toast ────────────────────────────────────────── */
.toast {
  margin: 0;
  flex: 1;
  text-align: left;
  font-size: 11px;
  font-weight: 500;
  color: #466646;
}
.toast-enter-active, .toast-leave-active { transition: opacity 240ms ease, transform 240ms ease; }
.toast-enter-from, .toast-leave-to       { opacity: 0; transform: translateY(4px); }

/* ── Character size segmented control ────────────────────── */
.size-group {
  display: flex;
  gap: 5px;
}

.size-btn {
  flex: 1;
  padding: 5px 0;
  font-size: 11.5px;
  font-weight: 600;
  border-radius: 8px;
  cursor: pointer;
  border: 1.5px solid rgba(119, 153, 119, 0.30);
  background: rgba(255, 255, 255, 0.38);
  color: rgba(44, 78, 44, 0.65);
  transition: background 120ms ease, border-color 120ms ease, color 120ms ease;
}

.size-btn:hover {
  background: rgba(255, 255, 255, 0.60);
  border-color: rgba(119, 153, 119, 0.50);
  color: #2c4e2c;
}

.size-btn.active {
  background: linear-gradient(135deg, #779977, #5a8060);
  border-color: transparent;
  color: white;
}
</style>
