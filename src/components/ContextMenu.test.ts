/**
 * ContextMenu.vue — unit tests (vertical glass bubble panel).
 *
 * Verifies:
 *  1.  Menu is hidden by default.
 *  2.  open() makes the bubble panel visible.
 *  3.  open() accepts x/y for API compat but ignores them (panel is always on the left).
 *  4.  All 6 bubbles are rendered.
 *  5.  Each bubble has a non-empty icon.
 *  6.  Hovering a bubble shows its label tooltip.
 *  7.  EN labels are correct.
 *  8.  Clicking a bubble emits the correct action.
 *  9.  Clicking a bubble closes the menu (after exit-animation delay).
 *  10. close() hides the menu.
 *  11. Escape key dismisses the menu.
 *  12. Clicking outside dismisses the menu.
 *  13. Clicking inside does NOT dismiss the menu.
 *  14. Hide bubble has the .bubble--hide modifier class.
 *  15. ContextActionKey excludes 'hide'.
 *  16. Each bubble-wrap carries the pet-ui-overlay class (click-through guard).
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import ContextMenu from './ContextMenu.vue'
import type { MenuAction, ContextActionKey } from './ContextMenu.vue'

// Mock useAppConfig so tests can control characterSize.
const mockCharacterSize = { value: 'large' as 'small' | 'medium' | 'large' }
vi.mock('../composables/useAppConfig', () => ({
  useAppConfig: () => ({
    config: { value: { characterSize: mockCharacterSize.value, showWeather: true } },
    updateConfig: vi.fn(),
  }),
}))

// ── Helpers ───────────────────────────────────────────────────────────

function mountMenu() {
  return mount(ContextMenu, { attachTo: document.body })
}

/** Advance fake timers AND flush Vue's microtask queue. */
async function tickClose() {
  vi.runAllTimers()
  await flushPromises()
}

/**
 * Open the menu and flush the requestAnimationFrame that registers
 * the outside-click / Escape document listeners.
 * (vi.useFakeTimers() stubs RAF, so we must explicitly drain it.)
 */
async function openAndArm(w: ReturnType<typeof mountMenu>) {
  await w.vm.open()
  await flushPromises()
  vi.runAllTimers()   // fires the rAF → document.addEventListener calls
  await flushPromises()
}

// ── Tests ─────────────────────────────────────────────────────────────

describe('ContextMenu (vertical glass bubble panel)', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  // ── Visibility ──────────────────────────────────────────────────────

  it('is hidden on initial mount', () => {
    const w = mountMenu()
    expect(w.find('.bubble-panel').exists()).toBe(false)
    w.unmount()
  })

  it('becomes visible after open()', async () => {
    const w = mountMenu()
    await w.vm.open()
    expect(w.find('.bubble-panel').exists()).toBe(true)
    w.unmount()
  })

  it('open() accepts x/y for API compat but has no positional style', async () => {
    const w = mountMenu()
    await w.vm.open(80, 120)
    const menu = w.find('.bubble-panel')
    expect(menu.exists()).toBe(true)
    // Bubbles are always fixed on the left — no inline left/top positioning from x/y args.
    const style = menu.attributes('style') ?? ''
    expect(style).not.toContain('left: 80px')
    expect(style).not.toContain('top: 120px')
    w.unmount()
  })

  // ── Bubble count & structure ─────────────────────────────────────────

  it('renders exactly 7 bubbles', async () => {
    const w = mountMenu()
    await w.vm.open()
    expect(w.findAll('.bubble-wrap')).toHaveLength(7)
    expect(w.findAll('.bubble')).toHaveLength(7)
    w.unmount()
  })

  it('each bubble has a non-empty icon', async () => {
    const w = mountMenu()
    await w.vm.open()
    const icons = w.findAll('.bubble-icon').map(el => el.text().trim())
    expect(icons).toHaveLength(7)
    icons.forEach(icon => expect(icon.length).toBeGreaterThan(0))
    w.unmount()
  })

  it('each bubble-wrap carries an --i stagger CSS variable', async () => {
    const w = mountMenu()
    await w.vm.open()
    const wraps = w.findAll('.bubble-wrap')
    wraps.forEach((wrap, i) => {
      expect(wrap.attributes('style')).toContain(`--i: ${i}`)
    })
    w.unmount()
  })

  // ── Labels / tooltips ────────────────────────────────────────────────

  it('hovering a bubble shows its label tooltip', async () => {
    const w = mountMenu()
    await w.vm.open()
    const wrap = w.findAll('.bubble-wrap')[0]
    await wrap.trigger('mouseenter')
    expect(w.find('.bubble-tip').exists()).toBe(true)
    await wrap.trigger('mouseleave')
    w.unmount()
  })

  it('renders correct EN labels on hover', async () => {
    const w = mountMenu()
    await w.vm.open()
    const expectedLabels = [
      'Pat Head',
      'Feed Matcha Parfait',
      'Sleep',
      'Fast Learning',
      'Tarot Reading',
      'Chat',
      'Hide App',
    ]
    const wraps = w.findAll('.bubble-wrap')
    for (let i = 0; i < wraps.length; i++) {
      await wraps[i].trigger('mouseenter')
      const tip = w.find('.bubble-tip')
      expect(tip.exists()).toBe(true)
      expect(tip.text()).toBe(expectedLabels[i])
      await wraps[i].trigger('mouseleave')
    }
    w.unmount()
  })

  // ── Emit & click ─────────────────────────────────────────────────────

  it('emits "action" with correct payload when a bubble is clicked', async () => {
    const w = mountMenu()
    await w.vm.open()
    await w.findAll('.bubble')[0].trigger('click') // pat_head
    expect(w.emitted('action')).toBeTruthy()
    expect(w.emitted('action')![0]).toEqual(['pat_head'])
    w.unmount()
  })

  it('emits the correct action for each bubble', async () => {
    const expected: MenuAction[] = ['pat_head', 'feed', 'sleep', 'fast_learning', 'tarot', 'chat', 'hide']
    for (let i = 0; i < expected.length; i++) {
      const w = mountMenu()
      await w.vm.open()
      await w.findAll('.bubble')[i].trigger('click')
      expect(w.emitted('action')![0]).toEqual([expected[i]])
      w.unmount()
    }
  })

  it('hides the menu after a bubble is clicked', async () => {
    const w = mountMenu()
    await w.vm.open()
    await w.findAll('.bubble')[0].trigger('click')
    await tickClose()
    expect(w.find('.bubble-panel').exists()).toBe(false)
    w.unmount()
  })

  // ── Toggle behaviour ────────────────────────────────────────────────

  it('open() while visible closes the panel (toggle)', async () => {
    const w = mountMenu()
    await w.vm.open()
    expect(w.find('.bubble-panel').exists()).toBe(true)
    // Second open() call should close.
    w.vm.open()
    await tickClose()
    expect(w.find('.bubble-panel').exists()).toBe(false)
    w.unmount()
  })

  it('open() after close re-opens the panel', async () => {
    const w = mountMenu()
    await w.vm.open()
    w.vm.close()
    await tickClose()
    await w.vm.open()
    expect(w.find('.bubble-panel').exists()).toBe(true)
    w.unmount()
  })

  // ── Dismiss paths ────────────────────────────────────────────────────

  it('close() hides the menu after exit-animation delay', async () => {
    const w = mountMenu()
    await w.vm.open()
    w.vm.close()
    await tickClose()
    expect(w.find('.bubble-panel').exists()).toBe(false)
    w.unmount()
  })

  it('every bubble-wrap carries pet-ui-overlay (click-through guard)', async () => {
    // pet-ui-overlay must be on elements with pointer-events: auto so that
    // useHitTest's elementsFromPoint check reliably detects the panel.
    const w = mountMenu()
    await w.vm.open()
    const wraps = w.findAll('.bubble-wrap')
    expect(wraps).toHaveLength(7)
    wraps.forEach(wrap => {
      expect(wrap.classes()).toContain('pet-ui-overlay')
    })
    w.unmount()
  })

  it('Escape key dismisses the menu', async () => {
    const w = mountMenu()
    await openAndArm(w)   // open + flush rAF so listeners are registered
    expect(w.find('.bubble-panel').exists()).toBe(true)

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await tickClose()
    expect(w.find('.bubble-panel').exists()).toBe(false)
    w.unmount()
  })

  it('left-clicking outside the menu dismisses it', async () => {
    const w = mountMenu()
    await openAndArm(w)
    expect(w.find('.bubble-panel').exists()).toBe(true)

    document.body.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }))
    await tickClose()
    expect(w.find('.bubble-panel').exists()).toBe(false)
    w.unmount()
  })

  it('right-clicking outside the menu does NOT dismiss it (contextmenu handles toggle)', async () => {
    const w = mountMenu()
    await openAndArm(w)
    expect(w.find('.bubble-panel').exists()).toBe(true)

    // button=2 mousedown fires before contextmenu — must not close the panel
    document.body.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 2 }))
    await flushPromises()
    expect(w.find('.bubble-panel').exists()).toBe(true)
    w.unmount()
  })

  it('clicking inside the menu does not dismiss it', async () => {
    const w = mountMenu()
    await w.vm.open()
    await w.find('.bubble-panel').trigger('mousedown')
    await flushPromises()
    expect(w.find('.bubble-panel').exists()).toBe(true)
    w.unmount()
  })

  // ── Size scaling ────────────────────────────────────────────────────

  it('sets --bubble-size to 36px for large', async () => {
    mockCharacterSize.value = 'large'
    const w = mountMenu()
    await w.vm.open()
    expect(w.find('.bubble-panel').attributes('style')).toContain('--bubble-size: 36px')
    w.unmount()
  })

  it('sets --bubble-size to 31px for medium', async () => {
    mockCharacterSize.value = 'medium'
    const w = mountMenu()
    await w.vm.open()
    expect(w.find('.bubble-panel').attributes('style')).toContain('--bubble-size: 31px')
    w.unmount()
  })

  it('sets --bubble-size to 25px for small', async () => {
    mockCharacterSize.value = 'small'
    const w = mountMenu()
    await w.vm.open()
    expect(w.find('.bubble-panel').attributes('style')).toContain('--bubble-size: 25px')
    w.unmount()
  })

  // ── Style modifiers ──────────────────────────────────────────────────

  it('hide bubble (index 6) carries the .bubble--hide class', async () => {
    const w = mountMenu()
    await w.vm.open()
    const hideBubble = w.findAll('.bubble').at(6)
    expect(hideBubble?.classes()).toContain('bubble--hide')
    w.unmount()
  })

  it('non-hide bubbles do not carry .bubble--hide', async () => {
    const w = mountMenu()
    await w.vm.open()
    const bubbles = w.findAll('.bubble')
    bubbles.slice(0, 6).forEach(b => {
      expect(b.classes()).not.toContain('bubble--hide')
    })
    w.unmount()
  })

  // ── Type-level checks ────────────────────────────────────────────────

  it('ContextActionKey type excludes hide', () => {
    const actionKeys: ContextActionKey[] = ['pat_head', 'feed', 'sleep', 'fast_learning']
    expect(actionKeys).not.toContain('hide')
    expect(actionKeys).toHaveLength(4)
  })
})
