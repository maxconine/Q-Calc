// adversarial coverage for the Tab answer-form cycle, on top of answerForms.test.ts
import { describe, expect, it } from 'vitest'
import { evaluateLine } from '../engine/evaluate'
import type { EvaluateOptions } from '../engine/types'
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
    rationalize: options.rationalize,
  }).map((f) => f.display)
}

describe('answerForms: fractions only when exact', () => {
  it('0.625 is exactly 5/8', () => {
    expect(forms('0.625')).toEqual(['5/8'])
  })
  it('2/7 is already a fraction, offered back as itself', () => {
    expect(forms('2/7')).toEqual(['2/7'])
  })
  it('a number with too much entropy for any small fraction or closed form has no forms', () => {
    expect(forms('2.71828182845')).toEqual([])
  })
})

describe('answerForms: negative and unit-bearing dual answers', () => {
  it('a negative trig answer offers its exact fraction', () => {
    // cos(120deg) = -0.5 = -1/2
    expect(forms('cos(120)')).toContain('-1/2')
  })
  it('a unit answer with an exact pi multiple keeps the unit on the closed form', () => {
    // pi multiples are written with an explicit '*' by exactForm (unlike "3sqrt(2)")
    expect(forms('2*pi cm')).toContain('2*pi cm')
  })
})

describe('answerForms: scientific-notation boundary', () => {
  it('below magnitude 3 does not offer scientific', () => {
    const f = forms('123.456789123')
    expect(f.some((d) => /e[+-]/.test(d))).toBe(false)
  })
  it('at magnitude 3 it does, with the number worked out by hand', () => {
    // 1000.123456789 to 12 significant figures in scientific notation
    expect(forms('1000.123456789')).toContain('1.00012345679e+3')
  })
  it('a small number past magnitude -3 offers scientific too', () => {
    expect(forms('0.0009999')).toContain('9.999e-4')
  })
})

describe('answerForms: rationalize option affects the closed form offered', () => {
  it('1/sqrt(2) offers a rationalized denominator by default', () => {
    expect(forms('1/sqrt(2)')).toContain('sqrt(2)/2')
  })
  it('with rationalize off it offers the unrationalized form instead', () => {
    const f = forms('1/sqrt(2)', { rationalize: false })
    expect(f.some((d) => d.includes('sqrt'))).toBe(true)
    expect(f).not.toContain('sqrt(2)/2')
  })
})

describe('answerForms: never offered for text, infinite or measured answers', () => {
  it('a solve message (text answer) has no forms', () => {
    const r = evaluateLine('x^2 = -1')
    expect(r.value?.kind).toBe('text')
    expect(forms('x^2 = -1')).toEqual([])
  })
  it('a sig-fig-mode measured unit conversion has no forms', () => {
    expect(forms('5.0 kg in lb', { sigFigMode: true })).toEqual([])
  })
  it('an uncertain (±) answer has no forms even outside sig fig mode', () => {
    expect(forms('10 ± 1')).toEqual([])
  })
})

describe('answerForms: never repeats the value already shown', () => {
  it('a whole number with no other forms', () => {
    expect(forms('7')).toEqual([])
  })
  it('in fraction mode, Tab still offers the plain decimal as the other form', () => {
    // the shown display is the fraction '1/2'; the decimal '0.5' is a genuinely different
    // string and is still offered, while a redundant re-derived '1/2' is filtered out
    expect(forms('1/2', { fractionMode: true })).toEqual(['0.5'])
  })
})
