import type { FortuneLevel, HexagramDefinition, IChingCast, PalacePosition, TrigramId } from './iching'
import type { DivinationTimeContext, EarthlyBranch } from './ichingCalendar'

type Element = 'wood' | 'fire' | 'earth' | 'metal' | 'water'

const ELEMENTS: Record<EarthlyBranch, Element> = {
  zi: 'water', chou: 'earth', yin: 'wood', mao: 'wood', chen: 'earth', si: 'fire',
  wu: 'fire', wei: 'earth', shen: 'metal', you: 'metal', xu: 'earth', hai: 'water',
}

const GENERATES: Record<Element, Element> = {
  wood: 'fire', fire: 'earth', earth: 'metal', metal: 'water', water: 'wood',
}
const CONTROLS: Record<Element, Element> = {
  wood: 'earth', earth: 'water', water: 'fire', fire: 'metal', metal: 'wood',
}
const OPPOSITE: Record<EarthlyBranch, EarthlyBranch> = {
  zi: 'wu', chou: 'wei', yin: 'shen', mao: 'you', chen: 'xu', si: 'hai',
  wu: 'zi', wei: 'chou', shen: 'yin', you: 'mao', xu: 'chen', hai: 'si',
}

const SHI: Record<PalacePosition, number> = {
  pure: 5, first: 0, second: 1, third: 2, fourth: 3, fifth: 4, wanderingSoul: 3, returningSoul: 2,
}

// Na Jia branches, bottom to top, first for the lower trigram then for the upper.
const NA_JIA: Record<TrigramId, readonly EarthlyBranch[]> = {
  qian: ['zi', 'yin', 'chen', 'wu', 'shen', 'xu'],
  dui: ['si', 'mao', 'chou', 'hai', 'you', 'wei'],
  li: ['mao', 'chou', 'hai', 'you', 'wei', 'si'],
  zhen: ['zi', 'yin', 'chen', 'wu', 'shen', 'xu'],
  xun: ['chou', 'hai', 'you', 'wei', 'si', 'mao'],
  kan: ['yin', 'chen', 'wu', 'shen', 'xu', 'zi'],
  gen: ['chen', 'wu', 'shen', 'xu', 'zi', 'yin'],
  kun: ['wei', 'si', 'mao', 'chou', 'hai', 'you'],
}

function branchAt(hexagram: HexagramDefinition, index: number): EarthlyBranch {
  const trigram = index < 3 ? hexagram.lowerTrigram : hexagram.upperTrigram
  return NA_JIA[trigram][index]
}

function influence(source: Element, target: Element): number {
  if (GENERATES[source] === target) return 2
  if (CONTROLS[source] === target) return -2
  return 0
}

function contextInfluence(branch: EarthlyBranch, shi: EarthlyBranch): number {
  if (branch === shi) return 2
  const source = ELEMENTS[branch]
  const target = ELEMENTS[shi]
  return source === target ? 1 : influence(source, target)
}

/** The existing simplified Liuyao rules, scored relative to the Shi line. */
export function evaluateDailyFortune(cast: IChingCast, time: DivinationTimeContext): {
  score: number
  level: FortuneLevel
} {
  const shiIndex = SHI[cast.primary.palacePosition]
  const shi = branchAt(cast.primary, shiIndex)
  const element = ELEMENTS[shi]
  let score = contextInfluence(time.monthBranch, shi) + contextInfluence(time.dayBranch, shi)
  if (OPPOSITE[time.monthBranch] === shi) score -= 2

  // Each kind of moving influence counts only once, regardless of the number of lines.
  const moving = cast.movingLineIndexes
    .filter(index => index !== shiIndex)
    .map(index => influence(ELEMENTS[branchAt(cast.primary, index)], element))
  if (moving.includes(2)) score += 2
  if (moving.includes(-2)) score -= 2

  if (cast.changed && cast.movingLineIndexes.includes(shiIndex)) {
    score += influence(ELEMENTS[branchAt(cast.changed, shiIndex)], element)
  }

  const level = score >= 4 ? 'greatFortune' : score >= 1 ? 'fortune' : score >= -1 ? 'neutral' : 'misfortune'
  return { score, level }
}
