import { describe, expect, it } from 'vitest'
import { analyzeIChingLines } from './iching'
import { createDivinationTimeContext, getSolarTermInstant } from './ichingCalendar'
import { evaluateDailyFortune } from './ichingFortune'

describe('divination calendar', () => {
  const timezone = 'Asia/Shanghai'
  const monthAt = (date: Date) => createDivinationTimeContext(date.toISOString(), timezone).monthBranch
  const before = (date: Date) => new Date(date.getTime() - 1)

  it('continues the Zi month across New Year until the instant of Xiao Han', () => {
    const xiaoHan = getSolarTermInstant(2026, 0)
    expect(monthAt(new Date('2025-12-31T16:00:00Z'))).toBe('zi')
    expect(monthAt(before(xiaoHan))).toBe('zi')
    expect(monthAt(xiaoHan)).toBe('chou')
  })

  it('changes months at Li Chun and computes the day branch in the chosen timezone', () => {
    const liChun = getSolarTermInstant(2026, 2)
    expect(monthAt(before(liChun))).toBe('chou')
    expect(monthAt(liChun)).toBe('yin')
    expect(createDivinationTimeContext('1970-01-01T12:00:00Z', 'UTC').dayBranch).toBe('chou')
    expect(createDivinationTimeContext('1970-01-01T16:00:00Z', timezone).dayBranch).toBe('yin')
  })

  it('rejects a fractional or out-of-range solar term and an invalid year', () => {
    for (const [year, term] of [[2026, 0.5], [2026, -1], [2026, 24], [NaN, 0], [2101, 0]]) {
      expect(() => getSolarTermInstant(year, term)).toThrow()
    }
  })
})

describe('daily fortune', () => {
  const qian = analyzeIChingLines([7, 7, 7, 7, 7, 7])

  it.each([
    ['xu', 'xu', 4, 'greatFortune'],
    ['chou', 'yin', -1, 'neutral'],
    ['yin', 'shen', -2, 'misfortune'],
  ] as const)('scores month %s and day %s as %s (%s)', (monthBranch, dayBranch, score, level) => {
    expect(evaluateDailyFortune(qian, { monthBranch, dayBranch })).toEqual({ score, level })
  })

  it('penalizes month opposition in addition to the main influence and leaves day opposition neutral', () => {
    expect(evaluateDailyFortune(qian, { monthBranch: 'chen', dayBranch: 'wu' }))
      .toEqual({ score: 1, level: 'fortune' })
    expect(evaluateDailyFortune(qian, { monthBranch: 'shen', dayBranch: 'chen' }))
      .toEqual({ score: 1, level: 'fortune' })
  })

  it('counts multiple supporting moving lines once and evaluates the changed Shi line', () => {
    const context = { monthBranch: 'zi', dayBranch: 'chen' } as const
    const moving = analyzeIChingLines([6, 6, 6, 6, 6, 6])
    const stillShi = analyzeIChingLines([6, 6, 6, 6, 6, 8])
    expect(evaluateDailyFortune(stillShi, context)).toEqual({ score: 2, level: 'fortune' })
    expect(evaluateDailyFortune(moving, context)).toEqual({ score: 4, level: 'greatFortune' })
  })
})
