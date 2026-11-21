<script setup lang="ts">
import type { HexagramLinePattern } from '../config/iching'

const props = defineProps<{
  pattern: HexagramLinePattern
  movingLineIndexes?: readonly number[]
  movingLabel: string
}>()

function isMoving(index: number): boolean {
  return props.movingLineIndexes?.includes(index) ?? false
}
</script>

<template>
  <div class="hexagram-lines" aria-hidden="false">
    <div
      v-for="index in [5, 4, 3, 2, 1, 0]"
      :key="index"
      class="hex-line"
      :class="{ 'is-yang': pattern[index], 'is-yin': !pattern[index], 'is-moving': isMoving(index) }"
    >
      <span class="line-shape" aria-hidden="true">
        <span class="segment segment-a" />
        <span v-if="!pattern[index]" class="segment segment-b" />
      </span>
      <span v-if="isMoving(index)" class="moving-dot" :aria-label="movingLabel">○</span>
    </div>
  </div>
</template>

<style scoped>
.hexagram-lines {
  width: 104px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  align-items: stretch;
}

.hex-line {
  position: relative;
  height: 10px;
}

.line-shape {
  display: flex;
  align-items: center;
  height: 10px;
}

.segment {
  height: 6px;
  border-radius: 999px;
  background: #23372c;
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.28);
}

.is-yang .segment {
  flex: 1 1 auto;
}

.is-yin .line-shape {
  gap: 12px;
}

.is-yin .segment {
  flex: 1 1 0;
}

.moving-dot {
  position: absolute;
  left: calc(100% + 6px);
  top: 50%;
  transform: translateY(-50%);
  font-size: 11px;
  line-height: 1;
  color: #7f6a3d;
  font-weight: 700;
  text-align: center;
}

.is-moving .segment {
  background: #5c4a28;
}

@media (prefers-reduced-motion: no-preference) {
  .hex-line {
    animation: line-in 260ms ease both;
  }

  @keyframes line-in {
    from { opacity: 0; transform: translateY(4px); }
    to { opacity: 1; transform: translateY(0); }
  }
}
</style>
