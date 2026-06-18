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
import { CHAT_MAX_HISTORY } from '../config/chat'

interface Msg {
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

// ── Public API (mirrors TarotCard open/dismiss) ───────────────────────────
function open() {
  visible.value = true
  nextTick(() => {
    inputRef.value?.focus()
    autoResize()
    scrollToBottom()
  })
}

function dismiss() {
  // Flush any buffered turns through silent extraction so a short conversation's
  // memories aren't stranded below the batch threshold (best-effort).
  void invoke('chat_flush_memory').catch(() => {})
  skipLeave.value = true        // vanish immediately — no fade at the new window pos
  visible.value = false
  void nextTick(() => { skipLeave.value = false })
}

defineExpose({ open, dismiss })

// ── Scroll helper ──────────────────────────────────────────────────────────
function scrollToBottom() {
  nextTick(() => {
    const el = listRef.value
    if (el) el.scrollTop = el.scrollHeight
  })
}
watch([messages, streaming], scrollToBottom, { deep: true })

// ── Send + stream ────────────────────────────────────────────────────────
async function send() {
  const text = input.value.trim()
  if (!text || busy.value) return

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
    nextTick(() => inputRef.value?.focus())
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault()
    void send()
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
        <button class="chat-close" :title="t.chat.close" @click="emit('close')">✕</button>
      </header>

      <div ref="listRef" class="chat-list">
        <p v-if="!messages.length && streaming === null" class="chat-empty">
          {{ t.chat.empty }}
        </p>

        <div
          v-for="(m, i) in messages"
          :key="i"
          class="msg"
          :class="m.role === 'user' ? 'msg--user' : 'msg--mutsumi'"
        >
          {{ m.content }}
        </div>

        <!-- Live streaming bubble -->
        <div v-if="streaming !== null" class="msg msg--mutsumi">
          <span v-if="streaming">{{ streaming }}</span>
          <span v-else class="chat-thinking">{{ t.chat.thinking }}</span>
        </div>
      </div>

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
            <!-- Voice input (incomplete) -->
            <button class="chat-tool-btn" :title="t.chat.voiceInput" disabled>
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
