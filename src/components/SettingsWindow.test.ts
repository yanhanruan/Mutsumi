/**
 * Unit tests for SettingsWindow — autostart toggle behaviour.
 *
 * All Tauri plugin calls are mocked so the tests run in Node/happy-dom
 * without a real desktop back-end.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'

// ── Mock Tauri core invoke ─────────────────────────────────────────
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'get_state') {
      return {
        pet:      { energy: 80, affection: 60, mood: 'content' },
        pomodoro: { phase: 'idle', focus_mins: 25, break_mins: 5, remaining_secs: 0, running: false },
      }
    }
    return null
  }),
}))

// ── Mock Tauri window API ──────────────────────────────────────────
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: vi.fn(() => ({ hide: vi.fn() })),
}))

// ── Mock autostart plugin ─────────────────────────────────────────
let _enabled = false

vi.mock('@tauri-apps/plugin-autostart', () => ({
  enable:    vi.fn(async () => { _enabled = true  }),
  disable:   vi.fn(async () => { _enabled = false }),
  isEnabled: vi.fn(async () => _enabled),
}))

import SettingsWindow from './SettingsWindow.vue'
import { enable, disable, isEnabled } from '@tauri-apps/plugin-autostart'

beforeEach(() => {
  _enabled = false
  vi.mocked(isEnabled).mockImplementation(async () => _enabled)
  vi.mocked(enable).mockImplementation(async () => { _enabled = true })
  vi.mocked(disable).mockImplementation(async () => { _enabled = false })
  vi.clearAllMocks()
  // Re-apply after clear
  vi.mocked(isEnabled).mockImplementation(async () => _enabled)
  vi.mocked(enable).mockImplementation(async () => { _enabled = true })
  vi.mocked(disable).mockImplementation(async () => { _enabled = false })
})

describe('SettingsWindow — autostart toggle', () => {
  it('renders the launch-on-startup checkbox', async () => {
    const wrapper = mount(SettingsWindow)
    await flushPromises()
    expect(wrapper.find('#autostart-toggle').exists()).toBe(true)
  })

  it('checkbox is unchecked when isEnabled() returns false', async () => {
    _enabled = false
    const wrapper = mount(SettingsWindow)
    await flushPromises()
    const cb = wrapper.find<HTMLInputElement>('#autostart-toggle')
    expect(cb.element.checked).toBe(false)
  })

  it('checkbox is checked when isEnabled() returns true', async () => {
    _enabled = true
    vi.mocked(isEnabled).mockResolvedValue(true)
    const wrapper = mount(SettingsWindow)
    await flushPromises()
    const cb = wrapper.find<HTMLInputElement>('#autostart-toggle')
    expect(cb.element.checked).toBe(true)
  })

  it('calls enable() when checkbox is turned on', async () => {
    _enabled = false
    const wrapper = mount(SettingsWindow)
    await flushPromises()

    // setValue on a checkbox sets the value AND triggers change event once.
    await wrapper.find('#autostart-toggle').setValue(true)
    await flushPromises()

    expect(enable).toHaveBeenCalledOnce()
    expect(disable).not.toHaveBeenCalled()
  })

  it('calls disable() when checkbox is turned off', async () => {
    _enabled = true
    vi.mocked(isEnabled).mockResolvedValue(true)
    const wrapper = mount(SettingsWindow)
    await flushPromises()

    await wrapper.find('#autostart-toggle').setValue(false)
    await flushPromises()

    expect(disable).toHaveBeenCalledOnce()
    expect(enable).not.toHaveBeenCalled()
  })
})
