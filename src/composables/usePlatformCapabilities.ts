/**
 * Runtime platform-capability state shared by every component in one webview.
 *
 * The Rust backend reports whether native integrations are available, degraded,
 * permission-gated, or unavailable. Consumers should hide controls only for an
 * explicit `unavailable`; a failed capability request fails open so a transient
 * IPC problem never removes established Windows features.
 */

import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export type CapabilityStatus =
  | 'available'
  | 'permissionRequired'
  | 'unavailable'
  | 'degraded'

export interface PlatformCapabilities {
  audioActivity:   CapabilityStatus
  mediaMetadata:   CapabilityStatus
  mediaTransport:  CapabilityStatus
  systemVolume:    CapabilityStatus
  idleDetection:   CapabilityStatus
  globalCursor:    CapabilityStatus
  atomicWindowGeometry: CapabilityStatus
  deepWebSearch:   CapabilityStatus
  hardwareDetails: CapabilityStatus
  revealInFolder:  CapabilityStatus
}

const capabilities = ref<PlatformCapabilities | null>(null)
const resolved = ref(false)
let request: Promise<void> | null = null

function loadOnce(): Promise<void> {
  if (request) return request
  request = invoke<PlatformCapabilities>('get_platform_capabilities')
    .then(value => { capabilities.value = value })
    .catch(() => { capabilities.value = null })
    .finally(() => { resolved.value = true })
  return request
}

export function usePlatformCapabilities() {
  void loadOnce()

  const musicAvailable = computed(() => {
    if (!resolved.value || !capabilities.value) return true
    return capabilities.value.audioActivity !== 'unavailable'
      || capabilities.value.mediaMetadata !== 'unavailable'
  })

  const idleScreensaverAvailable = computed(() => {
    if (!resolved.value || !capabilities.value) return true
    return capabilities.value.idleDetection === 'available'
  })

  return { capabilities, resolved, musicAvailable, idleScreensaverAvailable }
}
