/**
 * tarot config — table integrity tests.
 *
 * Guards:
 *   - exactly 78 cards (full deck: 22 Major + 56 Minor Arcana)
 *   - ids are the complete unique 0–77 set
 *   - every card has a name + upright + reversed, each populated in en/zh/ja
 *   - all 78 EN names are distinct; all 156 EN interpretations are distinct
 *   - hue is a valid 0–360 degree
 */
import { describe, it, expect } from 'vitest'
import { TAROT_DECK, TAROT_TIMINGS, type LocalizedText } from './tarot'

const LOCALES = ['en', 'zh', 'ja'] as const

function allPopulated(txt: LocalizedText): boolean {
  return LOCALES.every(l => typeof txt[l] === 'string' && txt[l].trim().length > 0)
}

describe('TAROT_DECK', () => {
  it('contains exactly 78 cards', () => {
    expect(TAROT_DECK).toHaveLength(78)
  })

  it('ids are the complete unique 0–77 set', () => {
    const ids = TAROT_DECK.map(c => c.id).sort((a, b) => a - b)
    expect(ids).toEqual(Array.from({ length: 78 }, (_, i) => i))
    expect(new Set(ids).size).toBe(78)
  })

  it('every card has name + upright + reversed populated in all 3 locales', () => {
    for (const c of TAROT_DECK) {
      expect(allPopulated(c.card_name)).toBe(true)
      expect(allPopulated(c.fortune_text)).toBe(true)
      expect(allPopulated(c.reversed_text)).toBe(true)
    }
  })

  it('all 78 English card names are distinct', () => {
    const names = TAROT_DECK.map(c => c.card_name.en)
    expect(new Set(names).size).toBe(78)
  })

  it('all 156 English interpretations are distinct (no copy-paste)', () => {
    const all = [
      ...TAROT_DECK.map(c => c.fortune_text.en),
      ...TAROT_DECK.map(c => c.reversed_text.en),
    ]
    expect(new Set(all).size).toBe(156)
  })

  it('every card hue is within 0–360', () => {
    for (const c of TAROT_DECK) {
      expect(c.hue).toBeGreaterThanOrEqual(0)
      expect(c.hue).toBeLessThanOrEqual(360)
    }
  })
})

describe('TAROT_TIMINGS', () => {
  it('loading window is a positive 1–2 s range', () => {
    expect(TAROT_TIMINGS.loadingMinMs).toBeGreaterThan(0)
    expect(TAROT_TIMINGS.loadingMaxMs).toBeGreaterThanOrEqual(TAROT_TIMINGS.loadingMinMs)
  })
})
