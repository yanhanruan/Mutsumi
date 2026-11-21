import { describe, it, expect } from 'vitest'
import {
  HEXAGRAMS,
  HEXAGRAM_PALACES,
  PALACE_POSITIONS,
  createIChingReading,
  findHexagramById,
  findHexagramByPattern,
  generateLine,
  generateLines,
  getCoinSidesForLine,
  getChangedPattern,
  getMovingLineIndexes,
  getHexagramPalaceInfo,
  getPrimaryPattern,
  isChangedYang,
  isMovingLine,
  isPrimaryYang,
  patternKey,
  type HexagramLinePattern,
  type IChingLines,
  type RandomSource,
} from './iching'

class SequenceRandom implements RandomSource {
  private index = 0
  private readonly values: readonly number[]

  constructor(values: readonly number[]) {
    this.values = values
  }

  nextInt(maxExclusive: number): number {
    const value = this.values[this.index++]
    if (value === undefined) throw new Error('No more random values')
    expect(value).toBeGreaterThanOrEqual(0)
    expect(value).toBeLessThan(maxExclusive)
    return value
  }
}

describe('I Ching line semantics', () => {
  it('maps line values to primary, changed, and moving states', () => {
    expect(isPrimaryYang(6)).toBe(false)
    expect(isChangedYang(6)).toBe(true)
    expect(isMovingLine(6)).toBe(true)

    expect(isPrimaryYang(7)).toBe(true)
    expect(isChangedYang(7)).toBe(true)
    expect(isMovingLine(7)).toBe(false)

    expect(isPrimaryYang(8)).toBe(false)
    expect(isChangedYang(8)).toBe(false)
    expect(isMovingLine(8)).toBe(false)

    expect(isPrimaryYang(9)).toBe(true)
    expect(isChangedYang(9)).toBe(false)
    expect(isMovingLine(9)).toBe(true)
  })

  it('maps coin backs to the matching yin or yang line value', () => {
    expect(getCoinSidesForLine(6)).toEqual(['front', 'front', 'front'])
    expect(getCoinSidesForLine(7)).toEqual(['back', 'front', 'front'])
    expect(getCoinSidesForLine(8)).toEqual(['back', 'back', 'front'])
    expect(getCoinSidesForLine(9)).toEqual(['back', 'back', 'back'])
  })
})

describe('I Ching line ordering', () => {
  it('keeps generated lines in bottom-to-top order', () => {
    const lines = generateLines(new SequenceRandom([
      0, 0, 0,
      0, 0, 1,
      0, 1, 1,
      1, 1, 1,
      0, 0, 0,
      1, 1, 1,
    ]))
    expect(lines).toEqual([6, 7, 8, 9, 6, 9])
  })

  it('visual reversal does not mutate canonical data', () => {
    const lines: IChingLines = [6, 7, 8, 9, 7, 8]
    const visual = [...lines].reverse()
    expect(visual).toEqual([8, 7, 9, 8, 7, 6])
    expect(lines).toEqual([6, 7, 8, 9, 7, 8])
  })

  it('moving-line indexes use zero-based bottom-to-top positions', () => {
    expect(getMovingLineIndexes([6, 7, 8, 7, 8, 9])).toEqual([0, 5])
  })
})

describe('I Ching random generation', () => {
  it('deterministic injected randomness produces expected coin totals', () => {
    expect(generateLine(new SequenceRandom([0, 0, 0]))).toBe(6)
    expect(generateLine(new SequenceRandom([0, 0, 1]))).toBe(7)
    expect(generateLine(new SequenceRandom([0, 1, 1]))).toBe(8)
    expect(generateLine(new SequenceRandom([1, 1, 1]))).toBe(9)
  })

  it('one generated line always belongs to 6 | 7 | 8 | 9', () => {
    for (const bits of [[0, 0, 0], [0, 0, 1], [0, 1, 1], [1, 1, 1]]) {
      expect([6, 7, 8, 9]).toContain(generateLine(new SequenceRandom(bits)))
    }
  })

  it('six-line generation always returns exactly six values', () => {
    const lines = generateLines(new SequenceRandom(Array.from({ length: 18 }, (_, i) => i % 2)))
    expect(lines).toHaveLength(6)
  })
})

describe('I Ching hexagram mapping', () => {
  it('contains exactly 64 hexagrams', () => {
    expect(HEXAGRAMS).toHaveLength(64)
  })

  it('contains every King Wen number from 1 to 64 exactly once', () => {
    const numbers = HEXAGRAMS.map(h => h.kingWenNumber).sort((a, b) => a - b)
    expect(numbers).toEqual(Array.from({ length: 64 }, (_, i) => i + 1))
    expect(new Set(numbers).size).toBe(64)
  })

  it('contains unique line patterns', () => {
    const keys = HEXAGRAMS.map(h => patternKey(h.lines))
    expect(new Set(keys).size).toBe(64)
  })

  it('resolves all possible six-line patterns', () => {
    for (let mask = 0; mask < 64; mask++) {
      const pattern = Array.from({ length: 6 }, (_, i) => Boolean(mask & (1 << i))) as unknown as HexagramLinePattern
      expect(findHexagramByPattern(pattern).kingWenNumber).toBeGreaterThanOrEqual(1)
    }
  })

  it('maps Qian and Kun to King Wen 1 and 2', () => {
    expect(findHexagramByPattern([true, true, true, true, true, true]).kingWenNumber).toBe(1)
    expect(findHexagramByPattern([false, false, false, false, false, false]).kingWenNumber).toBe(2)
  })

  it('resolves a representative mixed pattern to the canonical hexagram', () => {
    expect(findHexagramByPattern([true, false, false, false, true, false]).kingWenNumber).toBe(3)
  })
})

describe('Jing Fang Eight-Palace classification', () => {
  it('classifies all 64 canonical hexagrams', () => {
    expect(HEXAGRAMS).toHaveLength(64)
    for (const hexagram of HEXAGRAMS) {
      expect(HEXAGRAM_PALACES).toContain(hexagram.palace)
      expect(PALACE_POSITIONS).toContain(hexagram.palacePosition)
      expect(getHexagramPalaceInfo(hexagram.id)).toEqual({
        palace: hexagram.palace,
        position: hexagram.palacePosition,
      })
    }
  })

  it('contains exactly eight hexagrams and one of every position per palace', () => {
    for (const palace of HEXAGRAM_PALACES) {
      const members = HEXAGRAMS.filter(hexagram => hexagram.palace === palace)
      expect(members).toHaveLength(8)
      expect(new Set(members.map(hexagram => hexagram.palacePosition))).toEqual(new Set(PALACE_POSITIONS))
    }
  })

  it.each([
    ['hexagram01', 'qian', 'pure'],
    ['hexagram02', 'kun', 'pure'],
    ['hexagram44', 'qian', 'first'],
    ['hexagram35', 'qian', 'wanderingSoul'],
    ['hexagram14', 'qian', 'returningSoul'],
  ] as const)('classifies %s as %s / %s', (id, palace, position) => {
    expect(getHexagramPalaceInfo(id)).toEqual({ palace, position })
  })

  it('resolves primary and changed palace information independently', () => {
    const reading = createIChingReading([6, 7, 8, 9, 7, 8], '2026-07-12', '2026-07-12T00:00:00.000Z', 'palace-pair')
    expect(reading.primaryHexagramId).toBe('hexagram47')
    expect(reading.changedHexagramId).toBe('hexagram60')
    expect(getHexagramPalaceInfo(reading.primaryHexagramId)).toEqual({ palace: 'dui', position: 'first' })
    expect(getHexagramPalaceInfo(reading.changedHexagramId!)).toEqual({ palace: 'kan', position: 'first' })
  })

  it('does not depend on date, time, randomness, or moving-line count', () => {
    const stillQian = createIChingReading([7, 7, 7, 7, 7, 7], '1900-01-01', '1900-01-01T00:00:00.000Z', 'still')
    generateLine(new SequenceRandom([1, 0, 1]))
    const movingQian = createIChingReading([9, 7, 7, 7, 7, 7], '2999-12-31', '2999-12-31T23:59:59.999Z', 'moving')

    expect(stillQian.movingLineIndexes).toHaveLength(0)
    expect(movingQian.movingLineIndexes).toHaveLength(1)
    expect(getHexagramPalaceInfo(stillQian.primaryHexagramId)).toEqual({ palace: 'qian', position: 'pure' })
    expect(getHexagramPalaceInfo(movingQian.primaryHexagramId)).toEqual({ palace: 'qian', position: 'pure' })
    expect(getHexagramPalaceInfo(movingQian.changedHexagramId!)).toEqual({ palace: 'qian', position: 'first' })
    expect(stillQian).not.toHaveProperty('palace')
    expect(movingQian).not.toHaveProperty('palacePosition')
  })

  it('keeps the canonical definition as the classification source', () => {
    expect(findHexagramById('hexagram35')).toMatchObject({ palace: 'qian', palacePosition: 'wanderingSoul' })
  })
})

describe('I Ching changed hexagram', () => {
  it('has no moving lines for all young-yang lines', () => {
    expect(getMovingLineIndexes([7, 7, 7, 7, 7, 7])).toEqual([])
  })

  it('omits changedHexagramId when no lines move', () => {
    const reading = createIChingReading([7, 7, 7, 7, 7, 7], '2026-07-12', '2026-07-12T00:00:00.000Z', 'r1')
    expect(reading.changedHexagramId).toBeNull()
  })

  it('changes old yin to yang and old yang to yin while preserving non-moving lines', () => {
    const lines: IChingLines = [6, 7, 8, 9, 7, 8]
    expect(getPrimaryPattern(lines)).toEqual([false, true, false, true, true, false])
    expect(getChangedPattern(lines)).toEqual([true, true, false, false, true, false])
  })

  it('resolves a representative changed hexagram', () => {
    const reading = createIChingReading([6, 7, 8, 9, 7, 8], '2026-07-12', '2026-07-12T00:00:00.000Z', 'r2')
    expect(reading.primaryHexagramId).toBe('hexagram47')
    expect(reading.changedHexagramId).toBe('hexagram60')
  })
})
