const BRANCHES = ['zi', 'chou', 'yin', 'mao', 'chen', 'si', 'wu', 'wei', 'shen', 'you', 'xu', 'hai'] as const
export type EarthlyBranch = typeof BRANCHES[number]

export interface DivinationTimeContext {
  monthBranch: EarthlyBranch
  dayBranch: EarthlyBranch
}

const TERM_MINUTES = [0, 21208, 42467, 63836, 85337, 107014, 128867, 150921, 173149, 195551, 218072, 240693, 263343, 285989, 308563, 331033, 353350, 375494, 397447, 419210, 440795, 462224, 483532, 504758] as const
const TERM_EPOCH = Date.UTC(1900, 0, 6, 2, 5)
const TROPICAL_YEAR_MS = 31556925974.7

export function getSolarTermInstant(year: number, termIndex: number): Date {
  if (!Number.isInteger(year) || year < 1900 || year > 2100 ||
      !Number.isInteger(termIndex) || termIndex < 0 || termIndex >= TERM_MINUTES.length) {
    throw new Error('Solar-term date is outside the supported range')
  }
  return new Date(TERM_EPOCH + (year - 1900) * TROPICAL_YEAR_MS + TERM_MINUTES[termIndex] * 60_000)
}

export function createDivinationTimeContext(createdAt: string, timezone: string): DivinationTimeContext {
  const instant = new Date(createdAt)
  const parts = new Intl.DateTimeFormat('en-CA', {
    timeZone: timezone,
    year: 'numeric',
    month: 'numeric',
    day: 'numeric',
  }).formatToParts(instant)
  const value = (type: string) => Number(parts.find(part => part.type === type)?.value)
  const year = value('year')
  const dayNumber = Math.floor(Date.UTC(year, value('month') - 1, value('day')) / 86_400_000)
  // 1970-01-01 was a Chou day (index 37 in the sexagenary cycle).
  const dayIndex = ((dayNumber + 37) % 12 + 12) % 12

  // Before Xiao Han (term 0), the previous December's Zi month still applies.
  let monthBranch: EarthlyBranch = 'zi'
  for (let term = 22; term >= 0; term -= 2) {
    if (instant >= getSolarTermInstant(year, term)) {
      monthBranch = BRANCHES[(term / 2 + 1) % 12]
      break
    }
  }
  return { monthBranch, dayBranch: BRANCHES[dayIndex] }
}
