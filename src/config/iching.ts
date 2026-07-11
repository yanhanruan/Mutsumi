export type LineValue = 6 | 7 | 8 | 9
export type CoinSide = 'front' | 'back'

export type TrigramId =
  | 'qian'
  | 'dui'
  | 'li'
  | 'zhen'
  | 'xun'
  | 'kan'
  | 'gen'
  | 'kun'

export type HexagramPalace =
  | 'qian'
  | 'dui'
  | 'li'
  | 'zhen'
  | 'xun'
  | 'kan'
  | 'gen'
  | 'kun'

export type PalacePosition =
  | 'pure'
  | 'first'
  | 'second'
  | 'third'
  | 'fourth'
  | 'fifth'
  | 'wanderingSoul'
  | 'returningSoul'

export type HexagramId =
  | 'hexagram01' | 'hexagram02' | 'hexagram03' | 'hexagram04'
  | 'hexagram05' | 'hexagram06' | 'hexagram07' | 'hexagram08'
  | 'hexagram09' | 'hexagram10' | 'hexagram11' | 'hexagram12'
  | 'hexagram13' | 'hexagram14' | 'hexagram15' | 'hexagram16'
  | 'hexagram17' | 'hexagram18' | 'hexagram19' | 'hexagram20'
  | 'hexagram21' | 'hexagram22' | 'hexagram23' | 'hexagram24'
  | 'hexagram25' | 'hexagram26' | 'hexagram27' | 'hexagram28'
  | 'hexagram29' | 'hexagram30' | 'hexagram31' | 'hexagram32'
  | 'hexagram33' | 'hexagram34' | 'hexagram35' | 'hexagram36'
  | 'hexagram37' | 'hexagram38' | 'hexagram39' | 'hexagram40'
  | 'hexagram41' | 'hexagram42' | 'hexagram43' | 'hexagram44'
  | 'hexagram45' | 'hexagram46' | 'hexagram47' | 'hexagram48'
  | 'hexagram49' | 'hexagram50' | 'hexagram51' | 'hexagram52'
  | 'hexagram53' | 'hexagram54' | 'hexagram55' | 'hexagram56'
  | 'hexagram57' | 'hexagram58' | 'hexagram59' | 'hexagram60'
  | 'hexagram61' | 'hexagram62' | 'hexagram63' | 'hexagram64'

export type HexagramLinePattern = readonly [
  boolean,
  boolean,
  boolean,
  boolean,
  boolean,
  boolean,
]

export type IChingLines = readonly [
  LineValue,
  LineValue,
  LineValue,
  LineValue,
  LineValue,
  LineValue,
]

export interface RandomSource {
  nextInt(maxExclusive: number): number
}

export interface TrigramDefinition {
  id: TrigramId
  lines: readonly [boolean, boolean, boolean]
}

export interface HexagramDefinition {
  id: HexagramId
  kingWenNumber: number
  upperTrigram: TrigramId
  lowerTrigram: TrigramId
  lines: HexagramLinePattern
  palace: HexagramPalace
  palacePosition: PalacePosition
}

export interface HexagramPalaceInfo {
  palace: HexagramPalace
  position: PalacePosition
}

export interface IChingReading {
  id: string
  dateKey: string
  createdAt: string
  lines: IChingLines
  primaryHexagramId: HexagramId
  changedHexagramId: HexagramId | null
  movingLineIndexes: number[]
  timezone: string
  fortuneRuleVersion: 1 | 2
  fortuneLevel: 'greatFortune' | 'fortune' | 'neutral' | 'misfortune'
  fortuneScore: number
}

export interface IChingHexagramLocale {
  name: string
  shortName: string
  subtitle: string
  judgment: string
  reflection: string
  lines: readonly [string, string, string, string, string, string]
  commentary?: string
  imageText?: string
  lineCommentaries?: readonly [string, string, string, string, string, string]
}

export interface IChingLocale {
  title: string
  intro: string
  quietPrompt: string
  begin: string
  progress: string
  todaysReading: string
  primaryHexagram: string
  changedHexagram: string
  movingLine: string
  movingLines: string
  noMovingLines: string
  judgment: string
  reflection: string
  commentary: string
  imageText: string
  allLines: string
  upperTrigram: string
  lowerTrigram: string
  trigramNames: Record<TrigramId, string>
  palaceNames: Record<HexagramPalace, string>
  palacePositions: Record<PalacePosition, string>
  details: string
  collapseDetails: string
  history: string
  emptyHistory: string
  back: string
  close: string
  cancel: string
  generateAnother: string
  rerollTitle: string
  rerollDescription: string
  confirmReroll: string
  keepCurrent: string
  replacementNotice: string
  noChangeExplanation: string
  transitionExplanation: string
  disclaimer: string
  storageRecovered: string
  generationError: string
  generationBusy: string
  coinsAria: string
  movingLineAria: string
  linePositions: readonly [string, string, string, string, string, string]
  fortune: {
    title: string
    basis: string
    disclaimer: string
    levels: Record<IChingReading['fortuneLevel'], string>
    summaries: Record<IChingReading['fortuneLevel'], string>
    factors: Record<string, string>
  }
  hexagrams: Record<HexagramId, IChingHexagramLocale>
}

export const ICHING_WINDOW_DIMS: Record<'small' | 'medium' | 'large', [number, number]> = {
  small: [310, 470],
  medium: [352, 530],
  large: [394, 590],
}

export const TRIGRAMS: readonly TrigramDefinition[] = [
  { id: 'qian', lines: [true, true, true] },
  { id: 'dui', lines: [true, true, false] },
  { id: 'li', lines: [true, false, true] },
  { id: 'zhen', lines: [true, false, false] },
  { id: 'xun', lines: [false, true, true] },
  { id: 'kan', lines: [false, true, false] },
  { id: 'gen', lines: [false, false, true] },
  { id: 'kun', lines: [false, false, false] },
]

const TRIGRAM_BY_ID = new Map<TrigramId, TrigramDefinition>(TRIGRAMS.map(t => [t.id, t]))

const hexagramIds = Array.from({ length: 64 }, (_, i) =>
  `hexagram${String(i + 1).padStart(2, '0')}` as HexagramId)

const KING_WEN_TRIGRAMS: readonly (readonly [TrigramId, TrigramId])[] = [
  ['qian', 'qian'], ['kun', 'kun'], ['kan', 'zhen'], ['gen', 'kan'],
  ['kan', 'qian'], ['qian', 'kan'], ['kun', 'kan'], ['kan', 'kun'],
  ['xun', 'qian'], ['qian', 'dui'], ['kun', 'qian'], ['qian', 'kun'],
  ['qian', 'li'], ['li', 'qian'], ['kun', 'gen'], ['zhen', 'kun'],
  ['dui', 'zhen'], ['gen', 'xun'], ['kun', 'dui'], ['xun', 'kun'],
  ['li', 'zhen'], ['gen', 'li'], ['gen', 'kun'], ['kun', 'zhen'],
  ['qian', 'zhen'], ['gen', 'qian'], ['gen', 'zhen'], ['dui', 'xun'],
  ['kan', 'kan'], ['li', 'li'], ['dui', 'gen'], ['zhen', 'xun'],
  ['qian', 'gen'], ['zhen', 'qian'], ['li', 'kun'], ['kun', 'li'],
  ['xun', 'li'], ['li', 'dui'], ['kan', 'gen'], ['zhen', 'kan'],
  ['gen', 'dui'], ['xun', 'zhen'], ['dui', 'qian'], ['qian', 'xun'],
  ['dui', 'kun'], ['kun', 'xun'], ['dui', 'kan'], ['kan', 'xun'],
  ['dui', 'li'], ['li', 'xun'], ['zhen', 'zhen'], ['gen', 'gen'],
  ['xun', 'gen'], ['zhen', 'dui'], ['zhen', 'li'], ['li', 'gen'],
  ['xun', 'xun'], ['dui', 'dui'], ['xun', 'kan'], ['kan', 'dui'],
  ['xun', 'dui'], ['zhen', 'gen'], ['kan', 'li'], ['li', 'kan'],
]

export const HEXAGRAM_PALACES: readonly HexagramPalace[] = [
  'qian', 'dui', 'li', 'zhen', 'xun', 'kan', 'gen', 'kun',
]

export const PALACE_POSITIONS: readonly PalacePosition[] = [
  'pure', 'first', 'second', 'third', 'fourth', 'fifth', 'wanderingSoul', 'returningSoul',
]

const EIGHT_PALACE_KING_WEN_NUMBERS: Record<HexagramPalace, readonly [number, number, number, number, number, number, number, number]> = {
  qian: [1, 44, 33, 12, 20, 23, 35, 14],
  dui: [58, 47, 45, 31, 39, 15, 62, 54],
  li: [30, 56, 50, 64, 4, 59, 6, 13],
  zhen: [51, 16, 40, 32, 46, 48, 28, 17],
  xun: [57, 9, 37, 42, 25, 21, 27, 18],
  kan: [29, 60, 3, 63, 49, 55, 36, 7],
  gen: [52, 22, 26, 41, 38, 10, 61, 53],
  kun: [2, 24, 19, 11, 34, 43, 5, 8],
}

const PALACE_INFO_BY_KING_WEN_NUMBER = new Map<number, HexagramPalaceInfo>()
for (const palace of HEXAGRAM_PALACES) {
  EIGHT_PALACE_KING_WEN_NUMBERS[palace].forEach((kingWenNumber, index) => {
    PALACE_INFO_BY_KING_WEN_NUMBER.set(kingWenNumber, {
      palace,
      position: PALACE_POSITIONS[index],
    })
  })
}

function trigramLines(id: TrigramId): readonly [boolean, boolean, boolean] {
  const trigram = TRIGRAM_BY_ID.get(id)
  if (!trigram) throw new Error(`Unknown trigram: ${id}`)
  return trigram.lines
}

export const HEXAGRAMS: readonly HexagramDefinition[] = KING_WEN_TRIGRAMS.map(([upperTrigram, lowerTrigram], index) => {
  const lower = trigramLines(lowerTrigram)
  const upper = trigramLines(upperTrigram)
  const kingWenNumber = index + 1
  const palaceInfo = PALACE_INFO_BY_KING_WEN_NUMBER.get(kingWenNumber)
  if (!palaceInfo) throw new Error(`Missing palace classification for King Wen ${kingWenNumber}`)
  return {
    id: hexagramIds[index],
    kingWenNumber,
    upperTrigram,
    lowerTrigram,
    lines: [lower[0], lower[1], lower[2], upper[0], upper[1], upper[2]],
    palace: palaceInfo.palace,
    palacePosition: palaceInfo.position,
  }
})

export function patternKey(pattern: HexagramLinePattern): string {
  // Pattern keys are encoded bottom-to-top; index 0 is the first/bottom line.
  return pattern.map(line => (line ? '1' : '0')).join('')
}

const HEXAGRAM_BY_PATTERN = new Map<string, HexagramDefinition>(
  HEXAGRAMS.map(hexagram => [patternKey(hexagram.lines), hexagram]),
)

const HEXAGRAM_BY_ID = new Map<HexagramId, HexagramDefinition>(
  HEXAGRAMS.map(hexagram => [hexagram.id, hexagram]),
)

export function findHexagramByPattern(pattern: HexagramLinePattern): HexagramDefinition {
  const hexagram = HEXAGRAM_BY_PATTERN.get(patternKey(pattern))
  if (!hexagram) throw new Error(`Unknown hexagram pattern: ${patternKey(pattern)}`)
  return hexagram
}

export function findHexagramById(id: HexagramId): HexagramDefinition {
  const hexagram = HEXAGRAM_BY_ID.get(id)
  if (!hexagram) throw new Error(`Unknown hexagram id: ${id}`)
  return hexagram
}

export function getHexagramPalaceInfo(hexagramId: HexagramId): HexagramPalaceInfo {
  const definition = findHexagramById(hexagramId)
  return {
    palace: definition.palace,
    position: definition.palacePosition,
  }
}

export function isPrimaryYang(value: LineValue): boolean {
  return value === 7 || value === 9
}

export function isChangedYang(value: LineValue): boolean {
  return value === 6 || value === 7
}

export function isMovingLine(value: LineValue): boolean {
  return value === 6 || value === 9
}

export function getCoinSidesForLine(value: LineValue): readonly [CoinSide, CoinSide, CoinSide] {
  const backCount = value - 6
  return Array.from({ length: 3 }, (_, index) => index < backCount ? 'back' : 'front') as [CoinSide, CoinSide, CoinSide]
}

export function getPrimaryPattern(lines: IChingLines): HexagramLinePattern {
  return lines.map(isPrimaryYang) as unknown as HexagramLinePattern
}

export function getChangedPattern(lines: IChingLines): HexagramLinePattern {
  return lines.map(isChangedYang) as unknown as HexagramLinePattern
}

export function getMovingLineIndexes(lines: IChingLines): number[] {
  return lines.flatMap((line, index) => isMovingLine(line) ? [index] : [])
}

export function generateLine(randomSource: RandomSource): LineValue {
  const total = (2 + randomSource.nextInt(2)) + (2 + randomSource.nextInt(2)) + (2 + randomSource.nextInt(2))
  if (total !== 6 && total !== 7 && total !== 8 && total !== 9) {
    throw new Error(`Invalid generated line total: ${total}`)
  }
  return total
}

export function generateLines(randomSource: RandomSource): IChingLines {
  // Generated and persisted in bottom-to-top order: index 0 is the bottom line.
  return [
    generateLine(randomSource),
    generateLine(randomSource),
    generateLine(randomSource),
    generateLine(randomSource),
    generateLine(randomSource),
    generateLine(randomSource),
  ]
}

export function createCryptoRandomSource(): RandomSource {
  return {
    nextInt(maxExclusive: number): number {
      if (!Number.isInteger(maxExclusive) || maxExclusive <= 0 || maxExclusive > 0xffffffff) {
        throw new Error(`Invalid maxExclusive: ${maxExclusive}`)
      }
      const cryptoApi = globalThis.crypto
      if (!cryptoApi?.getRandomValues) {
        throw new Error('Web Crypto is not available')
      }
      const range = 0x100000000
      const limit = range - (range % maxExclusive)
      const buffer = new Uint32Array(1)
      do {
        cryptoApi.getRandomValues(buffer)
      } while (buffer[0] >= limit)
      return buffer[0] % maxExclusive
    },
  }
}

export function createIChingReading(
  lines: IChingLines,
  dateKey: string,
  createdAt: string,
  id: string,
  fortune?: Pick<IChingReading, 'timezone' | 'fortuneRuleVersion' | 'fortuneLevel' | 'fortuneScore'>,
): IChingReading {
  const primary = findHexagramByPattern(getPrimaryPattern(lines))
  const movingLineIndexes = getMovingLineIndexes(lines)
  const changed = movingLineIndexes.length > 0
    ? findHexagramByPattern(getChangedPattern(lines))
    : null

  return {
    id,
    dateKey,
    createdAt,
    lines,
    primaryHexagramId: primary.id,
    changedHexagramId: changed?.id ?? null,
    movingLineIndexes,
    timezone: fortune?.timezone ?? 'UTC',
    fortuneRuleVersion: fortune?.fortuneRuleVersion ?? 2,
    fortuneLevel: fortune?.fortuneLevel ?? 'neutral',
    fortuneScore: fortune?.fortuneScore ?? 0,
  }
}

export function createReadingId(randomSource: RandomSource, now = Date.now()): string {
  const chunk = () => randomSource.nextInt(0x10000).toString(16).padStart(4, '0')
  return `iching-${now.toString(36)}-${chunk()}${chunk()}`
}

export function generateIChingReading(
  randomSource: RandomSource,
  dateKey: string,
  createdAt = new Date().toISOString(),
): IChingReading {
  const lines = generateLines(randomSource)
  return createIChingReading(lines, dateKey, createdAt, createReadingId(randomSource))
}

export function isLineValue(value: unknown): value is LineValue {
  return value === 6 || value === 7 || value === 8 || value === 9
}

export function isHexagramId(value: unknown): value is HexagramId {
  return typeof value === 'string' && HEXAGRAM_BY_ID.has(value as HexagramId)
}

export function isIChingLines(value: unknown): value is IChingLines {
  return Array.isArray(value) && value.length === 6 && value.every(isLineValue)
}

export function validateIChingReading(value: unknown): value is IChingReading {
  if (!value || typeof value !== 'object') return false
  const reading = value as Partial<IChingReading>
  if (typeof reading.id !== 'string' || reading.id.trim() === '') return false
  if (typeof reading.dateKey !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(reading.dateKey)) return false
  if (typeof reading.createdAt !== 'string' || Number.isNaN(Date.parse(reading.createdAt))) return false
  if (!isIChingLines(reading.lines)) return false
  if (!isHexagramId(reading.primaryHexagramId)) return false
  if (reading.changedHexagramId !== null && !isHexagramId(reading.changedHexagramId)) return false
  if (!Array.isArray(reading.movingLineIndexes)) return false
  if (!reading.movingLineIndexes.every(index => Number.isInteger(index) && index >= 0 && index < 6)) return false
  if (typeof reading.timezone !== 'string' || reading.timezone.length === 0) return false
  if ((reading.fortuneRuleVersion !== 1 && reading.fortuneRuleVersion !== 2) || !['greatFortune', 'fortune', 'neutral', 'misfortune'].includes(reading.fortuneLevel ?? '')) return false
  if (typeof reading.fortuneScore !== 'number' || !Number.isFinite(reading.fortuneScore)) return false

  const recalculated = createIChingReading(reading.lines, reading.dateKey, reading.createdAt, reading.id)
  return reading.primaryHexagramId === recalculated.primaryHexagramId &&
    reading.changedHexagramId === recalculated.changedHexagramId &&
    JSON.stringify(reading.movingLineIndexes) === JSON.stringify(recalculated.movingLineIndexes)
}
