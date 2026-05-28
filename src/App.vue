<script setup lang="ts">
/**
 * App router — chooses between PetWindow and SettingsWindow based on
 * the URL query param `window`. Tauri creates one webview per window
 * label, all pointing at the same index.html; the URL param tells us
 * which to render.
 */
import { computed } from 'vue'
import PetWindow from './components/PetWindow.vue'
import SettingsWindow from './components/SettingsWindow.vue'

const isSettings = computed(() => {
  const p = new URLSearchParams(window.location.search)
  return p.get('window') === 'settings'
})
</script>

<template>
  <SettingsWindow v-if="isSettings" />
  <PetWindow v-else />
</template>
