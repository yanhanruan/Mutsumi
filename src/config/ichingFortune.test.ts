import { describe, expect, it } from 'vitest'
import { HEXAGRAMS, createIChingReading } from './iching'
import { createDivinationTimeContext, getMonthBranch, getSolarTermInstant } from './ichingCalendar'
import {
  areOpposedBranches, buildNaJiaChart, doesControl, doesGenerate, evaluateDailyFortune,
  evaluateContextInfluence,
  getBranchElement, getControllingElement, getFortuneLevel, getGeneratingElement,
  getShiLineIndex, getYingLineIndex, isJiShen, isYuanShen,
  type EarthlyBranch, type FiveElement,
} from './ichingFortune'

const elements: readonly FiveElement[] = ['wood', 'fire', 'earth', 'metal', 'water']
const branches: readonly EarthlyBranch[] = ['zi','chou','yin','mao','chen','si','wu','wei','shen','you','xu','hai']

describe('five-element and branch helpers', () => {
  it('implements every generation and control relationship and inverse lookup', () => {
    const generation = [['wood','fire'],['fire','earth'],['earth','metal'],['metal','water'],['water','wood']] as const
    const control = [['wood','earth'],['earth','water'],['water','fire'],['fire','metal'],['metal','wood']] as const
    for (const [source, target] of generation) { expect(doesGenerate(source,target)).toBe(true); expect(getGeneratingElement(target)).toBe(source) }
    for (const [source, target] of control) { expect(doesControl(source,target)).toBe(true); expect(getControllingElement(target)).toBe(source) }
    expect(elements.every(element => !doesGenerate(element, element) && !doesControl(element, element))).toBe(true)
  })

  it('maps branches to elements and only recognizes the six opposition pairs', () => {
    expect(branches.map(getBranchElement)).toEqual(['water','earth','wood','wood','earth','fire','fire','earth','metal','metal','earth','water'])
    for (const [a,b] of [['zi','wu'],['chou','wei'],['yin','shen'],['mao','you'],['chen','xu'],['si','hai']] as const) {
      expect(areOpposedBranches(a,b)).toBe(true); expect(areOpposedBranches(b,a)).toBe(true)
    }
    expect(areOpposedBranches('zi','chou')).toBe(false)
  })
})

describe('Shi/Ying and Na Jia', () => {
  it('resolves one distinct valid Shi and Ying line for all hexagrams', () => {
    for (const hexagram of HEXAGRAMS) {
      const shi=getShiLineIndex(hexagram.id), ying=getYingLineIndex(hexagram.id)
      expect(shi).toBeGreaterThanOrEqual(0); expect(shi).toBeLessThan(6); expect(ying).not.toBe(shi)
    }
    expect(getShiLineIndex('hexagram01')).toBe(5)
    expect(getShiLineIndex('hexagram44')).toBe(0)
    expect(getShiLineIndex('hexagram35')).toBe(3)
    expect(getShiLineIndex('hexagram14')).toBe(2)
  })

  it('builds canonical six-line Na Jia charts bottom-to-top', () => {
    for (const hexagram of HEXAGRAMS) {
      const chart=buildNaJiaChart(hexagram.id,[0,5]); expect(chart).toHaveLength(6)
      expect(chart.map(line=>line.lineIndex)).toEqual([0,1,2,3,4,5])
      expect(chart.filter(line=>line.isShi)).toHaveLength(1); expect(chart.filter(line=>line.isYing)).toHaveLength(1)
      expect(chart.every(line=>elements.includes(line.element)&&branches.includes(line.earthlyBranch))).toBe(true)
    }
    expect(buildNaJiaChart('hexagram01',[0,5]).map(line=>[line.heavenlyStem,line.earthlyBranch])).toEqual([
      ['jia','zi'],['jia','yin'],['jia','chen'],['ren','wu'],['ren','shen'],['ren','xu'],
    ])
  })
})

describe('time context and evaluation', () => {
  it('derives deterministic sexagenary days and switches month at solar-term boundaries', () => {
    const context=createDivinationTimeContext('1970-01-01T12:00:00Z','UTC')
    expect([context.dayStem,context.dayBranch]).toEqual(['xin','chou'])
    const liChun=getSolarTermInstant(2026,2)
    expect(getMonthBranch(new Date(liChun.getTime()-1).toISOString(),'Asia/Shanghai')).toBe('chou')
    expect(getMonthBranch(liChun.toISOString(),'Asia/Shanghai')).toBe('yin')
  })

  it('uses Shi as Yong Shen, caps moving influences, returns factors, and maps levels', () => {
    expect(isYuanShen('water','wood')).toBe(true); expect(isJiShen('metal','wood')).toBe(true)
    let reading: ReturnType<typeof createIChingReading> | undefined
    for (let mask=0; mask<64 && !reading; mask++) {
      const lines=Array.from({length:6},(_,index)=>(mask&(1<<index))?9:6) as [6|9,6|9,6|9,6|9,6|9,6|9]
      const candidate=createIChingReading(lines,'2026-07-12','2026-07-12T12:00:00Z',`score-${mask}`)
      const chart=buildNaJiaChart(candidate.primaryHexagramId,candidate.movingLineIndexes), shi=chart.find(line=>line.isShi)!
      if (chart.filter(line=>line.isMoving&&!line.isShi&&isYuanShen(line.element,shi.element)).length>=2) reading=candidate
    }
    expect(reading).toBeDefined()
    const selected=reading!
    const result=evaluateDailyFortune(selected,{createdAt:selected.createdAt,timezone:'UTC',monthBranch:'zi',dayStem:'jia',dayBranch:'chou'})
    expect(result.factors.length).toBeGreaterThan(0)
    expect(result.factors.filter(f=>f.id==='moving-yuan')).toHaveLength(1)
    expect(result.factors.filter(f=>f.id==='moving-ji').length).toBeLessThanOrEqual(1)
    expect(result.score).toBe(result.factors.reduce((sum,f)=>sum+f.score,0))
    expect(result.factors.find(f=>f.id==='moving-yuan')?.lineIndexes?.length).toBeGreaterThanOrEqual(2)
    expect(new Set(result.factors.map(f=>f.id)).size).toBe(result.factors.length)
    expect(getFortuneLevel(4)).toBe('greatFortune'); expect(getFortuneLevel(1)).toBe('fortune'); expect(getFortuneLevel(0)).toBe('neutral'); expect(getFortuneLevel(-1)).toBe('neutral'); expect(getFortuneLevel(-2)).toBe('misfortune')
  })

  it('does not double-count exact temporal branch matches', () => {
    const reading=createIChingReading([7,7,7,7,7,7],'2026-07-12','2026-07-12T12:00:00Z','exact')
    const shi=buildNaJiaChart(reading.primaryHexagramId,[]).find(line=>line.isShi)!
    const result=evaluateDailyFortune(reading,{createdAt:reading.createdAt,timezone:'UTC',monthBranch:shi.earthlyBranch,dayStem:'jia',dayBranch:shi.earthlyBranch})
    expect(result.factors.filter(f=>f.source==='month')).toHaveLength(1)
    expect(result.factors.filter(f=>f.source==='day')).toHaveLength(1)
    expect(result.score).toBe(4)
  })

  it('applies mutually exclusive month rules plus an additional opposition penalty', () => {
    expect(evaluateContextInfluence('zi','zi','water','month')).toEqual([
      expect.objectContaining({id:'month-same',score:2}),
    ])
    expect(evaluateContextInfluence('hai','mao','wood','month')).toEqual([
      expect.objectContaining({id:'month-support',score:2}),
    ])
    expect(evaluateContextInfluence('shen','yin','wood','month')).toEqual([
      expect.objectContaining({id:'month-restrain',score:-2}),
      expect.objectContaining({id:'month-opposition',score:-2}),
    ])
    expect(evaluateContextInfluence('yin','mao','wood','month')).toEqual([
      expect.objectContaining({id:'month-sameElement',score:1}),
    ])
  })

  it('keeps day opposition neutral while preserving the main day relationship', () => {
    expect(evaluateContextInfluence('zi','zi','water','day')).toEqual([
      expect.objectContaining({id:'day-same',score:2}),
    ])
    expect(evaluateContextInfluence('hai','mao','wood','day')).toEqual([
      expect.objectContaining({id:'day-support',score:2}),
    ])
    expect(evaluateContextInfluence('shen','yin','wood','day')).toEqual([
      expect.objectContaining({id:'day-restrain',score:-2}),
      expect.objectContaining({id:'day-opposition',score:0}),
    ])
  })
})
