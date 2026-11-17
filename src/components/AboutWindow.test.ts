import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'

import type { PlatformCapabilities } from '../composables/usePlatformCapabilities'
import { en } from '../i18n/locales/en'

const mocks = vi.hoisted(() => ({
  capabilities: null as { value: PlatformCapabilities | null } | null,
}))

vi.mock('../composables/usePlatformCapabilities', async () => {
  const { ref } = await vi.importActual<typeof import('vue')>('vue')
  mocks.capabilities = ref<PlatformCapabilities | null>(null)
  return {
    usePlatformCapabilities: () => ({ capabilities: mocks.capabilities }),
  }
})

vi.mock('../composables/useAppConfig', () => ({
  useAppConfig: () => ({
    config: {
      value: {
        language: 'en',
        updateLastCheck: null,
        updateLastCheckStatus: null,
      },
    },
  }),
}))

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ close: vi.fn() }),
}))

vi.mock('@tauri-apps/api/app', () => ({
  getVersion: vi.fn(async () => '1.5.3'),
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => null),
}))

import AboutWindow from './AboutWindow.vue'

const available: PlatformCapabilities = {
  audioActivity: 'available',
  mediaMetadata: 'available',
  mediaTransport: 'available',
  systemVolume: 'available',
  idleDetection: 'available',
  globalCursor: 'available',
  atomicWindowGeometry: 'available',
  deepWebSearch: 'available',
  hardwareDetails: 'available',
  revealInFolder: 'available',
}

beforeEach(() => {
  mocks.capabilities!.value = null
})

describe('AboutWindow capability-aware feature list', () => {
  it('renders the Windows media-controller claim when the backend supports it', async () => {
    mocks.capabilities!.value = available
    const wrapper = mount(AboutWindow)
    await flushPromises()

    expect(wrapper.text()).toContain(en.aboutAudioActivityFeature)
    expect(wrapper.text()).toContain(en.aboutMediaControllerFeature)
  })

  it('renders degraded audio but not the unavailable media controller on macOS', async () => {
    mocks.capabilities!.value = {
      ...available,
      audioActivity: 'degraded',
      mediaMetadata: 'unavailable',
      mediaTransport: 'unavailable',
      systemVolume: 'unavailable',
    }
    const wrapper = mount(AboutWindow)
    await flushPromises()

    expect(wrapper.text()).toContain(en.aboutAudioActivityDegradedFeature)
    expect(wrapper.text()).not.toContain(en.aboutAudioActivityFeature)
    expect(wrapper.text()).not.toContain(en.aboutMediaControllerFeature)
  })

  it('does not advertise unresolved platform integrations', async () => {
    const wrapper = mount(AboutWindow)
    await flushPromises()

    expect(wrapper.text()).not.toContain(en.aboutAudioActivityFeature)
    expect(wrapper.text()).not.toContain(en.aboutAudioActivityDegradedFeature)
    expect(wrapper.text()).not.toContain(en.aboutMediaControllerFeature)
  })

  it('updates after the asynchronous capability request resolves', async () => {
    const wrapper = mount(AboutWindow)
    expect(wrapper.text()).not.toContain(en.aboutAudioActivityDegradedFeature)

    mocks.capabilities!.value = {
      ...available,
      audioActivity: 'degraded',
      mediaMetadata: 'unavailable',
      mediaTransport: 'unavailable',
      systemVolume: 'unavailable',
    }
    await flushPromises()

    expect(wrapper.text()).toContain(en.aboutAudioActivityDegradedFeature)
    expect(wrapper.text()).not.toContain(en.aboutMediaControllerFeature)
  })
})
