<script setup lang="ts">
/**
 * App router — chooses between PetWindow and SettingsWindow based on
 * the URL query param `window`. Tauri creates one webview per window
 * label, all pointing at the same index.html; the URL param tells us
 * which to render.
 *
 * On mount, syncs the detected frontend locale to the Rust tray so its
 * menu items are localised to match navigator.language.
 */
import { computed, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import PetWindow from './components/PetWindow.vue'
import SettingsWindow from './components/SettingsWindow.vue'
import TarotDraw from './components/TarotDraw.vue'
import { detectLocale } from './i18n'

const windowKind = computed(() => {
  const p = new URLSearchParams(window.location.search)
  return p.get('window')   // 'settings' | 'tarot' | null (pet)
})

onMounted(async () => {
  // Sync tray menu labels with the frontend-detected system locale.
  // This runs on every window instance, which is harmless — the tray
  // is shared and the last write wins (all windows detect the same locale).
  try {
    await invoke('set_tray_locale', { locale: detectLocale() })
  } catch { /* tray update is best-effort */ }
})
</script>

<template>
  <SettingsWindow v-if="windowKind === 'settings'" />
  <TarotDraw v-else-if="windowKind === 'tarot'" />
  <PetWindow v-else />
</template>
