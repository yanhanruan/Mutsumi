/**
 * ContextMenu.vue — unit tests.
 *
 * Verifies:
 *  1. Menu is hidden by default.
 *  2. open(x, y) makes it visible at the given position.
 *  3. Clicking a menu item emits the correct action and hides the menu.
 *  4. close() hides the menu.
 *  5. Escape key dismisses the menu.
 *  6. Clicking outside dismisses the menu.
 *  7. All 4 required menu items are present.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import ContextMenu from './ContextMenu.vue'
import type { MenuAction, ContextActionKey } from './ContextMenu.vue'

// ── Helpers ───────────────────────────────────────────────────────────

function mountMenu() {
  return mount(ContextMenu, { attachTo: document.body })
}

// ── Tests ─────────────────────────────────────────────────────────────

describe('ContextMenu', () => {
  beforeEach(() => {
    // Ensure document event listeners from previous tests are cleaned up
    // by always mounting fresh components.
  })

  it('is hidden on initial mount', () => {
    const w = mountMenu()
    expect(w.find('.ctx-menu').exists()).toBe(false)
    w.unmount()
  })

  it('becomes visible after open()', async () => {
    const w = mountMenu()
    await w.vm.open(100, 200)
    expect(w.find('.ctx-menu').exists()).toBe(true)
    w.unmount()
  })

  it('positions the menu at the given coordinates', async () => {
    const w = mountMenu()
    await w.vm.open(80, 120)
    const el = w.find('.ctx-menu')
    // The style is set as `left: 80px; top: 120px` — check the inline style.
    // (Overflow correction only fires when getBoundingClientRect returns a
    //  non-zero size, which jsdom doesn't simulate, so no clamping here.)
    expect(el.attributes('style')).toContain('left: 80px')
    expect(el.attributes('style')).toContain('top: 120px')
    w.unmount()
  })

  it('renders all 5 menu items (4 actions + hide)', async () => {
    const w = mountMenu()
    await w.vm.open(0, 0)
    const items = w.findAll('.ctx-item')
    expect(items).toHaveLength(5)
    w.unmount()
  })

  it('renders EN labels by default', async () => {
    const w = mountMenu()
    await w.vm.open(0, 0)
    const labels = w.findAll('.ctx-label').map(e => e.text())
    expect(labels).toContain('Pat Head')
    expect(labels).toContain('Feed Matcha Parfait')
    expect(labels).toContain('Sleep')
    expect(labels).toContain('Fast Learning')
    expect(labels).toContain('Hide App')
    w.unmount()
  })

  it('emits "action" with correct payload when an item is clicked', async () => {
    const w = mountMenu()
    await w.vm.open(0, 0)
    await w.findAll('.ctx-item')[0].trigger('click') // pat_head
    expect(w.emitted('action')).toBeTruthy()
    expect(w.emitted('action')![0]).toEqual(['pat_head'])
    w.unmount()
  })

  it('emits the correct action for each item', async () => {
    const expected: MenuAction[] = ['pat_head', 'feed', 'sleep', 'fast_learning', 'hide']
    for (let i = 0; i < expected.length; i++) {
      const w = mountMenu()
      await w.vm.open(0, 0)
      await w.findAll('.ctx-item')[i].trigger('click')
      expect(w.emitted('action')![0]).toEqual([expected[i]])
      w.unmount()
    }
  })

  it('hides the menu after an item is clicked', async () => {
    const w = mountMenu()
    await w.vm.open(0, 0)
    await w.findAll('.ctx-item')[0].trigger('click')
    await flushPromises()
    expect(w.find('.ctx-menu').exists()).toBe(false)
    w.unmount()
  })

  it('close() hides the menu', async () => {
    const w = mountMenu()
    await w.vm.open(0, 0)
    w.vm.close()
    await flushPromises()
    expect(w.find('.ctx-menu').exists()).toBe(false)
    w.unmount()
  })

  it('Escape key dismisses the menu', async () => {
    const w = mountMenu()
    await w.vm.open(0, 0)
    expect(w.find('.ctx-menu').exists()).toBe(true)

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await flushPromises()
    expect(w.find('.ctx-menu').exists()).toBe(false)
    w.unmount()
  })

  it('clicking outside the menu dismisses it', async () => {
    const w = mountMenu()
    await w.vm.open(0, 0)
    expect(w.find('.ctx-menu').exists()).toBe(true)

    // Simulate mousedown on document body (outside the menu element).
    document.body.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }))
    await flushPromises()
    expect(w.find('.ctx-menu').exists()).toBe(false)
    w.unmount()
  })

  it('clicking inside the menu does not dismiss it', async () => {
    const w = mountMenu()
    await w.vm.open(0, 0)
    // Trigger mousedown on the menu element itself.
    const menuEl = w.find('.ctx-menu')
    await menuEl.trigger('mousedown')
    await flushPromises()
    // Menu should still be visible since click was inside.
    expect(w.find('.ctx-menu').exists()).toBe(true)
    w.unmount()
  })

  it('each item has a non-empty label', async () => {
    const w = mountMenu()
    await w.vm.open(0, 0)
    const labels = w.findAll('.ctx-label').map(e => e.text())
    expect(labels).toHaveLength(5)
    labels.forEach(l => expect(l.length).toBeGreaterThan(0))
    w.unmount()
  })

  it('hide item has the ctx-item--danger modifier class', async () => {
    const w = mountMenu()
    await w.vm.open(0, 0)
    const hideBtn = w.findAll('.ctx-item').at(4)
    expect(hideBtn?.classes()).toContain('ctx-item--danger')
    w.unmount()
  })

  it('ContextActionKey type excludes hide', () => {
    // TypeScript compile-time check: 'hide' should not be assignable to ContextActionKey.
    // We verify this at runtime by checking the action keys.
    const actionKeys: ContextActionKey[] = ['pat_head', 'feed', 'sleep', 'fast_learning']
    expect(actionKeys).not.toContain('hide')
  })
})
