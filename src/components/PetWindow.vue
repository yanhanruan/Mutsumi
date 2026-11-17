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
import { ref, computed, onMounted, onUnmounted, watch, nextTick } from 'vue'
import { useAnimator } from '../composables/useAnimator'
import { DEFAULT_ANIMATIONS, IDLE_VARIANTS } from '../config/animations'
import { useAudioReaction } from '../composables/useAudioReaction'
import { useUpdateCheck } from '../composables/useUpdateCheck'
import { useMidnightAutoSleep } from '../composables/useMidnightAutoSleep'
import { prepareFlightCollision, startFlightInsetsSync } from '../composables/useFlightCollision'
import { useHitTest } from '../composables/useHitTest'
import { usePetStatus } from '../composables/usePetStatus'
import { useI18n } from '../i18n'
import { MUTSUMI_ALL_QUOTES } from '../data/mutsumiQuotes'
import { getCurrentWindow, currentMonitor } from '@tauri-apps/api/window'
import { LogicalSize } from '@tauri-apps/api/dpi'
import { invoke } from '@tauri-apps/api/core'
import { useAppConfig, CHAR_SIZE_DIMS, SYS_WINDOW_DIMS } from '../composables/useAppConfig'
import { useWeatherAvailable } from '../composables/useWeatherAvailable'
import { usePlatformCapabilities } from '../composables/usePlatformCapabilities'
import { TAROT_WINDOW_DIMS } from '../config/tarot'
import { CHAT_WINDOW_DIMS } from '../config/chat'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import ChatBubble from './ChatBubble.vue'
import PomodoroBadge from './PomodoroBadge.vue'
import WeatherBadge from './WeatherBadge.vue'
import MusicBadge from './MusicBadge.vue'
import SleepZzz from './SleepZzz.vue'
import TarotCard from './TarotCard.vue'
import ChatPanel from './ChatPanel.vue'
import SystemStateOverlay from './SystemStateOverlay.vue'
import ContextMenu, { type MenuAction, type ContextActionKey } from './ContextMenu.vue'

const imgRef = ref<HTMLImageElement | null>(null)

const {
  currentName,
  ready,
  sleeping,
  flying,
  spriteOpacity,
  queueAnim,
  setAnim,
  enterSleep,
  exitSleep,
  enterFlight,
  exitFlight,
  getPending,
  cancelPending,
  getCurrentImage,
  getAnimFrames,
  setIdleVariant,
  setAudioActive,
} = useAnimator(DEFAULT_ANIMATIONS, imgRef)
useAudioReaction(queueAnim, currentName, getPending, cancelPending, setAudioActive)
usePetStatus(setIdleVariant)
// Daily background check for a newer release; opens the update pop-up if found.
useUpdateCheck()
// True while a full-window overlay (tarot card / chat / system state panel) is
// open. Declared before useHitTest (which reads them) and the size watch below.
const tarotActive = ref(false)
const chatActive  = ref(false)
const sysStateActive = ref(false)
// Any full-window overlay is open — gates pet interaction + sprite/badge display.
const overlayOpen = computed(() => tarotActive.value || chatActive.value || sysStateActive.value)

// Persistent character heading. Updated by `balloon-facing` events during
// flight and NOT reset on landing, so the orientation she last flew in carries
// over to idle and every other animation. Only left-facing frames exist, so a
// 'right' heading mirrors the sprite (scaleX(-1)); the hit-test below flips its
// alpha sampling to match so click-through stays aligned.
const heading = ref<'left' | 'right'>('left')

// Per-pixel click-through: transparent regions of the pet pass clicks
// through to whatever is behind the window. While an overlay is open, only the
// overlay's panel is interactive; the area around it stays click-through.
useHitTest(getCurrentImage, () => overlayOpen.value, () => heading.value === 'right')

// The flying art (fly_left + fly_to_idle) is graded a touch brighter and less
// saturated than the idle frames, so the fly↔idle handoff shows a colour pop.
// A light CSS filter (see .fly-graded) pulls those frames back to the idle
// look. Applied to all three fly clips so the whole sequence — and both idle
// boundaries — stay consistent. Filter values were fitted against the
// same-pose pair idle/001 vs fly_to_idle/186; brightness/saturate don't touch
// alpha, so hit-testing and flight collision are unaffected.
const flyGraded = computed(() =>
  currentName.value === 'flying' ||
  currentName.value === 'fly_enter' ||
  currentName.value === 'fly_exit')

// TODO re-enable click animation — see onMouseUp:
// Animations that must not be interrupted by a click (bubble still shows).
// pat_head: mid-animation abort would look jarring.
// Music chain: interrupting headphones/music breaks the hardwired sequence.
// const NO_CLICK_INTERRUPT = new Set([
//   'pat_head',
//   'headphones_on', 'headphones_off',
//   'music1', 'music2', 'music3', 'music4',
// ])

const { t, locale } = useI18n()
const { config } = useAppConfig()
const { weatherAvailable } = useWeatherAvailable()
const { musicAvailable } = usePlatformCapabilities()

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
    if (overlayOpen.value) return
    const [w, h] = CHAR_SIZE_DIMS[size]
    getCurrentWindow().setSize(new LogicalSize(w, h))
  },
  { immediate: true },
)

const bubbleRef  = ref<InstanceType<typeof ChatBubble> | null>(null)
const contextRef = ref<InstanceType<typeof ContextMenu> | null>(null)
const tarotRef   = ref<InstanceType<typeof TarotCard> | null>(null)
const chatRef    = ref<InstanceType<typeof ChatPanel> | null>(null)
const sysStateRef = ref<InstanceType<typeof SystemStateOverlay> | null>(null)

// ── Tarot overlay ──────────────────────────────────────────────────
// Integrated in-window reading (not a separate OS window). On open the main
// window grows to a card-suitable size scaled by the 大中小 setting and the
// pet sprite is hidden; on close both are restored to the pet's position/size.
// (tarotActive is declared above — the size watch reads it on its immediate run.)
let savedPos: Awaited<ReturnType<ReturnType<typeof getCurrentWindow>['outerPosition']>> | null = null

// Apply size + position in one atomic native op (SetWindowPos / NSWindow setFrame).
// so the window lands at its final bounds in a single step. Two separate ops
// (setSize then center) produced a visible grow-then-recenter jump and could
// re-enter tao's paint flush and panic. Bounds are physical pixels.
async function setBounds(x: number, y: number, pw: number, ph: number) {
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
  await setBounds(x, y, pw, ph)
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
  await setBounds(savedPos?.x ?? 0, savedPos?.y ?? 0, lw * sf, lh * sf)
  // Reveal the pet at the restored small position.
  tarotActive.value = false
}

// ── Chat overlay ───────────────────────────────────────────────────
// Same window-grow/restore choreography as the tarot overlay, sized via
// CHAT_WINDOW_DIMS. The pet sprite + badges hide while chat is open.
async function openChat() {
  const win = getCurrentWindow()
  try { savedPos = await win.outerPosition() } catch { savedPos = null }
  bubbleRef.value?.hide()
  chatActive.value = true
  chatRef.value?.open()          // show the overlay BEFORE growing — it covers the window
  await nextTick()
  await nextPaint()

  const [lw, lh] = CHAT_WINDOW_DIMS[config.value.characterSize]
  const sf  = await win.scaleFactor()
  const mon = await currentMonitor()
  const pw  = lw * sf
  const ph  = lh * sf
  let x = savedPos?.x ?? 0
  let y = savedPos?.y ?? 0
  if (mon) {
    x = mon.position.x + (mon.size.width  - pw) / 2
    y = mon.position.y + (mon.size.height - ph) / 2
  }
  await setBounds(x, y, pw, ph)
}

async function closeChat() {
  const win = getCurrentWindow()
  const [lw, lh] = CHAR_SIZE_DIMS[config.value.characterSize]
  const sf = await win.scaleFactor()
  chatRef.value?.dismiss()
  await nextTick()
  await nextPaint()
  await setBounds(savedPos?.x ?? 0, savedPos?.y ?? 0, lw * sf, lh * sf)
  chatActive.value = false
}

// ── System State overlay ───────────────────────────────────────────
// Same choreography as above, sized via SYS_WINDOW_DIMS.
async function openSysState() {
  const win = getCurrentWindow()
  try { savedPos = await win.outerPosition() } catch { savedPos = null }
  bubbleRef.value?.hide()
  sysStateActive.value = true
  sysStateRef.value?.open()
  await nextTick()
  await nextPaint()

  const [lw, lh] = SYS_WINDOW_DIMS[config.value.characterSize]
  const sf  = await win.scaleFactor()
  const mon = await currentMonitor()
  const pw  = lw * sf
  const ph  = lh * sf
  let x = savedPos?.x ?? 0
  let y = savedPos?.y ?? 0
  if (mon) {
    x = mon.position.x + (mon.size.width  - pw) / 2
    y = mon.position.y + (mon.size.height - ph) / 2
  }
  await setBounds(x, y, pw, ph)
}

async function closeSysState() {
  const win = getCurrentWindow()
  const [lw, lh] = CHAR_SIZE_DIMS[config.value.characterSize]
  const sf = await win.scaleFactor()
  sysStateRef.value?.dismiss()
  await nextTick()
  await nextPaint()
  await setBounds(savedPos?.x ?? 0, savedPos?.y ?? 0, lw * sf, lh * sf)
  sysStateActive.value = false
}

// ── Sleep mode ─────────────────────────────────────────────────────
// `sleeping` (from useAnimator) is the single source of truth. The animator
// owns the idle↔sleep frame swap and its crossfade (enterSleep/exitSleep);
// PetWindow only layers the app-level reactions: a backend energy nudge and
// the rest-state bubble on the way in.
async function sleepPet() {
  if (sleeping.value) return
  await enterSleep()
  void invoke('pet_context_action', { action: 'sleep' })
  bubbleRef.value?.show(t.value.contextResponses.sleep)
}

async function wakePet() {
  await exitSleep()
}

function toggleSleep() {
  if (sleeping.value) wakePet()
  else                sleepPet()
}

// Auto-sleep at midnight, but only when she's genuinely idle (not already
// asleep, no overlay open, not mid music/click/pat animation).
useMidnightAutoSleep({
  canSleep: () =>
    !sleeping.value && !overlayOpen.value && IDLE_VARIANTS.has(currentName.value),
  sleep: sleepPet,
})

// ── Mouse interaction ──────────────────────────────────────────────
const DRAG_THRESHOLD = 5
let pressX = 0
let pressY = 0
let pressed = false
let didDrag = false

function onMouseDown(e: MouseEvent) {
  if (e.button !== 0 || overlayOpen.value) return
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
  if (e.button !== 0 || overlayOpen.value) return
  if (pressed && !didDrag) {
    // True click — no movement.
    if (sleeping.value) {
      // Asleep: she stays asleep and just mumbles. No pet_click, no waking.
      const lines = t.value.sleepTalk
      bubbleRef.value?.show(lines[Math.floor(Math.random() * lines.length)])
    } else {
      // TODO re-enable click animation:
      // Suppress the click animation while in music mode so it doesn't
      // interrupt headphones/music playback. The bubble still shows.
      // if (!NO_CLICK_INTERRUPT.has(currentName.value)) {
      //   setAnim('click')   // immediate switch — no pending-anim delay
      // }
      void invoke('pet_click')
      const quote = MUTSUMI_ALL_QUOTES[Math.floor(Math.random() * MUTSUMI_ALL_QUOTES.length)]
      bubbleRef.value?.show(quote[locale.value])
    }
  }
  pressed = false
  didDrag = false
}

// ── Right-click context menu ───────────────────────────────────────

function onContextMenu(e: MouseEvent) {
  e.preventDefault()
  if (overlayOpen.value) return
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
  if (action === 'chat') {
    await openChat()
    return
  }
  if (action === 'sys_state') {
    await openSysState()
    return
  }
  // Sleep toggles the max-priority rest state (enter ↔ back to idle).
  if (action === 'sleep') {
    toggleSleep()
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
let unlistenBalloon: UnlistenFn | null = null
let unlistenFacing: UnlistenFn | null = null

// ── Flying (balloon) mode ──────────────────────────────────────────
// Rust owns when flight happens (idle detector / flight controller) and
// emits `toggle-balloon-mode` on transitions; the animator plays the
// idle↔fly morphs + flying loop on the pet's own <img> while flight.rs
// glides the window. `balloon-facing` updates `heading` (declared above),
// which mirrors the sprite and persists after landing.
// Stops the per-frame collision-insets poller (useFlightCollision); the
// poller only runs while she is airborne.
let stopInsetsSync: (() => void) | null = null

// The window must hold still while the fly_enter morph plays and only start
// gliding once the flying loop begins. The animator hands off fly_enter →
// flying by name, so that transition is the cue to let Rust move the window.
watch(currentName, name => {
  if (name === 'flying') {
    void invoke('flight_begin_motion').catch(() => {})
  }
})

onMounted(async () => {
  // Sync persisted chat settings to the backend so saved choices survive restarts
  // (the backend boots from .env defaults until these are applied).
  void invoke('set_search_engine', { engine: config.value.searchEngine }).catch(() => {})
  void invoke('set_search_enabled', { enabled: config.value.searchEnabled }).catch(() => {})
  void invoke('qwen_set_chat_model', { model: config.value.chatModel }).catch(() => {})
  void invoke('flight_set_screensaver', {
    enabled: config.value.flyingScreensaver,
    waitMins: config.value.flyingWaitMins,
  }).catch(() => {})

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
  unlistenBalloon = await listen<{ active: boolean }>('toggle-balloon-mode', e => {
    if (e.payload.active) {
      enterFlight()
      // Pixel-perfect bounces: scan the frames' alpha once (chunked,
      // cached), then report the displayed frame's margins to Rust as
      // playback advances — see useFlightCollision.
      void prepareFlightCollision(getAnimFrames('flying'))
      stopInsetsSync?.()
      stopInsetsSync = startFlightInsetsSync(getCurrentImage)
    } else {
      exitFlight()
      stopInsetsSync?.()
      stopInsetsSync = null
    }
  })
  unlistenFacing = await listen<{ facing: 'left' | 'right' }>('balloon-facing', e => {
    heading.value = e.payload.facing
  })
})

onUnmounted(() => {
  unlistenWillHide?.()
  unlistenShow?.()
  unlistenLateNight?.()
  unlistenBalloon?.()
  unlistenFacing?.()
  stopInsetsSync?.()
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
    <img
      v-show="ready && !overlayOpen"
      ref="imgRef"
      class="frame"
      :class="{ mirrored: heading === 'right', 'fly-graded': flyGraded }"
      :style="{ opacity: spriteOpacity }"
      draggable="false"
    />
    <Transition name="zzz-fade">
      <SleepZzz v-if="sleeping && !overlayOpen && !flying && config.showZzz" />
    </Transition>
    <PomodoroBadge v-if="!overlayOpen && !flying" />
    <WeatherBadge v-if="!overlayOpen && !flying && config.showWeather && weatherAvailable !== false" />
    <MusicBadge v-if="!overlayOpen && !flying && config.showMusic && musicAvailable" />
    <div class="bubble-anchor">
      <ChatBubble ref="bubbleRef" />
    </div>
  </div>
  <ContextMenu ref="contextRef" :sleeping="sleeping" @action="onContextAction" />
  <TarotCard ref="tarotRef" @close="closeTarot" />
  <ChatPanel ref="chatRef" @close="closeChat" />
  <SystemStateOverlay ref="sysStateRef" @close="closeSysState" />
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
  /* Soft dip-to-transparent crossfade when entering/leaving sleep, driven by
     the animator's spriteOpacity. Keep in sync with SLEEP_FADE_MS in
     useAnimator.ts. */
  transition: opacity 220ms ease;
}
/* Only left-facing frames exist — mirror the whole sprite for a right-facing
   heading (from `balloon-facing`), which persists across idle and every other
   animation, not just flight. */
.frame.mirrored {
  transform: scaleX(-1);
}
/* Colour-match the flying art to the idle frames (fitted: idle/001 vs
   fly_to_idle/186). brightness+saturate commute, so order is irrelevant; both
   leave alpha untouched. Toggled instantly (no transition) so the graded fly
   frame lines up exactly with the idle frame at the handoff. */
.frame.fly-graded {
  filter: brightness(0.98) saturate(1.08);
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
/* Gentle fade for the whole zzz group as she drifts off / wakes up, so the
   loop doesn't pop in or get cut mid-drift. */
.zzz-fade-enter-active,
.zzz-fade-leave-active {
  transition: opacity 320ms ease;
}
.zzz-fade-enter-from,
.zzz-fade-leave-to {
  opacity: 0;
}
</style>
