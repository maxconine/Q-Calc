import { describe, expect, it } from 'vitest'
import { evaluateLine } from '../engine/evaluate'
import type { EvaluateOptions } from '../engine/types'
import { unitAlternatives } from '../engine/units'
import { answerForms } from './answerForms'

function forms(expr: string, options: EvaluateOptions = {}): string[] {
  const r = evaluateLine(expr, options)
  if (!r.value) return []
  return answerForms({
    value: r.value,
    display: r.display,
    exact: r.exact,
    meas: r.meas,
    sigFigs: options.sigFigs ?? 12,
    sigFigMode: options.sigFigMode,
  }).map((f) => f.display)
}

describe('answerForms', () => {
  it('offers a fraction only when it is the same number', () => {
    expect(forms('0.375')).toEqual(['3/8'])
    expect(forms('1/3')).toEqual(['1/3'])
    expect(forms('0.1+0.2')).toEqual(['3/10'])
    // 3.14159 is not π, and 314159/100000 just restates the decimal
    expect(forms('3.14159')).toEqual([])
  })

  it('offers closed forms with π and roots', () => {
    expect(forms('pi/4')).toEqual(['pi/4'])
    expect(forms('0.5*pi')).toEqual(['pi/2'])
  })

  it('splits a dual answer into its sides', () => {
    expect(forms('sin(60)')).toEqual(['0.866025403784', 'sqrt(3)/2'])
    expect(forms('sqrt(2)*3')).toEqual(['4.24264068712', '3sqrt(2)'])
  })

  it('offers scientific notation only for big and small magnitudes', () => {
    expect(forms('2')).toEqual([])
    expect(forms('123456.789')).toEqual(['1.23456789e+5'])
    expect(forms('0.000123')).toContain('1.23e-4')
    // already scientific: nothing new to show
    expect(forms('1e22')).toEqual([])
  })

  it('offers a few other units of the same kind', () => {
    expect(forms('12 kg in lb')).toEqual(['423.287543395 oz', '12000 g', '12 kg'])
    expect(forms('72 F in C')).toEqual(['72 °F', '295.372222222 K', '531.67 °R'])
    expect(forms('2 hr')).toEqual(['7200 s', '120 min', '0.0833333333333 d'])
  })

  it('never re-expresses a measured answer', () => {
    expect(forms('5 ± 0.2')).toEqual([])
    expect(forms('12.0 kg in lb', { sigFigMode: true })).toEqual([])
  })

  it('carries the converted value for unit forms', () => {
    const r = evaluateLine('12 kg in lb')
    const oz = answerForms({ value: r.value!, display: r.display, sigFigs: 12 })[0]
    expect(oz?.value?.unit).toBe('oz')
    expect(oz?.value?.n).toBeCloseTo(423.287543, 5)
  })

  it('has nothing for text answers or non-finite values', () => {
    expect(answerForms({ value: { kind: 'text', n: 0, text: 'hi' }, display: 'hi', sigFigs: 12 })).toEqual([])
    expect(answerForms({ value: { kind: 'number', n: Infinity }, display: '∞', sigFigs: 12 })).toEqual([])
  })
})

describe('unitAlternatives', () => {
  it('is empty without a known unit', () => {
    expect(unitAlternatives({ kind: 'number', n: 5 })).toEqual([])
    expect(unitAlternatives({ kind: 'number', n: 0, unit: 'kg', unitId: 'kg' })).toEqual([])
  })

  it('still converts a zero temperature', () => {
    const alts = unitAlternatives({ kind: 'number', n: 0, unit: '°C', unitId: 'c' })
    expect(alts.map((v) => v.unit)).toContain('°F')
  })
})
