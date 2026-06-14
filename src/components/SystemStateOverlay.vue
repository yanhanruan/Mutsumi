<script setup lang="ts">
/**
 * SystemStateOverlay — lightweight system state awareness module.
 *
 * Displays CPU, Memory, Network, Uptime, and Battery status.
 * Visuals: Small pie charts for CPU/RAM, progress bar for battery.
 * Styling is consistent with TarotCard overlay (frosted green glass).
 */
import { ref, onMounted, onUnmounted } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useI18n } from '../i18n'

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

const emit = defineEmits<{ close: [] }>()

const { t } = useI18n()

const visible = ref(false)
const state = ref<SystemState | null>(null)

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
  visible.value = true
}

function requestClose() {
  emit('close')
}

function dismiss() {
  visible.value = false
}

defineExpose({ open, dismiss })
</script>

<template>
  <Transition name="fade">
    <div v-if="visible" class="system-overlay">
      <div class="panel pet-ui-overlay" data-tauri-drag-region>
        <button class="ctrl-btn" @click.stop="requestClose" @mousedown.stop>×</button>

        <h2 class="panel-title" data-tauri-drag-region>{{ t.sys.title }}</h2>

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
  background: rgba(245, 250, 245, 0.94);
  border: 1px solid rgba(148, 185, 148, 0.45);
  border-radius: 14px;
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
  margin: 0 0 16px;
  text-align: center;
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
      #5a9960 calc(var(--p) - 1%),
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

/* ── Transition ──────────────────────────────────────────────────── */
.fade-enter-active, .fade-leave-active { transition: opacity 0.2s ease; }
.fade-enter-from, .fade-leave-to { opacity: 0; }
</style>
