<script setup lang="ts">
/**
 * ChatHistory — advanced search over the persisted chat transcript.
 *
 * A compact full-overlay shown on top of ChatPanel (the pet window is ≤400px
 * wide). Supports keyword search and/or a date range, combinable; results are
 * newest-first. Clicking a result emits `jump` with the message id so the parent
 * can navigate the live thread to that message.
 *
 * Backed by the `chat_search_history` Tauri command. Open/close is parent-driven
 * via the exposed methods (mirrors TarotCard / ChatPanel).
 */
import { ref, watch, computed, nextTick, onBeforeUnmount } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '../i18n'
import type { StoredMessage } from '../config/chat'
import DatePicker from './DatePicker.vue'
import { minimizeWindow } from '../composables/useWindowControls'

const emit = defineEmits<{ jump: [id: number]; close: [] }>()
const { t, locale } = useI18n()

const SEARCH_LIMIT = 60
const DEBOUNCE_MS = 250

const visible  = ref(false)
const query    = ref('')
const fromDate = ref('')   // 'YYYY-MM-DD' from the DatePicker (empty = unset)
const toDate   = ref('')
const results  = ref<StoredMessage[]>([])
const searched = ref(false)  // a query has run at least once with active filters

const inputRef = ref<HTMLInputElement | null>(null)

function open() {
  visible.value = true
  searched.value = false
  results.value = []
  nextTick(() => inputRef.value?.focus())
}
function close() {
  visible.value = false
}
defineExpose({ open, close })

// ── Date helpers ────────────────────────────────────────────────────────────
/** Local start-of-day epoch seconds for a 'YYYY-MM-DD' string, or null. */
function dayStart(d: string): number | null {
  if (!d) return null
  const ms = new Date(`${d}T00:00:00`).getTime()
  return Number.isNaN(ms) ? null : Math.floor(ms / 1000)
}
/** Local end-of-day epoch seconds (inclusive) for 'YYYY-MM-DD', or null. */
function dayEnd(d: string): number | null {
  if (!d) return null
  const ms = new Date(`${d}T23:59:59`).getTime()
  return Number.isNaN(ms) ? null : Math.floor(ms / 1000)
}

const hasFilters = computed(
  () => !!query.value.trim() || !!fromDate.value || !!toDate.value,
)

// ── Search (debounced) ───────────────────────────────────────────────────────
let timer: ReturnType<typeof setTimeout> | undefined

function scheduleSearch() {
  clearTimeout(timer)
  timer = setTimeout(runSearch, DEBOUNCE_MS)
}

async function runSearch() {
  if (!hasFilters.value) {
    results.value = []
    searched.value = false
    return
  }
  searched.value = true
  try {
    results.value = await invoke<StoredMessage[]>('chat_search_history', {
      query: query.value.trim() || null,
      start: dayStart(fromDate.value),
      end: dayEnd(toDate.value),
      limit: SEARCH_LIMIT,
    })
  } catch {
    results.value = []
  }
}

watch([query, fromDate, toDate], scheduleSearch)
onBeforeUnmount(() => clearTimeout(timer))

// ── Display helpers ───────────────────────────────────────────────────────────
const localeTag = computed(
  () => ({ en: 'en-US', zh: 'zh-CN', ja: 'ja-JP' }[locale.value] ?? 'en-US'),
)
function fmtTime(sec: number): string {
  return new Date(sec * 1000).toLocaleString(localeTag.value, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}
function roleLabel(role: string): string {
  return role === 'user' ? '🐱' : '🥒'
}
</script>

<template>
  <Transition name="hist">
    <div v-if="visible" class="chat-history pet-ui-overlay">
      <!-- Titlebar: ‹ back-to-chat + drag region + window controls. -->
      <header class="hist-header" data-tauri-drag-region>
        <button class="hist-back" :data-tip="t.chat.historyClose" @mousedown.stop @click="close">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none">
            <path d="M15 5l-7 7 7 7" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/>
          </svg>
        </button>
        <span class="hist-title" data-tauri-drag-region>{{ t.chat.history }}</span>
        <div class="win-controls" @mousedown.stop>
          <button class="wbtn wbtn-min" :data-tip="t.chat.minimize" @click="minimizeWindow">
            <svg width="10" height="2" viewBox="0 0 10 2"><rect width="10" height="1.5" rx="0.75" fill="currentColor"/></svg>
          </button>
          <button class="wbtn wbtn-close" :data-tip="t.chat.close" @click="emit('close')">
            <svg width="10" height="10" viewBox="0 0 10 10"><line x1="1" y1="1" x2="9" y2="9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><line x1="9" y1="1" x2="1" y2="9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>
          </button>
        </div>
      </header>

      <!-- Filters -->
      <div class="hist-filters">
        <input
          ref="inputRef"
          v-model="query"
          class="hist-search"
          type="search"
          :placeholder="t.chat.searchPlaceholder"
          maxlength="100"
        />
        <div class="hist-dates">
          <div class="date-field">
            <span class="date-label">{{ t.chat.dateFrom }}</span>
            <DatePicker v-model="fromDate" align="left" />
          </div>
          <div class="date-field">
            <span class="date-label">{{ t.chat.dateTo }}</span>
            <DatePicker v-model="toDate" align="right" />
          </div>
        </div>
      </div>

      <!-- Results -->
      <div class="hist-results">
        <div v-if="!searched || !results.length" class="hist-empty">
          <svg class="hist-empty-icon" width="34" height="34" viewBox="0 0 24 24" fill="none">
            <circle cx="11" cy="11" r="7" stroke="currentColor" stroke-width="1.8"/>
            <path d="M16 16l4.5 4.5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>
          </svg>
          <p class="hist-hint">{{ !searched ? t.chat.searchHint : t.chat.searchNoResults }}</p>
        </div>
        <button
          v-for="m in results"
          :key="m.id"
          class="hist-item"
          :class="m.role === 'user' ? 'hist-item--user' : 'hist-item--mutsumi'"
          @click="emit('jump', m.id)"
        >
          <div class="hist-item-meta">
            <span class="hist-role">{{ roleLabel(m.role) }}</span>
            <span class="hist-time">{{ fmtTime(m.created_at) }}</span>
          </div>
          <div class="hist-snippet">{{ m.content }}</div>
        </button>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.chat-history {
  position: fixed;
  inset: 0;
  display: flex;
  flex-direction: column;
  padding: 10px;
  gap: 8px;
  box-sizing: border-box;
  z-index: 60;            /* above the chat panel */

  border-radius: 12px;    /* match SettingsWindow .shell */
  background: rgba(236, 246, 236, 0.96);
  backdrop-filter: blur(24px) saturate(160%);
  -webkit-backdrop-filter: blur(24px) saturate(160%);
  color: #1a2e1a;
  font-family: system-ui, "Segoe UI", "Noto Sans SC", "Noto Sans JP", sans-serif;
}

/* ── Header ──────────────────────────────────────────────────────── */
.hist-header {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}
.hist-title { font-size: 13px; font-weight: 600; color: rgba(30, 52, 30, 0.9); }

/* Back-to-chat — quiet borderless icon button (not a window control). */
.hist-back {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 22px;
  flex-shrink: 0;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  background: transparent;
  color: rgba(40, 70, 40, 0.6);
  transition: background 120ms ease, color 120ms ease, transform 80ms ease;
}
.hist-back:hover  { background: rgba(119, 153, 119, 0.16); color: rgba(20, 50, 20, 0.9); }
.hist-back:active { transform: scale(0.90); }

/* ── Window controls (mirror SettingsWindow .win-controls / .wbtn) ── */
.win-controls {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
  margin-left: auto;   /* push the controls to the right edge */
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
.wbtn-min:hover   { background: rgba(200, 145, 30, 0.82); color: #fff; }
.wbtn-close:hover { background: rgba(220, 60, 60, 0.82);  color: #fff; }

/* ── Hover tooltip (mirrors TarotCard .ctrl[data-tip]) ──────────────── */
[data-tip] { position: relative; }
[data-tip]::after {
  content: attr(data-tip);
  position: absolute;
  top: calc(100% + 7px);
  left: 50%;
  transform: translateX(-50%);
  white-space: nowrap;
  padding: 4px 9px;
  border-radius: 8px;
  font-size: 11px;
  font-weight: 500;
  color: #2a4a2a;
  background: rgba(245, 250, 245, 0.97);
  border: 1px solid rgba(148, 185, 148, 0.45);
  box-shadow: 0 2px 8px rgba(40, 70, 40, 0.14);
  pointer-events: none;
  z-index: 90;
  opacity: 0;
  visibility: hidden;
  transition: opacity 120ms ease, visibility 120ms ease;
}
[data-tip]:hover::after { opacity: 1; visibility: visible; }
/* The back button hugs the left edge — anchor its tooltip there so it can't clip. */
.hist-back[data-tip]::after { left: 0; transform: none; }

/* ── Filters ─────────────────────────────────────────────────────── */
/* position+z-index so the date-picker popover paints above the results list */
.hist-filters { position: relative; z-index: 5; display: flex; flex-direction: column; gap: 8px; flex-shrink: 0; }
.hist-search {
  width: 100%;
  box-sizing: border-box;
  padding: 7px 10px;
  border: 1px solid rgba(148, 185, 148, 0.5);
  border-radius: 9px;
  background: rgba(255, 255, 255, 0.9);
  color: #1a2e1a;
  font-size: 12.5px;
  outline: none;
  transition: border-color 150ms ease, box-shadow 150ms ease;
}
.hist-search:focus {
  border-color: rgba(119, 153, 119, 0.85);
  box-shadow: 0 0 0 2px rgba(119, 153, 119, 0.18);
}
.hist-dates { display: flex; gap: 8px; }
.date-field {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 5px;
}
.date-label {
  flex-shrink: 0;
  font-size: 11px;
  font-weight: 500;
  color: rgba(40, 70, 40, 0.6);
}

/* ── Results ─────────────────────────────────────────────────────── */
.hist-results {
  flex: 1;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding-right: 2px;
}
.hist-results::-webkit-scrollbar { width: 5px; }
.hist-results::-webkit-scrollbar-thumb {
  background: rgba(119, 153, 119, 0.4);
  border-radius: 3px;
}
.hist-empty {
  margin: auto;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 0 16px;
}
.hist-empty-icon { color: rgba(119, 153, 119, 0.55); }
.hist-hint {
  margin: 0;
  font-size: 12px;
  text-align: center;
  color: rgba(40, 70, 40, 0.5);
}
.hist-item {
  display: flex;
  flex-direction: column;
  gap: 3px;
  text-align: left;
  width: 100%;
  box-sizing: border-box;
  padding: 7px 9px;
  border-radius: 9px;
  border: 1px solid rgba(148, 185, 148, 0.35);
  background: rgba(255, 255, 255, 0.82);
  cursor: pointer;
  transition: background 120ms ease, border-color 120ms ease, transform 120ms ease;
}
.hist-item:hover {
  background: rgba(255, 255, 255, 0.98);
  border-color: rgba(119, 153, 119, 0.6);
}
.hist-item:active { transform: scale(0.99); }
.hist-item--user { border-left: 3px solid rgba(119, 153, 119, 0.7); }
.hist-item--mutsumi { border-left: 3px solid rgba(148, 185, 148, 0.45); }
.hist-item-meta {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 10.5px;
  color: rgba(40, 70, 40, 0.55);
}
.hist-role { font-size: 11px; }
.hist-snippet {
  font-size: 12px;
  line-height: 1.35;
  color: #1a2e1a;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  word-break: break-word;
}

/* ── Enter / leave ───────────────────────────────────────────────── */
.hist-enter-active, .hist-leave-active { transition: opacity 180ms ease, transform 180ms ease; }
.hist-enter-from, .hist-leave-to { opacity: 0; transform: translateX(12px); }
</style>
