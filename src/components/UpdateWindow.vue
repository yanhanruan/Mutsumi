<script setup lang="ts">
/**
 * UpdateWindow — the "update available" pop-up (?window=update).
 *
 * All lifecycle logic lives in the pure state machine
 * (src/config/updateFlow.ts); this component only dispatches events into it
 * and renders the resulting phase. That split is what guarantees ordering
 * safety (e.g. an interrupted download can never display as installed) — see
 * updateFlow.test.ts for the full failure-mode table.
 *
 * Two entry paths (see update_window.rs):
 *   • Daily check: the pet window pre-fetched the update and stashed display
 *     metadata; we read it via get_pending_update and render immediately — no
 *     network re-check here.
 *   • Manual "Check for updates" (from About): no metadata, so we run check()
 *     ourselves and record the outcome (About shows "last checked" + status).
 *
 * The updater's Update object can't cross windows, so when the user actually
 * installs we call check() once more to obtain a fresh object to download.
 */
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getVersion } from '@tauri-apps/api/app'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { LogicalSize } from '@tauri-apps/api/dpi'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { check } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import { detectLocale, setLocale, useI18n } from '../i18n'
import { useAppConfig } from '../composables/useAppConfig'
import {
  SNOOZE_MAX_DAYS,
  SNOOZE_MIN_DAYS,
  clampSnoozeDays,
  computeSnoozeUntil,
} from '../config/updatePolicy'
import {
  INITIAL_UPDATE_FLOW,
  transition,
  type UpdateFlowEvent,
  type UpdateFlowState,
  type UpdatePhase,
} from '../config/updateFlow'

interface PendingUpdate {
  version: string
  notes: string
}

const { t } = useI18n()
const { config, updateConfig } = useAppConfig()
const win = getCurrentWindow()

// ── State machine ────────────────────────────────────────────────────

const flow = ref<UpdateFlowState>({ ...INITIAL_UPDATE_FLOW })

function dispatch(event: UpdateFlowEvent): void {
  flow.value = transition(flow.value, event)
}

const currentVersion = ref('') // installed version — context, not flow state
const snoozeDays = ref(SNOOZE_MIN_DAYS)

let unlistenPending: UnlistenFn | null = null

/** Record the outcome of a check so the About window can show it. */
function recordCheck(status: 'success' | 'error'): Promise<void> {
  return updateConfig({
    updateLastCheck: new Date().toISOString(),
    updateLastCheckStatus: status,
  })
}

function errMsg(e: unknown): string {
  return e instanceof Error ? e.message : String(e ?? '')
}

/** Reject if `p` doesn't settle within `ms`, so a stalled network check surfaces
 *  as the failed view instead of spinning forever. */
function withTimeout<T>(p: Promise<T>, ms: number): Promise<T> {
  return Promise.race([
    p,
    new Promise<T>((_, reject) => setTimeout(() => reject(new Error('update check timed out')), ms)),
  ])
}
const CHECK_TIMEOUT_MS = 20000

// ── Window sizing ────────────────────────────────────────────────────
// The window is created compact (400×250, see update_window.rs); grow it only
// for the content-heavy "available" view so the one-line states aren't
// swimming in empty space.

const SIZE_FULL: [number, number] = [440, 520]
const SIZE_FAILED: [number, number] = [420, 320]
const SIZE_COMPACT: [number, number] = [400, 250]

function sizeForPhase(p: UpdatePhase): [number, number] {
  if (p === 'available') return SIZE_FULL
  if (p === 'failed') return SIZE_FAILED
  return SIZE_COMPACT
}

let lastSize: [number, number] = SIZE_COMPACT
async function applyWindowSize(p: UpdatePhase): Promise<void> {
  const [w, h] = sizeForPhase(p)
  if (w === lastSize[0] && h === lastSize[1]) return // no-op if unchanged
  lastSize = [w, h]
  try {
    await win.setSize(new LogicalSize(w, h))
    await win.center()
  } catch { /* geometry is best-effort */ }
}

watch(() => flow.value.phase, p => { void applyWindowSize(p) })

// ── Dev mock harness (dev builds only) ───────────────────────────────
// Guarded by import.meta.env.DEV, so all of this is dead-code-eliminated from
// production bundles. To force a scenario, run in ANY window's devtools (all
// windows share the same origin/localStorage):
//
//   localStorage.setItem('mutsumi-update-mock', 'available')
//
// values: 'available' | 'available:install-fail' | 'uptodate' | 'error'
// then reopen the pop-up (About → "Check for updates"). Remove the key to
// restore the real updater:
//
//   localStorage.removeItem('mutsumi-update-mock')

const MOCK_KEY = 'mutsumi-update-mock'

function devMockMode(): string | null {
  return import.meta.env.DEV ? localStorage.getItem(MOCK_KEY) : null
}

/** Simulate the check outcome. Returns true when a mock scenario is active. */
function devMockCheck(): boolean {
  const mode = devMockMode()
  if (!mode) return false
  if (mode.startsWith('available')) {
    void recordCheck('success')
    dispatch({
      type: 'CHECK_FOUND',
      version: '9.9.9-mock',
      notes: [
        "What's new in 9.9.9-mock (dev mock)",
        '',
        '- In-app auto-update: get notified when a new version is out',
        '- Fixed the flying-mode ↔ music-mode transition',
        '- Various performance and stability improvements',
      ].join('\n'),
    })
  } else if (mode === 'uptodate') {
    void recordCheck('success')
    dispatch({ type: 'CHECK_NONE' })
  } else {
    void recordCheck('error')
    dispatch({ type: 'CHECK_FAIL', detail: `mock: simulated check failure (${MOCK_KEY}=${mode})` })
  }
  return true
}

/** Simulate the download/install flow. Returns true when mocking is active. */
function devMockInstall(): boolean {
  const mode = devMockMode()
  if (!mode) return false
  const failAt = mode === 'available:install-fail' ? 56 : Infinity
  const timer = setInterval(() => {
    const next = flow.value.percent + 8
    if (next >= failAt) {
      clearInterval(timer)
      dispatch({ type: 'FAIL', detail: `mock: download interrupted at ${flow.value.percent}% (simulated)` })
      return
    }
    if (next >= 100) {
      clearInterval(timer)
      dispatch({ type: 'DOWNLOAD_DONE' })
      setTimeout(() => dispatch({ type: 'INSTALL_DONE' }), 900) // no relaunch in mock
      return
    }
    dispatch({ type: 'PROGRESS', percent: next })
  }, 160)
  return true
}

// ── Locale ───────────────────────────────────────────────────────────
// Keep this window localised like every other (manual choice wins, else system).
watch(
  () => config.value.language,
  lang => setLocale(lang ?? detectLocale()),
  { immediate: true },
)

const downloadingLabel = computed(() =>
  t.value.updateDownloading.replace('{percent}', String(flow.value.percent)),
)

// ── Actions ──────────────────────────────────────────────────────────

/** Populate the view from stashed metadata, or check() when there is none. */
async function load(): Promise<void> {
  dispatch({ type: 'CHECK_START' })
  try {
    currentVersion.value = await getVersion()
  } catch {
    /* getVersion should not fail; leave blank if it does */
  }

  if (devMockCheck()) return

  const pending = await invoke<PendingUpdate | null>('get_pending_update')
  if (pending) {
    // Pre-fetched by the daily check, which already recorded the timestamp.
    dispatch({ type: 'CHECK_FOUND', version: pending.version, notes: pending.notes })
    return
  }

  // Manual path — no pre-fetched update, so check ourselves and record it.
  try {
    const update = await withTimeout(check(), CHECK_TIMEOUT_MS)
    await recordCheck('success')
    if (update) {
      dispatch({ type: 'CHECK_FOUND', version: update.version, notes: update.body ?? '' })
    } else {
      dispatch({ type: 'CHECK_NONE' })
    }
  } catch (e) {
    await recordCheck('error')
    dispatch({ type: 'CHECK_FAIL', detail: errMsg(e) })
  }
}

async function updateNow(): Promise<void> {
  dispatch({ type: 'INSTALL_START' })

  if (devMockInstall()) return

  try {
    const update = await withTimeout(check(), CHECK_TIMEOUT_MS)
    if (!update) {
      // The release vanished between the offer and the install click. Surface
      // it as an install failure (with the reason) rather than silently
      // flipping to "up to date" mid-download.
      dispatch({ type: 'FAIL', detail: 'the release is no longer available on the server' })
      return
    }

    let total = 0
    let downloaded = 0
    await update.downloadAndInstall(event => {
      switch (event.event) {
        case 'Started':
          total = event.data.contentLength ?? 0
          downloaded = 0
          break
        case 'Progress':
          downloaded += event.data.chunkLength
          dispatch({
            type: 'PROGRESS',
            percent: total > 0 ? (downloaded / total) * 100 : 0,
          })
          break
        case 'Finished':
          dispatch({ type: 'DOWNLOAD_DONE' })
          break
      }
    })

    dispatch({ type: 'INSTALL_DONE' })
    await relaunch()
  } catch (e) {
    dispatch({ type: 'FAIL', detail: errMsg(e) })
  }
}

/** Retry after a failure: a failed check re-checks; a failed download/install
 *  returns to the offer so "Update now" can be pressed again. */
function retry(): void {
  const during = flow.value.failedDuring
  dispatch({ type: 'RETRY' })
  if (during === 'check') void load()
}

async function remindLater(): Promise<void> {
  const until = computeSnoozeUntil(Date.now(), snoozeDays.value)
  await updateConfig({ updateSnoozeUntil: until })
  win.close()
}

function onSnoozeInput(e: Event): void {
  const el = e.target as HTMLInputElement
  const clamped = clampSnoozeDays(Number(el.value))
  snoozeDays.value = clamped
  // Force the field to show the clamped value when the typed number is out of
  // range. Vue's diff skips the DOM patch when the clamped number is unchanged
  // (e.g. 300 and 30 both clamp to 30), which is how "309999" appeared to stick.
  if (Number(el.value) !== clamped) el.value = String(clamped)
}

function closeWindow(): void {
  win.close()
}

onMounted(async () => {
  await load()
  await applyWindowSize(flow.value.phase) // size to whatever load() resolved to
  // If the window is already open and the pet window refreshes the metadata,
  // re-read it (open_update_window emits this after re-stashing).
  unlistenPending = await listen('pending-update-changed', () => {
    void load()
  })
})

onUnmounted(() => {
  unlistenPending?.()
})
</script>

<template>
  <div class="shell">
    <div class="orb orb-a" />
    <div class="orb orb-b" />

    <header class="titlebar" data-tauri-drag-region>
      <div class="title-identity" data-tauri-drag-region>
        <span class="title-logo">🥒</span>
        <span class="title-name">Mutsumi</span>
        <span class="title-sep">·</span>
        <span class="title-sub">{{ t.updateWindowTitle }}</span>
      </div>

      <div class="win-controls" @mousedown.stop>
        <button class="wbtn wbtn-close" :title="t.close" @click="closeWindow">
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
            <line x1="1" y1="1" x2="9" y2="9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
            <line x1="9" y1="1" x2="1" y2="9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
          </svg>
        </button>
      </div>
    </header>

    <main class="content">
      <!-- Checking ------------------------------------------------------->
      <section v-if="flow.phase === 'checking'" class="center-state">
        <div class="spinner" />
      </section>

      <!-- Up to date ----------------------------------------------------->
      <section v-else-if="flow.phase === 'notAvailable'" class="center-state">
        <div class="state-emoji">🌿</div>
        <p class="state-text">{{ t.updateUpToDate }}</p>
      </section>

      <!-- Failed (with the underlying reason + retry) --------------------->
      <section v-else-if="flow.phase === 'failed'" class="center-state">
        <div class="state-emoji">🥺</div>
        <p class="state-text">{{ t.updateCheckFailed }}</p>
        <p v-if="flow.errorDetail" class="error-detail">{{ flow.errorDetail }}</p>
        <button class="btn btn-ghost" @click="retry">{{ t.updateRetryBtn }}</button>
      </section>

      <!-- Installed — brief, right before relaunch ----------------------->
      <section v-else-if="flow.phase === 'installed'" class="center-state">
        <div class="state-emoji">🎉</div>
        <p class="state-text">{{ t.updateInstalledRestarting }}</p>
      </section>

      <!-- Downloading / installing --------------------------------------->
      <section v-else-if="flow.phase === 'downloading' || flow.phase === 'installing'" class="center-state">
        <div class="progress-wrap">
          <div class="progress-track">
            <div class="progress-fill" :style="{ width: flow.percent + '%' }" />
          </div>
          <p class="state-text">
            {{ flow.phase === 'installing' ? t.updateInstalling : downloadingLabel }}
          </p>
        </div>
      </section>

      <!-- Update available ----------------------------------------------->
      <template v-else>
        <section class="hero">
          <div class="app-mark">🥒</div>
          <div>
            <h1>{{ t.updateAvailableTitle }}</h1>
            <p class="version-line">
              <span class="v-old">{{ t.updateCurrentVersionLabel }} v{{ currentVersion }}</span>
              <span class="v-arrow">→</span>
              <span class="v-new">{{ t.updateNewVersionLabel }} v{{ flow.version }}</span>
            </p>
          </div>
        </section>

        <section class="card notes-card">
          <h2 class="card-title">{{ t.updateReleaseNotesTitle }}</h2>
          <pre class="notes">{{ flow.notes }}</pre>
        </section>

        <section class="actions">
          <div class="snooze-row">
            <label class="snooze-label" for="snooze-days">{{ t.updateSnoozeDaysLabel }}</label>
            <input
              id="snooze-days"
              class="snooze-input"
              type="number"
              :min="SNOOZE_MIN_DAYS"
              :max="SNOOZE_MAX_DAYS"
              :value="snoozeDays"
              @input="onSnoozeInput"
            >
          </div>
          <p class="snooze-hint">{{ t.updateSnoozeHint }}</p>
          <div class="btn-row">
            <button class="btn btn-ghost" @click="remindLater">{{ t.updateRemindLaterBtn }}</button>
            <button class="btn btn-primary" @click="updateNow">{{ t.updateNowBtn }}</button>
          </div>
        </section>
      </template>
    </main>
  </div>
</template>

<style scoped>
.shell {
  position: relative;
  width: 100%;
  height: 100vh;
  overflow: hidden;
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

.orb {
  position: absolute;
  border-radius: 50%;
  pointer-events: none;
  z-index: 0;
}

.orb-a {
  width: 240px;
  height: 240px;
  background: rgba(119, 153, 119, 0.30);
  filter: blur(72px);
  top: -80px;
  right: -70px;
}

.orb-b {
  width: 180px;
  height: 180px;
  background: rgba(80, 160, 130, 0.22);
  filter: blur(64px);
  bottom: -50px;
  left: -45px;
}

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
.title-name { font-size: 13px; font-weight: 700; color: #1a3a1a; }
.title-sep { font-size: 11px; color: rgba(40, 80, 40, 0.35); }
.title-sub {
  font-size: 10px;
  font-weight: 500;
  color: rgba(40, 80, 40, 0.50);
  text-transform: uppercase;
  letter-spacing: 0.6px;
}

.win-controls { display: flex; align-items: center; gap: 6px; flex-shrink: 0; }

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
.wbtn:hover { background: rgba(220, 60, 60, 0.82); color: white; }
.wbtn:active { transform: scale(0.90); }

.content {
  position: relative;
  z-index: 1;
  flex: 1;
  overflow-y: auto;
  padding: 14px;
  display: flex;
  flex-direction: column;
  gap: 11px;
  scrollbar-width: thin;
  scrollbar-color: rgba(119, 153, 119, 0.30) transparent;
}
.content::-webkit-scrollbar { width: 4px; }
.content::-webkit-scrollbar-thumb { background: rgba(119, 153, 119, 0.30); border-radius: 2px; }

.hero,
.card {
  background: rgba(255, 255, 255, 0.42);
  backdrop-filter: blur(20px) saturate(170%);
  -webkit-backdrop-filter: blur(20px) saturate(170%);
  border: 1px solid rgba(145, 190, 145, 0.35);
  border-radius: 12px;
  box-shadow: 0 1px 8px rgba(50, 90, 50, 0.07), inset 0 1px 0 rgba(255, 255, 255, 0.80);
}

.hero {
  display: grid;
  grid-template-columns: 46px 1fr;
  gap: 12px;
  align-items: center;
  padding: 13px;
  flex-shrink: 0;
}

.app-mark {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 46px;
  height: 46px;
  border-radius: 12px;
  background: linear-gradient(135deg, rgba(119, 153, 119, 0.22), rgba(255, 255, 255, 0.46));
  border: 1px solid rgba(119, 153, 119, 0.28);
  font-size: 25px;
}

.hero h1 { margin: 0 0 5px; font-size: 16px; line-height: 1.2; color: #1a3a1a; }

.version-line { margin: 0; display: flex; align-items: center; gap: 7px; flex-wrap: wrap; font-size: 12.5px; }
.v-old { color: rgba(38, 76, 38, 0.62); }
.v-arrow { color: rgba(38, 76, 38, 0.45); }
.v-new { color: #1f6b3f; font-weight: 700; }

.card { padding: 11px 12px; }
.notes-card { flex: 1; min-height: 90px; display: flex; flex-direction: column; }
.card-title {
  margin: 0 0 8px;
  font-size: 9.5px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.7px;
  color: rgba(45, 85, 45, 0.55);
}

.notes {
  margin: 0;
  flex: 1;
  overflow-y: auto;
  white-space: pre-wrap;
  word-break: break-word;
  font-family: inherit;
  font-size: 12.5px;
  line-height: 1.55;
  color: #2c4e2c;
}

.actions { display: flex; flex-direction: column; gap: 9px; flex-shrink: 0; }

.snooze-row { display: flex; align-items: center; gap: 10px; }
.snooze-label { font-size: 12px; color: rgba(45, 85, 45, 0.75); }
.snooze-input {
  width: 58px;
  padding: 5px 8px;
  border: 1px solid rgba(119, 153, 119, 0.40);
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.6);
  font-size: 13px;
  color: #1a3a1a;
  text-align: center;
}
.snooze-input:focus { outline: none; border-color: rgba(49, 95, 71, 0.7); }

.snooze-hint { margin: 0; font-size: 11px; line-height: 1.5; color: rgba(45, 85, 45, 0.60); }

.btn-row { display: flex; gap: 9px; justify-content: flex-end; }
.btn {
  padding: 8px 16px;
  border: none;
  border-radius: 9px;
  font-size: 13px;
  font-weight: 650;
  cursor: pointer;
  transition: transform 80ms ease, background 120ms ease, box-shadow 120ms ease;
}
.btn:active { transform: scale(0.96); }
.btn-ghost { background: rgba(0, 0, 0, 0.06); color: rgba(40, 70, 40, 0.75); }
.btn-ghost:hover { background: rgba(0, 0, 0, 0.10); }
.btn-primary {
  background: linear-gradient(135deg, #4c9a6f, #327a52);
  color: white;
  box-shadow: 0 2px 8px rgba(50, 120, 80, 0.30);
}
.btn-primary:hover { box-shadow: 0 3px 12px rgba(50, 120, 80, 0.42); }

.center-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  text-align: center;
  padding: 20px;
}
.state-text { margin: 0; font-size: 13.5px; line-height: 1.5; color: #2c4e2c; font-weight: 600; }

.state-emoji {
  font-size: 46px;
  line-height: 1;
  filter: drop-shadow(0 2px 6px rgba(50, 90, 50, 0.18));
  animation: pop 260ms cubic-bezier(0.34, 1.56, 0.64, 1);
}
@keyframes pop { from { transform: scale(0.6); opacity: 0; } to { transform: scale(1); opacity: 1; } }

.error-detail {
  margin: 0;
  max-width: 320px;
  font-size: 11px;
  line-height: 1.5;
  color: rgba(120, 70, 50, 0.72);
  word-break: break-word;
}

.progress-wrap { width: 100%; max-width: 300px; display: flex; flex-direction: column; gap: 10px; }
.progress-track {
  width: 100%;
  height: 8px;
  border-radius: 4px;
  background: rgba(119, 153, 119, 0.22);
  overflow: hidden;
}
.progress-fill {
  height: 100%;
  border-radius: 4px;
  background: linear-gradient(90deg, #4c9a6f, #327a52);
  transition: width 160ms ease;
}

.spinner {
  width: 34px;
  height: 34px;
  border-radius: 50%;
  border: 3px solid rgba(119, 153, 119, 0.25);
  border-top-color: #327a52;
  animation: spin 0.8s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }
</style>
