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
import { computed, onMounted, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import PetWindow from './components/PetWindow.vue'
import SettingsWindow from './components/SettingsWindow.vue'
import { detectLocale, setLocale, useI18n } from './i18n'
import { useAppConfig } from './composables/useAppConfig'

const isSettings = computed(() => {
  const p = new URLSearchParams(window.location.search)
  return p.get('window') === 'settings'
})

const { locale } = useI18n()
const { config } = useAppConfig()

// Apply the active UI language. The manual choice (config.language) wins;
// `null` falls back to navigator detection. `immediate` applies it on mount,
// and the watch keeps every window in sync when the setting changes in another
// window (useAppConfig broadcasts config changes across windows).
watch(
  () => config.value.language,
  lang => setLocale(lang ?? detectLocale()),
  { immediate: true },
)

onMounted(async () => {
  // Sync the tray menu labels with the effective locale (manual or detected).
  // Runs per window; the tray is shared so the last write wins.
  try {
    await invoke('set_tray_locale', { locale: locale.value })
  } catch { /* tray update is best-effort */ }
})
</script>

<template>
  <SettingsWindow v-if="isSettings" />
  <PetWindow v-else />
</template>
