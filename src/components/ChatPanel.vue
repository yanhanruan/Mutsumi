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
import { ref, computed, nextTick, watch, onMounted, onBeforeUnmount } from 'vue'
import { invoke, Channel, convertFileSrc } from '@tauri-apps/api/core'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { useI18n } from '../i18n'
import {
  CHAT_MAX_HISTORY, CHAT_HISTORY_PAGE, CHAT_TIME_GROUP_GAP,
  IMAGE_MAX_COUNT, IMAGE_EXT_WHITELIST, type StoredMessage,
} from '../config/chat'
import ChatHistory from './ChatHistory.vue'
import EmojiPicker from './EmojiPicker.vue'
import { setTaskbarVisible, minimizeWindow } from '../composables/useWindowControls'
import { vTip } from '../composables/useTooltip'
import { useAppConfig, type ChatModelKey } from '../composables/useAppConfig'

interface Msg {
  id?: number          // present once loaded from / persisted to the transcript
  role: 'user' | 'assistant'
  content: string
  created_at: number   // unix seconds (live turns use the client clock)
  kind?: 'text' | 'image'
  imagePath?: string   // absolute path for image rows (fed to convertFileSrc)
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
  return {
    id: m.id,
    role: m.role,
    content: m.content,
    created_at: m.created_at,
    kind: m.kind,
    imagePath: m.image_path ?? undefined,
  }
}

/** Render an image bubble's source through the asset protocol. */
function imgSrc(path: string): string {
  return convertFileSrc(path)
}

// ── Message time grouping ──────────────────────────────────────────────────
// Show one `mm-dd hh:mm` separator per group rather than a time on every bubble.
// A new group starts when the gap from the previous message exceeds the
// threshold; the label is the group's first message's time.
const timeLabels = computed<string[]>(() => {
  const out: string[] = []
  let prevTs: number | null = null
  for (const m of messages.value) {
    const newGroup = prevTs === null || m.created_at - prevTs > CHAT_TIME_GROUP_GAP
    out.push(newGroup ? fmtGroupTime(m.created_at) : '')
    prevTs = m.created_at
  }
  return out
})

function fmtGroupTime(sec: number): string {
  const d = new Date(sec * 1000)
  const p = (n: number) => String(n).padStart(2, '0')
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`
}

// ── Public API (mirrors TarotCard open/dismiss) ───────────────────────────
function open() {
  visible.value = true
  void setTaskbarVisible(true)   // expose a taskbar button while the panel is open
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
  showEmoji.value = false       // close the emoji popover
  imageTray.value = []          // drop any un-sent image picks
  dragOver.value = false
  alertMsg.value = null
  void setTaskbarVisible(false)  // pet floats again — drop the taskbar button
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
  if (busy.value) return
  if (imageTray.value.length) { await sendVision(); return }  // image turn

  const text = input.value.trim()
  if (!text) return

  // If browsing a history slice, snap back to the live tail first so the LLM
  // history below is the genuine recent conversation (not a stale slice) and the
  // new turn lands at the true end of the thread.
  if (!atPresent.value) await loadPresent()

  // Snapshot prior history BEFORE pushing the new turn (cap to recent pairs).
  const history = messages.value
    .slice(-CHAT_MAX_HISTORY * 2)
    .map(m => ({ role: m.role, content: m.content }))

  const nowSec = Math.floor(Date.now() / 1000)
  messages.value.push({ role: 'user', content: text, created_at: nowSec })
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
      created_at: Math.floor(Date.now() / 1000),
    })
  } catch {
    messages.value.push({
      role: 'assistant',
      content: t.value.chat.error,
      created_at: Math.floor(Date.now() / 1000),
    })
  } finally {
    streaming.value = null
    busy.value = false
    scrollToBottom()
    nextTick(() => inputRef.value?.focus())
  }
}

/** Send the tray images (+ optional caption) and stream Mutsumi's text reply. */
async function sendVision() {
  if (busy.value || !imageTray.value.length) return
  const paths = [...imageTray.value]
  const caption = input.value.trim()

  if (!atPresent.value) await loadPresent()
  const history = messages.value
    .slice(-CHAT_MAX_HISTORY * 2)
    .map(m => ({ role: m.role, content: m.content }))

  // Optimistic user bubble(s) — caption rides on the first image (matches storage).
  const nowSec = Math.floor(Date.now() / 1000)
  const startLen = messages.value.length
  paths.forEach((p, i) => {
    messages.value.push({
      role: 'user',
      kind: 'image',
      imagePath: p,
      content: i === 0 ? caption : '',
      created_at: nowSec,
    })
  })
  imageTray.value = []
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
    await invoke('chat_vision_stream', {
      message: caption,
      paths,
      locale: locale.value,
      history,
      onEvent: channel,
    })
    messages.value.push({
      role: 'assistant',
      content: finalContent || streaming.value || '',
      created_at: Math.floor(Date.now() / 1000),
    })
  } catch (err) {
    // Roll back the optimistic image bubble(s) and surface the reason as an alert.
    messages.value.splice(startLen)
    flash(alertFor(err))
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

// ── Emoji picker ─────────────────────────────────────────────────────────────
const showEmoji = ref(false)

function toggleEmoji() {
  showEmoji.value = !showEmoji.value
}

/** Insert the chosen emoji at the textarea caret (or append as a fallback). */
function insertEmoji(char: string) {
  const el = inputRef.value
  const start = el?.selectionStart ?? input.value.length
  const end = el?.selectionEnd ?? input.value.length
  const next = input.value.slice(0, start) + char + input.value.slice(end)
  if (next.length > 1000) return  // respect the textarea maxlength
  input.value = next
  showEmoji.value = false         // auto-close after picking one
  nextTick(() => {
    autoResize()
    el?.focus()
    const pos = start + char.length
    el?.setSelectionRange(pos, pos)
  })
}

// Close the popover on any pointer-down outside it (and outside the toggle).
function onEmojiOutside(e: PointerEvent) {
  const el = e.target as HTMLElement
  if (el.closest('.emoji-pop') || el.closest('.emoji-toggle')) return
  showEmoji.value = false
}
watch(showEmoji, v => {
  if (v) document.addEventListener('pointerdown', onEmojiOutside, true)
  else document.removeEventListener('pointerdown', onEmojiOutside, true)
})

// ── Chat model picker ──────────────────────────────────────────────────────
// Shares the persisted model choice with SettingsWindow through useAppConfig:
// changing it here broadcasts cross-window, so both pickers stay in sync. The
// watch below mirrors a change made in Settings back into this dropdown.
const { config, updateConfig } = useAppConfig()
const MODEL_OPTIONS: { key: ChatModelKey; isDefault?: boolean }[] = [
  { key: 'qwen3.7-max'   },
  { key: 'qwen3.7-plus', isDefault: true },
  { key: 'qwen3.6-flash' },
]
const localModel = ref<ChatModelKey>(config.value.chatModel)
watch(() => config.value.chatModel, v => { localModel.value = v })
const modelOpen = ref(false)

async function setModel(m: ChatModelKey) {
  localModel.value = m
  modelOpen.value = false
  await updateConfig({ chatModel: m })                       // persist + broadcast to Settings
  try { await invoke('qwen_set_chat_model', { model: m }) }  // apply to the backend now
  catch { /* best-effort; re-synced on next startup */ }
}

function onModelOutside(e: PointerEvent) {
  if ((e.target as HTMLElement).closest('.chat-model-selector')) return
  modelOpen.value = false
}
watch(modelOpen, v => {
  if (v) document.addEventListener('pointerdown', onModelOutside, true)
  else document.removeEventListener('pointerdown', onModelOutside, true)
})

// ── Image input ──────────────────────────────────────────────────────────────
// Acquisition never moves bytes over IPC: a file pick / OS drop yields a path,
// and a clipboard paste is staged to a temp file in Rust → also a path. Rust
// reads + validates (size/type); the frontend only gates the count.
const imageTray = ref<string[]>([])   // picked/dropped/staged absolute paths
const dragOver  = ref(false)
const alertMsg  = ref<string | null>(null)
let alertTimer: ReturnType<typeof setTimeout> | undefined
let unlistenDrop: UnlistenFn | undefined

function flash(msg: string) {
  alertMsg.value = msg
  clearTimeout(alertTimer)
  alertTimer = setTimeout(() => { alertMsg.value = null }, 2600)
}

/** Map a Rust validation error code (or anything) to a user-facing alert. */
function alertFor(err: unknown): string {
  const code = String(err)
  if (code.includes('image_too_large')) return t.value.chat.imageTooLarge
  if (code.includes('image_bad_type')) return t.value.chat.imageBadType
  return t.value.chat.error
}

function extOf(path: string): string {
  const m = /\.([^.\\/]+)$/.exec(path)
  return m ? m[1].toLowerCase() : ''
}

/** Add image paths to the tray, gating on the whitelist + count cap. */
function addPaths(paths: string[]) {
  const imgs = paths.filter(p => IMAGE_EXT_WHITELIST.includes(extOf(p)))
  if (!imgs.length) {
    if (paths.length) flash(t.value.chat.imageBadType)
    return
  }
  const room = IMAGE_MAX_COUNT - imageTray.value.length
  if (imgs.length > room) {
    flash(t.value.chat.imageTooMany)
    if (room > 0) imageTray.value.push(...imgs.slice(0, room))
  } else {
    imageTray.value.push(...imgs)
  }
}

function removeTrayItem(i: number) {
  imageTray.value.splice(i, 1)
}

async function pickImages() {
  if (busy.value) return
  if (imageTray.value.length >= IMAGE_MAX_COUNT) { flash(t.value.chat.imageTooMany); return }
  try {
    const sel = await openDialog({
      multiple: true,
      filters: [{ name: 'image', extensions: IMAGE_EXT_WHITELIST }],
    })
    if (!sel) return
    addPaths(Array.isArray(sel) ? sel : [sel])
  } catch { /* dialog dismissed / unavailable */ }
}

async function onPaste(e: ClipboardEvent) {
  const items = e.clipboardData?.items
  if (!items) return
  if (!Array.from(items).some(it => it.type.startsWith('image/'))) return
  e.preventDefault()   // an image is on the clipboard — don't also paste a path string
  if (imageTray.value.length >= IMAGE_MAX_COUNT) { flash(t.value.chat.imageTooMany); return }
  try {
    const staged = await invoke<{ path: string }>('chat_stage_clipboard_image')
    addPaths([staged.path])
  } catch (err) {
    flash(alertFor(err))
  }
}

// Native OS drag-drop yields paths (dragDropEnabled = true). Registered once;
// gated on `visible` so drops only count while the chat is open.
onMounted(async () => {
  try {
    unlistenDrop = await getCurrentWebview().onDragDropEvent(ev => {
      if (!visible.value) return
      const p = ev.payload
      if (p.type === 'enter' || p.type === 'over') {
        dragOver.value = true
      } else if (p.type === 'leave') {
        dragOver.value = false
      } else if (p.type === 'drop') {
        dragOver.value = false
        if (!busy.value) addPaths(p.paths ?? [])
      }
    })
  } catch { /* drag-drop unavailable in this environment */ }
})
onBeforeUnmount(() => {
  unlistenDrop?.()
  clearTimeout(alertTimer)
  document.removeEventListener('pointerdown', onModelOutside, true)
})

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
      <!-- Transient in-character alert (validation feedback) -->
      <Transition name="alert">
        <div v-if="alertMsg" class="chat-alert">{{ alertMsg }}</div>
      </Transition>

      <!-- Drag-drop overlay (Tauri native OS file drop) -->
      <div v-if="dragOver" class="chat-drop">
        <div class="chat-drop-inner">{{ t.chat.dropHint }}</div>
      </div>

      <!-- Titlebar: drag region + window controls (mirrors SettingsWindow). -->
      <header class="chat-header" data-tauri-drag-region>
        <span class="chat-title" data-tauri-drag-region>{{ t.chat.title }}</span>
        <div class="win-controls" @mousedown.stop>
          <button class="wbtn" :data-tip="t.chat.history" @click="openHistory">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none">
              <path d="M12 7v5l3 2" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
              <path d="M3.5 12a8.5 8.5 0 1 0 2.4-5.9M3.5 4v3h3" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
          </button>
          <button class="wbtn wbtn-min" :data-tip="t.chat.minimize" @click="minimizeWindow">
            <svg width="10" height="2" viewBox="0 0 10 2"><rect width="10" height="1.5" rx="0.75" fill="currentColor"/></svg>
          </button>
          <button class="wbtn wbtn-close" :data-tip="t.chat.close" @click="emit('close')">
            <svg width="10" height="10" viewBox="0 0 10 10"><line x1="1" y1="1" x2="9" y2="9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><line x1="9" y1="1" x2="1" y2="9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>
          </button>
        </div>
      </header>

      <div ref="listRef" class="chat-list" @scroll.passive="onScroll">
        <p v-if="!messages.length && streaming === null" class="chat-empty">
          {{ t.chat.empty }}
        </p>

        <template v-for="(m, i) in messages" :key="m.id ?? `live-${i}`">
          <div v-if="timeLabels[i]" class="chat-time-sep">{{ timeLabels[i] }}</div>
          <div
            :data-mid="m.id"
            class="msg"
            :class="[
              m.role === 'user' ? 'msg--user' : 'msg--mutsumi',
              { 'msg--image': m.kind === 'image', 'msg--highlight': m.id != null && m.id === highlightId },
            ]"
          >
            <template v-if="m.kind === 'image' && m.imagePath">
              <img class="msg-img" :src="imgSrc(m.imagePath)" :alt="t.chat.imageAlt" loading="lazy" />
              <div v-if="m.content" class="msg-caption">{{ m.content }}</div>
            </template>
            <template v-else>{{ m.content }}</template>
          </div>
        </template>

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
        v-tip="t.chat.jumpToLatest"
        @click="jumpToLatest"
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none">
          <path d="M6 9l6 6 6-6" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      </button>

      <!-- Composer card: image tray, textarea, toolbar -->
      <div class="chat-composer">
        <!-- Picked-image preview tray (created into bubbles only on send) -->
        <div v-if="imageTray.length" class="chat-tray">
          <div v-for="(p, i) in imageTray" :key="p" class="tray-item">
            <img class="tray-thumb" :src="imgSrc(p)" :alt="t.chat.imageAlt" />
            <button class="tray-remove" :title="t.chat.close" @click="removeTrayItem(i)">✕</button>
          </div>
        </div>

        <textarea
          ref="inputRef"
          v-model="input"
          class="chat-input"
          :placeholder="t.chat.placeholder"
          maxlength="1000"
          @keydown="onKeydown"
          @input="autoResize"
          @paste="onPaste"
        />
        <div class="chat-toolbar">
          <div class="toolbar-left">
            <!-- Attach image -->
            <button class="chat-tool-btn" v-tip="t.chat.attachImage" @click="pickImages">
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none">
                <rect x="3" y="4" width="18" height="16" rx="2.5" stroke="currentColor" stroke-width="2"/>
                <circle cx="8.5" cy="9.5" r="1.6" fill="currentColor"/>
                <path d="M5 18l4.5-5 3.5 4 2.5-3 3.5 4" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
            </button>
            <!-- Emoji picker toggle -->
            <button
              class="chat-tool-btn emoji-toggle"
              :class="{ 'is-active': showEmoji }"
              v-tip="t.chat.emoji"
              @click="toggleEmoji"
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none">
                <circle cx="12" cy="12" r="9" stroke="currentColor" stroke-width="2"/>
                <circle cx="9" cy="10" r="1.3" fill="currentColor"/>
                <circle cx="15" cy="10" r="1.3" fill="currentColor"/>
                <path d="M8.5 14a4 4 0 007 0" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
              </svg>
            </button>
          </div>

          <div class="toolbar-right">
            <!-- Model selector (functional — synced with Settings via useAppConfig) -->
            <div class="chat-model-selector">
              <button class="model-trigger" v-tip="t.chatModel" @click.stop="modelOpen = !modelOpen">
                <span class="model-name">{{ localModel }}</span>
                <!-- Extra tag (removed — the real picker supersedes the static badge)
                <span class="model-badge">Extra</span>
                -->
                <svg class="model-chevron" :class="{ open: modelOpen }" width="8" height="5" viewBox="0 0 8 5" fill="none">
                  <path d="M1 1l3 3 3-3" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/>
                </svg>
              </button>
              <div v-if="modelOpen" class="model-menu" @click.stop>
                <button
                  v-for="opt in MODEL_OPTIONS"
                  :key="opt.key"
                  class="model-option"
                  :class="{ active: localModel === opt.key }"
                  @click="setModel(opt.key)"
                >
                  {{ opt.key }}<template v-if="opt.isDefault"> ({{ t.modelDefaultTag }})</template>
                </button>
              </div>
            </div>
            <!-- Voice input (Web Speech API) -->
            <button
              class="chat-tool-btn"
              :class="{ 'is-listening': listening }"
              v-tip="voiceTitle()"
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
              v-tip="t.chat.send"
              :disabled="busy || (!input.trim() && !imageTray.length)"
              @click="send"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none">
                <path d="M12 19V5M5 12l7-7 7 7" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
            </button>
          </div>
        </div>
      </div>

      <!-- Emoji picker popover — opens above the composer's emoji button -->
      <Transition name="emoji-pop">
        <EmojiPicker
          v-if="showEmoji"
          class="emoji-pop"
          @select="insertEmoji"
          @close="showEmoji = false"
        />
      </Transition>

      <!-- History search overlay (sits on top of the panel when open).
           Its ✕ closes the whole chat panel (its ‹ goes back to the chat). -->
      <ChatHistory ref="historyRef" @jump="onHistoryJump" @close="emit('close')" />
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

  border-radius: 12px;   /* match SettingsWindow .shell */
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

/* ── Window controls (mirror SettingsWindow .win-controls / .wbtn) ── */
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

/* Centered time separator shown above each message group. */
.chat-time-sep {
  align-self: center;
  margin: 3px 0;
  font-size: 10.5px;
  color: rgba(40, 70, 40, 0.45);
  user-select: none;
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
  /* visible (not hidden) so the upward-opening model dropdown isn't clipped at
     the composer's top edge; the children are transparent, so the rounded
     background still clips itself cleanly. */
  overflow: visible;
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
.toolbar-left {
  display: flex;
  align-items: center;
  gap: 2px;
}
.toolbar-right {
  display: flex;
  align-items: center;
  gap: 5px;
}

/* ── Emoji picker ────────────────────────────────────────────────── */
.emoji-toggle.is-active {
  background: rgba(119, 153, 119, 0.18);
  color: rgba(30, 60, 30, 0.85);
}
.emoji-pop {
  position: absolute;
  left: 12px;
  bottom: 54px;        /* just above the toolbar (which stays put as input grows) */
  z-index: 70;
}
.emoji-pop-enter-active, .emoji-pop-leave-active {
  transition: opacity 140ms ease, transform 140ms ease;
}
.emoji-pop-enter-from, .emoji-pop-leave-to {
  opacity: 0;
  transform: translateY(6px) scale(0.98);
  transform-origin: bottom left;
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

/* ── Model selector (functional dropdown, synced with Settings) ──── */
.chat-model-selector { position: relative; }
.model-trigger {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 0 7px;
  height: 28px;
  border: none;
  border-radius: 7px;
  background: transparent;
  color: rgba(30, 60, 30, 0.60);
  font-size: 11px;
  font-weight: 500;
  cursor: pointer;
  user-select: none;
  transition: background 120ms ease;
}
.model-trigger:hover { background: rgba(119, 153, 119, 0.10); }
.model-name { font-weight: 600; color: rgba(30, 60, 30, 0.70); letter-spacing: -0.1px; }
.model-chevron {
  flex-shrink: 0;
  color: rgba(30, 60, 30, 0.45);
  transition: transform 180ms ease;
}
.model-chevron.open { transform: rotate(180deg); }

/* Opens upward — the composer sits at the bottom of the panel. */
.model-menu {
  position: absolute;
  bottom: calc(100% + 6px);
  right: 0;
  z-index: 80;
  min-width: 150px;
  background: rgba(240, 248, 240, 0.97);
  backdrop-filter: blur(24px) saturate(180%);
  -webkit-backdrop-filter: blur(24px) saturate(180%);
  border: 1.5px solid rgba(119, 153, 119, 0.35);
  border-radius: 10px;
  box-shadow:
    0 4px 20px rgba(40, 80, 40, 0.16),
    inset 0 1px 0 rgba(255, 255, 255, 0.60);
  padding: 4px;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.model-option {
  width: 100%;
  padding: 7px 10px;
  font-size: 12px;
  font-weight: 500;
  color: #2c4e2c;
  background: transparent;
  border: none;
  border-radius: 7px;
  text-align: left;
  white-space: nowrap;
  cursor: pointer;
  transition: background 100ms ease, color 100ms ease;
}
.model-option:hover  { background: rgba(119, 153, 119, 0.14); color: #1a3a1a; }
.model-option.active { background: rgba(119, 153, 119, 0.22); color: #1a3a1a; font-weight: 700; }

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

/* ── Image message bubbles ───────────────────────────────────────── */
.msg--image { padding: 4px; max-width: 70%; }
.msg-img {
  display: block;
  max-width: 100%;
  max-height: 200px;
  border-radius: 9px;
  object-fit: cover;
}
.msg-caption {
  padding: 5px 6px 2px;
  font-size: 12px;
  line-height: 1.35;
}
.msg--user.msg--image .msg-caption { color: #f3f8f3; }

/* ── Image preview tray (in the composer, above the textarea) ─────── */
.chat-tray {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  padding: 8px 8px 2px;
}
.tray-item { position: relative; width: 52px; height: 52px; }
.tray-thumb {
  width: 100%;
  height: 100%;
  object-fit: cover;
  border-radius: 8px;
  border: 1px solid rgba(148, 185, 148, 0.5);
}
.tray-remove {
  position: absolute;
  top: -6px;
  right: -6px;
  width: 17px;
  height: 17px;
  border-radius: 50%;
  border: none;
  background: rgba(60, 80, 60, 0.85);
  color: #fff;
  font-size: 9px;
  line-height: 1;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  opacity: 0;
  transition: opacity 120ms ease;
}
.tray-item:hover .tray-remove { opacity: 1; }

/* ── Transient alert (validation feedback) ───────────────────────── */
.chat-alert {
  position: absolute;
  top: 40px;
  left: 50%;
  transform: translateX(-50%);
  max-width: 86%;
  z-index: 80;
  padding: 7px 12px;
  border-radius: 10px;
  background: rgba(60, 84, 60, 0.94);
  color: #f3f8f3;
  font-size: 12px;
  text-align: center;
  box-shadow: 0 3px 12px rgba(40, 60, 40, 0.3);
}
.alert-enter-active, .alert-leave-active { transition: opacity 200ms ease, transform 200ms ease; }
.alert-enter-from, .alert-leave-to { opacity: 0; transform: translateX(-50%) translateY(-6px); }

/* ── Drag-drop overlay ───────────────────────────────────────────── */
.chat-drop {
  position: absolute;
  inset: 6px;
  z-index: 75;
  border: 2px dashed rgba(119, 153, 119, 0.85);
  border-radius: 14px;
  background: rgba(236, 246, 236, 0.82);
  display: flex;
  align-items: center;
  justify-content: center;
  pointer-events: none;
}
.chat-drop-inner {
  font-size: 13px;
  font-weight: 600;
  color: rgba(40, 80, 40, 0.85);
}

/* ── Enter / leave ───────────────────────────────────────────────── */
.chat-enter-active, .chat-leave-active { transition: opacity 200ms ease, transform 200ms ease; }
.chat-enter-from, .chat-leave-to { opacity: 0; transform: scale(0.97); }
</style>
