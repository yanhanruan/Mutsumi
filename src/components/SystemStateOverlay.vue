<script setup lang="ts">
/**
 * SystemStateOverlay — lightweight system state awareness module.
 *
 * Two pages, switched by a tab bar (the static hardware spec sheet is kept
 * off the live-status page on purpose):
 *  • Status   — live CPU / Memory / Network / Uptime / Battery (1 Hz stream).
 *  • Hardware — one-shot CPU / RAM / GPU / storage spec sheet, fetched on
 *               demand via the `get_hardware_info` command and cached (specs
 *               are static, so we only ask once per app run).
 * Styling is consistent with TarotCard overlay (frosted green glass).
 */
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '../i18n'
import { useOverlayVisibility } from '../composables/useOverlayVisibility'

// ── Types ──────────────────────────────────────────────────────────

export type BatteryStatus =
  | { type: 'Charging'; percent: number; time_to_full: number | null }
  | { type: 'Discharging'; percent: number; time_to_empty: number | null }
  | { type: 'PluggedIn'; percent: number }

export type NetworkKind = 'ethernet' | 'wifi' | 'other' | 'offline'

export interface SystemState {
  cpu_usage: number
  mem_usage: number
  network: NetworkKind
  uptime: number
  battery: BatteryStatus | null
}

// Mirror of the Rust `hardware::HardwareInfo` tree (serde-serialized).
export interface CpuInfo { model: string; cores: number; threads: number; frequency_mhz: number }
export interface MemoryInfo { total: number; available: number; used: number }
export interface GpuInfo { name: string; vram: number }
export interface PartitionInfo { mount_point: string; file_system: string; total: number; available: number }
export interface DriveInfo { model: string; kind: 'ssd' | 'hdd' | 'unknown'; total: number }
export interface StorageInfo { drives: DriveInfo[]; partitions: PartitionInfo[] }
export interface HardwareInfo { cpu: CpuInfo; memory: MemoryInfo; gpus: GpuInfo[]; storage: StorageInfo }

const emit = defineEmits<{ close: [] }>()

const { t } = useI18n()

// Visibility + the "skip leave fade on dismiss" guard that prevents an
// afterimage when PetWindow shrinks the window back on close.
const { visible, skipLeave, show, dismiss } = useOverlayVisibility()
const state = ref<SystemState | null>(null)

const activeTab = ref<'status' | 'hardware'>('status')
const hardware = ref<HardwareInfo | null>(null)
const hwLoading = ref(false)
const hwError = ref(false)

let unlisten: UnlistenFn | null = null

// ── Helpers ────────────────────────────────────────────────────────

const formatUptime = (seconds: number) => {
  const h = Math.floor(seconds / 3600)
  const m = Math.floor((seconds % 3600) / 60)
  return `${h}h ${m}m`
}

const formatTime = (seconds: number) => {
  const h = Math.floor(seconds / 3600)
  const m = Math.floor((seconds % 3600) / 60)
  if (h > 0) return `${h}h ${m}m`
  return `${m}m`
}

const networkIcon = (kind: NetworkKind) => {
  if (kind === 'wifi' || kind === 'ethernet') return '🌐'
  if (kind === 'offline') return '🚫'
  return '🌐'
}

const networkLabel = (kind: NetworkKind) => {
  if (kind === 'wifi') return t.value.sys.wifi
  if (kind === 'ethernet') return t.value.sys.ethernet
  if (kind === 'offline') return t.value.sys.offline
  return t.value.sys.online
}

/** Humanize a byte count to the largest sensible unit (B…TB). */
const formatBytes = (bytes: number) => {
  if (!bytes || bytes <= 0) return '—'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let v = bytes
  let i = 0
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024
    i++
  }
  // Whole numbers for B/KB and for large values; one decimal otherwise.
  const decimals = i <= 1 || v >= 100 ? 0 : 1
  return `${v.toFixed(decimals)} ${units[i]}`
}

const formatFreq = (mhz: number) =>
  mhz >= 1000 ? `${(mhz / 1000).toFixed(2)} GHz` : `${mhz} MHz`

const partUsedPct = (p: PartitionInfo) =>
  p.total > 0 ? ((p.total - p.available) / p.total) * 100 : 0

const memUsedPct = computed(() => {
  const m = hardware.value?.memory
  return m && m.total > 0 ? (m.used / m.total) * 100 : 0
})

const driveKindLabel = (kind: DriveInfo['kind']) =>
  kind === 'ssd' ? t.value.sys.hw.ssd : t.value.sys.hw.hdd

/** Map a 0–100 fill ratio to a colour band (green → yellow → amber → red). */
const levelClass = (pct: number) =>
  pct >= 75 ? 'level-crit' : pct >= 50 ? 'level-high' : pct >= 25 ? 'level-mid' : 'level-low'

/** Fetch the hardware spec sheet once and cache it for the app's lifetime. */
async function loadHardware() {
  if (hardware.value || hwLoading.value) return
  hwLoading.value = true
  hwError.value = false
  try {
    hardware.value = await invoke<HardwareInfo>('get_hardware_info')
  } catch (e) {
    console.error('get_hardware_info failed:', e)
    hwError.value = true
  } finally {
    hwLoading.value = false
  }
}

function selectTab(tab: 'status' | 'hardware') {
  activeTab.value = tab
  if (tab === 'hardware') void loadHardware()
}

// ── Lifecycle ──────────────────────────────────────────────────────

onMounted(async () => {
  unlisten = await listen<SystemState>('system-state', (event) => {
    state.value = event.payload
  })
})

onUnmounted(() => {
  unlisten?.()
})

// ── Public API ─────────────────────────────────────────────────────
function open() {
  activeTab.value = 'status'
  show()
}

function requestClose() {
  emit('close')
}

defineExpose({ open, dismiss })
</script>

<template>
  <Transition name="fade" :css="!skipLeave">
    <div v-if="visible" class="system-overlay">
      <div class="panel pet-ui-overlay" data-tauri-drag-region>
        <button class="ctrl-btn" @click.stop="requestClose" @mousedown.stop>×</button>

        <h2 class="panel-title" data-tauri-drag-region>{{ t.sys.title }}</h2>

        <!-- Tab switcher: live status vs. static hardware spec sheet -->
        <div class="tabs">
          <button class="tab" :class="{ active: activeTab === 'status' }"
                  @click.stop="selectTab('status')" @mousedown.stop>
            {{ t.sys.tabStatus }}
          </button>
          <button class="tab" :class="{ active: activeTab === 'hardware' }"
                  @click.stop="selectTab('hardware')" @mousedown.stop>
            {{ t.sys.tabHardware }}
          </button>
        </div>

        <!-- Both pages share one box. The status page is always laid out and
             sets the body height; the hardware page overlays it and scrolls
             internally — so the panel stays the same compact height on both
             tabs (and never grows past the window). -->
        <div class="pages">

        <!-- ── Status page ──────────────────────────────────────────── -->
        <div class="page status-page" :class="{ 'page-ghost': activeTab !== 'status' }">
        <div v-if="!state" class="loading-state">
          Loading...
        </div>

        <div v-else class="metrics-content">
          <!-- CPU & RAM Side-by-Side -->
          <div class="top-row">
            <div class="metric-card">
              <div class="pie-chart" :style="{ '--p': state.cpu_usage + '%' }"></div>
              <div class="metric-info">
                <span class="label">{{ t.sys.cpu }}</span>
                <span class="value">{{ Math.round(state.cpu_usage) }}%</span>
              </div>
            </div>
            <div class="metric-card">
              <div class="pie-chart" :style="{ '--p': state.mem_usage + '%' }"></div>
              <div class="metric-info">
                <span class="label">{{ t.sys.memory }}</span>
                <span class="value">{{ Math.round(state.mem_usage) }}%</span>
              </div>
            </div>
          </div>

          <!-- Divider -->
          <div class="divider"></div>

          <!-- List Metrics Grid -->
          <div class="list-grid">
            <!-- Network -->
            <span class="icon">{{ networkIcon(state.network) }}</span>
            <span class="label">{{ t.sys.network }}</span>
            <span class="value" :class="state.network === 'offline' ? 'offline' : 'online'">
              {{ networkLabel(state.network) }}
            </span>

            <!-- Uptime -->
            <span class="icon">⏱️</span>
            <span class="label">{{ t.sys.uptime }}</span>
            <span class="value">{{ formatUptime(state.uptime) }}</span>
          </div>

          <!-- Battery Section -->
          <div v-if="state.battery" class="battery-section">
            <div class="battery-header">
              <div class="battery-title">
                <span class="icon">🔋</span>
                <span class="label">{{ t.sys.battery }}</span>
              </div>
              <span class="value">{{ state.battery.percent }}%</span>
            </div>
            <div class="battery-track">
              <div class="battery-fill" :style="{ width: state.battery.percent + '%' }"
                   :class="state.battery.type.toLowerCase()"></div>
            </div>
            <div class="battery-status" v-if="state.battery.type === 'Charging'">
              {{ state.battery.time_to_full
                   ? t.sys.charging.replace('{time}', formatTime(state.battery.time_to_full))
                   : t.sys.chargingPlain }}
            </div>
            <div class="battery-status" v-if="state.battery.type === 'Discharging'">
              {{ state.battery.time_to_empty
                   ? t.sys.discharging.replace('{time}', formatTime(state.battery.time_to_empty))
                   : t.sys.dischargingPlain }}
            </div>
            <div class="battery-status" v-if="state.battery.type === 'PluggedIn'">
              {{ t.sys.pluggedIn }}
            </div>
          </div>
        </div>
        </div>

        <!-- ── Hardware page (overlays the status box, scrolls within it) ── -->
        <div v-show="activeTab === 'hardware'" class="page hw-page hw-content">
          <div v-if="hwLoading" class="loading-state">{{ t.sys.hw.loading }}</div>
          <div v-else-if="hwError" class="loading-state">{{ t.sys.hw.error }}</div>

          <template v-else-if="hardware">
            <!-- CPU -->
            <section class="hw-card">
              <div class="hw-card-label">{{ t.sys.cpu }}</div>
              <div class="hw-cpu-model">{{ hardware.cpu.model }}</div>
              <div class="hw-cpu-stats">
                <div class="hw-cpu-stat">
                  <span class="num">{{ hardware.cpu.cores }}</span>
                  <span class="cap">{{ t.sys.hw.cores }}</span>
                </div>
                <div class="hw-cpu-stat">
                  <span class="num">{{ hardware.cpu.threads }}</span>
                  <span class="cap">{{ t.sys.hw.threads }}</span>
                </div>
                <div class="hw-cpu-stat" v-if="hardware.cpu.frequency_mhz">
                  <span class="num">{{ formatFreq(hardware.cpu.frequency_mhz) }}</span>
                  <span class="cap">{{ t.sys.hw.frequency }}</span>
                </div>
              </div>
            </section>

            <!-- Memory (progress bar — clearer than a pie for used/total) -->
            <section class="hw-card">
              <div class="hw-card-label">{{ t.sys.memory }}</div>
              <div class="mem-bar">
                <div class="mem-head">
                  <span class="mem-used">{{ formatBytes(hardware.memory.used) }} / {{ formatBytes(hardware.memory.total) }}</span>
                  <span class="mem-pct">{{ Math.round(memUsedPct) }}%</span>
                </div>
                <div class="mem-track">
                  <div class="mem-fill" :class="levelClass(memUsedPct)"
                       :style="{ width: memUsedPct + '%' }"></div>
                </div>
                <div class="mem-sub">{{ formatBytes(hardware.memory.available) }} {{ t.sys.hw.free }}</div>
              </div>
            </section>

            <!-- GPU -->
            <section v-if="hardware.gpus.length" class="hw-card">
              <div class="hw-card-label">{{ t.sys.hw.gpu }}</div>
              <div v-for="(g, i) in hardware.gpus" :key="i" class="hw-row">
                <span class="hw-row-name">{{ g.name }}</span>
                <span v-if="g.vram" class="hw-row-meta">{{ t.sys.hw.vram }} {{ formatBytes(g.vram) }}</span>
              </div>
            </section>

            <!-- Physical drives -->
            <section v-if="hardware.storage.drives.length" class="hw-card">
              <div class="hw-card-label">{{ t.sys.hw.storage }}</div>
              <div v-for="(d, i) in hardware.storage.drives" :key="i" class="hw-row">
                <span class="hw-row-name">
                  {{ d.model }}
                  <span v-if="d.kind !== 'unknown'" class="hw-badge">{{ driveKindLabel(d.kind) }}</span>
                </span>
                <span class="hw-row-meta">{{ formatBytes(d.total) }}</span>
              </div>
            </section>

            <!-- Partitions -->
            <section v-if="hardware.storage.partitions.length" class="hw-card">
              <div class="hw-card-label">{{ t.sys.hw.partitions }}</div>
              <div v-for="(p, i) in hardware.storage.partitions" :key="i" class="hw-part">
                <div class="hw-part-head">
                  <span class="hw-part-mount">{{ p.mount_point }}</span>
                  <span class="hw-part-fs">{{ p.file_system }}</span>
                  <span class="hw-part-size">{{ formatBytes(p.total - p.available) }} / {{ formatBytes(p.total) }}</span>
                </div>
                <div class="hw-part-track">
                  <div class="hw-part-fill" :class="levelClass(partUsedPct(p))"
                       :style="{ width: partUsedPct(p) + '%' }"></div>
                </div>
                <div class="hw-part-free">{{ formatBytes(p.available) }} {{ t.sys.hw.free }}</div>
              </div>
            </section>
          </template>
        </div>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
/* ── Overlay ───────────────────────────────────────────────────────── */
.system-overlay {
  position: fixed;
  inset: 0;
  z-index: 1000;
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  user-select: none;
  font-family: system-ui, "Segoe UI", sans-serif;
  transition: transform 0.2s ease;
}

/* ── Panel styling (similar to TarotCard) ─────────────────────────── */
.panel {
  position: relative;
  width: min(90%, 280px);
  max-height: calc(100vh - 24px);
  display: flex;
  flex-direction: column;
  background: rgba(245, 250, 245, 0.94);
  border: 1px solid rgba(148, 185, 148, 0.45);
  border-radius: 14px;
  /* Clip the scrolling page to the rounded corners so no card edge bleeds
     past the border (was visible as a "cut" corner in small mode). */
  overflow: hidden;
  padding: 20px 20px 16px;
  backdrop-filter: blur(20px) saturate(160%);
  -webkit-backdrop-filter: blur(20px) saturate(160%);
  box-shadow: 0 2px 6px rgba(40, 70, 40, 0.06), inset 0 1px 0 rgba(255, 255, 255, 0.6);
  color: #1a2e1a;
}

/* Close button pinned to top right inner corner */
.ctrl-btn {
  position: absolute;
  top: 12px;
  right: 12px;
  width: 22px;
  height: 22px;
  border-radius: 50%;
  border: 1px solid rgba(119, 153, 119, 0.4);
  background: rgba(245, 250, 245, 0.9);
  color: #2a4a2a;
  font-size: 14px;
  line-height: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: transform 0.15s ease, background 0.15s ease;
  z-index: 10;
}
.ctrl-btn:hover {
  transform: scale(1.1);
  background: #fff;
}

.panel-title {
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.8px;
  color: rgba(45, 85, 45, 0.6);
  margin: 0 0 12px;
  text-align: center;
  flex-shrink: 0;
}

/* ── Tabs ──────────────────────────────────────────────────────────── */
.tabs {
  display: flex;
  gap: 4px;
  padding: 3px;
  margin: 0 0 14px;
  background: rgba(119, 153, 119, 0.12);
  border-radius: 9px;
  flex-shrink: 0;
}

.tab {
  flex: 1;
  padding: 5px 0;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: rgba(45, 85, 45, 0.7);
  font-family: inherit;
  font-size: 11px;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.15s ease, color 0.15s ease;
}

.tab:hover:not(.active) {
  color: #2a4a2a;
}

.tab.active {
  background: rgba(255, 255, 255, 0.85);
  color: #1a2e1a;
  box-shadow: 0 1px 2px rgba(40, 70, 40, 0.08);
}

/* ── Pages ─────────────────────────────────────────────────────────── */
/* The status page (in normal flow) sets this box's height; the hardware page
   is absolutely positioned on top and scrolls within the very same height —
   so the panel is identically compact on both tabs and never grows past the
   window. */
.pages {
  position: relative;
  flex: 0 1 auto;
  min-height: 0;
}

/* Scrollable body, scrollbar hidden. */
.page {
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-width: none;       /* Firefox */
  -ms-overflow-style: none;    /* legacy Edge */
}
.page::-webkit-scrollbar { width: 0; height: 0; display: none; }

/* Inactive status page: invisible but still laid out, so it keeps defining the
   box height while the hardware tab is showing. */
.page-ghost { visibility: hidden; }

/* Hardware overlay — fills the status box and scrolls. The whole panel stays a
   drag region, so dragging works anywhere; scroll the list with the wheel. */
.hw-page {
  position: absolute;
  inset: 0;
}

.loading-state {
  text-align: center;
  font-size: 12px;
  color: rgba(45, 85, 45, 0.6);
  padding: 20px 0;
}

/* ── Content Layout ──────────────────────────────────────────────── */
.metrics-content {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.top-row {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}

.metric-card {
  display: flex;
  flex-direction: row;
  align-items: center;
  gap: 10px;
  padding: 10px;
  background: rgba(255, 255, 255, 0.4);
  border-radius: 10px;
  border: 1px solid rgba(119, 153, 119, 0.2);
}

.pie-chart {
  width: 28px;
  height: 28px;
  border-radius: 50%;
  /* A hard conic stop renders a visibly serrated (stair-stepped) seam in
     WebView2. Feathering the fill→track transition by ~1% on each side smooths
     the seam away.
     The conic also converges to a muddy point at its centre — ugly at small
     percentages — so a matching radial hub covers the centre, turning the disc
     into a clean ring and hiding the convergence entirely. */
  background:
    radial-gradient(circle at center, rgba(245, 250, 245, 0.9) 0 5px, transparent 5.5px),
    conic-gradient(
      var(--fill, #5a9960) calc(var(--p) - 1%),
      rgba(119, 153, 119, 0.2) calc(var(--p) + 1%)
    );
  box-shadow: inset 0 0 0 4px rgba(245, 250, 245, 0.9);
  flex-shrink: 0;
}

.metric-info {
  display: flex;
  flex-direction: column;
}

.metric-info .label {
  font-size: 10px;
  font-weight: 600;
  color: rgba(45, 85, 45, 0.6);
  text-transform: uppercase;
}

.metric-info .value {
  font-size: 15px;
  font-weight: 700;
  color: #1a2e1a;
  line-height: 1.1;
}

.divider {
  height: 1px;
  background: rgba(119, 153, 119, 0.2);
  margin: 0 -4px;
}

/* ── List Grid ───────────────────────────────────────────────────── */
.list-grid {
  display: grid;
  grid-template-columns: auto 1fr auto;
  gap: 12px 10px;
  align-items: center;
  font-size: 12px;
  padding: 0 4px;
}

.list-grid .icon {
  font-size: 14px;
}

.list-grid .label {
  font-weight: 500;
  color: #2a4a2a;
}

.list-grid .value {
  font-weight: 600;
  text-align: right;
  white-space: nowrap;
}

.value.online { color: #5a9960; }
.value.offline { color: #a86a6a; }

/* ── Battery Section ─────────────────────────────────────────────── */
.battery-section {
  padding: 0 4px;
}

.battery-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 8px;
}

.battery-title {
  display: flex;
  align-items: center;
  gap: 10px;
}

.battery-title .icon { font-size: 14px; }
.battery-title .label { font-size: 12px; font-weight: 500; color: #2a4a2a; }
.battery-header .value { font-size: 12px; font-weight: 700; }

.battery-track {
  height: 6px;
  background: rgba(119, 153, 119, 0.2);
  border-radius: 4px;
  overflow: hidden;
  margin-bottom: 6px;
}

.battery-fill {
  height: 100%;
  border-radius: 4px;
  transition: width 0.3s ease;
}
.battery-fill.charging { background: #5a9960; }
.battery-fill.discharging { background: #a88b6a; }
.battery-fill.pluggedin { background: #5a9960; }

.battery-status {
  font-size: 10px;
  color: rgba(45, 85, 45, 0.6);
  text-align: right;
}

/* ── Hardware page ───────────────────────────────────────────────── */
.hw-content {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 0 2px 2px;
}

/* Every spec group is a soft card for a consistent, tidy look. */
.hw-card {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px 11px;
  background: rgba(255, 255, 255, 0.45);
  border: 1px solid rgba(119, 153, 119, 0.2);
  border-radius: 10px;
  flex-shrink: 0;   /* keep full height and let the page scroll, never compress */
}

.hw-card-label {
  font-size: 10px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.6px;
  color: rgba(45, 85, 45, 0.55);
}

/* CPU: model line + a divided stat strip (cores | threads | freq). */
.hw-cpu-model {
  font-size: 12px;
  font-weight: 700;
  color: #1a2e1a;
  line-height: 1.35;
}

.hw-cpu-stats {
  display: flex;
  border-top: 1px solid rgba(119, 153, 119, 0.18);
  padding-top: 8px;
}

.hw-cpu-stat {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  border-left: 1px solid rgba(119, 153, 119, 0.18);
}
.hw-cpu-stat:first-child { border-left: none; }

.hw-cpu-stat .num {
  font-size: 14px;
  font-weight: 700;
  color: #1a2e1a;
}

.hw-cpu-stat .cap {
  font-size: 9px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.4px;
  color: rgba(45, 85, 45, 0.55);
}

/* Memory: progress bar (clearer than a pie for used / total). */
.mem-bar {
  display: flex;
  flex-direction: column;
  gap: 5px;
}

.mem-head {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  font-size: 11px;
}
.mem-used { font-weight: 700; color: #1a2e1a; }
.mem-pct { font-weight: 700; color: #1a2e1a; }

.mem-track {
  height: 6px;
  background: rgba(119, 153, 119, 0.2);
  border-radius: 4px;
  overflow: hidden;
}

.mem-fill {
  height: 100%;
  background: var(--fill, #5a9960);
  border-radius: 4px;
  transition: width 0.3s ease;
}

.mem-sub {
  font-size: 9px;
  color: rgba(45, 85, 45, 0.5);
  text-align: right;
}

/* GPU / physical-drive rows. */
.hw-row {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: 8px;
}
.hw-row + .hw-row {
  padding-top: 8px;
  border-top: 1px solid rgba(119, 153, 119, 0.15);
}

.hw-row-name {
  font-size: 12px;
  font-weight: 600;
  color: #1a2e1a;
  line-height: 1.3;
  word-break: break-word;
}

.hw-row-meta {
  font-size: 11px;
  font-weight: 600;
  color: rgba(45, 85, 45, 0.7);
  white-space: nowrap;
}

.hw-badge {
  margin-left: 6px;
  padding: 1px 6px;
  border-radius: 999px;
  background: rgba(90, 153, 96, 0.16);
  color: #5a9960;
  font-size: 9px;
  font-weight: 700;
  letter-spacing: 0.4px;
  vertical-align: middle;
}

/* Partitions with a used/total bar. */
.hw-part {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.hw-part + .hw-part {
  margin-top: 8px;
  padding-top: 8px;
  border-top: 1px solid rgba(119, 153, 119, 0.15);
}

.hw-part-head {
  display: flex;
  align-items: baseline;
  gap: 8px;
  font-size: 11px;
}

.hw-part-mount {
  font-weight: 700;
  color: #1a2e1a;
}

.hw-part-fs {
  color: rgba(45, 85, 45, 0.5);
  font-size: 10px;
}

.hw-part-size {
  margin-left: auto;
  font-weight: 600;
  color: rgba(45, 85, 45, 0.7);
  white-space: nowrap;
}

.hw-part-track {
  height: 5px;
  background: rgba(119, 153, 119, 0.2);
  border-radius: 3px;
  overflow: hidden;
}

.hw-part-fill {
  height: 100%;
  background: var(--fill, #5a9960);
  border-radius: 3px;
  transition: width 0.3s ease;
}

.hw-part-free {
  font-size: 9px;
  color: rgba(45, 85, 45, 0.5);
  text-align: right;
}

/* Usage colour bands — shared by the pie (--fill in its conic) and the
   partition bars (background). 0–25 green · 25–50 yellow · 50–75 amber ·
   75–100 red. */
.level-low  { --fill: #5a9960; }
.level-mid  { --fill: #b6a93f; }
.level-high { --fill: #d89a4e; }
.level-crit { --fill: #cc6b6b; }

/* ── Transition ──────────────────────────────────────────────────── */
.fade-enter-active, .fade-leave-active { transition: opacity 0.2s ease; }
.fade-enter-from, .fade-leave-to { opacity: 0; }
</style>
