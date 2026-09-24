import { describe, expect, it } from 'vitest'
import { expectBlankOr, line } from './audit.helpers'
import { evaluateSheet } from './evaluate'

const sf = (text: string) => line(text, { sigFigMode: true }).display
const pm = (text: string) => line(text).display

describe('audit: sig-fig mode, products and quotients take the fewest sig figs', () => {
  it.each([
    ['4.0 / 3', '1.3'],
    ['1.50 * 1.50', '2.25'],
    ['2.0 * 3.00', '6.0'],
    ['0.00120 * 2', '0.00240'],
    ['1.23e3 * 2', '2.46e3'],
    ['9.99 * 1.0', '1.0e1'],
    ['0.5 * 2', '1'],
    ['3.0^2', '9.0'],
    ['sqrt(16.0)', '4.00'],
    ['2.00 * pi', '6.28'],
    ['1.0e-3 * 3', '0.0030'],
    ['6.0 / 2.00', '3.0'],
    ['12.34 * 2.0', '25'],
    ['100. * 2', '200.'],
    ['100 / 3.0', '33'],
    ['1.00 / 3', '0.333'],
    ['2.0^10', '1.0e3'],
    ['-2.50 * 2', '-5.00'],
    ['sin(90.0)', '1.00'],
    ['1.2e-7 * 3.00', '3.6e-7'],
    ['6.02e23 / 3.0', '2.0e23'],
    ['0.10 * 3', '0.30'],
    ['cbrt(27.0)', '3.00'],
  ])('%s → %s', (text, want) => {
    const got = sf(text)
    if (want === '200.') expect(['200.', '2.00e2']).toContain(got)
    else expect(got).toBe(want)
  })
})

describe('audit: sig-fig mode, sums keep the fewest decimal places', () => {
  it.each([
    ['1.0 + 2.26', '3.3'],
    ['12.0 - 11.9', '0.1'],
    ['10.0 - 10.0', '0.0'],
    ['100. + 0.7', '101'],
    ['1.234 + 5.6', '6.8'],
    ['0.001 + 0.0001', '0.001'],
    ['2.50 + 2.50', '5.00'],
  ])('%s → %s', (text, want) => {
    expect(sf(text)).toBe(want)
  })
})

describe('audit: sig-fig mode, logs keep as many decimals as the argument has sig figs', () => {
  it.each([
    ['log(100.)', '2.000'],
    ['ln(1.00)', '0.000'],
    ['log(2.0)', '0.30'],
    ['ln(10.0)', '2.303'],
    ['log(1.0e5)', '5.00'],
  ])('%s → %s', (text, want) => {
    expect(sf(text)).toBe(want)
  })
})

describe('audit: sig-fig mode leaves exact input alone', () => {
  it.each([
    ['2 + 2', '4'],
    ['10 / 4', '2.5'],
    ['7 * 3', '21'],
  ])('%s → %s', (text, want) => {
    expect(sf(text)).toBe(want)
  })

  it('keeps the unit on a measured unit answer', () => {
    expect(sf('2.50 cm * 2')).toBe('5.00 cm')
    expect(sf('2.50 m * 2.0 m')).toBe('5.0 m²')
    expect(sf('2.0 cm to in')).toBe('0.79 in')
    expect(sf('3.00 ft to m')).toBe('0.914 m')
    expect(sf('1.20 kg in g')).toBe('1.20e3 g')
  })

  it('carries sig figs through a variable', () => {
    const out = evaluateSheet(['x = 2.5', 'x * 2'], { sigFigMode: true })
    expect(out[1]!.display).toBe('5.0')
  })
})

describe('audit: ± propagation (worst case, ± binds tightest)', () => {
  it.each([
    ['(2.0 ± 0.1) - (1.0 ± 0.1)', '1.0 ± 0.2'],
    ['(4.0 ± 0.2) / 2', '2.00 ± 0.10'],
    ['3 * (2.0 ± 0.1)', '6.0 ± 0.3'],
    ['(3.0 ± 0.3)^2', '9.0 ± 1.8'],
    ['100 ± 1%', '100.0 ± 1.0'],
    ['5.0 ∓ 0.2', '5.0 ± 0.2'],
    ['-(5.0 ± 0.1)', '-5.0 ± 0.1'],
    ['2 ± 0.5 + 3', '5.0 ± 0.5'],
    ['(5.0 ± 0.1)^0.5', '2.24 ± 0.02'],
    ['1/(4.0 ± 0.1)', '0.250 ± 0.006'],
    ['(2.0 ± 0.1) * (3.0 ± 0.2)', '6.0 ± 0.7'],
    ['exp(1.0 ± 0.1)', '2.7 ± 0.3'],
    ['2.0 ± 0.1 * 2', '4.0 ± 0.2'],
    ['10.0 ± 0.5 - 2.0 ± 0.5', '8.0 ± 1.0'],
    ['(1.00 ± 0.01) * 1000', '1000 ± 10'],
    ['0.0120 ± 0.0005', '0.0120 ± 0.0005'],
    ['cos(60 ± 1)', '0.500 ± 0.015'],
  ])('%s → %s', (text, want) => {
    expect(pm(text)).toBe(want)
  })

  it('the value stays at the centre', () => {
    const r = line('(2.0 ± 0.1) * (3.0 ± 0.2)')
    expect(r.value?.n).toBeCloseTo(6, 12)
    expect(r.meas?.unc).toBeCloseTo(0.7, 12)
  })

  it('± with a unit keeps both', () => {
    expect(pm('(10 ± 1) * 2 m')).toBe('20 ± 2 m')
    expect(pm('3.0 ± 0.1 kg * 2')).toBe('6.0 ± 0.2 kg')
    expect(pm('(10 ± 1) cm to m')).toBe('0.10 ± 0.01 m')
    expect(pm('10 ± 5% m')).toBe('10.0 ± 0.5 m')
  })

  it('± through a unit conversion scales the uncertainty too', () => {
    const r = line('10.0 ± 0.5 kg to lb')
    expect(r.value?.n).toBeCloseTo(10 / 0.45359237, 9)
    expect(r.meas?.unc).toBeCloseTo(0.5 / 0.45359237, 9)
  })

  it('carries ± through a variable and through ans', () => {
    const vars = evaluateSheet(['x = 5.0 ± 0.1', 'x * 2'])
    expect(vars[1]!.display).toBe('10.0 ± 0.2')
    const ans = evaluateSheet(['2.0 ± 0.1', 'ans * 2'])
    expect(ans[1]!.display).toBe('4.0 ± 0.2')
  })

  it('a ± the walker cannot model is blank, never a bare number', () => {
    for (const t of ['max(1 ± 0.1, 2)', 'floor(2.5 ± 0.1)', '(1 ± 0.1)!', '2^(1 ± 0.1)']) {
      const d = pm(t)
      expect(d === '' || d.includes('±'), `${t} → ${d}`).toBe(true)
    }
  })

  it('a ± that cancels is blank or keeps its ±', () => {
    expectBlankOr('5 ± 0', ['5', '5 ± 0'])
  })
})
