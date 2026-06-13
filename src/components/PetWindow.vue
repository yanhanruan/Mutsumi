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
import { ref, onMounted, onUnmounted, watch, nextTick } from 'vue'
import { useAnimator, DEFAULT_ANIMATIONS } from '../composables/useAnimator'
import { useAudioReaction } from '../composables/useAudioReaction'
import { useHitTest } from '../composables/useHitTest'
import { usePetStatus } from '../composables/usePetStatus'
import { useI18n } from '../i18n'
import { MUTSUMI_ALL_QUOTES } from '../data/mutsumiQuotes'
import { getCurrentWindow, currentMonitor } from '@tauri-apps/api/window'
import { LogicalSize } from '@tauri-apps/api/dpi'
import { invoke } from '@tauri-apps/api/core'
import { useAppConfig, CHAR_SIZE_DIMS } from '../composables/useAppConfig'
import { useWeatherAvailable } from '../composables/useWeatherAvailable'
import { TAROT_WINDOW_DIMS } from '../config/tarot'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import ChatBubble from './ChatBubble.vue'
import PomodoroBadge from './PomodoroBadge.vue'
import WeatherBadge from './WeatherBadge.vue'
import BalloonPet from './BalloonPet.vue'
import TarotCard from './TarotCard.vue'
import SystemStateOverlay from './SystemStateOverlay.vue'
import ContextMenu, { type MenuAction, type ContextActionKey } from './ContextMenu.vue'

const imgRef = ref<HTMLImageElement | null>(null)

const {
  currentName,
  ready,
  queueAnim,
  setAnim,
  getPending,
  cancelPending,
  getCurrentImage,
  setIdleVariant,
} = useAnimator(DEFAULT_ANIMATIONS, imgRef)
useAudioReaction(queueAnim, currentName, getPending, cancelPending)
usePetStatus(setIdleVariant)
// Per-pixel click-through: transparent regions of the pet pass clicks
// through to whatever is behind the window.
useHitTest(getCurrentImage)

// Animations that must not be interrupted by a click (bubble still shows).
// pat_head: mid-animation abort would look jarring.
// Music chain: interrupting headphones/music breaks the hardwired sequence.
const NO_CLICK_INTERRUPT = new Set([
  'pat_head',
  'headphones_on', 'headphones_off',
  'music1', 'music2', 'music3', 'music4',
])

const { t, locale } = useI18n()
const { config } = useAppConfig()
const { weatherAvailable } = useWeatherAvailable()

// True while the tarot overlay is open (declared before the size watch below,
// which reads it on its immediate run).
const tarotActive = ref(false)

// ── Window show / hide transitions ────────────────────────────────
// petOpacity drives a CSS opacity transition. It starts at 1, goes to 0 when
// hiding, and goes back to 1 when pet-show is received.
const petOpacity = ref(1)


// ── Window sizing (Task 3) ─────────────────────────────────────────
// Resize the main window whenever the user changes the character size
// in the settings. `immediate: true` applies it on first mount too.
// While the tarot overlay is open the window is held at its larger tarot
// size, so skip pet-size syncing until it closes.
watch(
  () => config.value.characterSize,
  size => {
    if (tarotActive.value) return
    const [w, h] = CHAR_SIZE_DIMS[size]
    getCurrentWindow().setSize(new LogicalSize(w, h))
  },
  { immediate: true },
)

const bubbleRef  = ref<InstanceType<typeof ChatBubble> | null>(null)
const contextRef = ref<InstanceType<typeof ContextMenu> | null>(null)
const tarotRef   = ref<InstanceType<typeof TarotCard> | null>(null)
const sysStateRef = ref<InstanceType<typeof SystemStateOverlay> | null>(null)

// ── Tarot overlay ──────────────────────────────────────────────────
// Integrated in-window reading (not a separate OS window). On open the main
// window grows to a card-suitable size scaled by the 大中小 setting and the
// pet sprite is hidden; on close both are restored to the pet's position/size.
// (tarotActive is declared above — the size watch reads it on its immediate run.)
let savedPos: Awaited<ReturnType<ReturnType<typeof getCurrentWindow>['outerPosition']>> | null = null

// Apply size + position in ONE atomic native op (set_window_bounds → SetWindowPos)
// so the window lands at its final bounds in a single step. Two separate ops
// (setSize then center) produced a visible grow-then-recenter jump and could
// re-enter tao's paint flush and panic. Bounds are physical pixels.
async function setBounds(win: ReturnType<typeof getCurrentWindow>, x: number, y: number, pw: number, ph: number) {
  await invoke('set_window_bounds', { x: Math.round(x), y: Math.round(y), width: Math.round(pw), height: Math.round(ph) })
}

// Resolve after the browser has actually painted (two rAFs: the first schedules
// before the next paint, the second confirms it happened). Used to guarantee
// the overlay is on screen BEFORE we move the window, so the move never carries
// a stale pet/bubble frame to the new position.
const nextPaint = () => new Promise<void>(r =>
  requestAnimationFrame(() => requestAnimationFrame(() => r())),
)

// Ordering: the overlay must be painted before the window moves, so a
// wrongly-sized pet frame is never carried to the new centre.
async function openTarot() {
  const win = getCurrentWindow()
  try { savedPos = await win.outerPosition() } catch { savedPos = null }   // physical
  bubbleRef.value?.hide()
  tarotActive.value = true       // hide the pet sprite
  tarotRef.value?.open()         // show the overlay BEFORE growing — it covers the window
  await nextTick()               // flush the DOM update
  await nextPaint()              // …and wait until the overlay has actually painted

  const [lw, lh] = TAROT_WINDOW_DIMS[config.value.characterSize]
  const sf  = await win.scaleFactor()
  const mon = await currentMonitor()
  const pw  = lw * sf
  const ph  = lh * sf
  // Centre on the monitor the pet sits on; fall back to growing in place.
  let x = savedPos?.x ?? 0
  let y = savedPos?.y ?? 0
  if (mon) {
    x = mon.position.x + (mon.size.width  - pw) / 2
    y = mon.position.y + (mon.size.height - ph) / 2
  }
  await setBounds(win, x, y, pw, ph)
}

async function closeTarot() {
  const win = getCurrentWindow()
  const [lw, lh] = CHAR_SIZE_DIMS[config.value.characterSize]
  const sf = await win.scaleFactor()
  // Remove the card while the window is STILL centred (its correct place), and
  // wait for that now-empty/transparent frame to paint — so no card lingers to
  // flash at the pet's original position when we move the window back.
  tarotRef.value?.dismiss()
  await nextTick()
  await nextPaint()
  // The window is now empty/transparent; shrink + move it back invisibly.
  await setBounds(win, savedPos?.x ?? 0, savedPos?.y ?? 0, lw * sf, lh * sf)
  // Reveal the pet at the restored small position.
  tarotActive.value = false
}

// ── Mouse interaction ──────────────────────────────────────────────
const DRAG_THRESHOLD = 5
let pressX = 0
let pressY = 0
let pressed = false
let didDrag = false

function onMouseDown(e: MouseEvent) {
  if (e.button !== 0 || tarotActive.value) return
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
  if (e.button !== 0 || tarotActive.value) return
  if (pressed && !didDrag) {
    // True click — no movement.
    // Suppress the click animation while in music mode so it doesn't
    // interrupt headphones/music playback. The bubble still shows.
    if (!NO_CLICK_INTERRUPT.has(currentName.value)) {
      setAnim('click')   // immediate switch — no pending-anim delay
    }
    void invoke('pet_click')
    const quote = MUTSUMI_ALL_QUOTES[Math.floor(Math.random() * MUTSUMI_ALL_QUOTES.length)]
    bubbleRef.value?.show(quote[locale.value])
  }
  pressed = false
  didDrag = false
}

// ── Right-click context menu ───────────────────────────────────────

function onContextMenu(e: MouseEvent) {
  e.preventDefault()
  if (tarotActive.value) return
  contextRef.value?.open(e.clientX, e.clientY)
}

async function onContextAction(action: MenuAction) {
  // Frontend-only actions — no backend command, no response bubble.
  if (action === 'hide') {
    void invoke('hide_pet')
    return
  }
  if (action === 'tarot') {
    await openTarot()
    return
  }
  if (action === 'sys_state') {
    sysStateRef.value?.open()
    return
  }
  // Play the pat_head animation immediately (like click — no pending delay).
  if (action === 'pat_head') {
    setAnim('pat_head')
  }
  await invoke('pet_context_action', { action })
  bubbleRef.value?.show(t.value.contextResponses[action as ContextActionKey])
}

// ── Late-night reminder ────────────────────────────────────────────
// The backend fires `late-night-reminder` once per night when the local
// hour first crosses into [00:00, 04:59). Show the locale-appropriate phrase.
let unlistenLateNight: UnlistenFn | null = null
let unlistenWillHide: UnlistenFn | null = null
let unlistenShow: UnlistenFn | null = null

onMounted(async () => {
  // Fade-out: Rust emits "pet-will-hide" before w.hide(); drive the CSS transition.
  unlistenWillHide = await listen('pet-will-hide', () => {
    petOpacity.value = 0
  })
  
  // Fade-in: Rust emits "pet-show" right after w.show().
  // We use requestAnimationFrame to ensure the browser has painted the 0-opacity
  // frame (which it held while hidden) before we flip it back to 1.
  unlistenShow = await listen('pet-show', () => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        petOpacity.value = 1
      })
    })
  })
  unlistenLateNight = await listen('late-night-reminder', () => {
    bubbleRef.value?.show(t.value.lateNightReminder)
  })
})

onUnmounted(() => {
  unlistenWillHide?.()
  unlistenShow?.()
  unlistenLateNight?.()
})
</script>

<template>
  <div
    class="pet"
    :style="{ opacity: petOpacity }"
    @mousedown="onMouseDown"
    @mousemove="onMouseMove"
    @mouseup="onMouseUp"
    @contextmenu="onContextMenu"
  >
    <img v-show="ready && !tarotActive" ref="imgRef" class="frame" draggable="false" />
    <PomodoroBadge v-if="!tarotActive" />
    <WeatherBadge v-if="!tarotActive && config.showWeather && weatherAvailable !== false" />
    <div class="bubble-anchor">
      <ChatBubble ref="bubbleRef" />
    </div>
  </div>
  <ContextMenu ref="contextRef" @action="onContextAction" />
  <TarotCard ref="tarotRef" @close="closeTarot" />
  <SystemStateOverlay ref="sysStateRef" />
  <BalloonPet />
</template>

<style scoped>
.pet {
  position: relative;
  width: 100%;
  height: 100%;
  background: transparent;
  cursor: pointer;
  transition: opacity 200ms ease;
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
