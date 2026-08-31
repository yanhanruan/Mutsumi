import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import { setLocale } from '../i18n'
import SystemStateOverlay, { type SystemState } from './SystemStateOverlay.vue'

let systemStateListener: ((event: { payload: SystemState }) => void) | undefined

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (event: string, listener: (event: { payload: SystemState }) => void) => {
    if (event === 'system-state') systemStateListener = listener
    return vi.fn()
  }),
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => null),
}))

describe('SystemStateOverlay network degradation', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    setLocale('en')
    systemStateListener = undefined
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('shows native detection failure as unavailable instead of online', async () => {
    const wrapper = mount(SystemStateOverlay)
    await flushPromises()

    ;(wrapper.vm as unknown as { open: () => void }).open()
    systemStateListener?.({
      payload: {
        cpu_usage: 12,
        mem_usage: 34,
        network: 'unavailable',
        uptime: 56,
        battery: null,
      },
    })
    vi.advanceTimersByTime(1200)
    await nextTick()

    const value = wrapper.find('.value.unavailable')
    expect(value.exists()).toBe(true)
    expect(value.text()).toBe('Unavailable')
    expect(wrapper.find('.value.online').exists()).toBe(false)
    wrapper.unmount()
  })
})
