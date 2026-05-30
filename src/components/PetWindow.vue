<script setup lang="ts">
/**
 * PetWindow — main desktop pet display.
 *
 * - Animation playback delegated to useAnimator (state machine + RAF loop).
 * - Drag is deferred: we don't call Tauri's startDragging() until the cursor
 *   has actually moved past a 5 px threshold. Calling it on every mousedown
 *   swallows the mouseup (the OS captures the mouse), making click handling
 *   impossible. Defer instead — quick clicks then dispatch normally.
 * - Chat bubble is absolutely positioned above the pet so the pet fills the
 *   entire window.
 */
import { ref, onMounted, onUnmounted, watch } from 'vue'
import { useAnimator } from '../composables/useAnimator'
import { useAudioReaction } from '../composables/useAudioReaction'
import { useHitTest } from '../composables/useHitTest'
import { usePetStatus } from '../composables/usePetStatus'
import { useI18n } from '../i18n'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { LogicalSize } from '@tauri-apps/api/dpi'
import { invoke } from '@tauri-apps/api/core'
import { useAppConfig, CHAR_SIZE_DIMS } from '../composables/useAppConfig'
import { useWeatherAvailable } from '../composables/useWeatherAvailable'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import ChatBubble from './ChatBubble.vue'
import PomodoroBadge from './PomodoroBadge.vue'
import WeatherBadge from './WeatherBadge.vue'
import BalloonPet from './BalloonPet.vue'
import ContextMenu, { type MenuAction, type ContextActionKey } from './ContextMenu.vue'

const {
  currentSrc,
  currentName,
  queueAnim,
  setAnim,
  getPending,
  cancelPending,
  getCurrentImage,
  setIdleVariant,
} = useAnimator()
useAudioReaction(queueAnim, currentName, getPending, cancelPending)
usePetStatus(setIdleVariant)
// Per-pixel click-through: transparent regions of the pet pass clicks
// through to whatever is behind the window.
useHitTest(getCurrentImage)

// Animations during which click animation is suppressed (bubble still shows).
const MUSIC_MODE = new Set(['headphones_on', 'headphones_off', 'music1', 'music2', 'music3', 'music4'])

const { t } = useI18n()
const { config } = useAppConfig()
const { weatherAvailable } = useWeatherAvailable()

// ── Window sizing (Task 3) ─────────────────────────────────────────
// Resize the main window whenever the user changes the character size
// in the settings. `immediate: true` applies it on first mount too.
watch(
  () => config.value.characterSize,
  size => {
    const [w, h] = CHAR_SIZE_DIMS[size]
    getCurrentWindow().setSize(new LogicalSize(w, h))
  },
  { immediate: true },
)

const bubbleRef  = ref<InstanceType<typeof ChatBubble> | null>(null)
const contextRef = ref<InstanceType<typeof ContextMenu> | null>(null)

// ── Mouse interaction ──────────────────────────────────────────────
const DRAG_THRESHOLD = 5
let pressX = 0
let pressY = 0
let pressed = false
let didDrag = false

function onMouseDown(e: MouseEvent) {
  if (e.button !== 0) return
  pressX = e.screenX
  pressY = e.screenY
  pressed = true
  didDrag = false
}

async function onMouseMove(e: MouseEvent) {
  if (!pressed || didDrag) return
  const dx = Math.abs(e.screenX - pressX)
  const dy = Math.abs(e.screenY - pressY)
  if (dx + dy >= DRAG_THRESHOLD) {
    // First real movement — hand off to OS-driven drag.
    didDrag = true
    bubbleRef.value?.hide()
    await getCurrentWindow().startDragging()
    // startDragging resolves when the user releases the mouse. The webview
    // does not see a mouseup after that (the OS consumed it), so reset
    // press state here.
    pressed = false
    // Persist gentle drag (drag duration tracking not implemented yet).
    void invoke('pet_drag_end', { rough: false })
  }
}

function onMouseUp(e: MouseEvent) {
  if (e.button !== 0) return
  if (pressed && !didDrag) {
    // True click — no movement.
    // Suppress the click animation while in music mode so it doesn't
    // interrupt headphones/music playback. The bubble still shows.
    if (!MUSIC_MODE.has(currentName.value)) {
      setAnim('click')   // immediate switch — no pending-anim delay
    }
    void invoke('pet_click')
    const phrases = t.value.clickPhrases
    const phrase = phrases[Math.floor(Math.random() * phrases.length)]
    bubbleRef.value?.show(phrase)
  }
  pressed = false
  didDrag = false
}

// ── Right-click context menu ───────────────────────────────────────

function onContextMenu(e: MouseEvent) {
  e.preventDefault()
  contextRef.value?.open(e.clientX, e.clientY)
}

async function onContextAction(action: MenuAction) {
  // Task 5: hide is frontend-only — no backend command, no bubble.
  if (action === 'hide') {
    await getCurrentWindow().hide()
    return
  }
  await invoke('pet_context_action', { action })
  bubbleRef.value?.show(t.value.contextResponses[action as ContextActionKey])
}

// ── Late-night reminder ────────────────────────────────────────────
// The backend fires `late-night-reminder` once per night when the local
// hour first crosses into [00:00, 04:59). Show the locale-appropriate phrase.
let unlistenLateNight: UnlistenFn | null = null

onMounted(async () => {
  unlistenLateNight = await listen('late-night-reminder', () => {
    bubbleRef.value?.show(t.value.lateNightReminder)
  })
})

onUnmounted(() => {
  unlistenLateNight?.()
})
</script>

<template>
  <div
    class="pet"
    @mousedown="onMouseDown"
    @mousemove="onMouseMove"
    @mouseup="onMouseUp"
    @contextmenu="onContextMenu"
  >
    <img v-if="currentSrc" :src="currentSrc" class="frame" draggable="false" />
    <PomodoroBadge />
    <WeatherBadge v-if="config.showWeather && weatherAvailable !== false" />
    <div class="bubble-anchor">
      <ChatBubble ref="bubbleRef" />
    </div>
  </div>
  <ContextMenu ref="contextRef" @action="onContextAction" />
  <BalloonPet />
</template>

<style scoped>
.pet {
  position: relative;
  width: 100%;
  height: 100%;
  background: transparent;
  cursor: pointer;
}
.frame {
  width: 100%;
  height: 100%;
  object-fit: contain;
  display: block;
  pointer-events: none;
  -webkit-user-drag: none;
}
.bubble-anchor {
  position: absolute;
  top: 4px;
  left: 0;
  right: 0;
  display: flex;
  justify-content: center;
  pointer-events: none;
}
</style>
