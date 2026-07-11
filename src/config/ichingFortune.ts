import {
  findHexagramById,
  validateIChingReading,
  type HexagramId,
  type HexagramPalace,
  type IChingReading,
  type PalacePosition,
  type TrigramId,
} from './iching'
import type { DivinationTimeContext } from './ichingCalendar'

export type HeavenlyStem = 'jia'|'yi'|'bing'|'ding'|'wu'|'ji'|'geng'|'xin'|'ren'|'gui'
export type EarthlyBranch = 'zi'|'chou'|'yin'|'mao'|'chen'|'si'|'wu'|'wei'|'shen'|'you'|'xu'|'hai'
export type FiveElement = 'wood'|'fire'|'earth'|'metal'|'water'
export type SixRelation = 'siblings'|'parents'|'offspring'|'wealth'|'officials'
export type FortuneLevel = 'greatFortune'|'fortune'|'neutral'|'misfortune'
export type FortuneFactorSource = 'month'|'day'|'movingYuanShen'|'movingJiShen'|'shiChange'
type LineIndex = 0|1|2|3|4|5

export interface NaJiaLine {
  lineIndex: LineIndex
  heavenlyStem: HeavenlyStem
  earthlyBranch: EarthlyBranch
  element: FiveElement
  relation: SixRelation
  isShi: boolean
  isYing: boolean
  isMoving: boolean
}

export interface FortuneFactor {
  id: string
  score: number
  source: FortuneFactorSource
  i18nKey: string
  lineIndexes?: readonly LineIndex[]
}

export interface FortuneEvaluation {
  score: number
  level: FortuneLevel
  factors: FortuneFactor[]
}

const BRANCH_ELEMENTS: Record<EarthlyBranch, FiveElement> = { zi:'water',chou:'earth',yin:'wood',mao:'wood',chen:'earth',si:'fire',wu:'fire',wei:'earth',shen:'metal',you:'metal',xu:'earth',hai:'water' }
const PALACE_ELEMENTS: Record<HexagramPalace, FiveElement> = { qian:'metal',dui:'metal',li:'fire',zhen:'wood',xun:'wood',kan:'water',gen:'earth',kun:'earth' }
const GENERATES: Record<FiveElement, FiveElement> = { wood:'fire',fire:'earth',earth:'metal',metal:'water',water:'wood' }
const CONTROLS: Record<FiveElement, FiveElement> = { wood:'earth',earth:'water',water:'fire',fire:'metal',metal:'wood' }
const SHI: Record<PalacePosition, LineIndex> = { pure:5,first:0,second:1,third:2,fourth:3,fifth:4,wanderingSoul:3,returningSoul:2 }
const NA_JIA: Record<TrigramId, { lower: readonly [HeavenlyStem, EarthlyBranch, EarthlyBranch, EarthlyBranch]; upper: readonly [HeavenlyStem, EarthlyBranch, EarthlyBranch, EarthlyBranch] }> = {
  qian:{lower:['jia','zi','yin','chen'],upper:['ren','wu','shen','xu']}, dui:{lower:['ding','si','mao','chou'],upper:['ding','hai','you','wei']},
  li:{lower:['ji','mao','chou','hai'],upper:['ji','you','wei','si']}, zhen:{lower:['geng','zi','yin','chen'],upper:['geng','wu','shen','xu']},
  xun:{lower:['xin','chou','hai','you'],upper:['xin','wei','si','mao']}, kan:{lower:['wu','yin','chen','wu'],upper:['wu','shen','xu','zi']},
  gen:{lower:['bing','chen','wu','shen'],upper:['bing','xu','zi','yin']}, kun:{lower:['yi','wei','si','mao'],upper:['gui','chou','hai','you']},
}

export const getBranchElement = (branch: EarthlyBranch): FiveElement => BRANCH_ELEMENTS[branch]
export const doesGenerate = (source: FiveElement, target: FiveElement): boolean => GENERATES[source] === target
export const doesControl = (source: FiveElement, target: FiveElement): boolean => CONTROLS[source] === target
export const getGeneratingElement = (target: FiveElement): FiveElement => (Object.keys(GENERATES) as FiveElement[]).find(key => GENERATES[key] === target)!
export const getControllingElement = (target: FiveElement): FiveElement => (Object.keys(CONTROLS) as FiveElement[]).find(key => CONTROLS[key] === target)!
export const isYuanShen = (line: FiveElement, yong: FiveElement): boolean => doesGenerate(line, yong)
export const isJiShen = (line: FiveElement, yong: FiveElement): boolean => doesControl(line, yong)

export function areOpposedBranches(a: EarthlyBranch, b: EarthlyBranch): boolean {
  return ([['zi','wu'],['chou','wei'],['yin','shen'],['mao','you'],['chen','xu'],['si','hai']] as const)
    .some(([left,right]) => (a===left&&b===right)||(a===right&&b===left))
}

export function getShiLineIndex(id: HexagramId): LineIndex { return SHI[findHexagramById(id).palacePosition] }
export function getYingLineIndex(id: HexagramId): LineIndex { return ((getShiLineIndex(id) + 3) % 6) as LineIndex }

function relation(line: FiveElement, palace: FiveElement): SixRelation {
  if (line===palace) return 'siblings'
  if (doesGenerate(line,palace)) return 'parents'
  if (doesGenerate(palace,line)) return 'offspring'
  if (doesControl(palace,line)) return 'wealth'
  return 'officials'
}

export function buildNaJiaChart(id: HexagramId, moving: readonly number[]): readonly NaJiaLine[] {
  const hex = findHexagramById(id)
  const shi = getShiLineIndex(id)
  const ying = getYingLineIndex(id)
  const palace = PALACE_ELEMENTS[hex.palace]
  return Array.from({length:6}, (_, raw) => {
    const lineIndex=raw as LineIndex
    const half=raw<3 ? NA_JIA[hex.lowerTrigram].lower : NA_JIA[hex.upperTrigram].upper
    const branch=half[(raw%3)+1] as EarthlyBranch
    const element=getBranchElement(branch)
    return {lineIndex,heavenlyStem:half[0],earthlyBranch:branch,element,relation:relation(element,palace),isShi:lineIndex===shi,isYing:lineIndex===ying,isMoving:moving.includes(lineIndex)}
  })
}

export function evaluateContextInfluence(
  contextBranch: EarthlyBranch,
  shiBranch: EarthlyBranch,
  shiElement: FiveElement,
  source: 'month' | 'day',
): FortuneFactor[] {
  const factors: FortuneFactor[] = []
  const contextElement = getBranchElement(contextBranch)
  let id = ''
  let score = 0

  if (contextBranch === shiBranch) { id = 'same'; score = 2 }
  else if (doesGenerate(contextElement, shiElement)) { id = 'support'; score = 2 }
  else if (doesControl(contextElement, shiElement)) { id = 'restrain'; score = -2 }
  else if (contextElement === shiElement) { id = 'sameElement'; score = 1 }

  if (id) {
    factors.push({ id: `${source}-${id}`, score, source, i18nKey: `fortuneFactors.${source}${id[0].toUpperCase()}${id.slice(1)}` })
  }

  if (areOpposedBranches(contextBranch, shiBranch)) {
    factors.push({ id: `${source}-opposition`, score: source === 'month' ? -2 : 0, source, i18nKey: `fortuneFactors.${source}Opposition` })
  }
  return factors
}

export function getFortuneLevel(score:number): FortuneLevel {
  if(score>=4) return 'greatFortune'
  if(score>=1) return 'fortune'
  if(score>=-1) return 'neutral'
  return 'misfortune'
}

/** A simplified heuristic model inspired by traditional Liuyao relationships. */
export function evaluateDailyFortune(reading:IChingReading,time:DivinationTimeContext):FortuneEvaluation {
  if(!validateIChingReading(reading)) throw new Error('Invalid I Ching reading')
  const chart=buildNaJiaChart(reading.primaryHexagramId,reading.movingLineIndexes)
  const shi=chart.find(line=>line.isShi)!
  const factors: FortuneFactor[] = [
    ...evaluateContextInfluence(time.monthBranch,shi.earthlyBranch,shi.element,'month'),
    ...evaluateContextInfluence(time.dayBranch,shi.earthlyBranch,shi.element,'day'),
  ]
  const active=chart.filter(line=>line.isMoving&&!line.isShi)
  const yuan=active.filter(line=>isYuanShen(line.element,shi.element))
  const ji=active.filter(line=>isJiShen(line.element,shi.element))
  if(yuan.length) factors.push({id:'moving-yuan',score:2,source:'movingYuanShen',i18nKey:'fortuneFactors.movingYuan',lineIndexes:yuan.map(line=>line.lineIndex)})
  if(ji.length) factors.push({id:'moving-ji',score:-2,source:'movingJiShen',i18nKey:'fortuneFactors.movingJi',lineIndexes:ji.map(line=>line.lineIndex)})

  if(shi.isMoving&&reading.changedHexagramId) {
    const changed=buildNaJiaChart(reading.changedHexagramId,[])[shi.lineIndex]
    if(doesGenerate(changed.element,shi.element)) factors.push({id:'shi-change-support',score:2,source:'shiChange',i18nKey:'fortuneFactors.shiChangeSupport',lineIndexes:[shi.lineIndex]})
    else if(doesControl(changed.element,shi.element)) factors.push({id:'shi-change-restraint',score:-2,source:'shiChange',i18nKey:'fortuneFactors.shiChangeRestraint',lineIndexes:[shi.lineIndex]})
  }

  if (new Set(factors.map(factor => factor.id)).size !== factors.length) throw new Error('Duplicate fortune factor IDs')
  const score=factors.reduce((sum,factor)=>sum+factor.score,0)
  return {score,level:getFortuneLevel(score),factors}
}
