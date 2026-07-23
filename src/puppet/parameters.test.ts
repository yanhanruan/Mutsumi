/**
 * Tests for the puppet parameter model (src/puppet/parameters.ts).
 * Pure value math — deterministic, no DOM/WebGL.
 */
import { describe, it, expect } from 'vitest'
import {
  clampParam,
  defaultValues,
  indexDefs,
  normalizeParam,
  type ParameterDef,
} from './parameters'

const angleX:  ParameterDef = { id: 'ParamAngleX',  min: -30, max: 30, default: 0 }
const eyeOpen: ParameterDef = { id: 'ParamEyeLOpen', min: 0,   max: 1,  default: 1 }

describe('clampParam()', () => {
  it('passes through in-range values', () => {
    expect(clampParam(angleX, 15)).toBe(15)
  })
  it('clamps to the bounds', () => {
    expect(clampParam(angleX, -100)).toBe(-30)
    expect(clampParam(angleX, 100)).toBe(30)
  })
  it('collapses NaN to the rest value', () => {
    expect(clampParam(eyeOpen, NaN)).toBe(1)
  })
})

describe('normalizeParam()', () => {
  it('maps min→0, max→1, midpoint→0.5', () => {
    expect(normalizeParam(angleX, -30)).toBe(0)
    expect(normalizeParam(angleX, 30)).toBe(1)
    expect(normalizeParam(angleX, 0)).toBe(0.5)
  })
  it('clamps out-of-range values before normalizing', () => {
    expect(normalizeParam(eyeOpen, 5)).toBe(1)
    expect(normalizeParam(eyeOpen, -5)).toBe(0)
  })
  it('collapses a zero-width range to 0', () => {
    expect(normalizeParam({ id: 'x', min: 2, max: 2, default: 2 }, 2)).toBe(0)
  })
})

describe('defaultValues()', () => {
  it('seeds every parameter at its rest value', () => {
    const v = defaultValues([angleX, eyeOpen])
    expect(v.get('ParamAngleX')).toBe(0)
    expect(v.get('ParamEyeLOpen')).toBe(1)
    expect(v.size).toBe(2)
  })
})

describe('indexDefs()', () => {
  it('indexes defs by id', () => {
    const m = indexDefs([angleX, eyeOpen])
    expect(m.get('ParamAngleX')).toBe(angleX)
    expect(m.size).toBe(2)
  })
})
