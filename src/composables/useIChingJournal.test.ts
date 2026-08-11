import { describe, it, expect, beforeEach, vi } from 'vitest'
import { localDateKey } from './useTarotJournal'
import {
  createIChingReading,
  type IChingLines,
  type RandomSource,
} from '../config/iching'

class SequenceRandom implements RandomSource {
  private index = 0
  private readonly values: readonly number[]

  constructor(values: readonly number[]) {
    this.values = values
  }

  nextInt(maxExclusive: number): number {
    const value = this.values[this.index++]
    if (value === undefined) throw new Error('No more random values')
    if (value < 0 || value >= maxExclusive) throw new Error('Out-of-range random value')
    return value
  }
}

function bitsFor(lines: IChingLines): number[] {
  return lines.flatMap(line => {
    if (line === 6) return [0, 0, 0]
    if (line === 7) return [0, 0, 1]
    if (line === 8) return [0, 1, 1]
    return [1, 1, 1]
  })
}

function randomFor(...readings: IChingLines[]): SequenceRandom {
  return new SequenceRandom(readings.flatMap((lines, index) => [...bitsFor(lines), index + 1, index + 2]))
}

function makeStorage(failWrites = false) {
  const store = new Map<string, string>()
  return {
    store,
    api: {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => {
        if (failWrites) throw new Error('quota')
        store.set(k, v)
      },
      removeItem: (k: string) => { store.delete(k) },
      clear: () => store.clear(),
    },
  }
}

async function loadJournal(randomSource = randomFor([7, 7, 7, 7, 7, 7]), now = () => new Date(2026, 6, 12, 9, 0)) {
  const mod = await import('./useIChingJournal')
  return { mod, journal: mod.useIChingJournal({ randomSource, now }) }
}

describe('useIChingJournal', () => {
  beforeEach(() => {
    vi.resetModules()
    const { api } = makeStorage()
    vi.stubGlobal('localStorage', api)
  })

  it('saves the first completed reading for a date and returns it on reopen', async () => {
    const { journal } = await loadJournal(randomFor([7, 7, 7, 7, 7, 7]))
    const reading = journal.createReading('2026-07-12')
    expect(reading.dateKey).toBe('2026-07-12')
    expect(journal.getReadingForDate('2026-07-12')).toEqual(reading)
  })

  it('reopening on the same date does not generate a new reading or mutate storage', async () => {
    const { journal } = await loadJournal(randomFor([7, 7, 7, 7, 7, 7], [6, 6, 6, 6, 6, 6]))
    const first = journal.createReading('2026-07-12')
    const second = journal.createReading('2026-07-12')
    expect(second).toEqual(first)
    expect(journal.getHistory()).toHaveLength(1)
  })

  it('rerolling replaces the existing result for the same date with a new ID', async () => {
    const { journal } = await loadJournal(randomFor([7, 7, 7, 7, 7, 7], [6, 6, 6, 6, 6, 6]))
    const oldReading = journal.createReading('2026-07-12')
    const newReading = journal.rerollReading('2026-07-12')
    expect(newReading.id).not.toBe(oldReading.id)
    expect(journal.getReadingForDate('2026-07-12')).toEqual(newReading)
    expect(journal.getHistory()).toHaveLength(1)
    expect(journal.getHistory().some(reading => reading.id === oldReading.id)).toBe(false)
  })

  it('rerolling one date does not modify another date', async () => {
    const { journal } = await loadJournal(randomFor(
      [7, 7, 7, 7, 7, 7],
      [8, 8, 8, 8, 8, 8],
      [6, 6, 6, 6, 6, 6],
    ))
    const firstDate = journal.createReading('2026-07-11')
    journal.createReading('2026-07-12')
    journal.rerollReading('2026-07-12')
    expect(journal.getReadingForDate('2026-07-11')).toEqual(firstDate)
    expect(journal.getHistory()).toHaveLength(2)
  })

  it('cancelling confirmation changes nothing because reroll is not called', async () => {
    const { journal } = await loadJournal(randomFor([7, 7, 7, 7, 7, 7], [6, 6, 6, 6, 6, 6]))
    const first = journal.createReading('2026-07-12')
    expect(journal.getReadingForDate('2026-07-12')).toEqual(first)
  })

  it('generation failure retains the previous result and saves no partial lines', async () => {
    const { journal } = await loadJournal(randomFor([7, 7, 7, 7, 7, 7]))
    const first = journal.createReading('2026-07-12')
    const failingRandom = new SequenceRandom([0, 0])
    const { journal: failingJournal } = await loadJournal(failingRandom)
    expect(() => failingJournal.rerollReading('2026-07-12')).toThrow()
    expect(failingJournal.getReadingForDate('2026-07-12')).toEqual(first)
    expect(failingJournal.getHistory()).toHaveLength(1)
  })

  it('persistence failure retains the previous result safely', async () => {
    const storage = makeStorage()
    vi.stubGlobal('localStorage', storage.api)
    vi.resetModules()
    const { journal } = await loadJournal(randomFor([7, 7, 7, 7, 7, 7]))
    const first = journal.createReading('2026-07-12')

    const failingStorage = makeStorage(true)
    failingStorage.store.set('mutsumi_iching_journal', storage.store.get('mutsumi_iching_journal')!)
    vi.stubGlobal('localStorage', failingStorage.api)
    vi.resetModules()
    const { journal: failingJournal } = await loadJournal(randomFor([6, 6, 6, 6, 6, 6]))
    expect(() => failingJournal.rerollReading('2026-07-12')).toThrow('quota')
    expect(failingJournal.journal.value.readings).toEqual([first])
  })

  it('prevents overlapping generation attempts', async () => {
    const { journal } = await loadJournal(randomFor([7, 7, 7, 7, 7, 7]))
    journal.isGenerating.value = true
    expect(() => journal.createReading('2026-07-12')).toThrow('already in progress')
    expect(journal.getHistory()).toHaveLength(0)
  })

  it('initializes missing, malformed, and unsupported storage as an empty journal', async () => {
    const { journal, mod } = await loadJournal()
    expect(journal.getHistory()).toEqual([])

    localStorage.setItem(mod.ICHING_JOURNAL_KEY, 'not json')
    expect(journal.getHistory()).toEqual([])

    localStorage.setItem(mod.ICHING_JOURNAL_KEY, JSON.stringify({ version: 99, readings: [] }))
    expect(journal.getHistory()).toEqual([])
  })

  it('normalizes duplicate dates by keeping the newest valid record', async () => {
    const { mod, journal } = await loadJournal()
    const oldReading = createIChingReading([7, 7, 7, 7, 7, 7], '2026-07-12', '2026-07-12T01:00:00.000Z', 'old')
    const newReading = createIChingReading([6, 6, 6, 6, 6, 6], '2026-07-12', '2026-07-12T02:00:00.000Z', 'new')
    localStorage.setItem(mod.ICHING_JOURNAL_KEY, JSON.stringify({ version: 1, readings: [oldReading, newReading] }))
    expect(journal.getHistory()).toEqual([newReading])
  })

  it('rejects invalid line values and repairs unknown hexagram IDs from valid lines', async () => {
    const { mod, journal } = await loadJournal()
    const badLines = { ...createIChingReading([7, 7, 7, 7, 7, 7], '2026-07-12', '2026-07-12T01:00:00.000Z', 'bad'), lines: [5, 7, 7, 7, 7, 7] }
    const repairable = { ...createIChingReading([6, 6, 6, 6, 6, 6], '2026-07-13', '2026-07-13T01:00:00.000Z', 'repair'), primaryHexagramId: 'unknown' }
    localStorage.setItem(mod.ICHING_JOURNAL_KEY, JSON.stringify({ version: 1, readings: [badLines, repairable] }))
    const history = journal.getHistory()
    expect(history).toHaveLength(1)
    expect(history[0].primaryHexagramId).toBe('hexagram02')
  })

  it('keeps tarot and I Ching storage keys independent', async () => {
    const { mod, journal } = await loadJournal(randomFor([7, 7, 7, 7, 7, 7]))
    localStorage.setItem('mutsumi_tarot_daily', JSON.stringify({ date: '2026-07-12', id: 1, reversed: false }))
    journal.createReading('2026-07-12')
    expect(localStorage.getItem('mutsumi_tarot_daily')).toContain('"id":1')
    expect(localStorage.getItem(mod.ICHING_JOURNAL_KEY)).toContain('hexagram01')
  })

  it('orders history newest first and uses tarot local date convention', async () => {
    const now = () => new Date(2026, 6, 12, 23, 30)
    const { journal } = await loadJournal(randomFor([7, 7, 7, 7, 7, 7], [8, 8, 8, 8, 8, 8]), now)
    journal.createReading('2026-07-11')
    journal.createReading()
    expect(localDateKey(now())).toBe('2026-07-12')
    expect(journal.getHistory().map(reading => reading.dateKey)).toEqual(['2026-07-12', '2026-07-11'])
  })
})
