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
  background: rgba(255, 255, 255, 0.22);
  backdrop-filter: blur(14px) saturate(180%);
  -webkit-backdrop-filter: blur(14px) saturate(180%);
  border: 1px solid rgba(255, 255, 255, 0.35);
  border-radius: 14px;
  box-shadow: 0 4px 18px rgba(0, 0, 0, 0.15);
  color: #2a1a3a;
  font-size: 13px;
  line-height: 1.35;
  text-align: center;
  user-select: none;
  pointer-events: none;
}
.text {
  font-family: system-ui, "Segoe UI", sans-serif;
}
.tail {
  position: absolute;
  bottom: -6px;
  left: 50%;
  transform: translateX(-50%) rotate(45deg);
  width: 10px;
  height: 10px;
  background: rgba(255, 255, 255, 0.22);
  border-right: 1px solid rgba(255, 255, 255, 0.35);
  border-bottom: 1px solid rgba(255, 255, 255, 0.35);
  backdrop-filter: blur(14px) saturate(180%);
  -webkit-backdrop-filter: blur(14px) saturate(180%);
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
