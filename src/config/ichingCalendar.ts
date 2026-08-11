import type { EarthlyBranch, HeavenlyStem } from './ichingFortune'

export interface DivinationTimeContext {
  createdAt: string
  timezone: string
  monthBranch: EarthlyBranch
  dayStem: HeavenlyStem
  dayBranch: EarthlyBranch
}

const STEMS: readonly HeavenlyStem[] = ['jia', 'yi', 'bing', 'ding', 'wu', 'ji', 'geng', 'xin', 'ren', 'gui']
const BRANCHES: readonly EarthlyBranch[] = ['zi', 'chou', 'yin', 'mao', 'chen', 'si', 'wu', 'wei', 'shen', 'you', 'xu', 'hai']
const TERM_MINUTES = [0, 21208, 42467, 63836, 85337, 107014, 128867, 150921, 173149, 195551, 218072, 240693, 263343, 285989, 308563, 331033, 353350, 375494, 397447, 419210, 440795, 462224, 483532, 504758] as const
const TERM_EPOCH = Date.UTC(1900, 0, 6, 2, 5)
const TROPICAL_YEAR_MS = 31556925974.7

export function getSolarTermInstant(year: number, termIndex: number): Date {
  if (year < 1900 || year > 2100 || termIndex < 0 || termIndex > 23) throw new Error('Solar-term date is outside the supported range')
  return new Date(TERM_EPOCH + (year - 1900) * TROPICAL_YEAR_MS + TERM_MINUTES[termIndex] * 60_000)
}

function zonedParts(date: Date, timezone: string): { year: number; month: number; day: number } {
  const parts = new Intl.DateTimeFormat('en-CA', { timeZone: timezone, year: 'numeric', month: '2-digit', day: '2-digit' }).formatToParts(date)
  const value = (type: string) => Number(parts.find(part => part.type === type)?.value)
  return { year: value('year'), month: value('month'), day: value('day') }
}

export function getMonthBranch(createdAt: string, timezone: string): EarthlyBranch {
  const instant = new Date(createdAt)
  const { year } = zonedParts(instant, timezone)
  const boundaries = [
    { at: getSolarTermInstant(year - 1, 22), branch: 'chou' }, { at: getSolarTermInstant(year, 0), branch: 'chou' },
    { at: getSolarTermInstant(year, 2), branch: 'yin' }, { at: getSolarTermInstant(year, 4), branch: 'mao' },
    { at: getSolarTermInstant(year, 6), branch: 'chen' }, { at: getSolarTermInstant(year, 8), branch: 'si' },
    { at: getSolarTermInstant(year, 10), branch: 'wu' }, { at: getSolarTermInstant(year, 12), branch: 'wei' },
    { at: getSolarTermInstant(year, 14), branch: 'shen' }, { at: getSolarTermInstant(year, 16), branch: 'you' },
    { at: getSolarTermInstant(year, 18), branch: 'xu' }, { at: getSolarTermInstant(year, 20), branch: 'hai' },
    { at: getSolarTermInstant(year, 22), branch: 'zi' },
  ] as const
  return [...boundaries].reverse().find(item => instant >= item.at)?.branch ?? 'zi'
}

export function createDivinationTimeContext(createdAt: string, timezone: string): DivinationTimeContext {
  const instant = new Date(createdAt)
  if (Number.isNaN(instant.getTime())) throw new Error('Invalid reading timestamp')
  const { year, month, day } = zonedParts(instant, timezone)
  const dayNumber = Math.floor(Date.UTC(year, month - 1, day) / 86_400_000)
  // 1970-01-01 was a Xin-Chou day (index 37).
  const cycleIndex = ((dayNumber + 37) % 60 + 60) % 60
  return { createdAt, timezone, monthBranch: getMonthBranch(createdAt, timezone), dayStem: STEMS[cycleIndex % 10], dayBranch: BRANCHES[cycleIndex % 12] }
}
