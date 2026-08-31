import { afterEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'

const mocks = vi.hoisted(() => ({ macOS: false }))

vi.mock('../config/desktopPlatform', () => ({
  isMacOSDesktop: () => mocks.macOS,
}))

import WindowTitlebar from './WindowTitlebar.vue'

afterEach(() => {
  mocks.macOS = false
})

describe('WindowTitlebar', () => {
  it('keeps the existing desktop chrome outside macOS', () => {
    const wrapper = mount(WindowTitlebar, {
      props: {
        subtitle: 'settings',
        closeLabel: 'Close',
        minimizable: true,
        minimizeLabel: 'Minimize',
      },
    })

    expect(wrapper.find('.titlebar').classes()).not.toContain('platform-macos')
    expect(wrapper.findAll('.wbtn').map(button => button.classes()[1])).toEqual([
      'wbtn-min',
      'wbtn-close',
    ])
    expect(wrapper.find('.wbtn-close').attributes('aria-label')).toBe('Close')
  })

  it('marks macOS chrome and exposes localized minimize/close actions', async () => {
    mocks.macOS = true
    const wrapper = mount(WindowTitlebar, {
      props: {
        subtitle: '設定',
        closeLabel: '閉じる',
        minimizable: true,
        minimizeLabel: '最小化',
      },
    })

    expect(wrapper.find('.titlebar').classes()).toContain('platform-macos')
    expect(wrapper.findAll('.wbtn').map(button => button.classes()[1])).toEqual([
      'wbtn-close',
      'wbtn-min',
    ])
    expect(wrapper.find('.wbtn-min').attributes('title')).toBe('最小化')
    expect(wrapper.find('.wbtn-close').attributes('title')).toBe('閉じる')

    await wrapper.find('.wbtn-min').trigger('click')
    await wrapper.find('.wbtn-close').trigger('click')
    expect(wrapper.emitted('minimize')).toHaveLength(1)
    expect(wrapper.emitted('close')).toHaveLength(1)
  })

  it('renders only close when the fixed-size window is not minimizable', () => {
    const wrapper = mount(WindowTitlebar, {
      props: { subtitle: 'about', closeLabel: 'Close' },
    })

    expect(wrapper.findAll('.wbtn')).toHaveLength(1)
    expect(wrapper.find('.wbtn-close').exists()).toBe(true)
  })
})
