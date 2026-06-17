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

      <div class="chat-input-row">
        <textarea
          ref="inputRef"
          v-model="input"
          class="chat-input"
          rows="1"
          :placeholder="t.chat.placeholder"
          @keydown="onKeydown"
        />
        <button
          class="chat-send"
          :title="t.chat.send"
          :disabled="busy || !input.trim()"
          @click="send"
        >➤</button>
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

/* ── Input row ───────────────────────────────────────────────────── */
.chat-input-row {
  display: flex;
  align-items: flex-end;
  gap: 6px;
  flex-shrink: 0;
}
.chat-input {
  flex: 1;
  resize: none;
  max-height: 72px;
  padding: 7px 9px;
  border-radius: 10px;
  border: 1px solid rgba(148, 185, 148, 0.5);
  background: rgba(255, 255, 255, 0.9);
  color: #1a2e1a;
  font-family: inherit;
  font-size: 12.5px;
  line-height: 1.35;
  outline: none;
  transition: border-color 150ms ease, box-shadow 150ms ease;
}
.chat-input:focus {
  border-color: rgba(119, 153, 119, 0.85);
  box-shadow: 0 0 0 2px rgba(119, 153, 119, 0.18);
}
.chat-send {
  flex-shrink: 0;
  width: 32px;
  height: 32px;
  border-radius: 50%;
  border: none;
  cursor: pointer;
  background: rgba(119, 153, 119, 0.92);
  color: #f3f8f3;
  font-size: 13px;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: transform 150ms ease, background 150ms ease, opacity 150ms ease;
}
.chat-send:hover:not(:disabled) { transform: scale(1.08); background: rgba(119, 153, 119, 1); }
.chat-send:active:not(:disabled) { transform: scale(0.92); }
.chat-send:disabled { opacity: 0.4; cursor: default; }

/* ── Enter / leave ───────────────────────────────────────────────── */
.chat-enter-active, .chat-leave-active { transition: opacity 200ms ease, transform 200ms ease; }
.chat-enter-from, .chat-leave-to { opacity: 0; transform: scale(0.97); }
</style>
