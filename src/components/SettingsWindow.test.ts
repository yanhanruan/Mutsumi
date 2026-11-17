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
    if (cmd === 'qwen_key_status') {
      return { configured: false, credentialStoreAvailable: true }
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
import { invoke } from '@tauri-apps/api/core'

beforeEach(() => {
  _enabled = false
  vi.mocked(isEnabled).mockImplementation(async () => _enabled)
  vi.mocked(enable).mockImplementation(async () => { _enabled = true })
  vi.mocked(disable).mockImplementation(async () => { _enabled = false })
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === 'get_state') {
      return {
        pet:      { energy: 80, affection: 60, mood: 'content' },
        pomodoro: { phase: 'idle', focus_mins: 25, break_mins: 5, remaining_secs: 0, running: false },
      }
    }
    if (cmd === 'qwen_key_status') {
      return { configured: false, credentialStoreAvailable: true }
    }
    return null
  })
  vi.clearAllMocks()
  // Re-apply after clear
  vi.mocked(isEnabled).mockImplementation(async () => _enabled)
  vi.mocked(enable).mockImplementation(async () => { _enabled = true })
  vi.mocked(disable).mockImplementation(async () => { _enabled = false })
})

describe('SettingsWindow — autostart toggle', () => {
  it('uses localized controls supported by the fixed-size window', async () => {
    const wrapper = mount(SettingsWindow)
    await flushPromises()

    expect(wrapper.find('.wbtn-min').attributes('aria-label')).toBe('Minimize')
    expect(wrapper.find('.wbtn-close').attributes('aria-label')).toBe('Close')
    expect(wrapper.find('.wbtn-max').exists()).toBe(false)
  })

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

  it('disables the control and explains when autostart status is unavailable', async () => {
    vi.mocked(isEnabled).mockRejectedValue(new Error('LaunchAgent unavailable'))
    const wrapper = mount(SettingsWindow)
    await flushPromises()

    expect(wrapper.find<HTMLInputElement>('#autostart-toggle').element.disabled).toBe(true)
    expect(wrapper.text()).toContain('Launch on startup is unavailable on this system.')
  })

  it('rolls back the toggle and reports an enable failure', async () => {
    vi.mocked(enable).mockRejectedValue(new Error('permission denied'))
    const wrapper = mount(SettingsWindow)
    await flushPromises()

    const checkbox = wrapper.find<HTMLInputElement>('#autostart-toggle')
    await checkbox.setValue(true)
    await flushPromises()

    expect(checkbox.element.checked).toBe(false)
    expect(wrapper.text()).toContain('Could not change the launch-on-startup setting.')
  })

  it('shows a persistent warning when startup Keychain reading failed', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'get_state') {
        return {
          pet:      { energy: 80, affection: 60, mood: 'content' },
          pomodoro: { phase: 'idle', focus_mins: 25, break_mins: 5, remaining_secs: 0, running: false },
        }
      }
      if (cmd === 'qwen_key_status') {
        return { configured: false, credentialStoreAvailable: false }
      }
      return null
    })

    const wrapper = mount(SettingsWindow)
    await flushPromises()

    expect(wrapper.text()).toContain(
      'The system credential store is unavailable. A saved API key may not have been loaded.',
    )
  })
})
