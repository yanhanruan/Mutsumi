import { shallowRef } from 'vue'
import { localDateKey } from './useTarotJournal'
import {
  analyzeIChingLines,
  FORTUNE_LEVELS,
  generateLines,
  isIChingLines,
  type CoinFlip,
  type FortuneLevel,
  type IChingCast,
  type IChingLines,
} from '../config/iching'
import { createDivinationTimeContext } from '../config/ichingCalendar'
import { evaluateDailyFortune } from '../config/ichingFortune'

interface ReadingData {
  dateKey: string
  createdAt: string
  lines: IChingLines
  fortuneLevel: FortuneLevel
}

export type IChingReading = ReadingData & IChingCast
export const ICHING_JOURNAL_KEY = 'mutsumi_iching_journal'
const JOURNAL_BACKUP_KEY = ICHING_JOURNAL_KEY + '_backup'

function isDateKey(value: unknown): value is string {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return false
  const date = new Date(`${value}T00:00:00Z`)
  return Number.isFinite(date.getTime()) && date.toISOString().slice(0, 10) === value
}

function isCreatedAt(value: unknown): value is string {
  if (typeof value !== 'string') return false
  const date = /^(\d{4}-\d{2}-\d{2})T\d{2}:\d{2}(?::\d{2}(?:\.\d+)?)?(?:Z|[+-]\d{2}:\d{2})$/.exec(value)?.[1]
  return isDateKey(date) && Number.isFinite(Date.parse(value))
}

function decodeReading(value: unknown): { reading: IChingReading; repaired: boolean } | null {
  if (!value || typeof value !== 'object') return null
  const { dateKey, createdAt, lines, fortuneLevel, timezone } =
    value as Partial<ReadingData & { timezone: string }>
  if (!isDateKey(dateKey)) return null
  if (!isCreatedAt(createdAt)) return null
  if (!isIChingLines(lines)) return null

  const cast = analyzeIChingLines(lines)
  let level: FortuneLevel = 'neutral'
  let repaired = false
  if (fortuneLevel && FORTUNE_LEVELS.includes(fortuneLevel)) {
    level = fortuneLevel
  } else {
    // Older records may lack a saved level. An invalid legacy timezone must
    // not make the underlying cast or the rest of the journal unreadable.
    repaired = fortuneLevel !== undefined || (timezone !== undefined && typeof timezone !== 'string')
    try {
      const time = createDivinationTimeContext(createdAt, typeof timezone === 'string' ? timezone : 'UTC')
      level = evaluateDailyFortune(cast, time).level
    } catch {
      repaired = true
    }
  }
  return { reading: { ...cast, dateKey, createdAt, fortuneLevel: level }, repaired }
}

function newestFirst(a: IChingReading, b: IChingReading): number {
  return b.dateKey.localeCompare(a.dateKey)
}

function readHistory(raw: string | null): { readings: IChingReading[]; needsBackup: boolean } {
  if (raw === null) return { readings: [], needsBackup: false }
  const stored = JSON.parse(raw) as { version?: number; readings?: unknown[] } | null
  if (stored?.version !== 1 || !Array.isArray(stored.readings)) {
    throw new Error('Unrecognized I Ching journal')
  }

  const byDate = new Map<string, IChingReading>()
  let needsBackup = false
  for (const value of stored.readings) {
    const decoded = decodeReading(value)
    if (!decoded) {
      needsBackup = true
      continue
    }
    const { reading, repaired } = decoded
    needsBackup ||= repaired
    const previous = byDate.get(reading.dateKey)
    if (!previous || Date.parse(reading.createdAt) >= Date.parse(previous.createdAt)) {
      byDate.set(reading.dateKey, reading)
    }
  }
  if (stored.readings.length && !byDate.size) {
    throw new Error('I Ching journal has no readable entries')
  }
  return { readings: [...byDate.values()].sort(newestFirst), needsBackup }
}

function backUpJournal(raw: string) {
  const stored = localStorage.getItem(JOURNAL_BACKUP_KEY)
  const backups: unknown = stored === null ? [] : JSON.parse(stored)
  if (!Array.isArray(backups) || backups.some(backup => typeof backup !== 'string')) {
    throw new Error('Unrecognized I Ching journal backup')
  }
  // A failed main write may leave this snapshot already backed up.
  if (!backups.includes(raw)) {
    localStorage.setItem(JOURNAL_BACKUP_KEY, JSON.stringify([...backups, raw]))
  }
}

export function useIChingJournal({
  flipCoin,
  now = () => new Date(),
}: { flipCoin?: CoinFlip; now?: () => Date } = {}) {
  const history = shallowRef<IChingReading[]>([])

  try {
    history.value = readHistory(localStorage.getItem(ICHING_JOURNAL_KEY)).readings
  } catch {
    // Keep the panel usable, but require a successful read before any write.
  }

  function getTodayReading(dateKey = localDateKey(now())): IChingReading | null {
    return history.value.find(reading => reading.dateKey === dateKey) ?? null
  }

  function castToday(replace = false): IChingReading {
    // Another open panel or a recovery may have changed storage since mount.
    const raw = localStorage.getItem(ICHING_JOURNAL_KEY)
    const { readings: latest, needsBackup } = readHistory(raw)
    const instant = now()
    const dateKey = localDateKey(instant)
    const existing = latest.find(reading => reading.dateKey === dateKey)
    if (existing && !replace) {
      history.value = latest
      return existing
    }

    const cast = analyzeIChingLines(generateLines(flipCoin))
    const createdAt = instant.toISOString()
    const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC'
    const time = createDivinationTimeContext(createdAt, timezone)
    const reading: IChingReading = {
      ...cast, dateKey, createdAt, fortuneLevel: evaluateDailyFortune(cast, time).level,
    }
    const next = [reading, ...latest.filter(item => item.dateKey !== dateKey)].sort(newestFirst)
    const readings = next.map(({ dateKey, createdAt, lines, fortuneLevel }): ReadingData =>
      ({ dateKey, createdAt, lines, fortuneLevel }))
    const updated = JSON.stringify({ version: 1, readings })
    if (needsBackup && raw !== null) backUpJournal(raw)
    if (localStorage.getItem(ICHING_JOURNAL_KEY) !== raw) {
      throw new Error('I Ching journal changed while preparing the save')
    }
    // Change the displayed history only after storage accepts the replacement.
    localStorage.setItem(ICHING_JOURNAL_KEY, updated)
    history.value = next
    return reading
  }

  return { history, getTodayReading, castToday }
}
