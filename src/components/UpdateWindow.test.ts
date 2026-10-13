import { flushPromises, mount } from '@vue/test-utils'
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'

import UpdateWindow from './UpdateWindow.vue'

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  getVersion: vi.fn(),
  setSize: vi.fn(),
  center: vi.fn(),
  close: vi.fn(),
  listen: vi.fn(),
  unlisten: vi.fn(),
  check: vi.fn(),
  relaunch: vi.fn(),
  updateConfig: vi.fn(),
  config: { value: { language: 'en' } },
}))

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }))
vi.mock('@tauri-apps/api/app', () => ({ getVersion: mocks.getVersion }))
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    setSize: mocks.setSize,
    center: mocks.center,
    close: mocks.close,
  }),
}))
vi.mock('@tauri-apps/api/dpi', () => ({
  LogicalSize: class LogicalSize {
    width: number
    height: number

    constructor(width: number, height: number) {
      this.width = width
      this.height = height
    }
  },
}))
vi.mock('@tauri-apps/api/event', () => ({ listen: mocks.listen }))
vi.mock('@tauri-apps/plugin-updater', () => ({ check: mocks.check }))
vi.mock('@tauri-apps/plugin-process', () => ({ relaunch: mocks.relaunch }))
vi.mock('../composables/useAppConfig', () => ({
  useAppConfig: () => ({
    config: mocks.config,
    updateConfig: mocks.updateConfig,
  }),
}))

const pendingUpdate = {
  version: '9.9.9-test',
  notes: 'Finder lifecycle and updater readiness\nSecurity fixes',
}

const originalLocalStorage = Object.getOwnPropertyDescriptor(globalThis, 'localStorage')
const storage = new Map<string, string>()
const localStorageStub = {
  get length() { return storage.size },
  clear: () => storage.clear(),
  getItem: (key: string) => storage.get(key) ?? null,
  key: (index: number) => [...storage.keys()][index] ?? null,
  removeItem: (key: string) => storage.delete(key),
  setItem: (key: string, value: string) => storage.set(key, String(value)),
}

beforeAll(() => {
  Object.defineProperty(globalThis, 'localStorage', {
    configurable: true,
    value: localStorageStub,
  })
})

beforeEach(() => {
  localStorage.clear()
  mocks.config.value.language = 'en'
  mocks.invoke.mockReset().mockImplementation((command: string) => {
    if (command === 'get_pending_update') return Promise.resolve(pendingUpdate)
    return Promise.reject(new Error(`unexpected invoke: ${command}`))
  })
  mocks.getVersion.mockReset().mockResolvedValue('1.5.3')
  mocks.setSize.mockReset().mockResolvedValue(undefined)
  mocks.center.mockReset().mockResolvedValue(undefined)
  mocks.close.mockReset()
  mocks.listen.mockReset().mockResolvedValue(mocks.unlisten)
  mocks.unlisten.mockReset()
  mocks.check.mockReset()
  mocks.relaunch.mockReset()
  mocks.updateConfig.mockReset().mockResolvedValue(undefined)
})

afterEach(() => {
  localStorage.clear()
})

afterAll(() => {
  if (originalLocalStorage) {
    Object.defineProperty(globalThis, 'localStorage', originalLocalStorage)
  } else {
    Reflect.deleteProperty(globalThis, 'localStorage')
  }
})

describe('available update payload', () => {
  it.each([
    ['zh', '发现新版本', '当前', '最新', '立即更新'],
    ['ja', '新しいバージョンがあります', '現在', '最新', '今すぐ更新'],
    ['en', 'A new version is available', 'Current', 'New', 'Update now'],
  ])('renders the prefetched offer in %s without a second network check', async (
    language,
    title,
    currentLabel,
    newLabel,
    updateButton,
  ) => {
    mocks.config.value.language = language
    const wrapper = mount(UpdateWindow, {
      global: {
        stubs: {
          WindowTitlebar: { template: '<div data-test="titlebar" />' },
        },
      },
    })

    await flushPromises()

    expect(wrapper.find('.hero h1').text()).toBe(title)
    expect(wrapper.find('.v-old').text()).toBe(`${currentLabel} v1.5.3`)
    expect(wrapper.find('.v-new').text()).toBe(`${newLabel} v${pendingUpdate.version}`)
    expect(wrapper.find('.notes').text()).toBe(pendingUpdate.notes)
    expect(wrapper.find('.btn-primary').text()).toBe(updateButton)
    expect(mocks.invoke).toHaveBeenCalledWith('get_pending_update')
    expect(mocks.check).not.toHaveBeenCalled()
    expect(mocks.setSize).toHaveBeenCalledWith({ width: 440, height: 520 })
    expect(mocks.center).toHaveBeenCalled()

    wrapper.unmount()
    expect(mocks.unlisten).toHaveBeenCalled()
  })
})
