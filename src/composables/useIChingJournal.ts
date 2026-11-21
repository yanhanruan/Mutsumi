import { ref } from 'vue'
import { localDateKey } from './useTarotJournal'
import {
  createCryptoRandomSource,
  createIChingReading,
  createReadingId,
  findHexagramByPattern,
  generateLines,
  getChangedPattern,
  getMovingLineIndexes,
  getPrimaryPattern,
  isHexagramId,
  isIChingLines,
  validateIChingReading,
  type IChingReading,
  type RandomSource,
} from '../config/iching'
import { createDivinationTimeContext } from '../config/ichingCalendar'
import { evaluateDailyFortune } from '../config/ichingFortune'

export interface IChingJournalData {
  version: 1
  readings: IChingReading[]
}

interface UseIChingJournalOptions {
  randomSource?: RandomSource
  now?: () => Date
}

export const ICHING_JOURNAL_KEY = 'mutsumi_iching_journal'

function emptyJournal(): IChingJournalData {
  return { version: 1, readings: [] }
}

function readRawStorage(): string | null {
  try {
    return localStorage.getItem(ICHING_JOURNAL_KEY)
  } catch {
    return null
  }
}

function writeJournal(data: IChingJournalData): void {
  localStorage.setItem(ICHING_JOURNAL_KEY, JSON.stringify(data))
}

function dateMs(reading: IChingReading): number {
  return Date.parse(reading.createdAt)
}

function newestFirst(a: IChingReading, b: IChingReading): number {
  const byDate = b.dateKey.localeCompare(a.dateKey)
  if (byDate !== 0) return byDate
  return dateMs(b) - dateMs(a)
}

function repairReading(value: unknown): IChingReading | null {
  if (validateIChingReading(value)) return value
  if (!value || typeof value !== 'object') return null
  const reading = value as Partial<IChingReading>
  if (typeof reading.id !== 'string' || reading.id.trim() === '') return null
  if (typeof reading.dateKey !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(reading.dateKey)) return null
  if (typeof reading.createdAt !== 'string' || Number.isNaN(Date.parse(reading.createdAt))) return null
  if (!isIChingLines(reading.lines)) return null

  const timezone = typeof reading.timezone === 'string' && reading.timezone ? reading.timezone : 'UTC'
  const base = createIChingReading(reading.lines, reading.dateKey, reading.createdAt, reading.id)
  const evaluation = evaluateDailyFortune(base, createDivinationTimeContext(reading.createdAt, timezone))
  const repaired = createIChingReading(reading.lines, reading.dateKey, reading.createdAt, reading.id, {
    timezone, fortuneRuleVersion: 2, fortuneLevel: evaluation.level, fortuneScore: evaluation.score,
  })
  if (reading.primaryHexagramId && isHexagramId(reading.primaryHexagramId)) {
    const actualPrimary = findHexagramByPattern(getPrimaryPattern(reading.lines)).id
    if (reading.primaryHexagramId !== actualPrimary) return null
  }
  if (reading.changedHexagramId && isHexagramId(reading.changedHexagramId)) {
    const moving = getMovingLineIndexes(reading.lines)
    const actualChanged = moving.length > 0 ? findHexagramByPattern(getChangedPattern(reading.lines)).id : null
    if (reading.changedHexagramId !== actualChanged) return null
  }
  return repaired
}

export function normalizeIChingJournal(value: unknown): IChingJournalData {
  if (!value || typeof value !== 'object') return emptyJournal()
  const data = value as Partial<IChingJournalData>
  if (data.version !== 1 || !Array.isArray(data.readings)) return emptyJournal()

  const byDate = new Map<string, IChingReading>()
  for (const raw of data.readings) {
    const reading = repairReading(raw)
    if (!reading) continue
    const existing = byDate.get(reading.dateKey)
    if (!existing || dateMs(reading) >= dateMs(existing)) {
      byDate.set(reading.dateKey, reading)
    }
  }

  return { version: 1, readings: Array.from(byDate.values()).sort(newestFirst) }
}

function readJournal(): IChingJournalData {
  const raw = readRawStorage()
  if (!raw) return emptyJournal()
  try {
    return normalizeIChingJournal(JSON.parse(raw))
  } catch {
    return emptyJournal()
  }
}

function replaceReading(data: IChingJournalData, reading: IChingReading): IChingJournalData {
  return {
    version: 1,
    readings: [reading, ...data.readings.filter(item => item.dateKey !== reading.dateKey)].sort(newestFirst),
  }
}

const journal = ref<IChingJournalData>(readJournal())
const isGenerating = ref(false)

export function useIChingJournal(options: UseIChingJournalOptions = {}) {
  const randomSource = options.randomSource ?? createCryptoRandomSource()
  const now = options.now ?? (() => new Date())

  function refresh(): void {
    journal.value = readJournal()
  }

  function getReadingForDate(dateKey = localDateKey(now())): IChingReading | null {
    refresh()
    return journal.value.readings.find(reading => reading.dateKey === dateKey) ?? null
  }

  function getTodayReading(): IChingReading | null {
    return getReadingForDate(localDateKey(now()))
  }

  function getHistory(): IChingReading[] {
    refresh()
    return [...journal.value.readings]
  }

  function buildReading(dateKey: string): IChingReading {
    const completionTime = now()
    const createdAt = completionTime.toISOString()
    const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC'
    const lines = generateLines(randomSource)
    const id = createReadingId(randomSource, completionTime.getTime())
    const base = createIChingReading(lines, dateKey, createdAt, id)
    const evaluation = evaluateDailyFortune(base, createDivinationTimeContext(createdAt, timezone))
    const reading = createIChingReading(lines, dateKey, createdAt, id, {
      timezone, fortuneRuleVersion: 2, fortuneLevel: evaluation.level, fortuneScore: evaluation.score,
    })
    if (!validateIChingReading(reading)) throw new Error('Generated invalid I Ching reading')
    return reading
  }

  function persistReplacement(reading: IChingReading): IChingReading {
    const previous = journal.value
    const next = replaceReading(previous, reading)
    writeJournal(next)
    journal.value = next
    return reading
  }

  function createReading(dateKey = localDateKey(now())): IChingReading {
    if (isGenerating.value) throw new Error('I Ching generation already in progress')
    const existing = getReadingForDate(dateKey)
    if (existing) return existing

    isGenerating.value = true
    try {
      const reading = buildReading(dateKey)
      return persistReplacement(reading)
    } finally {
      isGenerating.value = false
    }
  }

  function rerollReading(dateKey = localDateKey(now())): IChingReading {
    if (isGenerating.value) throw new Error('I Ching generation already in progress')
    refresh()
    isGenerating.value = true
    try {
      const reading = buildReading(dateKey)
      return persistReplacement(reading)
    } finally {
      isGenerating.value = false
    }
  }

  return {
    journal,
    isGenerating,
    getReadingForDate,
    getTodayReading,
    getHistory,
    createReading,
    rerollReading,
    refresh,
  }
}
