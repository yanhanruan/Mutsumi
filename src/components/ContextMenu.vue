<script setup lang="ts">
/**
 * ContextMenu — Vertical Glass Bubble Panel.
 *
 * Scrollable frosted-glass bubbles on the left edge of the window.
 * Vue <Transition> owns enter/leave timing — no manual closing state,
 * no setTimeout, no !important overrides.
 */
import { ref, computed, watch, nextTick } from 'vue'
import { useI18n } from '../i18n'
import type { Translations } from '../i18n/types'
import { useAppConfig } from '../composables/useAppConfig'

// ── Types ────────────────────────────────────────────────────────────

export type ContextActionKey = 'pat_head' | 'feed' | 'sleep' | 'fast_learning'
/** 'tarot', 'iching', 'chat' and 'hide' are frontend-only actions (no backend command). */
export type MenuAction = ContextActionKey | 'tarot' | 'iching' | 'sys_state' | 'chat' | 'hide'

type MenuLabelKey = keyof Translations['contextMenuItems']

interface BubbleDef {
  action: MenuAction
  icon:   string
  /**
   * Alternate icon + label shown while the pet is asleep, turning the bubble
   * into a toggle. Today only `sleep` uses it (💤/Sleep → ☀️/Wake Up); kept
   * as data so any future toggle bubble can opt in without touching render
   * logic.
   */
  sleepingIcon?:     string
  sleepingLabelKey?: MenuLabelKey
}

const BUBBLE_DEFS: BubbleDef[] = [
  { action: 'pat_head',      icon: '✋' },
  { action: 'feed',          icon: '🍵' },
  { action: 'sleep',         icon: '💤', sleepingIcon: '☀️', sleepingLabelKey: 'wake' },
  { action: 'fast_learning', icon: '📚' },
  { action: 'tarot',         icon: '🔮' },
  { action: 'iching',        icon: '☯' },
  { action: 'sys_state',     icon: '🖥️' },
  { action: 'chat',          icon: '💬' },
  { action: 'hide',          icon: '👻' },
]

// ── State ────────────────────────────────────────────────────────────

const { t } = useI18n()
const { config } = useAppConfig()
const props = withDefaults(defineProps<{ sleeping?: boolean }>(), { sleeping: false })
const emit = defineEmits<{ action: [action: MenuAction] }>()

// ── Size scaling ─────────────────────────────────────────────────────
// Bubble dimensions scale with the character size setting.
// Base (large): 36 px bubble. Scale factors match window-width ratios:
//   small 140/200 = 0.70 → 25 px  |  medium 170/200 = 0.85 → 31 px

const BUBBLE_PX: Record<string, number> = { small: 25, medium: 31, large: 36 }

const panelStyle = computed(() => {
  const px = BUBBLE_PX[config.value.characterSize] ?? 36
  return { '--bubble-size': `${px}px` }
})

const visible       = ref(false)
const hoveredAction = ref<MenuAction | null>(null)
const panelRef      = ref<HTMLElement | null>(null)
const floatingTip   = ref<{ label: string; top: number; left: number } | null>(null)
const isScrolling   = ref(false)
// When closing because a bubble was clicked, skip the leave transition: the
// fade-out would otherwise play *after* a tarot-triggered window move and
// appear as ghost bubbles at the new window centre.
const skipLeave     = ref(false)
let scrollTimer: ReturnType<typeof setTimeout> | null = null

const items = computed(() =>
  BUBBLE_DEFS.map(b => {
    // A bubble with a sleeping-state alternate flips icon + label while asleep.
    const asleep   = props.sleeping && !!b.sleepingIcon
    const labelKey: MenuLabelKey = asleep && b.sleepingLabelKey ? b.sleepingLabelKey : b.action
    return {
      action: b.action,
      icon:   asleep ? b.sleepingIcon! : b.icon,
      label:  t.value.contextMenuItems[labelKey],
    }
  })
)

// ── Public API ───────────────────────────────────────────────────────

function open(_x = 0, _y = 0) {
  if (visible.value) {
    close()
  } else {
    visible.value = true
  }
}

function close() {
  if (!visible.value) return
  hoveredAction.value = null
  floatingTip.value = null
  isScrolling.value = false
  if (scrollTimer) {
    clearTimeout(scrollTimer)
    scrollTimer = null
  }
  visible.value = false
  document.removeEventListener('mousedown', onDocumentClick, true)
  document.removeEventListener('keydown',   onKeyDown,       true)
}

function showTip(item: { action: MenuAction; label: string }, target: EventTarget | null) {
  hoveredAction.value = item.action
  if (!(target instanceof HTMLElement)) return
  const rect = target.getBoundingClientRect()
  floatingTip.value = {
    label: item.label,
    top: rect.top + rect.height / 2,
    left: rect.right + 8,
  }
}

function hideTip() {
  hoveredAction.value = null
  floatingTip.value = null
}

function refreshTipPosition() {
  if (!hoveredAction.value || !panelRef.value) return
  const wrap = panelRef.value.querySelector<HTMLElement>(`[data-action="${hoveredAction.value}"]`)
  const item = items.value.find(i => i.action === hoveredAction.value)
  if (wrap && item) showTip(item, wrap)
}

function onPanelScroll() {
  isScrolling.value = true
  refreshTipPosition()
  if (scrollTimer) clearTimeout(scrollTimer)
  scrollTimer = setTimeout(() => {
    isScrolling.value = false
    scrollTimer = null
  }, 650)
}

function onBubbleClick(action: MenuAction) {
  emit('action', action)
  skipLeave.value = true   // vanish immediately — no fade-out at the new window pos
  close()
  void nextTick(() => { skipLeave.value = false })
}

// ── Outside-click / Escape ───────────────────────────────────────────

function onDocumentClick(e: MouseEvent) {
  // Right-click (button 2) fires mousedown before contextmenu.
  // If we closed here, open() would immediately re-open — toggle would never work.
  // Let contextmenu handle the right-click toggle; we only dismiss on left-click outside.
  if (e.button !== 0) return
  const el = document.querySelector('.bubble-panel')
  if (!el?.contains(e.target as Node)) close()
}

function onKeyDown(e: KeyboardEvent) {
  if (e.key === 'Escape') close()
}

watch(visible, v => {
  if (v) {
    // One frame delay so the right-click mousedown that opened the panel
    // is not immediately caught as an outside click.
    requestAnimationFrame(() => {
      document.addEventListener('mousedown', onDocumentClick, true)
      document.addEventListener('keydown',   onKeyDown,       true)
    })
  }
})

defineExpose({ open, close })
</script>

<template>
  <!-- duration: enter covers stagger (4×45ms) + transition (300ms) = 480ms → 520ms;
       leave matches transition duration (160ms) → 180ms -->
  <Transition name="panel" :css="!skipLeave" :duration="{ enter: 520, leave: 180 }">
    <div
      v-if="visible"
      ref="panelRef"
      class="bubble-panel pet-ui-overlay"
      :class="{ scrolling: isScrolling }"
      :style="panelStyle"
      @scroll="onPanelScroll"
    >
      <div
        v-for="(item, i) in items"
        :key="item.action"
        class="bubble-wrap pet-ui-overlay"
        :data-action="item.action"
        :style="{ '--i': i }"
        @mouseenter="showTip(item, $event.currentTarget)"
        @mouseleave="hideTip"
      >
        <button
          class="bubble"
          :class="{ 'bubble--hide': item.action === 'hide' }"
          @click.stop="onBubbleClick(item.action)"
        >
          <span class="bubble-icon">{{ item.icon }}</span>
        </button>

      </div>
    </div>
  </Transition>
  <Transition name="tip">
    <div
      v-if="floatingTip"
      class="bubble-tip pet-ui-overlay"
      :style="{ ...panelStyle, top: `${floatingTip.top}px`, left: `${floatingTip.left}px` }"
    >
      {{ floatingTip.label }}
    </div>
  </Transition>
</template>

<style scoped>
/* ── Panel ───────────────────────────────────────────────────────── */
/* --bubble-size drives all size-sensitive values via calc().
   Default (36px) matches the "large" character setting.             */
.bubble-panel {
  --bubble-size: 36px; /* overridden by inline :style per size tier  */

  position: fixed;
  left: calc(var(--bubble-size) * 0.22);   /* ~8 px at large */
  top: 50%;
  transform: translateY(-50%);
  z-index: 999;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: calc(var(--bubble-size) * 0.19);    /* ~7 px at large */
  max-height: calc(100vh - calc(var(--bubble-size) * 0.44));
  overflow-y: auto;
  overflow-x: hidden;
  overscroll-behavior: contain;
  padding-block: calc(var(--bubble-size) * 0.11);
  padding-right: calc(var(--bubble-size) * 0.12);
  pointer-events: auto;
  scrollbar-width: none;
  scrollbar-color: transparent transparent;
}

.bubble-panel.scrolling {
  scrollbar-width: thin;
  scrollbar-color: rgba(119, 153, 119, 0.45) transparent;
}

.bubble-panel::-webkit-scrollbar {
  width: 0;
}

.bubble-panel.scrolling::-webkit-scrollbar {
  width: 5px;
}

.bubble-panel::-webkit-scrollbar-track {
  background: transparent;
}

.bubble-panel::-webkit-scrollbar-thumb {
  border-radius: 999px;
  background: transparent;
}

.bubble-panel.scrolling::-webkit-scrollbar-thumb {
  background: rgba(119, 153, 119, 0.42);
}

/* ── Enter ───────────────────────────────────────────────────────── */
/* Explicit initial state — applied synchronously on DOM insertion,
   so no flash before the transition starts.                         */
.panel-enter-from .bubble-wrap {
  opacity: 0;
  transform: translateX(-14px);
}
/* Staggered transition to natural (visible) state.
   Suppress .bubble hover transition so it can't compete.            */
.panel-enter-active .bubble-wrap {
  transition:
    opacity  300ms cubic-bezier(0.22, 1, 0.36, 1),
    transform 300ms cubic-bezier(0.22, 1, 0.36, 1);
  transition-delay: calc(var(--i, 0) * 45ms);
}
.panel-enter-active .bubble { transition: none; }

/* ── Leave ───────────────────────────────────────────────────────── */
/* All bubbles fade-slide out together; no hover interaction.        */
.panel-leave-active .bubble-wrap {
  transition:
    opacity  160ms ease-in,
    transform 160ms ease-in;
  pointer-events: none;
}
.panel-leave-active .bubble { transition: none; }
.panel-leave-to .bubble-wrap {
  opacity: 0;
  transform: translateX(-10px);
}

/* ── Bubble wrapper ──────────────────────────────────────────────── */
.bubble-wrap {
  position: relative;
  display: flex;
  align-items: center;
  pointer-events: auto;
}

/* ── Circular button ─────────────────────────────────────────────── */
.bubble {
  width:  var(--bubble-size);
  height: var(--bubble-size);
  border-radius: 50%;
  border: 1px solid rgba(119, 153, 119, 0.28);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;

  background: rgba(228, 242, 228, 0.72);
  backdrop-filter: blur(24px) saturate(160%);
  -webkit-backdrop-filter: blur(24px) saturate(160%);
  box-shadow:
    0 2px 10px rgba(40, 70, 40, 0.10),
    inset 0 1px 0 rgba(255, 255, 255, 0.68);

  transition:
    transform    260ms cubic-bezier(0.34, 0, 0.24, 1),
    background   180ms ease,
    box-shadow   180ms ease,
    border-color 180ms ease;
}

.bubble:hover {
  transform: translateX(5px) scale(1.08);
  background: rgba(240, 250, 240, 0.90);
  border-color: rgba(119, 153, 119, 0.50);
  box-shadow:
    0 6px 20px rgba(40, 70, 40, 0.16),
    0 0 0 2.5px rgba(119, 153, 119, 0.22),
    inset 0 1px 0 rgba(255, 255, 255, 0.80);
}

.bubble:active {
  transform: translateX(3px) scale(0.93);
  transition-duration: 80ms;
}

/* Hide bubble — cool neutral, shifts warm-red on hover */
.bubble--hide {
  background: rgba(235, 235, 240, 0.68);
  border-color: rgba(150, 150, 168, 0.28);
}

.bubble--hide:hover {
  background: rgba(255, 242, 242, 0.90);
  border-color: rgba(200, 110, 110, 0.42);
  box-shadow:
    0 6px 20px rgba(160, 50, 50, 0.12),
    0 0 0 2.5px rgba(200, 100, 100, 0.16),
    inset 0 1px 0 rgba(255, 255, 255, 0.80);
}

/* ── Icon ────────────────────────────────────────────────────────── */
.bubble-icon {
  font-size: calc(var(--bubble-size) * 0.44);  /* ~16 px at large */
  line-height: 1;
  user-select: none;
  pointer-events: none;
}

/* ── Tooltip (slides in to the right) ───────────────────────────── */
.bubble-tip {
  position: fixed;
  transform: translateY(-50%);
  white-space: nowrap;
  pointer-events: none;
  z-index: 1000;

  background: rgba(236, 246, 236, 0.92);
  backdrop-filter: blur(20px) saturate(150%);
  -webkit-backdrop-filter: blur(20px) saturate(150%);
  border: 1px solid rgba(119, 153, 119, 0.30);
  border-radius: 7px;
  padding: 2px 7px;

  font-family: system-ui, "Segoe UI", "Noto Sans SC", "Noto Sans JP", sans-serif;
  font-size: calc(var(--bubble-size) * 0.40);  /* ~8.6 px at large */
  font-weight: 600;
  color: rgba(30, 52, 30, 0.85);
  letter-spacing: 0.01em;
  box-shadow: 0 2px 8px rgba(40, 70, 40, 0.10);
}

.tip-enter-active { transition: opacity 150ms ease, transform 150ms cubic-bezier(0.22, 1, 0.36, 1); }
.tip-leave-active { transition: opacity 100ms ease; }
.tip-enter-from   { opacity: 0; transform: translateY(-50%) translateX(-6px); }
.tip-leave-to     { opacity: 0; }
</style>
