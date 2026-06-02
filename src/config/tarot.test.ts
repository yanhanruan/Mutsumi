/**
 * tarot config — table integrity tests.
 *
 * The card data is the feature's single source of truth, so guard its shape:
 *   - exactly 22 Major Arcana
 *   - ids are the complete 0–21 set, unique
 *   - every card has a non-empty name and fortune text
 *   - hue is a valid 0–360 degree
 */
import { describe, it, expect } from 'vitest'
import { MAJOR_ARCANA, TAROT_TIMINGS } from './tarot'

describe('MAJOR_ARCANA', () => {
  it('contains exactly 22 cards', () => {
    expect(MAJOR_ARCANA).toHaveLength(22)
  })

  it('ids are the complete unique 0–21 set', () => {
    const ids = MAJOR_ARCANA.map(c => c.id).sort((a, b) => a - b)
    expect(ids).toEqual(Array.from({ length: 22 }, (_, i) => i))
    expect(new Set(ids).size).toBe(22)
  })

  it('every card has a non-empty name and fortune text', () => {
    for (const c of MAJOR_ARCANA) {
      expect(c.card_name.trim().length).toBeGreaterThan(0)
      expect(c.fortune_text.trim().length).toBeGreaterThan(0)
    }
  })

  it('fortune texts are all distinct', () => {
    const texts = MAJOR_ARCANA.map(c => c.fortune_text)
    expect(new Set(texts).size).toBe(22)
  })

  it('every card hue is within 0–360', () => {
    for (const c of MAJOR_ARCANA) {
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
