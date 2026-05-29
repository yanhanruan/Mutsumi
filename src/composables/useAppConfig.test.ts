/**
 * useAppConfig — unit tests.
 *
 * Tests:
 *  1. CHAR_SIZE_DIMS has correct relative ordering.
 *  2. config always holds a valid CharacterSize.
 *  3. updateConfig patches individual keys reactively.
 *  4. Partial patches leave untouched keys intact.
 *  5. updateConfig persists to localStorage.
 */

import { describe, it, expect, vi, beforeAll } from 'vitest'

// ── Setup localStorage mock before module import ──────────────────────

const localStorageMock = (() => {
  let store: Record<string, string> = {}
  return {
    getItem:    (k: string) => store[k] ?? null,
    setItem:    (k: string, v: string) => { store[k] = v },
    removeItem: (k: string) => { delete store[k] },
    clear:      () => { store = {} },
  }
})()

beforeAll(() => {
  Object.defineProperty(globalThis, 'localStorage', {
    value: localStorageMock,
    writable: true,
  })
})

// ── Mock Tauri event API ──────────────────────────────────────────────

vi.mock('@tauri-apps/api/event', () => ({
  emit:   vi.fn().mockResolvedValue(undefined),
  listen: vi.fn().mockResolvedValue(() => {}),
}))

import { CHAR_SIZE_DIMS, useAppConfig, type CharacterSize } from './useAppConfig'

// ── Tests ─────────────────────────────────────────────────────────────

describe('CHAR_SIZE_DIMS', () => {
  it('defines entries for all three size tiers', () => {
    const tiers: CharacterSize[] = ['small', 'medium', 'large']
    for (const t of tiers) {
      expect(CHAR_SIZE_DIMS[t]).toBeDefined()
      expect(CHAR_SIZE_DIMS[t]).toHaveLength(2)
    }
  })

  it('large is bigger than medium', () => {
    expect(CHAR_SIZE_DIMS.large[0]).toBeGreaterThan(CHAR_SIZE_DIMS.medium[0])
    expect(CHAR_SIZE_DIMS.large[1]).toBeGreaterThan(CHAR_SIZE_DIMS.medium[1])
  })

  it('medium is bigger than small', () => {
    expect(CHAR_SIZE_DIMS.medium[0]).toBeGreaterThan(CHAR_SIZE_DIMS.small[0])
    expect(CHAR_SIZE_DIMS.medium[1]).toBeGreaterThan(CHAR_SIZE_DIMS.small[1])
  })

  it('each entry has positive width and height', () => {
    for (const [w, h] of Object.values(CHAR_SIZE_DIMS)) {
      expect(w).toBeGreaterThan(0)
      expect(h).toBeGreaterThan(0)
    }
  })
})

describe('useAppConfig', () => {
  it('config holds a valid CharacterSize', () => {
    const { config } = useAppConfig()
    expect(['small', 'medium', 'large']).toContain(config.value.characterSize)
  })

  it('config showWeather is a boolean', () => {
    const { config } = useAppConfig()
    expect(typeof config.value.showWeather).toBe('boolean')
  })

  it('updateConfig patches characterSize reactively', async () => {
    const { config, updateConfig } = useAppConfig()
    const original = config.value.characterSize
    const next: CharacterSize = original === 'small' ? 'large' : 'small'
    await updateConfig({ characterSize: next })
    expect(config.value.characterSize).toBe(next)
    // restore
    await updateConfig({ characterSize: original })
  })

  it('updateConfig patches showWeather reactively', async () => {
    const { config, updateConfig } = useAppConfig()
    const original = config.value.showWeather
    await updateConfig({ showWeather: !original })
    expect(config.value.showWeather).toBe(!original)
    // restore
    await updateConfig({ showWeather: original })
  })

  it('partial patch leaves other keys unchanged', async () => {
    const { config, updateConfig } = useAppConfig()
    const origSize = config.value.characterSize
    const origWeather = config.value.showWeather
    await updateConfig({ showWeather: !origWeather })
    expect(config.value.characterSize).toBe(origSize)
    // restore
    await updateConfig({ showWeather: origWeather })
  })

  it('updateConfig persists to localStorage', async () => {
    const { updateConfig } = useAppConfig()
    await updateConfig({ characterSize: 'medium' })
    const stored = JSON.parse(localStorageMock.getItem('mutsumi_app_config') ?? '{}')
    expect(stored.characterSize).toBe('medium')
  })

  it('two calls to useAppConfig share the same config ref', async () => {
    const a = useAppConfig()
    const b = useAppConfig()
    await a.updateConfig({ characterSize: 'small' })
    expect(b.config.value.characterSize).toBe('small')
    // restore
    await a.updateConfig({ characterSize: 'large' })
  })
})
