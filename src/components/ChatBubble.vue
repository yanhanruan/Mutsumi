<script setup lang="ts">
/**
 * ChatBubble — pet's speech bubble.
 *
 * Glassmorphism-styled (frosted glass) bubble that floats above the pet.
 * Auto-hides after a fixed timeout. Parent calls `show(message)` to display.
 */
import { ref, onUnmounted } from 'vue'

const AUTO_HIDE_MS = 3000

const visible = ref(false)
const message = ref('')
let hideTimer: number | null = null

function clearTimer() {
  if (hideTimer !== null) {
    window.clearTimeout(hideTimer)
    hideTimer = null
  }
}

function show(text: string) {
  clearTimer()
  message.value = text
  visible.value = true
  hideTimer = window.setTimeout(() => {
    visible.value = false
    hideTimer = null
  }, AUTO_HIDE_MS)
}

function hide() {
  clearTimer()
  visible.value = false
}

onUnmounted(clearTimer)

defineExpose({ show, hide })
</script>

<template>
  <Transition name="bubble">
    <div v-if="visible" class="bubble pet-ui-overlay">
      <div class="text">{{ message }}</div>
      <div class="tail" />
    </div>
  </Transition>
</template>

<style scoped>
.bubble {
  position: relative;
  max-width: 200px;
  margin: 0 auto;
  padding: 10px 14px;
  /* Opaque enough to guarantee text contrast against any desktop background */
  background: rgba(245, 250, 245, 0.94);
  backdrop-filter: blur(20px) saturate(160%);
  -webkit-backdrop-filter: blur(20px) saturate(160%);
  border: 1px solid rgba(148, 185, 148, 0.45);
  border-radius: 14px;
  /* Subtle shadow — just enough to lift off the pet, not dominate */
  box-shadow:
    0 2px 8px rgba(40, 70, 40, 0.12),
    inset 0 1px 0 rgba(255, 255, 255, 0.80);
  color: #1a2e1a;
  font-size: 13px;
  line-height: 1.35;
  text-align: center;
  user-select: none;
  pointer-events: none;
  z-index: 2;
}
.text {
  font-family: system-ui, "Segoe UI", "Noto Sans SC", "Noto Sans JP", sans-serif;
  font-weight: 500;
}
.tail {
  position: absolute;
  bottom: -6px;
  left: 50%;
  transform: translateX(-50%) rotate(45deg);
  width: 10px;
  height: 10px;
  background: rgba(245, 250, 245, 0.94);
  border-right: 1px solid rgba(148, 185, 148, 0.45);
  border-bottom: 1px solid rgba(148, 185, 148, 0.45);
}

.bubble-enter-active,
.bubble-leave-active {
  transition: opacity 180ms ease, transform 180ms ease;
}
.bubble-enter-from,
.bubble-leave-to {
  opacity: 0;
  transform: translateY(4px);
}
</style>
