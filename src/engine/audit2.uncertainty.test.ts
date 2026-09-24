import { describe, expect, it } from 'vitest'
import { line } from './audit.helpers'

const pm = (text: string) => line(text).display
const sf = (text: string) => line(text, { sigFigMode: true }).display

describe('audit2: sig figs on functions the original audit skipped', () => {
  it('sin(30.0) keeps 3 sig figs: 0.500', () => {
    expect(sf('sin(30.0)')).toBe('0.500')
  })
  it('sin(pi/6.00) in radians keeps the 3 sig figs of 6.00: 0.500', () => {
    expect(line('sin(pi/6.00)', { sigFigMode: true, angleMode: 'rad' }).display).toBe('0.500')
  })
  it('hypot(3.0, 4.00) keeps the fewer sig figs of its two arguments: 5.0', () => {
    expect(sf('hypot(3.0, 4.00)')).toBe('5.0')
  })
  it('mean(2.0, 4.00) keeps the fewer sig figs: 3.0', () => {
    expect(sf('mean(2.0, 4.00)')).toBe('3.0')
  })
  it('cbrt(27.0) keeps 3 sig figs: 3.00', () => {
    expect(sf('cbrt(27.0)')).toBe('3.00')
  })
  it('abs(-3.50) keeps 3 sig figs: 3.50', () => {
    expect(sf('abs(-3.50)')).toBe('3.50')
  })
})

describe('audit2: atan2 does not carry sig figs through, unlike its sibling hypot', () => {
  // hypot is in measure.ts's MIN_SIG_FNS and correctly narrows to the fewest sig figs of its
  // two arguments (see above: hypot(3.0, 4.00) -> "5.0"). atan2 also takes two measured
  // arguments but appears in neither SIG_FNS nor MIN_SIG_FNS, so walkFunction returns null for
  // it and the whole expression falls back to full, unrounded precision.
  // input: 'atan2(3.0, 4.00)' in sig-fig mode, expected: 2 sig figs, "37"
  // actual: "36.8698976458" (full double precision, sig figs silently dropped)
  it('atan2(3.0, 4.00) keeps the fewer sig figs of its arguments: 37', () => {
    expect(sf('atan2(3.0, 4.00)')).toBe('37')
  })
})

describe('audit2: ± through functions the original audit did not cover', () => {
  it('asinh(1.0 ± 0.1) is 0.88 ± 0.07', () => {
    // f(1)=asinh(1)=0.881373587; f(1.1)-f(1)=0.068684..., f(1)-f(0.9)=0.072488...; the
    // walker takes the worse (larger) of the two, ~0.0725, which rounds to one sig fig as 0.07
    expect(pm('asinh(1.0 ± 0.1)')).toBe('0.88 ± 0.07')
  })
  it('atanh(0.50 ± 0.01) is 0.549 ± 0.013', () => {
    expect(pm('atanh(0.50 ± 0.01)')).toBe('0.549 ± 0.013')
  })
  it('sqrt(-1.0 ± 0.1) is blank, not a wrong number', () => {
    expect(pm('sqrt(-1.0 ± 0.1)')).toBe('')
  })
  it('dividing by a measurement centred on zero is blank', () => {
    expect(pm('1/(0.0 ± 0.1)')).toBe('')
  })
})

describe('audit2: ± through nested, multi-step expressions', () => {
  it('((2.0 ± 0.1) + (3.0 ± 0.1)) * (4.0 ± 0.1) is 20.0 ± 1.3', () => {
    // the sum is 5.0 ± 0.2 (worst-case add), then *4.0±0.1: unc = |5|*0.1 + |4|*0.2 = 1.3,
    // which rounds to one sig fig as 1
    expect(pm('((2.0 ± 0.1) + (3.0 ± 0.1)) * (4.0 ± 0.1)')).toBe('20.0 ± 1.3')
  })
  it('(10.0 ± 0.1) - (9.0 ± 0.1) is 1.0 ± 0.2, not float noise', () => {
    // unlike the plain-decimal cancellation cases (12.0 - 11.9 etc.), the ± walker computes
    // dp/unc from the literals directly rather than from the raw float subtraction
    expect(pm('(10.0 ± 0.1) - (9.0 ± 0.1)')).toBe('1.0 ± 0.2')
  })
  it('(3.0 ± 0.1) - (5.0 ± 0.1) keeps its negative sign: -2.0 ± 0.2', () => {
    expect(pm('(3.0 ± 0.1) - (5.0 ± 0.1)')).toBe('-2.0 ± 0.2')
  })
  it('a bare negative literal with ± needs no subtraction to display correctly', () => {
    expect(pm('-2.0 ± 0.1')).toBe('-2.0 ± 0.1')
  })
})

describe('audit2: scientific-notation literals keep their written sig figs', () => {
  it('6.0200e23 / 2 keeps the 5 sig figs of the mantissa: 3.0100e23', () => {
    expect(sf('6.0200e23 / 2')).toBe('3.0100e23')
  })
  it('a bare 1.20e3 keeps its 3 sig figs and scientific form', () => {
    expect(sf('1.20e3')).toBe('1.20e3')
  })
  it('1.5e-3 * 2 keeps 2 sig figs: 0.0030', () => {
    expect(sf('1.5e-3 * 2')).toBe('0.0030')
  })
})

describe('audit2: a typed uncertainty is never rounded, a calculated one is', () => {
  it.each([
    ['(2.0 ± 0.95)', '2.00 ± 0.95'],
    ['(2.0 ± 0.35)', '2.00 ± 0.35'],
    ['(2.0 ± 0.15)', '2.00 ± 0.15'],
    ['(2.0 ± 0.25)', '2.00 ± 0.25'],
  ])('%s → %s (as typed)', (text, want) => {
    expect(pm(text)).toBe(want)
  })

  // unc.toPrecision(1) rounds the stored double, and 0.35 is stored a hair below
  it.each([
    ['(2.0 ± 0.35)*1', '2.0 ± 0.3'],
    ['(2.0 ± 0.75)*1', '2.0 ± 0.8'],
    ['(2.0 ± 0.15)*1', '2.00 ± 0.15'],
  ])('%s → %s (calculated)', (text, want) => {
    expect(pm(text)).toBe(want)
  })
})

describe('audit2: sig figs are only tracked when asked for', () => {
  it('5.00 * 2 without sig-fig mode is a plain "10", not "10.0"', () => {
    expect(line('5.00 * 2').display).toBe('10')
  })
  it('the same expression in sig-fig mode keeps the 3 sig figs of 5.00', () => {
    expect(sf('5.00 * 2')).toBe('10.0')
  })
})
