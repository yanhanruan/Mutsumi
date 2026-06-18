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

const emit = defineEmits<{ jump: [id: number]; close: [] }>()
const { t, locale } = useI18n()

const SEARCH_LIMIT = 60
const DEBOUNCE_MS = 250

const visible  = ref(false)
const query    = ref('')
const fromDate = ref('')   // 'YYYY-MM-DD' from <input type="date">
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
      <header class="hist-header">
        <button class="hist-back" :title="t.chat.historyClose" @click="close">‹</button>
        <span class="hist-title">{{ t.chat.history }}</span>
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
          <label class="date-field">
            <span>{{ t.chat.dateFrom }}</span>
            <input v-model="fromDate" type="date" class="hist-date" />
          </label>
          <label class="date-field">
            <span>{{ t.chat.dateTo }}</span>
            <input v-model="toDate" type="date" class="hist-date" />
          </label>
        </div>
      </div>

      <!-- Results -->
      <div class="hist-results">
        <p v-if="!searched" class="hist-hint">{{ t.chat.searchHint }}</p>
        <p v-else-if="!results.length" class="hist-hint">{{ t.chat.searchNoResults }}</p>
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
.hist-back {
  border: none;
  background: transparent;
  cursor: pointer;
  font-size: 20px;
  line-height: 1;
  color: rgba(40, 70, 40, 0.7);
  padding: 0 4px;
  border-radius: 6px;
  transition: background 150ms ease;
}
.hist-back:hover { background: rgba(119, 153, 119, 0.16); }
.hist-title { font-size: 13px; font-weight: 600; color: rgba(30, 52, 30, 0.9); }

/* ── Filters ─────────────────────────────────────────────────────── */
.hist-filters { display: flex; flex-direction: column; gap: 6px; flex-shrink: 0; }
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
.hist-dates { display: flex; gap: 6px; }
.date-field {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
  color: rgba(40, 70, 40, 0.6);
}
.hist-date {
  flex: 1;
  min-width: 0;
  box-sizing: border-box;
  padding: 4px 6px;
  border: 1px solid rgba(148, 185, 148, 0.5);
  border-radius: 7px;
  background: rgba(255, 255, 255, 0.9);
  color: #1a2e1a;
  font-size: 11px;
  font-family: inherit;
  outline: none;
}
.hist-date:focus { border-color: rgba(119, 153, 119, 0.85); }

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
.hist-hint {
  margin: auto;
  font-size: 12px;
  text-align: center;
  color: rgba(40, 70, 40, 0.45);
  padding: 0 16px;
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
