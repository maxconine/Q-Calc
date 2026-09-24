import { describe, expect, it } from 'vitest'
import { prettyTokens } from '../components/QuickInput'
import { evaluateLine, evaluateSheet } from './evaluate'
import { formatSig, formatUncertain, literalMeas, measure, sanitizeMeas, typedDigits } from './measure'

const sf = (text: string) => evaluateLine(text, { sigFigMode: true }).display
const pm = (text: string) => evaluateLine(text).display

describe('literal sig figs', () => {
  it.each([
    ['2.50', 3, 2],
    ['0.0025', 2, 4],
    ['6.02e23', 3, -21],
    ['300.', 3, 0],
    ['2.', 1, 0],
    ['6e23', 1, -23],
    ['1.00e2', 3, 0],
  ])('%s → %i s.f., %i dp', (text, sig, dp) => {
    expect(literalMeas(text)).toMatchObject({ sig, dp })
  })

  it('treats bare integers as exact', () => {
    expect(literalMeas('300')).toMatchObject({ sig: Infinity, dp: Infinity })
    expect(literalMeas('2')).toMatchObject({ sig: Infinity })
  })
})

describe('sig fig mode', () => {
  it.each([
    ['2.50 * 2', '5.00'],
    ['2.50 * 3.1', '7.8'],
    ['2.50 * 2.', '5'],
    ['12.11 + 0.3', '12.4'],
    ['3.00 * pi', '9.42'],
    ['(2.50)^2', '6.25'],
    ['sqrt(2.50)', '1.58'],
    ['log(2.50)', '0.398'],
    ['ln(2.50)', '0.916'],
    ['sin(30.0)', '0.500'],
    ['mean(2.50, 3.1)', '2.8'],
    ['6.02e23 * 2', '1.20e24'],
    ['300. / 3', '1.00e2'],
    ['2.5 * 4', '1.0e1'],
    ['2.50 - 2.50', '0.00'],
    ['2.50', '2.50'],
  ])('%s → %s', (text, want) => {
    expect(sf(text)).toBe(want)
  })

  it('leaves exact answers to the normal display', () => {
    expect(sf('2 + 2')).toBe('4')
    expect(sf('1/3')).toBe(evaluateLine('1/3').display)
    expect(evaluateLine('1/4', { sigFigMode: true, fractionMode: true }).display).toBe('1/4')
  })

  it('bypasses fraction mode and the display precision for measured answers', () => {
    expect(evaluateLine('2.50 * 3.1', { sigFigMode: true, fractionMode: true, sigFigs: 12 }).display).toBe('7.8')
    expect(evaluateLine('1.000 / 3', { sigFigMode: true, sigFigs: 2 }).display).toBe('0.3333')
  })

  it('does not show an exact form for a measured answer', () => {
    expect(evaluateLine('3.00 * pi', { sigFigMode: true }).exact).toBeUndefined()
  })

  it('carries a variable’s sig figs to later lines', () => {
    const [, b] = evaluateSheet(['x = 2.50', 'x * 3.1'], { sigFigMode: true })
    expect(b!.display).toBe('7.8')
  })

  it('carries sig figs through ans and stored measures', () => {
    expect(evaluateLine('ans * 2', { sigFigMode: true, ans: 2.5, measures: { ans: { sig: 3, dp: 2 } } }).display).toBe('5.00')
    expect(evaluateLine('x * 3.1', { sigFigMode: true, variables: { x: 2.5 }, measures: { x: { sig: 3, dp: 2 } } }).display).toBe('7.8')
  })

  it('keeps full precision for the value (no intermediate rounding)', () => {
    expect(evaluateLine('2.50 * 3.1', { sigFigMode: true }).value?.n).toBeCloseTo(7.75, 12)
  })

  it('falls back to the normal answer for anything unsupported', () => {
    expect(sf('floor(2.50)')).toBe('2')
    expect(sf('5 ft to cm')).toBe(evaluateLine('5 ft to cm').display)
    expect(sf('3.5!')).toBe(evaluateLine('3.5!').display)
  })

  it('is off by default', () => {
    expect(pm('2.50 * 2')).toBe('5')
  })
})

describe('uncertainty', () => {
  it.each([
    ['(5.0 ± 0.1) + (3.2 ± 0.2)', '8.2 ± 0.3'],
    ['(10.0 ± 0.2) * (2.0 ± 0.1)', '20.0 ± 1.4'],
    ['(2.0 ± 0.1)^2', '4.0 ± 0.4'],
    ['sqrt(4.0 ± 0.4)', '2.00 ± 0.10'],
    ['10 ± 0.7', '10.0 ± 0.7'],
    ['10 ± 5%', '10.0 ± 0.5'],
    ['-5 ± 0.25', '-5.00 ± 0.25'],
    ['(10 ± 1) / (2 ± 0.1)', '5.0 ± 0.8'],
    ['1234 ± 200', '1200 ± 200'],
  ])('%s → %s', (text, want) => {
    expect(pm(text)).toBe(want)
  })

  it('accepts +/- and ~ as typed', () => {
    expect(pm(prettyTokens('10 +/- 0.7'))).toBe('10.0 ± 0.7')
    expect(pm(prettyTokens('10 ~ 0.7'))).toBe('10.0 ± 0.7')
    expect(pm(prettyTokens('5+/-2'))).toBe('5 ± 2')
  })

  it('accepts +.- and spelled out plus minus', () => {
    expect(pm(prettyTokens('10 +.- 0.7'))).toBe('10.0 ± 0.7')
    expect(pm(prettyTokens('10 plusminus 0.7'))).toBe('10.0 ± 0.7')
    expect(pm(prettyTokens('10 plus minus 0.7'))).toBe('10.0 ± 0.7')
    expect(pm(prettyTokens('10 plus-minus 0.7'))).toBe('10.0 ± 0.7')
    expect(pm(prettyTokens('10 minusplus 0.7'))).toBe('10.0 ± 0.7')
  })

  it('leaves +- as plus a negative number', () => {
    expect(prettyTokens('5+-3')).toBe('5+-3')
    expect(prettyTokens('5-+3')).toBe('5-+3')
    expect(evaluateSheet([prettyTokens('5+-3')])[0]?.display).toBe('2')
  })

  it('accepts ∓ and -/+ as ±', () => {
    expect(pm('10 ∓ 0.7')).toBe('10.0 ± 0.7')
    expect(pm(prettyTokens('10 -/+ 0.7'))).toBe('10.0 ± 0.7')
    expect(pm(prettyTokens('5-/+2'))).toBe('5 ± 2')
    expect(pm('5.0 ∓ 0.2 * 3')).toBe('15.0 ± 0.6')
  })

  it('uses the interval worst case for other functions', () => {
    expect(pm('sin(30 ± 1)')).toBe('0.500 ± 0.015')
    expect(pm('ln(10 ± 1)')).toBe('2.30 ± 0.11')
  })

  it('answers with the central value', () => {
    const r = evaluateLine('(10.0 ± 0.2) * (2.0 ± 0.1)')
    expect(r.value?.n).toBeCloseTo(20, 12)
    expect(r.meas?.unc).toBeCloseTo(1.4, 12)
    expect(r.exact).toBeUndefined()
  })

  it('re-parses its own display', () => {
    expect(pm(pm('10 ± 0.7'))).toBe('10.0 ± 0.7')
    expect(pm(`(${pm('(5.0 ± 0.1) + (3.2 ± 0.2)')}) * 2`)).toBe('16.4 ± 0.6')
  })

  it('carries a variable’s uncertainty', () => {
    const [, b] = evaluateSheet(['x = 10 ± 0.7', 'x * 2'])
    expect(b!.display).toBe('20.0 ± 1.4')
  })

  it('applies sig-fig rounding first when both are on', () => {
    expect(evaluateLine('(10.0 ± 0.2) * (2.0 ± 0.1)', { sigFigMode: true }).display).toBe('20.0 ± 1.4')
    expect(evaluateLine('2.0 ± 0.05', { sigFigMode: true }).display).toBe('2.00 ± 0.05')
  })

  it('falls back to blank for ± it does not model', () => {
    expect(pm('mean(1 ± 0.1, 2)')).toBe('')
    expect(pm('5 m ± 1 m')).toBe('')
    expect(pm('±')).toBe('')
  })

  it('leaves ordinary subtraction of negatives alone', () => {
    expect(pm('5 - -2')).toBe('7')
    expect(pm('5+(-2)')).toBe('3')
    expect(pm(prettyTokens('5 - -2'))).toBe('7')
    expect(pm(prettyTokens('5+(-2)'))).toBe('3')
  })
})

describe('formatting helpers', () => {
  it('formatSig keeps trailing zeros', () => {
    expect(formatSig(5, 3, 2)).toBe('5.00')
    expect(formatSig(100, 3, 0)).toBe('1.00e2')
    expect(formatSig(0.000123, 2, 5)).toBe('0.00012')
  })

  it('formatUncertain rounds to one significant figure', () => {
    expect(formatUncertain(9.96, 0.74)).toBe('10.0 ± 0.7')
    expect(formatUncertain(8.2, 0.30000000000000004)).toBe('8.2 ± 0.3')
  })

  it('measure returns null for plain text', () => {
    expect(measure('hello')).toBeNull()
  })
})

describe('bugfix batch: ± binds tightest, works with units, never drops', () => {
  it('treats a ± b as one quantity', () => {
    expect(pm('5.0 ± 0.2 * 3')).toBe('15.0 ± 0.6')
    expect(pm('2 * 5.0 ± 0.2')).toBe('10.0 ± 0.4')
    expect(pm('5 ± 2 * 3 ± 1')).toBe('15 ± 11')
    expect(pm('10 ± 0.1 * 2')).toBe('20.0 ± 0.2')
  })
  it('never returns a bare number for a ± it cannot model', () => {
    for (const t of ['x ± 1', '(1+2) ± 1', '5 m ± 1 m']) expect(pm(t)).toBe('')
  })
  it('propagates ± through unit products and powers', () => {
    expect(pm('2.0 ± 0.1 m')).toBe('2.0 ± 0.1 m')
    expect(pm('(5.0 ± 0.1) cm * 2')).toBe('10.0 ± 0.2 cm')
    expect(pm('5.0 ± 0.1 cm * 2')).toBe('10.0 ± 0.2 cm')
    expect(pm('(2.0 ± 0.1 m)^2')).toBe('4.0 ± 0.4 m²')
    expect(pm('(10.0 ± 0.2 cm)')).toBe('10.0 ± 0.2 cm')
  })
  it('rounds unit answers to their sig figs', () => {
    expect(sf('12.0 kg in lb')).toBe('26.5 lbs')
    expect(sf('10.0 N / 2.0 kg')).toBe('5.0 m/s²')
    expect(sf('4.7 uF')).toBe('0.0000047 F')
    expect(sf('5 cm to cm')).toBe('5 cm')
  })
  it('counts exponents as exact', () => {
    expect(sf('2.0^0.5')).toBe(sf('sqrt(2.0)'))
    expect(sf('2.0^0.5')).toBe('1.4')
  })
})

describe('± rounding', () => {
  it('keeps two figures of an uncertainty that leads with a 1, else one', () => {
    expect(pm('(5.0 ± 0.14)*1')).toBe('5.00 ± 0.14')
    expect(pm('(5.0 ± 0.34)*1')).toBe('5.0 ± 0.3')
    expect(pm('(100 ± 15)*1')).toBe('100 ± 15')
    expect(pm('(2.0 ± 0.195)*1')).toBe('2.0 ± 0.2')
    expect(pm('2.0 ± 0.195')).toBe('2.000 ± 0.195')
  })
})

describe('a typed ± shows as written', () => {
  it.each([
    ['10 ± 1', '10 ± 1'],
    ['5.0 ± 0.1', '5.0 ± 0.1'],
    ['5.0 ± 0.1 m', '5.0 ± 0.1 m'],
    ['(10 ± 1) cm', '10 ± 1 cm'],
    ['-(5.0 ± 0.1)', '-5.0 ± 0.1'],
    ['-5.0 ± 0.1', '-5.0 ± 0.1'],
    ['5 ± 0.10', '5.00 ± 0.10'],
    ['10 ± 1.0', '10.0 ± 1.0'],
    ['1e3 ± 1e1', '1000 ± 10'],
    ['5.0 ± 0.1 cm to m', '0.050 ± 0.001 m'],
    ['5.0 ± 0.1 m to cm', '500 ± 10 cm'],
  ])('%s → %s', (text, want) => {
    expect(pm(text)).toBe(want)
  })
  it('a calculated ± keeps the rounding rule', () => {
    expect(pm('(10.0 ± 0.2) * (2.0 ± 0.1)')).toBe('20.0 ± 1.4')
    expect(pm('(5.0 ± 0.1) * 10')).toBe('50.0 ± 1.0')
    expect(pm('(5.0 ± 0.1) + 0')).toBe('5.00 ± 0.10')
    expect(pm('(5.0 ± 0.1 m) * 10 s')).toBe('50.0 ± 1.0 m s')
    expect(pm('100 ± 1%')).toBe('100.0 ± 1.0')
    // 0.1 ft is 1.2 in: new digits, so the rule applies
    expect(pm('5.0 ± 0.1 ft to in')).toBe('60.0 ± 1.2 in')
  })
  it('stays as written through variables and ans, and gains the digit once used', () => {
    expect(evaluateSheet(['x = 10 ± 1', 'x', 'ans', '2x']).map((r) => r.display)).toEqual(['10 ± 1', '10 ± 1', '10 ± 1', '20 ± 2'])
    expect(evaluateSheet(['x = 5.0 ± 0.1', 'x * 10']).map((r) => r.display)).toEqual(['5.0 ± 0.1', '50.0 ± 1.0'])
  })
  it('typed digits and history round trip', () => {
    expect(typedDigits('0.10')).toBe('10')
    expect(typedDigits('10')).toBe('1')
    expect(typedDigits('1e-1')).toBe('1')
    expect(sanitizeMeas({ unc: 1, uncDigits: '1' })).toEqual({ unc: 1, uncDigits: '1' })
    expect(sanitizeMeas({ unc: 1, uncDigits: 'x' })).toEqual({ unc: 1 })
    expect(formatUncertain(10, 1)).toBe('10.0 ± 1.0')
    expect(formatUncertain(10, 1, '1')).toBe('10 ± 1')
    expect(formatUncertain(10, 1.2, '1')).toBe('10.0 ± 1.2')
  })
})
