<script setup lang="ts">
import { computed } from 'vue'
import { isMacOSDesktop } from '../config/desktopPlatform'

type WindowControl = 'minimize' | 'close'

const props = withDefaults(defineProps<{
  subtitle: string
  closeLabel: string
  minimizable?: boolean
  minimizeLabel?: string
}>(), {
  minimizable: false,
  minimizeLabel: '',
})

const emit = defineEmits<{
  minimize: []
  close: []
}>()

const macOS = isMacOSDesktop()
const controls = computed<WindowControl[]>(() => {
  if (!props.minimizable) return ['close']
  return macOS ? ['close', 'minimize'] : ['minimize', 'close']
})

function activate(control: WindowControl): void {
  if (control === 'minimize') emit('minimize')
  else emit('close')
}
</script>

<template>
  <header
    class="titlebar"
    :class="{ 'platform-macos': macOS }"
    data-tauri-drag-region
  >
    <div class="title-identity" data-tauri-drag-region>
      <span class="title-logo">🥒</span>
      <span class="title-name">Mutsumi</span>
      <span class="title-sep">·</span>
      <span class="title-sub">{{ subtitle }}</span>
    </div>

    <div class="win-controls" @mousedown.stop>
      <button
        v-for="control in controls"
        :key="control"
        class="wbtn"
        :class="`wbtn-${control === 'minimize' ? 'min' : 'close'}`"
        :title="control === 'minimize' ? minimizeLabel : closeLabel"
        :aria-label="control === 'minimize' ? minimizeLabel : closeLabel"
        @click="activate(control)"
      >
        <svg v-if="control === 'minimize'" width="10" height="2" viewBox="0 0 10 2" aria-hidden="true">
          <rect width="10" height="1.5" rx="0.75" fill="currentColor" />
        </svg>
        <svg v-else width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
          <line x1="1" y1="1" x2="9" y2="9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
          <line x1="9" y1="1" x2="1" y2="9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
        </svg>
      </button>
    </div>
  </header>
</template>

<style scoped>
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
.title-name { font-size: 13px; font-weight: 700; color: #1a3a1a; letter-spacing: -0.2px; }
.title-sep { font-size: 11px; color: rgba(40, 80, 40, 0.35); }

.title-sub {
  font-size: 10px;
  font-weight: 500;
  color: rgba(40, 80, 40, 0.50);
  text-transform: uppercase;
  letter-spacing: 0.6px;
}

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
  padding: 0;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  background: rgba(0, 0, 0, 0.06);
  color: rgba(40, 70, 40, 0.55);
  transition: background 100ms ease, color 100ms ease, transform 80ms ease;
}

.wbtn:hover { background: rgba(0, 0, 0, 0.12); color: rgba(20, 50, 20, 0.85); }
.wbtn:active { transform: scale(0.90); }
.wbtn:focus-visible { outline: 2px solid rgba(41, 96, 67, 0.75); outline-offset: 2px; }
.wbtn-close:hover { background: rgba(220, 60, 60, 0.82); color: white; }
.wbtn-min:hover { background: rgba(200, 145, 30, 0.82); color: white; }

/* Match macOS traffic-light placement while preserving Windows chrome above. */
.platform-macos {
  justify-content: flex-start;
  gap: 10px;
  padding: 0 14px 0 12px;
}

.platform-macos .win-controls {
  order: -1;
  gap: 8px;
}

.platform-macos .wbtn {
  width: 12px;
  height: 12px;
  border: 0.5px solid rgba(0, 0, 0, 0.16);
  border-radius: 50%;
  color: transparent;
}

.platform-macos .wbtn-close { background: #ff5f57; }

.platform-macos .wbtn-min { background: #febc2e; }

.platform-macos .win-controls:hover .wbtn,
.platform-macos .wbtn:focus-visible {
  color: rgba(52, 38, 29, 0.72);
}

.platform-macos .wbtn:hover {
  filter: brightness(0.96);
}
</style>
