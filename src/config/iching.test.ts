import { describe, expect, it } from 'vitest'
import {
  analyzeIChingLines, findHexagramByPattern, generateLines, getCoinSidesForLine, isIChingLines,
  HEXAGRAMS, HEXAGRAM_PALACES, PALACE_POSITIONS, patternKey,
  type HexagramLinePattern,
} from './iching'

describe('coin casting', () => {
  it('preserves one, three, three and one outcomes for the four line values', () => {
    const totals = new Map<number, number>()
    for (let combination = 0; combination < 8; combination++) {
      const coins = [combination & 1, (combination >> 1) & 1, (combination >> 2) & 1] as const
      let index = 0
      const [value] = generateLines(() => coins[index++ % 3] as 0 | 1)
      totals.set(value, (totals.get(value) ?? 0) + 1)
    }
    expect(Object.fromEntries(totals)).toEqual({ 6: 1, 7: 3, 8: 3, 9: 1 })
  })

  it('casts six lines bottom to top and displays the correct number of coin backs', () => {
    const coins = [0, 0, 0, 1, 0, 0, 1, 1, 0, 1, 1, 1, 0, 0, 0, 1, 0, 0] as const
    const expected = [6, 7, 8, 9, 6, 7] as const
    let index = 0
    expect(generateLines(() => coins[index++])).toEqual(expected)
    expect(expected.slice(0, 4).map(getCoinSidesForLine)).toEqual([
      ['front', 'front', 'front'], ['back', 'front', 'front'],
      ['back', 'back', 'front'], ['back', 'back', 'back'],
    ])
  })
})

describe('hexagram analysis', () => {
  it('rejects six-line arrays with missing or invalid line values', () => {
    expect(isIChingLines([6, 7, 8, 9, 6, 7])).toBe(true)
    expect(isIChingLines(Array(6))).toBe(false)
    expect(isIChingLines([6, 7, 8, 9, 6, 10])).toBe(false)
  })

  it('maps each of the 64 patterns to its unique King Wen entry', () => {
    const ids = new Set()
    for (let bits = 0; bits < 64; bits++) {
      const pattern = Array.from({ length: 6 }, (_, i) => Boolean(bits & (1 << i))) as unknown as HexagramLinePattern
      const hexagram = findHexagramByPattern(pattern)
      expect(patternKey(hexagram.lines)).toBe(patternKey(pattern))
      expect(HEXAGRAMS[hexagram.kingWenNumber - 1]).toBe(hexagram)
      ids.add(hexagram.id)
    }
    expect(ids.size).toBe(64)
    expect(analyzeIChingLines([7, 7, 7, 7, 7, 7]).primary.kingWenNumber).toBe(1)
    expect(analyzeIChingLines([8, 8, 8, 8, 8, 8]).primary.kingWenNumber).toBe(2)
  })

  it('classifies all 64 hexagrams into eight palaces with one of each position', () => {
    for (const palace of HEXAGRAM_PALACES) {
      const group = HEXAGRAMS.filter(hexagram => hexagram.palace === palace)
      expect(group).toHaveLength(8)
      expect(new Set(group.map(hexagram => hexagram.palacePosition))).toEqual(new Set(PALACE_POSITIONS))
    }
  })

  it('flips old yin and old yang at the correct positions, leaving young lines alone', () => {
    const cast = analyzeIChingLines([6, 7, 8, 9, 7, 8])
    expect(cast.movingLineIndexes).toEqual([0, 3])
    expect(cast.primary).toMatchObject({ id: 'hexagram47', palace: 'dui', palacePosition: 'first' })
    expect(cast.changed).toMatchObject({ id: 'hexagram60', palace: 'kan', palacePosition: 'first' })
  })

  it('has no changed hexagram when all six lines are still', () => {
    expect(analyzeIChingLines([7, 7, 7, 7, 7, 7])).toMatchObject({ changed: null, movingLineIndexes: [] })
  })
})
