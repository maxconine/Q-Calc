// cases a seeded property fuzzer found: each one broke an identity that should hold
// (fraction form = decimal, exact form = decimal, display typed back = same quantity, sig figs by the rules)
import { describe, expect, it } from 'vitest'
import { evaluateSheet } from './evaluate'
import type { EvaluateOptions } from './types'

const run = (lines: string[], opts: EvaluateOptions = {}) => evaluateSheet(lines, opts).at(-1)!
const shown = (text: string, opts: EvaluateOptions = {}) => run([text], opts).display
const frac = { fractionMode: true }
const sig = { sigFigMode: true }

describe('fraction mode only claims a fraction the value really is', () => {
  it('an irrational above 10 stays decimal instead of landing on some p/q under 10,000', () => {
    expect(shown('π+35', frac)).toBe('38.1415926536')
    expect(shown('ln(17)', frac)).toBe('2.83321334406')
    expect(shown('((754.1073 - 1309) - sin(1)) * (9! + 2e4)', frac)).toBe('-212463999.153')
  })

  it('a tiny value is not shown as 0 or 0/1', () => {
    expect(shown('5e-13', frac)).toBe('5e-13')
    expect(shown('3e-10', frac)).toBe('3e-10')
    expect(shown('1e-11*3', frac)).toBe('3e-11')
  })

  it('a value just off an integer keeps its digits, and a whole number never shows /1', () => {
    expect(shown('3093+4e-8', frac)).toBe('3093.00000004')
    expect(shown('1e6+1e-10', frac)).toBe('1000000')
  })

  it('real fractions still show', () => {
    expect(shown('0.333333333', frac)).toBe('1/3')
    expect(shown('1234567/7777', frac)).toBe('1234567/7777')
    expect(shown('1000001/3', frac)).toBe('1000001/3')
    expect(shown('1/9973', frac)).toBe('1/9973')
    expect(shown('-2.5', frac)).toBe('-5/2')
    expect(shown('2032mm to ft', frac)).toBe('20/3 ft')
  })
})

describe('an exact form of 0 is only for a value that is 0', () => {
  it('a real 1e-11 gets no exact form', () => {
    expect(run(['sqrt(1e-22)']).exact).toBeUndefined()
    expect(run(['√(9e-8)^3']).exact).toBeUndefined()
    expect(run(['sin(3.14159265358979)'], { angleMode: 'rad' }).exact).toBeUndefined()
  })

  it('float noise that the engine already cleaned to 0 still reads 0', () => {
    expect(run(['sqrt(2)^2-2']).exact).toBe('0')
    expect(run(['sin(x)=0'], { angleMode: 'rad' }).display).toBe('0, 3.14159')
    expect(shown('x^3=x')).toBe('-1, 0, 1')
  })
})

describe('log with a space before its bracket is still base 10', () => {
  it('log (100) is 2, not ln 100', () => {
    expect(shown('log (100)')).toBe('2')
    expect(shown('2 log (100)')).toBe('4')
    expect(shown('log(log (1e10))')).toBe('1')
    expect(shown('log (8, 2)')).toBe('3')
  })

  it('the input turns `log pi` into `log π`, which must agree with log(pi)', () => {
    expect(shown('log π')).toBe(shown('log(pi)'))
    expect(shown('log τ')).toBe('0.798179868358')
  })
})

describe('sig figs follow the place a measured zero keeps', () => {
  it('a sum that rounds up to a power of ten keeps its decimal place', () => {
    expect(shown('0.359+0.6', sig)).toBe('1.0')
    expect(shown('(3.17*3.17) - 0.05', sig)).toBe('10.0')
  })

  it('a zero from cancelling measured values scales its place, not zero sig figs', () => {
    expect(shown('((491.2 - 491.2) / 2.001) - 0.6', sig)).toBe('-0.6')
    expect(shown('2.199 - ((0.382 - 0.382) / 46)', sig)).toBe('2.199')
    expect(shown('(302.37 - 302.37)*(302.37 - 302.37) + 6.6', sig)).toBe('6.6')
    expect(shown('(1.0-1.0)*2.001', sig)).toBe('0.0')
  })

  it('the usual rules are unchanged', () => {
    expect(shown('1.50*2.0', sig)).toBe('3.0')
    expect(shown('99.95+0.12', sig)).toBe('100.07')
    expect(shown('9.96*1.0', sig)).toBe('1.0e1')
    expect(shown('12.0 kg in lb', sig)).toBe('26.5 lbs')
  })
})

describe('case-sensitive SI prefixes on another unit symbol', () => {
  it('pA is a picoamp and pC a picocoulomb, not pascals and parsecs', () => {
    expect(run(['5 pA']).value?.unitId).toBe('pico_amp')
    expect(run(['5 pC']).value?.unitId).toBe('pico_coulomb')
    expect(shown('5 pA to psi')).toBe('improper unit conversion')
    expect(shown('5 hPa')).toBe('5 hPa')
  })

  it('the table names still win: pa is pascal, pc parsec, kt knots, uF microfarads', () => {
    expect(shown('5 pa')).toBe(shown('5 Pa'))
    expect(shown('5 pc')).toBe('16.3078188858 ly')
    expect(run(['5 kt']).value?.unitId).toBe('kmh')
    expect(shown('4.7 uF', sig)).toBe('0.0000047 F')
    expect(shown('5 mPa')).toBe('5 mPa')
    expect(shown('5 Mg')).toBe('5 t')
  })
})

describe('a unit answer typed back in reads as the same quantity', () => {
  it('knots show as kt, since kn is a kilonewton', () => {
    const r = run(['10 m/s to knots'])
    expect(r.display).toBe('19.4384449244 kt')
    expect(run([`${r.display} to m/s`]).value?.n).toBeCloseTo(10, 9)
  })

  it('darcy shows as darcy, since D reads as days', () => {
    expect(shown('2 darcy to darcy')).toBe('2 darcy')
    expect(run(['2 darcy to darcy', 'ans + 1 darcy']).display).toBe('3 darcy')
  })

  it('kyr, Å and the particle masses read back', () => {
    expect(shown('2 kyr to yr')).toBe('2000 yr')
    expect(run(['q = 3 millennia', 'q to yr']).display).toBe('3000 yr')
    expect(shown('5 Å to nm')).toBe('0.5 nm')
    expect(run(['2 mₑ to electronrestmass']).value?.n).toBeCloseTo(2, 9)
    expect(run(['2 mₚ to u']).value?.n).toBeCloseTo(2.01455293324, 9)
  })

  it('sq in and cu in end a line as units, while `5 kg in` still waits for its target', () => {
    expect(shown('5 sq in')).toBe('32.258 cm²')
    expect(shown('1 L to cu in')).toBe('61.0237440947 in³')
    expect(run(['q = 10 sq in', 'q*2']).display).toBe('129.032 cm²')
    expect(shown('5 kg in')).toBe('')
  })
})

describe('decisions for the owner', () => {
  it.fails('an explicit * ends a bare radicand: √2*π is √2·π like √2*3 is √2·3', () => {
    expect(shown('√2*π')).toBe('4.44288293816')
  })

  it.fails('standard gravity typed back reads as g₀, not grams', () => {
    const r = run(['2 m/s²'])
    expect(run([`${r.display} to m/s²`]).value?.n).toBeCloseTo(2, 9)
  })

  it.fails('an elementary-charge answer typed back is charge, not a number times e', () => {
    const r = run(['3 coulomb to electron'])
    expect(run([`${r.display} to coulomb`]).value?.n).toBeCloseTo(3, 9)
  })

  it.fails('a unit answer chained through ans keeps its sig figs', () => {
    expect(run(['8.0 s', 'ans*0.670'], sig).display).toBe('5.4 s')
  })

  it.fails('`as` converts like `to` instead of multiplying by attoseconds', () => {
    expect(shown('17 yr as months')).toBe('204 mo')
  })
})
