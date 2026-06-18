<script setup lang="ts">
/**
 * ChatPanel — in-pet chat overlay (RAG-backed conversation with Mutsumi).
 *
 * Rendered inside the main pet window (not a separate OS window), mirroring the
 * tarot overlay: PetWindow grows the window and hides the pet sprite while this
 * is open, then restores both on close. Styling matches the frosted-green glass
 * palette (see ChatBubble / ContextMenu / TarotCard).
 *
 * Streaming: replies arrive token-by-token over a Tauri `Channel` from the
 * `chat_stream` command. We accumulate deltas into a live bubble, then commit
 * the final text to the history when the stream ends.
 *
 * Keyboard: Esc closes; Enter sends (Shift+Enter inserts a newline). The root
 * carries `pet-ui-overlay` so the per-pixel hit-test keeps the window
 * interactive while chat is open.
 */
import { ref, nextTick, watch } from 'vue'
import { invoke, Channel } from '@tauri-apps/api/core'
import { useI18n } from '../i18n'
import { CHAT_MAX_HISTORY, CHAT_HISTORY_PAGE, type StoredMessage } from '../config/chat'
import ChatHistory from './ChatHistory.vue'

interface Msg {
  id?: number        // present once loaded from / persisted to the transcript
  role: 'user' | 'assistant'
  content: string
}

/** Streamed event shape from the Rust `chat_stream` command. */
type ChatEvent =
  | { kind: 'delta'; text: string }
  | { kind: 'done'; content: string }

const emit = defineEmits<{ close: [] }>()
const { t, locale } = useI18n()

// ── State ────────────────────────────────────────────────────────────────
const visible   = ref(false)
const messages  = ref<Msg[]>([])      // committed conversation turns
const streaming = ref<string | null>(null) // in-progress assistant text, or null
const busy      = ref(false)          // a request is in flight
const input     = ref('')
const skipLeave = ref(false)          // skip leave fade on dismiss (see dismiss())

const inputRef = ref<HTMLTextAreaElement | null>(null)
const listRef  = ref<HTMLDivElement | null>(null)
const historyRef = ref<InstanceType<typeof ChatHistory> | null>(null)

// ── Transcript pagination / present-vs-browsing state ──────────────────────
// The displayed `messages` is the true conversation tail only while `atPresent`.
// `oldestId` / `newestId` are keyset cursors for the loaded window.
const oldestId    = ref<number | null>(null)
const newestId    = ref<number | null>(null)
const atPresent   = ref(true)        // true iff the loaded window includes the newest msg
const atBottom    = ref(true)        // true iff scrolled to the bottom of the list
const hasMoreOlder = ref(true)       // false once upward pagination is exhausted
const loadingOlder = ref(false)
const loadingNewer = ref(false)
const highlightId = ref<number | null>(null)  // briefly highlighted jump target
let highlightTimer: ReturnType<typeof setTimeout> | undefined

const SCROLL_EDGE = 60  // px from an edge that triggers a page load

function toMsg(m: StoredMessage): Msg {
  return { id: m.id, role: m.role, content: m.content }
}

// ── Public API (mirrors TarotCard open/dismiss) ───────────────────────────
function open() {
  visible.value = true
  void loadPresent()
  nextTick(() => {
    inputRef.value?.focus()
    autoResize()
  })
}

function dismiss() {
  // Flush any buffered turns through silent extraction so a short conversation's
  // memories aren't stranded below the batch threshold (best-effort).
  void invoke('chat_flush_memory').catch(() => {})
  recognition?.abort()          // tear down any live mic session
  skipLeave.value = true        // vanish immediately — no fade at the new window pos
  visible.value = false
  void nextTick(() => { skipLeave.value = false })
}

defineExpose({ open, dismiss })

// ── Scroll helpers ───────────────────────────────────────────────────────────
function scrollToBottom() {
  nextTick(() => {
    const el = listRef.value
    if (el) el.scrollTop = el.scrollHeight
    atBottom.value = true
  })
}
/** Recompute whether the list is scrolled to (near) the bottom. */
function refreshAtBottom() {
  const el = listRef.value
  atBottom.value = !el || el.scrollHeight - el.scrollTop - el.clientHeight <= SCROLL_EDGE
}
// Follow the streaming reply to the bottom (we're always at present while sending).
watch(streaming, v => { if (v !== null) scrollToBottom() })

// ── Transcript loading ───────────────────────────────────────────────────────
/** Replace the view with the newest page — the genuine conversation tail. */
async function loadPresent() {
  const page = await invoke<StoredMessage[]>('chat_recent_messages', {
    before: null,
    limit: CHAT_HISTORY_PAGE,
  }).catch(() => [] as StoredMessage[])
  messages.value = page.map(toMsg)
  oldestId.value = page.length ? page[0].id : null
  newestId.value = page.length ? page[page.length - 1].id : null
  hasMoreOlder.value = page.length >= CHAT_HISTORY_PAGE
  atPresent.value = true
  scrollToBottom()
}

/** Prepend the page older than `oldestId`, preserving the scroll position. */
async function loadOlder() {
  if (loadingOlder.value || !hasMoreOlder.value || oldestId.value === null) return
  loadingOlder.value = true
  const el = listRef.value
  const prevHeight = el?.scrollHeight ?? 0
  try {
    const page = await invoke<StoredMessage[]>('chat_recent_messages', {
      before: oldestId.value,
      limit: CHAT_HISTORY_PAGE,
    })
    if (page.length) {
      messages.value = [...page.map(toMsg), ...messages.value]
      oldestId.value = page[0].id
      nextTick(() => {
        const el2 = listRef.value
        if (el2) el2.scrollTop = el2.scrollHeight - prevHeight  // keep view anchored
      })
    }
    if (page.length < CHAT_HISTORY_PAGE) hasMoreOlder.value = false
  } catch { /* best-effort */ } finally {
    loadingOlder.value = false
  }
}

/** Append the page newer than `newestId`; a short page means we've reached present. */
async function loadNewer() {
  if (loadingNewer.value || atPresent.value || newestId.value === null) return
  loadingNewer.value = true
  try {
    const page = await invoke<StoredMessage[]>('chat_messages_after', {
      afterId: newestId.value,
      limit: CHAT_HISTORY_PAGE,
    })
    if (page.length) {
      messages.value = [...messages.value, ...page.map(toMsg)]
      newestId.value = page[page.length - 1].id
    }
    if (page.length < CHAT_HISTORY_PAGE) atPresent.value = true
  } catch { /* best-effort */ } finally {
    loadingNewer.value = false
  }
}

function onScroll() {
  const el = listRef.value
  if (!el) return
  refreshAtBottom()
  if (el.scrollTop <= SCROLL_EDGE) void loadOlder()
  if (!atPresent.value && el.scrollHeight - el.scrollTop - el.clientHeight <= SCROLL_EDGE) {
    void loadNewer()
  }
}

/** Snap back to the live tail (the "jump to latest" affordance). */
async function jumpToLatest() {
  // Reload the tail only if it isn't already in the window; otherwise just scroll.
  if (!atPresent.value) await loadPresent()
  else scrollToBottom()
}

// ── History panel navigation ──────────────────────────────────────────────────
function openHistory() {
  historyRef.value?.open()
}

/**
 * Load a window *around* the chosen message — older context (≤ id) plus newer
 * context (> id) — so the full surrounding history is shown, not just the single
 * match. `atPresent` is true only if fewer than a page of newer messages exist
 * (i.e. the window already reaches the live tail).
 */
async function onHistoryJump(id: number) {
  historyRef.value?.close()
  const [older, newer] = await Promise.all([
    invoke<StoredMessage[]>('chat_recent_messages', { before: id + 1, limit: CHAT_HISTORY_PAGE })
      .catch(() => [] as StoredMessage[]),
    invoke<StoredMessage[]>('chat_messages_after', { afterId: id, limit: CHAT_HISTORY_PAGE })
      .catch(() => [] as StoredMessage[]),
  ])
  if (!older.length) return
  const window = [...older, ...newer]
  messages.value = window.map(toMsg)
  oldestId.value = window[0].id
  newestId.value = window[window.length - 1].id
  hasMoreOlder.value = older.length >= CHAT_HISTORY_PAGE
  atPresent.value = newer.length < CHAT_HISTORY_PAGE
  highlight(id)
  scrollToMessage(id)
}

function highlight(id: number) {
  clearTimeout(highlightTimer)
  highlightId.value = id
  highlightTimer = setTimeout(() => { highlightId.value = null }, 2200)
}

function scrollToMessage(id: number) {
  nextTick(() => {
    const el = listRef.value?.querySelector(`[data-mid="${id}"]`) as HTMLElement | null
    el?.scrollIntoView({ block: 'center' })
    nextTick(refreshAtBottom)   // anchor may not be at the bottom → show jump-to-latest
  })
}

// ── Send + stream ────────────────────────────────────────────────────────
async function send() {
  const text = input.value.trim()
  if (!text || busy.value) return

  // If browsing a history slice, snap back to the live tail first so the LLM
  // history below is the genuine recent conversation (not a stale slice) and the
  // new turn lands at the true end of the thread.
  if (!atPresent.value) await loadPresent()

  // Snapshot prior history BEFORE pushing the new turn (cap to recent pairs).
  const history = messages.value
    .slice(-CHAT_MAX_HISTORY * 2)
    .map(m => ({ role: m.role, content: m.content }))

  messages.value.push({ role: 'user', content: text })
  input.value = ''
  nextTick(() => autoResize())
  busy.value = true
  streaming.value = ''

  let finalContent = ''
  const channel = new Channel<ChatEvent>()
  channel.onmessage = ev => {
    if (ev.kind === 'delta') streaming.value = (streaming.value ?? '') + ev.text
    else if (ev.kind === 'done') finalContent = ev.content
  }

  try {
    await invoke('chat_stream', {
      message: text,
      locale: locale.value,
      history,
      onEvent: channel,
    })
    messages.value.push({
      role: 'assistant',
      content: finalContent || streaming.value || '',
    })
  } catch {
    messages.value.push({ role: 'assistant', content: t.value.chat.error })
  } finally {
    streaming.value = null
    busy.value = false
    scrollToBottom()
    nextTick(() => inputRef.value?.focus())
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault()
    void send()
  }
}

// ── Speech-to-text (Web Speech API) ────────────────────────────────────────
// Browser-native SpeechRecognition (Chromium ships it as `webkitSpeechRecognition`).
// The mic button is disabled outright when the API is absent.
interface SpeechRecognitionLike {
  lang: string
  continuous: boolean
  interimResults: boolean
  start(): void
  stop(): void
  abort(): void
  onresult: ((e: SpeechRecognitionEventLike) => void) | null
  onerror: (() => void) | null
  onend: (() => void) | null
}
interface SpeechRecognitionEventLike {
  results: ArrayLike<ArrayLike<{ transcript: string }>>
}

const SpeechRecognitionCtor = (
  (window as unknown as Record<string, unknown>).SpeechRecognition ||
  (window as unknown as Record<string, unknown>).webkitSpeechRecognition
) as (new () => SpeechRecognitionLike) | undefined

const sttSupported = !!SpeechRecognitionCtor
const listening = ref(false)
let recognition: SpeechRecognitionLike | null = null

// Map the active UI locale to a BCP-47 tag the recognizer understands.
const STT_LANG: Record<string, string> = { en: 'en-US', zh: 'zh-CN', ja: 'ja-JP' }

function voiceTitle() {
  if (!sttSupported) return t.value.chat.voiceUnsupported
  return listening.value ? t.value.chat.voiceListening : t.value.chat.voiceInput
}

function toggleVoice() {
  if (!sttSupported || busy.value) return
  if (listening.value) { recognition?.stop(); return }

  const rec = new SpeechRecognitionCtor!()
  rec.lang = STT_LANG[locale.value] ?? 'en-US'
  rec.continuous = false
  rec.interimResults = true

  // Append the (interim + final) transcript onto whatever was already typed.
  const base = input.value.trim()
  rec.onresult = ev => {
    let transcript = ''
    for (let i = 0; i < ev.results.length; i++) {
      transcript += ev.results[i][0].transcript
    }
    input.value = base ? `${base} ${transcript}` : transcript
    nextTick(() => autoResize())
  }
  rec.onerror = () => { listening.value = false }
  rec.onend = () => {
    listening.value = false
    recognition = null
    nextTick(() => inputRef.value?.focus())
  }

  recognition = rec
  listening.value = true
  try {
    rec.start()
  } catch {
    listening.value = false
    recognition = null
  }
}

// ── Textarea auto-resize ────────────────────────────────────────────────────
const MAX_INPUT_HEIGHT = 88  // ~4 lines (12.5px × 1.4 × 4 + 14px padding)

function autoResize() {
  const el = inputRef.value
  if (!el) return
  el.style.height = 'auto'
  el.style.height = `${Math.min(el.scrollHeight, MAX_INPUT_HEIGHT)}px`
}

// Esc closes while open (document-level so it works regardless of focus).
function onDocKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') emit('close')
}
watch(visible, v => {
  if (v) document.addEventListener('keydown', onDocKeydown, true)
  else document.removeEventListener('keydown', onDocKeydown, true)
})
</script>

<template>
  <Transition name="chat" :css="!skipLeave">
    <div v-if="visible" class="chat-panel pet-ui-overlay">
      <header class="chat-header">
        <span class="chat-title">{{ t.chat.title }}</span>
        <div class="chat-header-actions">
          <button class="chat-icon-btn" :title="t.chat.history" @click="openHistory">
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none">
              <path d="M12 7v5l3 2" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
              <path d="M3.5 12a8.5 8.5 0 1 0 2.4-5.9M3.5 4v3h3" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
          </button>
          <button class="chat-close" :title="t.chat.close" @click="emit('close')">✕</button>
        </div>
      </header>

      <div ref="listRef" class="chat-list" @scroll.passive="onScroll">
        <p v-if="!messages.length && streaming === null" class="chat-empty">
          {{ t.chat.empty }}
        </p>

        <div
          v-for="(m, i) in messages"
          :key="m.id ?? `live-${i}`"
          :data-mid="m.id"
          class="msg"
          :class="[
            m.role === 'user' ? 'msg--user' : 'msg--mutsumi',
            { 'msg--highlight': m.id != null && m.id === highlightId },
          ]"
        >
          {{ m.content }}
        </div>

        <!-- Live streaming bubble -->
        <div v-if="streaming !== null" class="msg msg--mutsumi">
          <span v-if="streaming">{{ streaming }}</span>
          <span v-else class="chat-thinking">{{ t.chat.thinking }}</span>
        </div>
      </div>

      <!-- Jump-to-latest (shown whenever not scrolled to the bottom) -->
      <button
        v-if="!atBottom"
        class="chat-jump-latest"
        :title="t.chat.jumpToLatest"
        @click="jumpToLatest"
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none">
          <path d="M6 9l6 6 6-6" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      </button>

      <!-- Composer card: textarea above, toolbar below -->
      <div class="chat-composer">
        <textarea
          ref="inputRef"
          v-model="input"
          class="chat-input"
          :placeholder="t.chat.placeholder"
          maxlength="1000"
          @keydown="onKeydown"
          @input="autoResize"
        />
        <div class="chat-toolbar">
          <!-- Attach file (incomplete) -->
          <button class="chat-tool-btn" :title="t.chat.attachFile" disabled>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none">
              <path d="M12 5v14M5 12h14" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
            </svg>
          </button>

          <div class="toolbar-right">
            <!-- Model selector (display-only) -->
            <div class="chat-model-selector" aria-hidden="true">
              <span class="model-name">qwen-plus</span>
              <span class="model-badge">Extra</span>
              <svg width="8" height="5" viewBox="0 0 8 5" fill="none">
                <path d="M1 1l3 3 3-3" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
            </div>
            <!-- Voice input (Web Speech API) -->
            <button
              class="chat-tool-btn"
              :class="{ 'is-listening': listening }"
              :title="voiceTitle()"
              :disabled="!sttSupported || busy"
              @click="toggleVoice"
            >
              <svg width="12" height="14" viewBox="0 0 24 28" fill="none">
                <rect x="8" y="1" width="8" height="14" rx="4" fill="currentColor"/>
                <path d="M4 14a8 8 0 0016 0" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
                <line x1="12" y1="22" x2="12" y2="27" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
              </svg>
            </button>
            <!-- Send -->
            <button
              class="chat-send"
              :title="t.chat.send"
              :disabled="busy || !input.trim()"
              @click="send"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none">
                <path d="M12 19V5M5 12l7-7 7 7" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
            </button>
          </div>
        </div>
      </div>

      <!-- History search overlay (sits on top of the panel when open) -->
      <ChatHistory ref="historyRef" @jump="onHistoryJump" />
    </div>
  </Transition>
</template>

<style scoped>
.chat-panel {
  position: fixed;
  inset: 0;
  display: flex;
  flex-direction: column;
  padding: 10px;
  gap: 8px;
  box-sizing: border-box;
  z-index: 50;

  background: rgba(236, 246, 236, 0.92);
  backdrop-filter: blur(24px) saturate(160%);
  -webkit-backdrop-filter: blur(24px) saturate(160%);
  color: #1a2e1a;
  font-family: system-ui, "Segoe UI", "Noto Sans SC", "Noto Sans JP", sans-serif;
}

/* ── Header ──────────────────────────────────────────────────────── */
.chat-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-shrink: 0;
}
.chat-title {
  font-size: 13px;
  font-weight: 600;
  color: rgba(30, 52, 30, 0.9);
}
.chat-header-actions {
  display: flex;
  align-items: center;
  gap: 2px;
}
.chat-icon-btn {
  border: none;
  background: transparent;
  cursor: pointer;
  color: rgba(40, 70, 40, 0.55);
  line-height: 0;
  padding: 3px 4px;
  border-radius: 6px;
  transition: background 150ms ease, color 150ms ease;
}
.chat-icon-btn:hover {
  background: rgba(119, 153, 119, 0.16);
  color: rgba(30, 60, 30, 0.85);
}
.chat-close {
  border: none;
  background: transparent;
  cursor: pointer;
  font-size: 13px;
  color: rgba(40, 70, 40, 0.55);
  line-height: 1;
  padding: 2px 4px;
  border-radius: 6px;
  transition: background 150ms ease, color 150ms ease;
}
.chat-close:hover {
  background: rgba(200, 110, 110, 0.16);
  color: rgba(160, 50, 50, 0.85);
}

/* ── Message list ────────────────────────────────────────────────── */
.chat-list {
  flex: 1;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 7px;
  padding-right: 2px;
}
.chat-list::-webkit-scrollbar { width: 5px; }
.chat-list::-webkit-scrollbar-thumb {
  background: rgba(119, 153, 119, 0.4);
  border-radius: 3px;
}
.chat-empty {
  margin: auto;
  font-size: 12px;
  color: rgba(40, 70, 40, 0.45);
}

.msg {
  max-width: 82%;
  padding: 7px 10px;
  border-radius: 12px;
  font-size: 12.5px;
  line-height: 1.4;
  white-space: pre-wrap;
  word-break: break-word;
}
.msg--user {
  align-self: flex-end;
  background: rgba(119, 153, 119, 0.92);
  color: #f3f8f3;
  border-bottom-right-radius: 4px;
}
.msg--mutsumi {
  align-self: flex-start;
  background: rgba(255, 255, 255, 0.86);
  border: 1px solid rgba(148, 185, 148, 0.4);
  border-bottom-left-radius: 4px;
}
/* Briefly flag a message jumped-to from History search. */
.msg--highlight { animation: msg-flash 2.2s ease-out; }
@keyframes msg-flash {
  0%, 25%  { box-shadow: 0 0 0 2px rgba(214, 184, 90, 0.85); background: rgba(250, 240, 200, 0.95); }
  100%     { box-shadow: 0 0 0 0 rgba(214, 184, 90, 0); }
}

/* ── Jump-to-latest (floating) ───────────────────────────────────── */
.chat-jump-latest {
  position: absolute;
  right: 16px;
  bottom: 112px;
  width: 30px;
  height: 30px;
  border-radius: 50%;
  border: 1px solid rgba(148, 185, 148, 0.5);
  background: rgba(255, 255, 255, 0.95);
  color: rgba(40, 80, 40, 0.8);
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  box-shadow: 0 2px 8px rgba(80, 120, 80, 0.25);
  transition: transform 150ms ease, box-shadow 150ms ease;
  z-index: 55;
}
.chat-jump-latest:hover {
  transform: scale(1.08);
  box-shadow: 0 3px 12px rgba(80, 120, 80, 0.35);
}
.chat-jump-latest:active { transform: scale(0.92); }

.chat-thinking {
  letter-spacing: 2px;
  opacity: 0.6;
  animation: pulse 1.2s ease-in-out infinite;
}
@keyframes pulse { 0%, 100% { opacity: 0.3 } 50% { opacity: 0.7 } }

/* ── Composer card (wraps textarea + toolbar) ────────────────────── */
.chat-composer {
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  border: 1px solid rgba(148, 185, 148, 0.50);
  border-radius: 12px;
  background: rgba(255, 255, 255, 0.90);
  overflow: hidden;
  transition: border-color 150ms ease, box-shadow 150ms ease;
}
.chat-composer:focus-within {
  border-color: rgba(119, 153, 119, 0.85);
  box-shadow: 0 0 0 2px rgba(119, 153, 119, 0.18);
}

/* ── Textarea (no border — composer card provides it) ────────────── */
.chat-input {
  width: 100%;
  resize: none;
  min-height: 36px;
  max-height: 88px;       /* ~4 lines; JS clamps to same value */
  overflow-y: auto;
  padding: 9px 11px 5px;
  border: none;
  background: transparent;
  color: #1a2e1a;
  font-family: inherit;
  font-size: 12.5px;
  line-height: 1.4;
  outline: none;
  box-sizing: border-box;
  scrollbar-width: thin;
  scrollbar-color: rgba(119, 153, 119, 0.30) transparent;
}
.chat-input::-webkit-scrollbar       { width: 4px; }
.chat-input::-webkit-scrollbar-track { background: transparent; }
.chat-input::-webkit-scrollbar-thumb { background: rgba(119, 153, 119, 0.30); border-radius: 2px; }

/* ── Toolbar row (below the textarea) ───────────────────────────── */
.chat-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 4px 6px 6px;
  gap: 4px;
}
.toolbar-right {
  display: flex;
  align-items: center;
  gap: 5px;
}

/* ── Small tool buttons (+ and mic) ─────────────────────────────── */
.chat-tool-btn {
  width: 28px;
  height: 28px;
  border-radius: 7px;
  border: none;
  background: transparent;
  color: rgba(30, 60, 30, 0.45);
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: background 120ms ease, color 120ms ease;
}
.chat-tool-btn:not(:disabled):hover {
  background: rgba(119, 153, 119, 0.12);
  color: rgba(30, 60, 30, 0.80);
}
.chat-tool-btn:disabled { opacity: 0.35; cursor: default; }
.chat-tool-btn.is-listening {
  background: rgba(200, 90, 90, 0.16);
  color: rgba(180, 50, 50, 0.9);
  animation: mic-pulse 1.2s ease-in-out infinite;
}
@keyframes mic-pulse {
  0%, 100% { box-shadow: 0 0 0 0 rgba(200, 90, 90, 0.0); }
  50%      { box-shadow: 0 0 0 4px rgba(200, 90, 90, 0.18); }
}

/* ── Model selector (display-only) ──────────────────────────────── */
.chat-model-selector {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 0 7px;
  height: 28px;
  border-radius: 7px;
  color: rgba(30, 60, 30, 0.60);
  font-size: 11px;
  font-weight: 500;
  cursor: default;
  user-select: none;
  transition: background 120ms ease;
}
.chat-model-selector:hover { background: rgba(119, 153, 119, 0.10); }
.model-name { font-weight: 600; color: rgba(30, 60, 30, 0.70); letter-spacing: -0.1px; }
.model-badge {
  padding: 1px 5px;
  border-radius: 4px;
  background: rgba(119, 153, 119, 0.14);
  font-size: 10px;
  font-weight: 600;
  color: rgba(40, 80, 40, 0.65);
}

/* ── Send button ─────────────────────────────────────────────────── */
.chat-send {
  width: 30px;
  height: 30px;
  border-radius: 8px;
  border: none;
  cursor: pointer;
  background: linear-gradient(135deg, #779977, #5a8060);
  color: #f3f8f3;
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: 0 2px 8px rgba(80, 120, 80, 0.30);
  transition: transform 150ms ease, box-shadow 150ms ease, opacity 150ms ease;
}
.chat-send:hover:not(:disabled) {
  transform: scale(1.06);
  box-shadow: 0 3px 12px rgba(80, 120, 80, 0.40);
}
.chat-send:active:not(:disabled) { transform: scale(0.92); }
.chat-send:disabled { opacity: 0.35; cursor: default; box-shadow: none; }

/* ── Enter / leave ───────────────────────────────────────────────── */
.chat-enter-active, .chat-leave-active { transition: opacity 200ms ease, transform 200ms ease; }
.chat-enter-from, .chat-leave-to { opacity: 0; transform: scale(0.97); }
</style>
