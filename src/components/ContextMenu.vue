<script setup lang="ts">
/**
 * ContextMenu — glassmorphic right-click menu for pet interactions.
 *
 * Positioned at the cursor location within the pet window. Automatically
 * flips up/left if the menu would overflow the viewport. Dismissed on
 * outside-click or after an item is selected.
 *
 * Usage:
 *   <ContextMenu ref="menuRef" @action="onAction" />
 *   menuRef.value?.open(x, y)    // call from right-click handler
 */

import { ref, nextTick } from 'vue'

// ── Types ────────────────────────────────────────────────────────────

export type MenuAction = 'pat_head' | 'feed' | 'sleep' | 'fast_learning'

export interface MenuItem {
  action:  MenuAction
  emoji:   string
  label:   string
  subtext: string
}

const ITEMS: MenuItem[] = [
  { action: 'pat_head',      emoji: '🤗', label: '摸头',      subtext: 'Pat Head'           },
  { action: 'feed',          emoji: '🍵', label: '投喂抹茶芭菲', subtext: 'Feed Matcha Parfait' },
  { action: 'sleep',         emoji: '😴', label: '睡觉',      subtext: 'Sleep'              },
  { action: 'fast_learning', emoji: '📚', label: '快速学习',   subtext: 'Fast Learning'      },
]

// ── State ────────────────────────────────────────────────────────────

const emit = defineEmits<{
  action: [action: MenuAction]
}>()

const visible = ref(false)
const menuX   = ref(0)
const menuY   = ref(0)
const menuRef = ref<HTMLElement | null>(null)

// ── Public API ───────────────────────────────────────────────────────

async function open(x: number, y: number) {
  visible.value = true
  menuX.value   = x
  menuY.value   = y

  // After render, nudge menu back inside viewport if it overflows.
  await nextTick()
  const el = menuRef.value
  if (!el) return
  const { width, height } = el.getBoundingClientRect()
  const vw = window.innerWidth
  const vh = window.innerHeight
  if (x + width  > vw) menuX.value = x - width
  if (y + height > vh) menuY.value = y - height
}

function close() {
  visible.value = false
}

function onItemClick(action: MenuAction) {
  emit('action', action)
  close()
}

// Dismiss on outside click (document listener added/removed with visibility).
function onDocumentClick(e: MouseEvent) {
  if (!menuRef.value?.contains(e.target as Node)) {
    close()
  }
}

function onKeyDown(e: KeyboardEvent) {
  if (e.key === 'Escape') close()
}

import { watch } from 'vue'
watch(visible, (v) => {
  if (v) {
    document.addEventListener('mousedown', onDocumentClick, true)
    document.addEventListener('keydown',   onKeyDown,       true)
  } else {
    document.removeEventListener('mousedown', onDocumentClick, true)
    document.removeEventListener('keydown',   onKeyDown,       true)
  }
})

defineExpose({ open, close })
</script>

<template>
  <Transition name="ctx">
    <div
      v-if="visible"
      ref="menuRef"
      class="ctx-menu"
      :style="{ left: `${menuX}px`, top: `${menuY}px` }"
      @contextmenu.prevent
    >
      <button
        v-for="item in ITEMS"
        :key="item.action"
        class="ctx-item"
        @click="onItemClick(item.action)"
      >
        <span class="ctx-emoji">{{ item.emoji }}</span>
        <span class="ctx-text">
          <span class="ctx-label">{{ item.label }}</span>
          <span class="ctx-sub">{{ item.subtext }}</span>
        </span>
      </button>
    </div>
  </Transition>
</template>

<style scoped>
.ctx-menu {
  position: fixed;
  z-index: 999;
  min-width: 160px;
  padding: 6px;
  border-radius: 14px;
  background: rgba(255, 255, 255, 0.22);
  backdrop-filter: blur(20px) saturate(180%);
  -webkit-backdrop-filter: blur(20px) saturate(180%);
  border: 1px solid rgba(255, 255, 255, 0.45);
  box-shadow:
    0 8px 32px rgba(100, 80, 160, 0.22),
    0 2px 8px  rgba(0,   0,   0,  0.12);
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.ctx-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  border: none;
  background: transparent;
  border-radius: 9px;
  cursor: pointer;
  transition: background 120ms ease, transform 80ms ease;
  width: 100%;
  text-align: left;
}

.ctx-item:hover {
  background: rgba(180, 140, 220, 0.22);
  transform: translateX(2px);
}

.ctx-item:active {
  background: rgba(160, 110, 210, 0.32);
  transform: translateX(2px) scale(0.98);
}

.ctx-emoji {
  font-size: 18px;
  line-height: 1;
  flex-shrink: 0;
}

.ctx-text {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.ctx-label {
  font-family: system-ui, "Segoe UI", "Noto Sans SC", sans-serif;
  font-size: 13px;
  font-weight: 600;
  color: rgba(50, 30, 80, 0.92);
  line-height: 1.2;
  white-space: nowrap;
}

.ctx-sub {
  font-family: system-ui, "Segoe UI", sans-serif;
  font-size: 10px;
  color: rgba(80, 60, 100, 0.60);
  line-height: 1.2;
  white-space: nowrap;
}

/* Entrance animation */
.ctx-enter-active {
  transition: opacity 140ms ease, transform 140ms cubic-bezier(0.34, 1.56, 0.64, 1);
}
.ctx-leave-active {
  transition: opacity 100ms ease, transform 100ms ease;
}
.ctx-enter-from {
  opacity: 0;
  transform: scale(0.88) translateY(-4px);
}
.ctx-leave-to {
  opacity: 0;
  transform: scale(0.94) translateY(-2px);
}
</style>
