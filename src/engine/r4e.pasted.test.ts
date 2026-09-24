import { describe, expect, it } from 'vitest'
import { flattenPastedText } from '../components/QuickInput'
import { evaluateLine, evaluateSheet } from './evaluate'
import { shown } from './audit.helpers'

const pasted = (text: string) => evaluateLine(flattenPastedText(text)).display

describe('pasted look-alike characters read like their ascii', () => {
  it.each([
    ['5 + 3', '5+3'],
    ['5 × 3', '5*3'],
    ['5​+3', '5+3'],
    ['﻿2+2', '2+2'],
    ['12­5 + 1', '125+1'],
    ['5 ‐ 3', '5-3'],
    ['5 ‑ 3', '5-3'],
    ['5 ﹣ 3', '5-3'],
    ['５ － ３', '5-3'],
    ['２＋３', '2+3'],
    ['（２＋３）×４', '(2+3)*4'],
    ['1⁄2', '1/2'],
    ['6 ∕ 4', '6/4'],
    ['5 ∗ 2', '5*2'],
    ['2 + 3', '2+3'],
    ['2 +\r\n3', '2+3'],
    ['(3.00 × 10⁸ m/s) ×\n(2 s)', '3e8 m/s * 2 s'],
  ])('%j', (paste, ascii) => {
    const want = shown(ascii)
    expect(want).not.toBe('')
    expect(pasted(paste)).toBe(want)
  })

  it('leaves en and em dashes blank rather than guess a minus', () => {
    expect(pasted('5 — 3')).toBe('')
    expect(pasted('5–10')).toBe('')
  })

  it('a fraction slash mixed number stays blank, not 2 × ½', () => {
    expect(pasted('2 1⁄2')).toBe('')
  })

  it('thin space thousands stay blank, never a product', () => {
    expect(pasted('1 000 + 1')).toBe('')
    expect(pasted('1 000 000')).toBe('')
  })

  it('a ligature spells its letters', () => {
    expect(flattenPastedText('ﬁve')).toBe('five')
  })
})

describe('unit symbols from PDFs', () => {
  it.each([
    ['20 ℃ in ℉', '20 degC in degF'],
    ['20 ℃', '20 °C'],
    ['5 Å in m', '5 angstrom in m'],
    ['5 Å in m', '5 angstrom in m'],
    ['5Å', '5 angstrom'],
    ['5 nm in Å', '5 nm in angstrom'],
    ['5 Ångström in m', '5 angstrom in m'],
  ])('%j', (text, ascii) => {
    const want = shown(ascii)
    expect(want).not.toBe('')
    expect(shown(text)).toBe(want)
  })
})

describe('a × 10^n mantissa carries its unit', () => {
  it.each([
    ['3.00 × 10^8 m/s', '3e8 m/s'],
    ['(3.00 × 10^8 m/s) × (2 s)', '3e8 m/s * 2 s'],
    ['6.626×10⁻³⁴ J s', '6.626e-34 J s'],
    ['6.67×10⁻¹¹ N·m²/kg²', '6.67e-11 N m^2/kg^2'],
    ['1.38 × 10⁻²³ J/K', '1.38e-23 J/K'],
    ['3 × 10^(-2) kg', '0.03 kg'],
  ])('%s', (text, ascii) => {
    const want = shown(ascii)
    expect(want).not.toBe('')
    expect(shown(text)).toBe(want)
  })

  it.each([
    ['1/2 × 10^3', '500'],
    ['1 / 2 × 10^3', '500'],
    ['5 ÷ 2 × 10^3', '2500'],
    ['2^-3 × 10^2', '12.5'],
    ['2 ^ - 3 × 10^2', '12.5'],
    ['2^3 × 10^2', '800'],
    ['3 × 10^2!', '300'],
    ['3 × 10^2^2', '30000'],
    ['2 - 3 × 10^2', '-298'],
    ['1e-3 × 10^2', '0.1'],
    ['1e+3 × 10^2', '100000'],
    ['3 × 10^-34 × 2', '6e-34'],
  ])('leaves %s to precedence', (text, want) => expect(shown(text)).toBe(want))
})

describe('a call never drops an argument it was given', () => {
  it.each(['sin(30, 60)', 'sin(1,000)', 'sqrt(4,9)', 'ln(1,000)', 'abs(1,000)', 'exp(1,2)', 'asin(1,2)', 'factorial(3,1)', 'floor(2.5,1)'])(
    '%s is blank',
    (text) => expect(shown(text)).toBe(''),
  )

  it('keeps the optional and multi argument ones', () => {
    expect(shown('round(2.345, 2)')).toBe('2.35')
    expect(shown('log(8, 2)')).toBe('3')
    expect(shown('clamp(5, 1, 3)')).toBe('3')
    expect(shown('max(1,000,2)')).toBe('2')
    expect(shown('atan2(1, 1)')).toBe('45')
    expect(shown('nCr(5, 2)')).toBe('10')
  })

  it('a user function given too many arguments is blank', () => {
    const out = evaluateSheet(['f(x)=x^2', 'f(2,3)', 'f(3)']).map((r) => r.display)
    expect(out.slice(1)).toEqual(['', '9'])
  })
})

describe('latex pastes', () => {
  it('\\pm is never a plus', () => {
    expect(shown('5 \\pm 2')).toBe('')
    expect(shown('\\pm 2')).toBe('')
    expect(shown('5 \\mp 2')).toBe('')
  })

  it('a subscript is not dropped', () => {
    expect(shown('2_{10} + 1')).toBe('')
    expect(shown('10_{2}')).toBe('')
    expect(evaluateSheet(['x = 3', 'x_{1} + 1'])[1]!.display).toBe('')
  })

  it.each([
    ['0.\\overline{3}', '1/3'],
    ['0.1\\overline{6}', '1/6'],
    ['1.\\overline{142857}', '8/7'],
    ['.\\overline{9}', '1'],
    ['2.25\\overline{0}', '2.25'],
  ])('repeating decimal %s', (text, ascii) => expect(shown(text)).toBe(shown(ascii)))

  it.each([
    ['\\frac{\\frac{1}{2}}{2}', '0.25'],
    ['\\frac{1}{\\frac{1}{\\frac{1}{2}}}', '0.5'],
    ['\\binom{\\binom{4}{2}}{2}', '15'],
    ['\\dfrac{1}{2}', '0.5'],
    ['\\tfrac{3}{4}', '0.75'],
    ['50\\%', '0.5'],
    ['50\\% of 20', '10'],
  ])('%s', (text, want) => expect(shown(text)).toBe(want))
})

describe('pathological pastes finish', () => {
  it('deep latex nesting evaluates or blanks without throwing', () => {
    expect(shown('\\frac{1}{'.repeat(60) + '2' + '}'.repeat(60))).toBe('2')
    expect(() => shown('\\frac{1}{'.repeat(300) + '2' + '}'.repeat(300))).not.toThrow()
    expect(() => shown('\\overline{'.repeat(100) + '2' + '}'.repeat(100))).not.toThrow()
    expect(() => shown(flattenPastedText('12.5\n'.repeat(400)))).not.toThrow()
    expect(() => shown(`2${'× 10^2 '.repeat(300)}`)).not.toThrow()
  })
})

describe('pasted forms still pending with the owner', () => {
  // word autoformats ` - ` into ` – `; a spaced en dash is almost always a minus
  it.fails('a spaced en dash is a minus', () => expect(pasted('5 – 3')).toBe('2'))
  // a mixed number glyph: `2½` is 2.5, never 2 × ½
  it.fails('vulgar fractions read as numbers', () => expect(pasted('2½')).toBe('2.5'))
  // two pasted lines join into `10 (2)`, a product nobody wrote
  it.fails('a pasted column is not multiplied together', () => expect(pasted('10\n(2)')).toBe(''))
  // `r` is rankine, so a pasted area formula reads as temperature squared
  it.fails('π r² from a formula sheet is blank', () => expect(shown('\\pi r^2')).toBe(''))
})
