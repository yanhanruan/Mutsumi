<script setup lang="ts">
/**
 * SettingsWindow — Pomodoro durations, pet status, system settings.
 *
 * Frameless (decorations:false, transparent:true) — the entire UI is
 * painted by Vue. A custom titlebar at the top carries the drag region
 * and the controls supported by this fixed-size window (minimize / close).
 *
 * Primary theme: #779977 (sage green).
 */
import { computed, onMounted, onUnmounted, nextTick, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { enable, disable, isEnabled } from '@tauri-apps/plugin-autostart'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useI18n, setLocale, type Locale } from '../i18n'
import {
  useAppConfig,
  FLYING_WAIT_MIN_MINS, FLYING_WAIT_MAX_MINS,
  type CharacterSize, type SearchEngineKey, type ChatModelKey,
} from '../composables/useAppConfig'
import { useWeatherAvailable } from '../composables/useWeatherAvailable'
import { usePlatformCapabilities } from '../composables/usePlatformCapabilities'
import { isMacOSDesktop } from '../config/desktopPlatform'
import WindowTitlebar from './WindowTitlebar.vue'

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
const autostartAvailable = ref(true)

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
const {
  musicAvailable,
  musicPanelAvailable,
  audioActivityDegraded,
  idleScreensaverAvailable,
} = usePlatformCapabilities()
const localShowWeather = ref<boolean>(config.value.showWeather)

watch(() => config.value.showWeather, v => { localShowWeather.value = v })

async function toggleWeather() {
  await updateConfig({ showWeather: localShowWeather.value })
}

// ── Music controller visibility ───────────────────────────────────
const localShowMusic = ref<boolean>(config.value.showMusic)
const musicToggleLabel = computed(() =>
  musicPanelAvailable.value ? t.value.showMusic : t.value.showAudioActivity)

watch(() => config.value.showMusic, v => { localShowMusic.value = v })

async function toggleMusic() {
  await updateConfig({ showMusic: localShowMusic.value })
}

// ── Sleep "zzz" effect visibility ─────────────────────────────────
const localShowZzz = ref<boolean>(config.value.showZzz)

watch(() => config.value.showZzz, v => { localShowZzz.value = v })

async function toggleZzz() {
  await updateConfig({ showZzz: localShowZzz.value })
}

// ── Auto-update check ─────────────────────────────────────────────
const localUpdateAutoCheck = ref<boolean>(config.value.updateAutoCheck)

watch(() => config.value.updateAutoCheck, v => { localUpdateAutoCheck.value = v })

async function toggleUpdateAutoCheck() {
  await updateConfig({ updateAutoCheck: localUpdateAutoCheck.value })
}

// ── Flying screensaver ────────────────────────────────────────────
const localFlying     = ref<boolean>(config.value.flyingScreensaver)
const localFlyingWait = ref<number>(config.value.flyingWaitMins)

watch(() => config.value.flyingScreensaver, v => { localFlying.value = v })
watch(() => config.value.flyingWaitMins,    v => { localFlyingWait.value = v })

async function applyFlying() {
  if (!idleScreensaverAvailable.value) return
  // Clamp the wait into the supported range (also enforced Rust-side).
  const mins = Math.min(FLYING_WAIT_MAX_MINS,
    Math.max(FLYING_WAIT_MIN_MINS, Math.round(localFlyingWait.value || FLYING_WAIT_MIN_MINS)))
  localFlyingWait.value = mins
  await updateConfig({ flyingScreensaver: localFlying.value, flyingWaitMins: mins })
  try { await invoke('flight_set_screensaver', { enabled: localFlying.value, waitMins: mins }) }
  catch { /* best-effort; re-synced on next startup */ }
}

// Global-hotkey status: when a hotkey failed to register at startup (another
// app owns the combination), surface WHY the shortcut is dead this session
// instead of leaving it silently broken. Backed by hotkeys.rs (HotkeysState).
interface HotkeyStatus {
  id:          string
  accelerator: string
  active:      boolean
}
const flightHotkey = ref<HotkeyStatus | null>(null)
const fallbackFlightAccelerator = isMacOSDesktop() ? '⌃⌥F' : 'Ctrl+Alt+F'

async function refreshHotkeys() {
  try {
    const all = await invoke<HotkeyStatus[]>('get_hotkey_status')
    flightHotkey.value = all.find(h => h.id === 'toggle-flight') ?? null
  } catch { /* status unavailable — just show no warning */ }
}

// ── Search engine (chat search-enhancement) ───────────────────────
type EngineLabelKey = 'duckduckgo' | 'bingCn' | 'bing' | 'google' | 'baidu'
const ENGINE_OPTIONS: { key: SearchEngineKey; labelKey: EngineLabelKey }[] = [
  { key: 'duckduckgo', labelKey: 'duckduckgo' },
  { key: 'bing-cn',    labelKey: 'bingCn'     },
  { key: 'bing',       labelKey: 'bing'       },
  { key: 'google',     labelKey: 'google'     },
  { key: 'baidu',      labelKey: 'baidu'      },
]

const localEngine = ref<SearchEngineKey>(config.value.searchEngine)
watch(() => config.value.searchEngine, v => { localEngine.value = v })

async function setSearchEngine(e: SearchEngineKey) {
  localEngine.value = e
  await updateConfig({ searchEngine: e })          // persist + broadcast
  try { await invoke('set_search_engine', { engine: e }) }  // apply to backend now
  catch { /* best-effort; synced again on next startup */ }
}

// ── Web search on/off (master toggle) ─────────────────────────────
const localSearchEnabled = ref<boolean>(config.value.searchEnabled)
watch(() => config.value.searchEnabled, v => { localSearchEnabled.value = v })

async function toggleSearchEnabled() {
  await updateConfig({ searchEnabled: localSearchEnabled.value })   // persist + broadcast
  try { await invoke('set_search_enabled', { enabled: localSearchEnabled.value }) }
  catch { /* best-effort; re-synced on next startup */ }
}

// ── Chat model picker ─────────────────────────────────────────────
const MODEL_OPTIONS: { key: ChatModelKey; isDefault?: boolean }[] = [
  { key: 'qwen3.7-max'   },
  { key: 'qwen3.7-plus', isDefault: true },
  { key: 'qwen3.6-flash' },
]
const localModel = ref<ChatModelKey>(config.value.chatModel)
watch(() => config.value.chatModel, v => { localModel.value = v })

async function setModel(m: ChatModelKey) {
  localModel.value = m
  await updateConfig({ chatModel: m })             // persist + broadcast
  try { await invoke('qwen_set_chat_model', { model: m }) }  // apply to backend now
  catch { /* best-effort; re-synced on next startup */ }
}

// ── Dropdowns (search engine + model) ─────────────────────────────
const engineDropOpen = ref(false)
const modelDropOpen = ref(false)

function onEngineOutside() {
  engineDropOpen.value = false
  document.removeEventListener('click', onEngineOutside)
}
function onModelOutside() {
  modelDropOpen.value = false
  document.removeEventListener('click', onModelOutside)
}

watch(engineDropOpen, open => {
  if (open) nextTick(() => document.addEventListener('click', onEngineOutside))
  else      document.removeEventListener('click', onEngineOutside)
})
watch(modelDropOpen, open => {
  if (open) nextTick(() => document.addEventListener('click', onModelOutside))
  else      document.removeEventListener('click', onModelOutside)
})

onUnmounted(() => {
  document.removeEventListener('click', onEngineOutside)
  document.removeEventListener('click', onModelOutside)
})

// ── Chat memory reset ──────────────────────────────────────────────
// Destructive, so a two-step confirm: first tap arms, second tap (within a few
// seconds) actually wipes. Avoids depending on a native confirm dialog.
const confirmingClear = ref(false)
let clearTimer: ReturnType<typeof setTimeout> | undefined

async function clearMemory() {
  if (!confirmingClear.value) {
    confirmingClear.value = true
    clearTimer = setTimeout(() => { confirmingClear.value = false }, 3000)
    return
  }
  clearTimeout(clearTimer)
  confirmingClear.value = false
  try {
    await invoke('chat_clear_memory')
    showStatus(t.value.clearMemoryDoneMsg)
  } catch (e) {
    console.error('clear memory failed', e)
  }
}

// ── Qwen / 百炼 API key ─────────────────────────────────────────────
// The chat/memory pipeline calls Alibaba Bailian (DashScope). End users supply
// their own key here; it's applied to the live backend client immediately and
// persisted on-device (never broadcast to the other window).
const apiKeyInput = ref('')
const apiKeyConfigured = ref(false)
const credentialStoreAvailable = ref(true)
const savingKey = ref(false)

interface QwenKeyStatus {
  configured: boolean
  credentialStoreAvailable: boolean
}

async function refreshApiKeyStatus() {
  try {
    const result = await invoke<QwenKeyStatus>('qwen_key_status')
    apiKeyConfigured.value = result.configured
    credentialStoreAvailable.value = result.credentialStoreAvailable
  } catch {
    apiKeyConfigured.value = false
    credentialStoreAvailable.value = false
  }
}

async function saveApiKey() {
  const key = apiKeyInput.value.trim()
  if (!key || savingKey.value) return
  savingKey.value = true
  try {
    apiKeyConfigured.value = await invoke<boolean>('qwen_set_api_key', { key })
    credentialStoreAvailable.value = true
    apiKeyInput.value = ''
    showStatus(t.value.apiKeySavedMsg)
  } catch (e) {
    console.error('save api key failed', e)
    credentialStoreAvailable.value = false
    showStatus(t.value.apiKeySaveFailedMsg)
  } finally {
    savingKey.value = false
  }
}

// Tutorial: how to obtain a Bailian (DashScope) API key — opens in the browser.
const API_KEY_HELP_URL = 'https://developer.aliyun.com/article/1714006'
function openApiKeyHelp() {
  openUrl(API_KEY_HELP_URL).catch(e => console.error('open help url failed', e))
}

async function clearApiKey() {
  if (savingKey.value) return
  savingKey.value = true
  try {
    apiKeyConfigured.value = await invoke<boolean>('qwen_set_api_key', { key: '' })
    credentialStoreAvailable.value = true
    apiKeyInput.value = ''
    showStatus(t.value.apiKeyClearedMsg)
  } catch (e) {
    console.error('clear api key failed', e)
    credentialStoreAvailable.value = false
    showStatus(t.value.apiKeyClearFailedMsg)
  } finally {
    savingKey.value = false
  }
}

// ── Window controls ────────────────────────────────────────────────

const win = getCurrentWindow()

function minimize()    { win.minimize() }
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
  try {
    autostart.value = await isEnabled()
    autostartAvailable.value = true
  } catch (e) {
    console.error('autostart status failed', e)
    autostart.value = false
    autostartAvailable.value = false
  }
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
    showStatus(t.value.autostartFailedMsg)
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
  await refreshApiKeyStatus()
  await refreshHotkeys()
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
    <WindowTitlebar
      :subtitle="t.settingsTitle"
      :close-label="t.close"
      :minimize-label="t.minimize"
      minimizable
      @minimize="minimize"
      @close="closeWindow"
    />

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

      <!-- Flying screensaver -->
      <section class="card">
        <h2 class="card-title">
          <span class="card-icon">🎈</span>{{ t.flyingScreensaver }}
        </h2>
        <p class="card-hint">{{ t.flyingScreensaverHint.replace('{keys}', flightHotkey?.accelerator ?? fallbackFlightAccelerator) }}</p>
        <p v-if="!idleScreensaverAvailable" class="hotkey-warn">
          ⚠️ {{ t.flyingScreensaverUnavailable }}
        </p>
        <p v-if="flightHotkey && !flightHotkey.active" class="hotkey-warn">
          ⚠️ {{ t.hotkeyUnavailable.replace('{keys}', flightHotkey.accelerator) }}
        </p>
        <div class="field-row">
          <label for="flying-toggle">{{ t.flyingScreensaver }}</label>
          <label class="toggle">
            <input
              id="flying-toggle"
              type="checkbox"
              v-model="localFlying"
              :disabled="!idleScreensaverAvailable"
              @change="applyFlying"
            />
            <span class="thumb" />
          </label>
        </div>
        <div class="field-row" :style="localFlying && idleScreensaverAvailable ? '' : 'opacity: 0.45; pointer-events: none;'">
          <label>{{ t.flyingWaitLabel }}</label>
          <div class="num-input">
            <input
              type="number"
              v-model.number="localFlyingWait"
              :min="FLYING_WAIT_MIN_MINS"
              :max="FLYING_WAIT_MAX_MINS"
              @change="applyFlying"
            />
            <span class="unit">{{ t.minuteUnit }}</span>
          </div>
        </div>
      </section>

      <!-- Chat model -->
      <section class="card" :style="{ zIndex: modelDropOpen ? 20 : 1, position: 'relative' }">
        <h2 class="card-title">
          <span class="card-icon">🧩</span>{{ t.chatModel }}
        </h2>
        <div class="dropdown">
          <button class="engine-trigger" @click.stop="modelDropOpen = !modelDropOpen">
            <span>{{ localModel }}<template v-if="localModel === 'qwen3.7-plus'"> ({{ t.modelDefaultTag }})</template></span>
            <svg class="engine-chevron" :class="{ open: modelDropOpen }" width="10" height="6" viewBox="0 0 10 6" fill="none">
              <path d="M1 1l4 4 4-4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
          </button>
          <div v-if="modelDropOpen" class="engine-menu" @click.stop>
            <button
              v-for="opt in MODEL_OPTIONS"
              :key="opt.key"
              class="engine-option"
              :class="{ active: localModel === opt.key }"
              @click="setModel(opt.key); modelDropOpen = false"
            >
              {{ opt.key }}<template v-if="opt.isDefault"> ({{ t.modelDefaultTag }})</template>
            </button>
          </div>
        </div>
        <p class="card-hint" style="margin: 8px 0 0;">{{ t.chatModelHint }}</p>
      </section>

      <!-- Search engine + on/off -->
      <section class="card" :style="{ zIndex: engineDropOpen ? 20 : 1, position: 'relative' }">
        <h2 class="card-title">
          <span class="card-icon">🔍</span>{{ t.searchEngine }}
        </h2>
        <div class="field-row" style="margin-bottom: 8px;">
          <label for="search-toggle">{{ t.searchEnabled }}</label>
          <label class="toggle">
            <input
              id="search-toggle"
              type="checkbox"
              v-model="localSearchEnabled"
              @change="toggleSearchEnabled"
            />
            <span class="thumb" />
          </label>
        </div>
        <div class="dropdown" :style="localSearchEnabled ? '' : 'opacity: 0.45; pointer-events: none;'">
          <button class="engine-trigger" @click.stop="engineDropOpen = !engineDropOpen">
            <span>{{ t.searchEngines[ENGINE_OPTIONS.find(o => o.key === localEngine)!.labelKey] }}</span>
            <svg class="engine-chevron" :class="{ open: engineDropOpen }" width="10" height="6" viewBox="0 0 10 6" fill="none">
              <path d="M1 1l4 4 4-4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
          </button>
          <div v-if="engineDropOpen" class="engine-menu" @click.stop>
            <button
              v-for="opt in ENGINE_OPTIONS"
              :key="opt.key"
              class="engine-option"
              :class="{ active: localEngine === opt.key }"
              @click="setSearchEngine(opt.key); engineDropOpen = false"
            >
              {{ t.searchEngines[opt.labelKey] }}
            </button>
          </div>
        </div>
        <p class="card-hint" style="margin: 8px 0 0;">{{ t.searchEnabledHint }}</p>
      </section>

      <!-- Qwen / 百炼 API key -->
      <section class="card">
        <h2 class="card-title">
          <span class="card-icon">🔑</span>{{ t.apiKey }}
        </h2>
        <p class="card-hint">{{ t.apiKeyHint }}</p>
        <p v-if="!credentialStoreAvailable" class="system-warning">
          ⚠️ {{ t.apiKeyCredentialStoreUnavailable }}
        </p>
        <button type="button" class="key-help" @click="openApiKeyHelp">{{ t.apiKeyHelp }} ↗</button>
        <div class="key-row">
          <input
            class="key-input"
            type="password"
            v-model="apiKeyInput"
            :placeholder="apiKeyConfigured ? t.apiKeySetPlaceholder : t.apiKeyPlaceholder"
            autocomplete="off"
            autocorrect="off"
            spellcheck="false"
            @keydown.enter="saveApiKey"
          />
          <button
            class="btn btn-primary key-save"
            :disabled="savingKey || !apiKeyInput.trim()"
            @click="saveApiKey"
          >{{ t.save }}</button>
        </div>
        <div class="key-foot">
          <span class="key-status" :class="{ ok: apiKeyConfigured }">
            {{ apiKeyConfigured ? t.apiKeyStatusSet : t.apiKeyStatusUnset }}
          </span>
          <button
            v-if="apiKeyConfigured"
            class="key-clear"
            :disabled="savingKey"
            @click="clearApiKey"
          >{{ t.apiKeyClear }}</button>
        </div>
      </section>

      <!-- Chat memory -->
      <section class="card">
        <h2 class="card-title">
          <span class="card-icon">🧠</span>{{ t.chatMemory }}
        </h2>
        <p class="card-hint">{{ t.chatMemoryHint }}</p>
        <button
          class="btn btn-danger"
          :class="{ confirming: confirmingClear }"
          @click="clearMemory"
        >
          {{ confirmingClear ? t.clearMemoryConfirm : t.clearMemory }}
        </button>
      </section>

      <!-- System -->
      <section class="card">
        <h2 class="card-title">
          <span class="card-icon">⚙️</span>{{ t.system }}
        </h2>

        <div class="field-row">
          <label for="autostart-toggle" :class="{ 'field-disabled': !autostartAvailable }">{{ t.launchOnStartup }}</label>
          <label class="toggle" :class="{ 'field-disabled': !autostartAvailable }">
            <input
              id="autostart-toggle"
              type="checkbox"
              v-model="autostart"
              :disabled="!autostartAvailable"
              @change="toggleAutostart"
            />
            <span class="thumb" />
          </label>
        </div>
        <p v-if="!autostartAvailable" class="system-warning">⚠️ {{ t.autostartUnavailable }}</p>

        <!-- Auto-update check -->
        <div class="field-row" style="margin-top: 6px;">
          <label for="update-toggle">{{ t.updateAutoCheck }}</label>
          <label class="toggle">
            <input
              id="update-toggle"
              type="checkbox"
              v-model="localUpdateAutoCheck"
              @change="toggleUpdateAutoCheck"
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

        <!-- Music controller visibility -->
        <div v-if="musicAvailable" class="field-row" style="margin-top: 6px;">
          <label for="music-toggle">{{ musicToggleLabel }}</label>
          <label class="toggle">
            <input
              id="music-toggle"
              type="checkbox"
              v-model="localShowMusic"
              @change="toggleMusic"
            />
            <span class="thumb" />
          </label>
        </div>
        <p v-if="musicAvailable && audioActivityDegraded" class="system-warning">
          ⚠️ {{ t.audioActivityDegradedHint }}
        </p>

        <!-- Sleep "zzz" effect visibility -->
        <div class="field-row" style="margin-top: 6px;">
          <label for="zzz-toggle">{{ t.showZzz }}</label>
          <label class="toggle">
            <input
              id="zzz-toggle"
              type="checkbox"
              v-model="localShowZzz"
              @change="toggleZzz"
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

.field-disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.system-warning {
  margin: 4px 0 0;
  color: rgba(130, 82, 16, 0.88);
  font-size: 11px;
  line-height: 1.4;
}

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

/* Destructive button (clear memory) — neutral until armed, red once confirming */
.btn-danger {
  background: rgba(255, 255, 255, 0.45);
  border: 1.5px solid rgba(200, 90, 90, 0.45);
  color: #a23a3a;
  backdrop-filter: blur(10px);
  -webkit-backdrop-filter: blur(10px);
}
.btn-danger:hover { background: rgba(220, 120, 120, 0.16); }
.btn-danger.confirming {
  background: linear-gradient(135deg, #c85454, #a23a3a);
  border-color: transparent;
  color: white;
  box-shadow: 0 2px 10px rgba(160, 60, 60, 0.34);
}

/* ── Card hint (small muted description under a card title) ───────── */
.card-hint {
  margin: 0 0 9px;
  font-size: 11.5px;
  line-height: 1.5;
  color: rgba(45, 85, 45, 0.62);
}

/* Hotkey-collision warning (a global shortcut failed to register). */
.hotkey-warn {
  margin: 0 0 9px;
  padding: 6px 9px;
  font-size: 11.5px;
  line-height: 1.5;
  color: #8a6d1a;
  background: rgba(224, 178, 66, 0.14);
  border: 1px solid rgba(224, 178, 66, 0.35);
  border-radius: 8px;
}

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

/* ── API key field ───────────────────────────────────────── */
.key-help {
  display: inline-block;
  margin: -4px 0 9px;
  padding: 0;
  font-size: 11.5px;
  font-weight: 600;
  color: #4a7a9a;
  background: transparent;
  border: none;
  cursor: pointer;
  transition: color 120ms ease;
}
.key-help:hover { color: #2c5f80; text-decoration: underline; }
.key-row { display: flex; gap: 7px; align-items: center; }
.key-input {
  flex: 1;
  min-width: 0;
  padding: 7px 10px;
  font-size: 12.5px;
  border: 1.5px solid rgba(119, 153, 119, 0.40);
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.72);
  color: #1a2e1a;
  outline: none;
  transition: border-color 140ms ease, box-shadow 140ms ease;
}
.key-input:focus {
  border-color: #779977;
  box-shadow: 0 0 0 3px rgba(119, 153, 119, 0.18);
}
.key-input::placeholder { color: rgba(45, 85, 45, 0.40); }
.key-save { flex-shrink: 0; }
.btn:disabled { opacity: 0.5; cursor: not-allowed; }

.key-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-top: 7px;
}
.key-status { font-size: 11px; font-weight: 600; color: rgba(160, 60, 60, 0.85); }
.key-status.ok { color: #3a7d3a; }
.key-clear {
  flex-shrink: 0;
  padding: 2px 6px;
  font-size: 11px;
  font-weight: 600;
  color: rgba(55, 88, 55, 0.56);
  background: transparent;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  transition: background 120ms ease, color 120ms ease;
}
.key-clear:hover { color: #a23a3a; background: rgba(220, 120, 120, 0.12); }

/* ── Search engine dropdown ───────────────────────────────── */
.dropdown {
  position: relative;
}

.engine-trigger {
  width: 100%;
  padding: 7px 10px;
  font-size: 12.5px;
  font-weight: 600;
  color: #2c4e2c;
  border: 1.5px solid rgba(119, 153, 119, 0.40);
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.72);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: space-between;
  text-align: left;
  transition: border-color 140ms ease, box-shadow 140ms ease;
}
.engine-trigger:hover,
.engine-trigger:focus-visible {
  border-color: #779977;
  box-shadow: 0 0 0 3px rgba(119, 153, 119, 0.18);
  outline: none;
}

.engine-chevron {
  flex-shrink: 0;
  color: rgba(44, 78, 44, 0.45);
  transition: transform 180ms ease;
}
.engine-chevron.open { transform: rotate(180deg); }

/* Opens upward — avoids being clipped by the scroll container's bottom edge */
.engine-menu {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  z-index: 10;
  background: rgba(240, 248, 240, 0.97);
  backdrop-filter: blur(24px) saturate(180%);
  -webkit-backdrop-filter: blur(24px) saturate(180%);
  border: 1.5px solid rgba(119, 153, 119, 0.35);
  border-radius: 10px;
  box-shadow:
    0 -4px 20px rgba(40, 80, 40, 0.12),
    inset 0 -1px 0 rgba(255, 255, 255, 0.60);
  padding: 4px;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.engine-option {
  width: 100%;
  padding: 7px 10px;
  font-size: 12.5px;
  font-weight: 500;
  color: #2c4e2c;
  background: transparent;
  border: none;
  border-radius: 7px;
  text-align: left;
  cursor: pointer;
  transition: background 100ms ease, color 100ms ease;
}
.engine-option:hover  { background: rgba(119, 153, 119, 0.14); color: #1a3a1a; }
.engine-option.active { background: rgba(119, 153, 119, 0.22); color: #1a3a1a; font-weight: 700; }
</style>
