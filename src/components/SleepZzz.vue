<script setup lang="ts">
/**
 * SleepZzz — the looping "zzz" that drifts up while Mutsumi sleeps.
 *
 * Pure CSS, no JS animation loop. Three "z" glyphs share one keyframe
 * (`zfloat`: fade in → slow drift toward the upper-right → fade out) and are
 * staggered by exactly one third of the cycle via animation-delay. That gives:
 *   - initially nothing, then z's appearing one after another;
 *   - at most three alive at once;
 *   - a fresh z rising as the oldest finishes fading — endlessly.
 *
 * Drift distance is expressed in `em`, so it scales automatically with the
 * glyph size, which in turn tracks the character-size setting. Decorative, so
 * the element is aria-hidden and needs no i18n.
 */
import { computed } from 'vue'
import { useAppConfig } from '../composables/useAppConfig'

const { config } = useAppConfig()

// Glyph size per character-size tier (mirrors the bubble scaling ratios).
const SIZE_PX: Record<string, number> = { small: 15, medium: 18, large: 22 }

const rootStyle = computed(() => ({
  '--zzz-size': `${SIZE_PX[config.value.characterSize] ?? 22}px`,
}))
</script>

<template>
  <div class="zzz" :style="rootStyle" aria-hidden="true">
    <span class="z z1">z</span>
    <span class="z z2">z</span>
    <span class="z z3">z</span>
  </div>
</template>

<style scoped>
/* Anchored near her head; the z's rise toward the upper-right from here. */
.zzz {
  position: absolute;
  top: 30%;
  left: 56%;
  z-index: 3;
  pointer-events: none;
  font-size: var(--zzz-size, 22px);
}

.z {
  position: absolute;
  top: 0;
  left: 0;
  font-family: 'Segoe UI Rounded', 'Quicksand', 'Varela Round', system-ui, sans-serif;
  font-weight: 700;
  font-style: italic;
  line-height: 1;
  /* Soft, creamy glyph with a gentle sage glow so it reads on any desktop. */
  color: rgba(247, 251, 247, 0.96);
  text-shadow:
    0 1px 2px rgba(60, 92, 70, 0.38),
    0 0 7px rgba(150, 190, 162, 0.55);
  opacity: 0;
  will-change: transform, opacity;
  /* 6s lifetime, staggered by a third (2s) → exactly three in flight. */
  animation: zfloat 6s linear infinite;
}
.z2 { animation-delay: 2s; }
.z3 { animation-delay: 4s; }

/* Fade in → slow drift up-right → fade out. Scaling is deliberately tiny
   (0.9 → 1.05) so the motion stays soft and calm, never bouncy. */
@keyframes zfloat {
  0%   { opacity: 0;    transform: translate(0, 0)          scale(0.9);  }
  14%  { opacity: 0.96; }
  85%  { opacity: 0.96; }
  100% { opacity: 0;    transform: translate(2.3em, -3.6em) scale(1.05); }
}

/* Respect reduced-motion: hold a single faint, static z instead of drifting. */
@media (prefers-reduced-motion: reduce) {
  .z  { animation: none; }
  .z1 { opacity: 0.9; }
  .z2, .z3 { display: none; }
}
</style>
