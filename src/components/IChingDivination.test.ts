import { mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'
import type { IChingLines } from '../config/iching'
import { localDateKey } from '../composables/useTarotJournal'
import { ICHING_JOURNAL_KEY as key } from '../composables/useIChingJournal'
import { setLocale } from '../i18n'
import { en } from '../i18n/locales/en'
import { ja } from '../i18n/locales/ja'
import { zh } from '../i18n/locales/zh'
import IChingDivination from './IChingDivination.vue'

const copy = en.iching
const saved = () => localStorage.getItem(key)
const reading = (dateKey = localDateKey(), lines: IChingLines = [7, 7, 7, 7, 7, 7]) =>
  ({ dateKey, createdAt: dateKey + 'T12:00:00Z', lines, fortuneLevel: 'fortune' })
const seed = (...readings: ReturnType<typeof reading>[]) =>
  localStorage.setItem(key, JSON.stringify({ version: 1, readings }))

async function openPanel() {
  const panel = mount(IChingDivination)
  panel.vm.open()
  await nextTick()
  return panel
}

type Panel = Awaited<ReturnType<typeof openPanel>>
const byText = (panel: Panel, label: string) => panel.findAll('button').find(button => button.text() === label)!
const byLabel = (panel: Panel, label: string) => panel.get<HTMLButtonElement>(`button[aria-label="${label}"]`)
const visibleLines = (panel: Panel) => panel.findAll('.hex-line:not(.is-hidden)').length

async function revealStep() {
  await vi.advanceTimersByTimeAsync(850)
}

beforeEach(() => {
  setLocale('en')
  localStorage.clear()
  vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] })
  vi.setSystemTime(new Date(2026, 6, 12, 12))
  vi.stubGlobal('matchMedia', () => ({ matches: false }))
})

afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

describe('I Ching panel', () => {
  it('reveals six lines, keeps a cancelled cast, and shows a confirmed replacement in history', async () => {
    const panel = await openPanel()
    await byText(panel, copy.begin).trigger('click')
    expect(visibleLines(panel)).toBe(0)
    for (let line = 1; line <= 6; line++) {
      await revealStep()
      expect(visibleLines(panel)).toBe(line)
    }
    await revealStep()
    expect(panel.get('h1').text()).toBe(copy.todaysReading)

    const original = saved()
    await byText(panel, copy.generateAnother).trigger('click')
    const staleConfirm = byText(panel, copy.confirmReroll).element
    byText(panel, copy.keepCurrent).element.click()
    // The old confirmation button is still mounted until Vue updates the DOM.
    staleConfirm.click()
    await nextTick()
    expect(saved()).toBe(original)
    expect(panel.get('h1').text()).toBe(copy.todaysReading)

    await byText(panel, copy.generateAnother).trigger('click')
    await byText(panel, copy.confirmReroll).trigger('click')
    await vi.runAllTimersAsync()
    expect(saved()).not.toBe(original)
    const hexagramName = panel.get('.hexagram-card h2').text()
    await byLabel(panel, copy.history).trigger('click')
    expect(panel.findAll('.history-entry strong').map(entry => entry.text())).toEqual([hexagramName])
    panel.unmount()
  })

  it.each(['closing', 'unmounting'])('clears the pending reveal on %s the panel', async exit => {
    const panel = await openPanel()
    const idleTimers = vi.getTimerCount()
    await byText(panel, copy.begin).trigger('click')
    await revealStep()
    expect(visibleLines(panel)).toBe(1)
    expect(vi.getTimerCount()).toBeGreaterThan(idleTimers)
    const original = saved()

    if (exit === 'closing') {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', cancelable: true }))
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', cancelable: true }))
      await byLabel(panel, copy.close).trigger('click')
      await revealStep()
      expect(panel.emitted('close')).toHaveLength(1)
      expect(visibleLines(panel)).toBe(1)
      panel.vm.dismiss()
      await nextTick()
      expect(panel.find('.iching-overlay').exists()).toBe(false)
      expect(vi.getTimerCount()).toBe(idleTimers)
    }
    panel.unmount()
    expect(vi.getTimerCount()).toBe(idleTimers)
    expect(saved()).toBe(original)
  })

  it('ignores a queued cast while the parent is still closing the panel', async () => {
    const panel = await openPanel()
    const idleTimers = vi.getTimerCount()
    byLabel(panel, copy.close).element.click()
    await byText(panel, copy.begin).trigger('click')
    expect(saved()).toBeNull()
    expect(vi.getTimerCount()).toBe(idleTimers)
    panel.unmount()
  })

  it('reuses today and returns from a historical result without rereading storage', async () => {
    const oldDay = '2000-01-01'
    seed(reading(), reading(oldDay))
    const storage = localStorage
    const before = saved()
    const getItem = vi.fn((k: string) => storage.getItem(k))
    vi.stubGlobal('localStorage', { getItem, setItem: storage.setItem.bind(storage) })
    const panel = await openPanel()
    expect(panel.get('h1').text()).toBe(copy.todaysReading)
    await byLabel(panel, copy.history).trigger('click')
    await panel.findAll('.history-entry')[1].trigger('click')
    expect(panel.get('h1').text()).toBe(oldDay)
    await byText(panel, copy.back).trigger('click')
    expect(panel.get('h1').text()).toBe(copy.history)
    panel.vm.dismiss()
    panel.vm.open()
    await nextTick()
    expect(panel.get('h1').text()).toBe(copy.todaysReading)
    expect(getItem).toHaveBeenCalledExactlyOnceWith(key)
    expect(storage.getItem(key)).toBe(before)
    panel.unmount()
  })

  it('updates an open reading across three languages while keeping the moving line at the bottom', async () => {
    seed(reading(localDateKey(), [9, 7, 7, 7, 7, 7]))
    const panel = await openPanel()
    await panel.get('button[aria-expanded]').trigger('click')
    const drawn = panel.get('.hexagram-card').findAll('.hex-line')
    expect(drawn.findIndex(line => line.classes('is-moving'))).toBe(5)
    for (const [locale, { iching }] of [['zh', zh], ['en', en], ['ja', ja]] as const) {
      setLocale(locale)
      await nextTick()
      const qian = iching.hexagrams.hexagram01
      expect(panel.get('h1').text()).toBe(iching.todaysReading)
      expect(panel.get('.hexagram-card h2').text()).toBe(qian.name)
      expect(panel.get('.fortune').text()).toContain(iching.fortune.summaries.fortune)
      const lines = panel.get('.line-details').findAll('li')
      expect(lines.map(line => line.get('p').text())).toEqual(qian.lines)
      expect(lines[0].classes('moving')).toBe(true)
      expect(lines[0].text()).toContain(iching.linePositions[0])
      expect(lines[5].text()).toContain(iching.linePositions[5])
    }
    panel.unmount()
  })
})
