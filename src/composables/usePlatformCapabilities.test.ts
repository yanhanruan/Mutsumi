import { beforeEach, describe, expect, it, vi } from 'vitest'

import type { PlatformCapabilities } from './usePlatformCapabilities'

const macCapabilities: PlatformCapabilities = {
  audioActivity:   'unavailable',
  mediaMetadata:   'unavailable',
  mediaTransport:  'unavailable',
  systemVolume:    'unavailable',
  idleDetection:   'available',
  globalCursor:    'available',
  atomicWindowGeometry: 'available',
  deepWebSearch:   'degraded',
  hardwareDetails: 'available',
  revealInFolder:  'available',
}

async function freshComposable(result: () => Promise<PlatformCapabilities>) {
  vi.resetModules()
  const invoke = vi.fn(result)
  vi.doMock('@tauri-apps/api/core', () => ({ invoke }))

  const { usePlatformCapabilities } = await import('./usePlatformCapabilities')
  const first = usePlatformCapabilities()
  const second = usePlatformCapabilities()
  await Promise.resolve()
  await Promise.resolve()
  await Promise.resolve()
  return { first, second, invoke }
}

beforeEach(() => {
  vi.resetModules()
})

describe('usePlatformCapabilities', () => {
  it('hides music when every audio and media capability is explicitly unavailable', async () => {
    const { first } = await freshComposable(() => Promise.resolve(macCapabilities))
    expect(first.resolved.value).toBe(true)
    expect(first.musicAvailable.value).toBe(false)
  })

  it('keeps the audio animation visible for a degraded activity signal', async () => {
    const capabilities = { ...macCapabilities, audioActivity: 'degraded' as const }
    const { first } = await freshComposable(() => Promise.resolve(capabilities))
    expect(first.musicAvailable.value).toBe(true)
    expect(first.audioActivityDegraded.value).toBe(true)
  })

  it('hides the controller panel when only audio activity is available', async () => {
    const capabilities = { ...macCapabilities, audioActivity: 'degraded' as const }
    const { first } = await freshComposable(() => Promise.resolve(capabilities))
    expect(first.musicAvailable.value).toBe(true)
    expect(first.musicPanelAvailable.value).toBe(false)
  })

  it('keeps the controller panel when media metadata is supported', async () => {
    const capabilities = { ...macCapabilities, mediaMetadata: 'available' as const }
    const { first } = await freshComposable(() => Promise.resolve(capabilities))
    expect(first.musicPanelAvailable.value).toBe(true)
  })

  it('fails open when the capability command is unavailable', async () => {
    const { first } = await freshComposable(() => Promise.reject(new Error('IPC unavailable')))
    expect(first.resolved.value).toBe(true)
    expect(first.capabilities.value).toBeNull()
    expect(first.musicAvailable.value).toBe(true)
    expect(first.musicPanelAvailable.value).toBe(true)
  })

  it('disables automatic idle flight when idle detection becomes unavailable', async () => {
    const capabilities = { ...macCapabilities, idleDetection: 'unavailable' as const }
    const { first } = await freshComposable(() => Promise.resolve(capabilities))
    expect(first.idleScreensaverAvailable.value).toBe(false)
  })

  it('enables automatic idle flight when the complete macOS adapter is available', async () => {
    const { first } = await freshComposable(() => Promise.resolve(macCapabilities))
    expect(first.idleScreensaverAvailable.value).toBe(true)
  })

  it('shares one backend request across composable consumers', async () => {
    const { first, second, invoke } = await freshComposable(() => Promise.resolve(macCapabilities))
    expect(invoke).toHaveBeenCalledTimes(1)
    expect(first.capabilities).toBe(second.capabilities)
  })
})
