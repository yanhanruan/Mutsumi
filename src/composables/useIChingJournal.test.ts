import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ICHING_JOURNAL_KEY as key, useIChingJournal } from './useIChingJournal'
import type { CoinFlip } from '../config/iching'

const backupKey = key + '_backup'
const openJournal = (day = 12, flipCoin: CoinFlip = () => 0) =>
  useIChingJournal({ now: () => new Date(2026, 6, day, 12), flipCoin })
const raw = () => localStorage.getItem(key)
const seed = (...readings: unknown[]) => localStorage.setItem(key, JSON.stringify({ version: 1, readings }))
const legacy = {
  id: 'old', dateKey: '2026-07-11', createdAt: '2026-07-11T12:00:00Z',
  lines: [7, 7, 7, 7, 7, 7], primaryHexagramId: 'hexagram01', changedHexagramId: null,
  movingLineIndexes: [], fortuneLevel: 'fortune', fortuneScore: 2, fortuneRuleVersion: 2, timezone: 'Asia/Shanghai',
}
const legacyOn = (dateKey: string) => ({ ...legacy, dateKey, createdAt: dateKey + 'T12:00:00Z' })
const summary = (journal: ReturnType<typeof openJournal>) =>
  journal.history.value.map(({ dateKey, fortuneLevel }) => [dateKey, fortuneLevel])

function stubStorage(overrides: Partial<Storage>) {
  const storage = localStorage
  vi.stubGlobal('localStorage', {
    getItem: storage.getItem.bind(storage), setItem: storage.setItem.bind(storage), ...overrides,
  })
  return storage
}

beforeEach(() => localStorage.clear())
afterEach(() => vi.unstubAllGlobals())

describe('I Ching journal', () => {
  it('reads legacy records and preserves their saved fortune when compacting the journal', () => {
    seed(legacy)
    openJournal().castToday()
    const { dateKey, createdAt, lines, fortuneLevel } = legacy
    expect(JSON.parse(raw()!).readings[1]).toEqual({ dateKey, createdAt, lines, fortuneLevel })
    expect(openJournal().history.value.map(item => item.primary.id)).toEqual(['hexagram02', 'hexagram01'])
  })

  it('keeps recoverable casts with bad fortune metadata and skips malformed entries on save', () => {
    seed(
      legacy, legacyOn('1900-01-01'),
      { ...legacyOn('1899-12-31'), fortuneLevel: undefined },
      { ...legacyOn('2101-01-01'), fortuneLevel: undefined },
      { ...legacyOn('2026-07-12'), timezone: 'Bad/Timezone', fortuneLevel: undefined },
      ...['2026-02-30', '2026-13-01', 20260717].map(dateKey => ({ ...legacy, dateKey })),
      { ...legacyOn('2026-07-17'), createdAt: 'bad timestamp' },
      { ...legacyOn('2026-07-18'), lines: '777777' },
      { ...legacyOn('2026-07-19'), createdAt: undefined },
      { ...legacyOn('2026-07-20'), lines: [1, 7, 7, 7, 7, 7] },
      null,
    )
    const journal = openJournal(14)
    const recovered = [
      ['2101-01-01', 'neutral'], ['2026-07-12', 'neutral'], ['2026-07-11', 'fortune'],
      ['1900-01-01', 'fortune'], ['1899-12-31', 'neutral'],
    ]
    expect(summary(journal)).toEqual(recovered)
    const added = journal.castToday()
    expect(summary(openJournal())).toEqual([recovered[0], [added.dateKey, added.fortuneLevel], ...recovered.slice(1)])
  })

  it('uses the newest legacy reading for a duplicated date', () => {
    seed({ ...legacy, createdAt: '2026-07-11T18:00:00Z', lines: [6, 6, 6, 6, 6, 6] }, legacy)
    expect(openJournal().history.value).toMatchObject([{ primary: { id: 'hexagram02' } }])
  })

  it('preserves records saved by another journal after this one opened', () => {
    seed(legacy)
    const first = openJournal(12)
    const second = openJournal(13, () => 1)
    const firstReading = first.castToday()
    const secondReading = second.castToday()
    expect(openJournal().history.value).toMatchObject([
      secondReading, firstReading, { dateKey: legacy.dateKey, lines: legacy.lines },
    ])
  })

  it('does not let an impossible ISO timestamp replace a valid record for the same day', () => {
    const original = legacyOn('2026-02-28')
    seed(original, { ...original, createdAt: '2026-02-30T00:00:00Z', lines: [9, 9, 9, 9, 9, 9] })
    const { dateKey, createdAt, lines } = original
    const expected = { dateKey, createdAt, lines }
    const journal = openJournal()
    expect(journal.history.value).toMatchObject([expected])
    journal.castToday()
    expect(openJournal().history.value[1]).toMatchObject(expected)
  })

  it('refuses to replace a mixed journal until its original bytes have been backed up', () => {
    const original = JSON.stringify({ version: 1, readings: [null, legacy] }, null, 2)
    const olderBackup = '{"version":1,"readings":[false]}'
    localStorage.setItem(key, original)
    localStorage.setItem(backupKey, JSON.stringify([olderBackup]))
    const journal = openJournal()
    let fullKey: string | null = backupKey
    const storage = stubStorage({
      setItem(k, value) {
        if (k === key) expect(JSON.parse(storage.getItem(backupKey)!)).toEqual([olderBackup, original])
        if (k === fullKey) throw new DOMException('storage quota', 'QuotaExceededError')
        storage.setItem(k, value)
      },
    })

    expect(journal.history.value.map(reading => reading.dateKey)).toEqual([legacy.dateKey])
    expect(() => journal.castToday()).toThrow('storage quota')
    expect(raw()).toBe(original)
    expect(storage.getItem(backupKey)).toBe(JSON.stringify([olderBackup]))
    expect(journal.history.value.map(reading => reading.dateKey)).toEqual([legacy.dateKey])

    fullKey = key
    expect(() => journal.castToday()).toThrow('storage quota')
    expect(raw()).toBe(original)
    expect(JSON.parse(storage.getItem(backupKey)!)).toEqual([olderBackup, original])

    fullKey = null
    journal.castToday()
    expect(JSON.parse(storage.getItem(backupKey)!)).toEqual([olderBackup, original])
    expect(openJournal().history.value.map(reading => reading.dateKey)).toEqual(['2026-07-12', legacy.dateKey])
  })

  it('reuses today and replaces only the active date after confirmation', () => {
    seed(legacy)
    let coin: 0 | 1 = 0
    const journal = openJournal(12, () => coin)
    const first = journal.castToday()
    const before = raw()
    const previousDay = journal.history.value[1]
    coin = 1
    expect(journal.castToday()).toEqual(first)
    expect(raw()).toBe(before)
    const replacement = journal.castToday(true)
    expect(replacement.lines).toEqual([9, 9, 9, 9, 9, 9])
    expect(openJournal().history.value).toEqual([replacement, previousDay])
    expect(openJournal(13).getTodayReading()).toBeNull()
  })

  it.each(['casting', 'saving'])('preserves the current reading if %s a replacement fails', stage => {
    seed(legacy)
    const before = raw()
    const fail = () => { throw new Error(stage + ' failed') }
    let flips = 0
    const journal = openJournal(11, () => stage === 'casting' && ++flips === 4 ? fail() : 0)
    const original = journal.history.value[0]
    if (stage === 'saving') stubStorage({ setItem: fail })
    expect(() => journal.castToday(true)).toThrow(stage + ' failed')
    expect(journal.history.value).toEqual([original])
    expect(raw()).toBe(before)
  })

  it('creates an absent journal without affecting tarot storage', () => {
    localStorage.setItem('mutsumi_tarot_daily', '{"card":1}')
    openJournal().castToday()
    expect(localStorage.getItem('mutsumi_tarot_daily')).toBe('{"card":1}')
  })

  it.each([
    ['invalid JSON', 'not json'],
    ['unsupported format', '{"version":99,"readings":[]}'],
    ['wrong record type', '{"version":1,"readings":{}}'],
    ['no recoverable entries', '{"version":1,"readings":[{"dateKey":5}]}'],
  ])('leaves stored data with %s untouched after a failed cast', (_label, original) => {
    localStorage.setItem(key, original)
    const journal = openJournal()
    expect(journal.history.value).toEqual([])
    expect(() => journal.castToday()).toThrow()
    expect(raw()).toBe(original)
  })

  it('blocks saving while reads fail and keeps older entries once reads recover', () => {
    seed(legacy)
    const before = raw()
    let readable = false
    const storage = stubStorage({
      getItem(k) {
        if (!readable) throw new Error('access denied')
        return storage.getItem(k)
      },
    })
    const journal = openJournal()
    expect(() => journal.castToday()).toThrow('access denied')
    expect(storage.getItem(key)).toBe(before)
    readable = true
    const newReading = journal.castToday()
    expect(summary(openJournal())).toEqual([[newReading.dateKey, newReading.fortuneLevel], ['2026-07-11', 'fortune']])
  })
})
