<script setup lang="ts">
/**
 * SystemStateOverlay — lightweight system state awareness module.
 *
 * Displays CPU, Memory, Temperature, Network, Uptime, and Battery status.
 * Visuals: Small pie charts for CPU/RAM, progress bar for battery.
 * Styling is consistent with TarotCard overlay (frosted green glass).
 */
import { ref, onMounted, onUnmounted } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useI18n } from '../i18n'
import { useAppConfig } from '../composables/useAppConfig'

// ── Types ──────────────────────────────────────────────────────────

export type BatteryStatus =
  | { type: 'Charging'; percent: number; time_to_full: number | null }
  | { type: 'Discharging'; percent: number; time_to_empty: number | null }
  | { type: 'PluggedIn'; percent: number }

export interface SystemState {
  cpu_usage: number
  mem_usage: number
  temperature: number | null
  network_connected: boolean
  uptime: number
  battery: BatteryStatus | null
}

const emit = defineEmits<{ close: [] }>()

const { t } = useI18n()
const { config } = useAppConfig()

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

function dismiss() {
  visible.value = false
  emit('close')
}

defineExpose({ open, dismiss })
</script>

<template>
  <Transition name="fade">
    <div v-if="visible" class="system-overlay pet-ui-overlay" :style="{ transform: `scale(${config.characterSize === 'large' ? 1.2 : config.characterSize === 'small' ? 0.8 : 1})` }">
      <div class="controls">
        <button class="ctrl-btn" @click.stop="dismiss">×</button>
      </div>

      <div class="panel">
        <h2 class="panel-title">System Status</h2>

        <div v-if="!state" class="loading-state">
          Loading...
        </div>

        <div v-else class="metrics-grid">
          <!-- CPU -->
          <div class="metric-card">
            <div class="pie-chart" :style="{ '--p': state.cpu_usage + '%' }"></div>
            <div class="metric-info">
              <span class="label">CPU</span>
              <span class="value">{{ Math.round(state.cpu_usage) }}%</span>
            </div>
          </div>

          <!-- Memory -->
          <div class="metric-card">
            <div class="pie-chart" :style="{ '--p': state.mem_usage + '%' }"></div>
            <div class="metric-info">
              <span class="label">Memory</span>
              <span class="value">{{ Math.round(state.mem_usage) }}%</span>
            </div>
          </div>

          <!-- Temperature -->
          <div class="metric-row">
            <span class="icon">🌡️</span>
            <span class="label">Temp</span>
            <span class="value">{{ state.temperature ? Math.round(state.temperature) + '°C' : '--' }}</span>
          </div>

          <!-- Network -->
          <div class="metric-row">
            <span class="icon">🌐</span>
            <span class="label">Network</span>
            <span class="value" :class="state.network_connected ? 'online' : 'offline'">
              {{ state.network_connected ? 'Online' : 'Offline' }}
            </span>
          </div>

          <!-- Uptime -->
          <div class="metric-row">
            <span class="icon">⏱️</span>
            <span class="label">Uptime</span>
            <span class="value">{{ formatUptime(state.uptime) }}</span>
          </div>

          <!-- Battery -->
          <div v-if="state.battery" class="battery-section">
            <div class="battery-header">
              <span class="icon">🔋</span>
              <span class="label">Battery</span>
              <span class="value">{{ state.battery.percent }}%</span>
            </div>
            <div class="battery-track">
              <div class="battery-fill" :style="{ width: state.battery.percent + '%' }"
                   :class="state.battery.type.toLowerCase()"></div>
            </div>
            <div class="battery-status" v-if="state.battery.type === 'Charging' && state.battery.time_to_full">
              Charging ({{ formatTime(state.battery.time_to_full) }} to full)
            </div>
            <div class="battery-status" v-if="state.battery.type === 'Discharging' && state.battery.time_to_empty">
              Discharging ({{ formatTime(state.battery.time_to_empty) }} left)
            </div>
            <div class="battery-status" v-if="state.battery.type === 'PluggedIn'">
              Plugged In
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
  width: min(90%, 280px);
  background: rgba(245, 250, 245, 0.94);
  border: 1px solid rgba(148, 185, 148, 0.45);
  border-radius: 14px;
  padding: 16px;
  backdrop-filter: blur(20px) saturate(160%);
  -webkit-backdrop-filter: blur(20px) saturate(160%);
  box-shadow: 0 4px 14px rgba(40, 70, 40, 0.16), inset 0 1px 0 rgba(255, 255, 255, 0.8);
  color: #1a2e1a;
}

.panel-title {
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.8px;
  color: rgba(45, 85, 45, 0.6);
  margin: 0 0 14px;
  text-align: center;
}

.loading-state {
  text-align: center;
  font-size: 12px;
  color: rgba(45, 85, 45, 0.6);
  padding: 20px 0;
}

.metrics-grid {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

/* ── Custom Pie Charts for CPU/RAM ───────────────────────────────── */
.metric-card {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 12px;
  background: rgba(255, 255, 255, 0.4);
  border-radius: 10px;
  border: 1px solid rgba(119, 153, 119, 0.2);
}

.pie-chart {
  width: 32px;
  height: 32px;
  border-radius: 50%;
  background: conic-gradient(#5a9960 var(--p), rgba(119, 153, 119, 0.2) 0);
  box-shadow: inset 0 0 0 4px rgba(245, 250, 245, 0.9);
}

.metric-info {
  display: flex;
  flex-direction: column;
  flex: 1;
}

.metric-info .label {
  font-size: 10px;
  font-weight: 600;
  color: rgba(45, 85, 45, 0.6);
  text-transform: uppercase;
}

.metric-info .value {
  font-size: 14px;
  font-weight: 700;
  color: #1a2e1a;
}

/* ── Simple Metric Rows ──────────────────────────────────────────── */
.metric-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  padding: 4px 6px;
}

.metric-row .icon {
  font-size: 14px;
}

.metric-row .label {
  flex: 1;
  font-weight: 500;
  color: #2a4a2a;
}

.metric-row .value {
  font-weight: 600;
}

.value.online { color: #5a9960; }
.value.offline { color: #a86a6a; }

/* ── Battery Section ─────────────────────────────────────────────── */
.battery-section {
  margin-top: 4px;
  padding-top: 12px;
  border-top: 1px solid rgba(119, 153, 119, 0.2);
}

.battery-header {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  margin-bottom: 8px;
}

.battery-header .label { flex: 1; font-weight: 500; color: #2a4a2a; }
.battery-header .value { font-weight: 700; }

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

/* ── Controls ────────────────────────────────────────────────────── */
.controls {
  position: absolute;
  top: 12px;
  right: 12px;
  z-index: 10;
}

.ctrl-btn {
  width: 24px;
  height: 24px;
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
}

.ctrl-btn:hover {
  transform: scale(1.1);
  background: #fff;
}

/* ── Transition ──────────────────────────────────────────────────── */
.fade-enter-active, .fade-leave-active { transition: opacity 0.2s ease; }
.fade-enter-from, .fade-leave-to { opacity: 0; }
</style>
