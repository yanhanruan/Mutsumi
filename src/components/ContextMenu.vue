<script setup lang="ts">
/**
 * ContextMenu — glassmorphic right-click menu for pet interactions.
 *
 * No emojis — plain text labels only, tightly packed.
 * Positioned at the cursor location within the pet window. Automatically
 * flips up/left if the menu would overflow the viewport. Dismissed on
 * outside-click or after an item is selected.
 */

import { ref, nextTick, computed } from 'vue'
import { watch } from 'vue'
import { useI18n } from '../i18n'

// ── Types ────────────────────────────────────────────────────────────

export type MenuAction = 'pat_head' | 'feed' | 'sleep' | 'fast_learning'

const ACTIONS: MenuAction[] = ['pat_head', 'feed', 'sleep', 'fast_learning']

const { t } = useI18n()

const items = computed(() =>
  ACTIONS.map(action => ({
    action,
    label: t.value.contextMenuItems[action],
  }))
)

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

function onDocumentClick(e: MouseEvent) {
  if (!menuRef.value?.contains(e.target as Node)) close()
}

function onKeyDown(e: KeyboardEvent) {
  if (e.key === 'Escape') close()
}

watch(visible, v => {
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
        v-for="item in items"
        :key="item.action"
        class="ctx-item"
        @click="onItemClick(item.action)"
      >
        {{ item.label }}
      </button>
    </div>
  </Transition>
</template>

<style scoped>
.ctx-menu {
  position: fixed;
  z-index: 999;
  /* width is driven by content; constrain to viewport */
  width: max-content;
  max-width: 170px;
  padding: 3px;
  border-radius: 10px;

  background: rgba(246, 244, 255, 0.97);
  backdrop-filter: blur(32px) saturate(210%);
  -webkit-backdrop-filter: blur(32px) saturate(210%);

  border: 1px solid rgba(190, 165, 235, 0.38);
  /* single crisp inner highlight — no heavy outer shadow */
  box-shadow:
    0 6px 24px rgba(50, 20, 90, 0.16),
    0 2px  6px rgba(0,   0,  0, 0.08),
    inset 0 1px 0 rgba(255, 255, 255, 0.85);

  display: flex;
  flex-direction: column;
  gap: 1px;
}

.ctx-item {
  display: block;
  width: 100%;
  padding: 6px 12px;
  border: none;
  background: transparent;
  border-radius: 7px;
  cursor: pointer;
  text-align: left;
  box-sizing: border-box;

  font-family: system-ui, "Segoe UI", "Noto Sans SC", "Noto Sans JP", sans-serif;
  font-size: 11.5px;
  font-weight: 520;
  color: rgba(28, 14, 52, 0.88);
  line-height: 1.4;
  white-space: nowrap;

  transition: background 90ms ease;
}

.ctx-item:hover {
  background: rgba(140, 100, 210, 0.10);
}

.ctx-item:active {
  background: rgba(120, 80, 200, 0.18);
  transform: scale(0.98);
}

/* Pop-in from cursor */
.ctx-enter-active {
  transition: opacity 120ms ease, transform 120ms cubic-bezier(0.18, 1.4, 0.52, 1);
}
.ctx-leave-active {
  transition: opacity 70ms ease;
}
.ctx-enter-from { opacity: 0; transform: scale(0.86) translateY(-4px); }
.ctx-leave-to   { opacity: 0; }
</style>
