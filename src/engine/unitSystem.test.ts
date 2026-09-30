import { describe, expect, it } from 'vitest'
import { evaluateLine, evaluateSheet } from './evaluate'

/**
 * An answer's unit follows the units typed: mostly metric in, metric out; mostly US (SAE) in, US out.
 * Stress, strain and speed work used to come back in the other system or in raw SI (`F = 10 kN` stored
 * as 2248 lbf, `10 kN / 20 mm^2` as 500000000 Pa, `3 mm / 2 s` as 0.0015 m/s).
 */
const shown = (text: string) => evaluateLine(text).display
const sheet = (...lines: string[]) => evaluateSheet(lines).map((r) => r.display)

describe('the answer stays in the system of the units typed', () => {
  it('adds in the unit typed', () => {
    expect(shown('3mm+8mm')).toBe('11 mm')
    expect(shown('3in+4in')).toBe('7 in')
    expect(shown('2 ft + 3 in')).toBe('2.25 ft')
  })

  it('multiplies lengths into an area of the same length', () => {
    expect(shown('3mm*8mm')).toBe('24 mm²')
    expect(shown('3in*4in')).toBe('12 in²')
    expect(shown('2 ft * 3 ft')).toBe('6 ft²')
    expect(shown('(3 in)^3')).toBe('27 in³')
    expect(shown('pi * (5 mm)^2')).toBe('78.5398163397 mm²')
  })

  it('folds lengths of one system into one unit', () => {
    expect(shown('6 in * 2 ft')).toBe('144 in²')
    expect(shown('1 km * 1 mm')).toBe('1 m²')
  })

  it('goes with the majority, and an even split stays SI', () => {
    expect(shown('2 in * 3 in / 4 mm')).toBe('38.1 in')
    expect(shown('3in*2mm')).toBe('0.0001524 m²')
  })
})

describe('stress and strain', () => {
  it('metric stress is in MPa, not raw Pa', () => {
    expect(shown('5000 N / 10 mm^2')).toBe('500 MPa')
    expect(shown('10 kN / 20 mm^2')).toBe('500 MPa')
    expect(shown('10 kN / (5 mm * 4 mm)')).toBe('500 MPa')
    expect(shown('10 kN / (pi * (10 mm)^2 / 4)')).toBe('127.323954474 MPa')
    expect(shown('1000 kg/m^3 * 9.81 m/s^2 * 10 m')).toBe('98.1 kPa')
  })

  it('US stress is in psi', () => {
    expect(shown('1000 lbf / 2 in^2')).toBe('500 psi')
    expect(shown('1000 lbf / (pi * (0.5 in)^2 / 4)')).toBe('5092.95817894 psi')
    expect(shown('1000 lbf * 12 in / 2 in^3')).toBe('6000 psi')
  })

  it('Hooke’s law keeps the modulus’s system and an engineering prefix', () => {
    expect(shown('200 GPa * 0.001')).toBe('200 MPa')
    expect(shown('29000 ksi * 0.002')).toBe('58 ksi')
  })

  it('strain is a plain ratio', () => {
    expect(shown('250 MPa / 200 GPa')).toBe('0.00125')
    expect(shown('0.5 mm / 100 mm')).toBe('0.005')
  })

  it('elongation PL/AE comes back as a length in the typed system', () => {
    expect(shown('5 kN * 2 m / (200 GPa * 100 mm^2)')).toBe('0.5 mm')
    expect(shown('1000 lbf * 10 in / (29e6 psi * 0.5 in^2)')).toBe('0.000689655172414 in')
  })

  it('stress times area is a force', () => {
    expect(shown('100 MPa * 50 mm^2')).toBe('5 kN')
    expect(shown('20 ksi * 0.5 in^2')).toBe('10 kip')
  })

  it('a metric stress/strain sheet stays metric from start to end', () => {
    expect(sheet('F = 10 kN', 'A = 20 mm^2', 'sigma = F/A', 'E = 200 GPa', 'eps = sigma/E', 'L = 500 mm', 'delta = eps*L')).toEqual([
      '10 kN',
      '20 mm²',
      '500 MPa',
      '200 GPa',
      '0.0025',
      '500 mm',
      '1.25 mm',
    ])
  })

  it('a US stress/strain sheet stays US from start to end', () => {
    expect(sheet('P = 1000 lbf', 'A = 0.5 in^2', 's = P/A', 'E = 29e6 psi', 'strain = s/E', 'L = 10 in', 'dL = strain*L')).toEqual([
      '1000 lbf',
      '0.5 in²',
      '2000 psi',
      '29000000 psi',
      '0.0000689655172414',
      '10 in',
      '0.000689655172414 in',
    ])
  })

  it('bending stress Mc/I', () => {
    expect(sheet('M = 500 lbf*ft', 'c = 2 in', 'I = 10 in^4', 'M*c/I')).toEqual(['500 lbf·ft', '2 in', '10 in^4', '1200 psi'])
    expect(sheet('M = 5 kN*m', 'c = 50 mm', 'I = 1e6 mm^4', 'M*c/I')).toEqual(['5 kN·m', '50 mm', '1000000 mm^4', '250 MPa'])
  })
})

describe('E is free to hold a modulus or an energy', () => {
  it('stores and uses E, while lowercase e stays Euler’s number', () => {
    expect(sheet('E = 5', 'E*2', '2E', 'E/2 + e')).toEqual(['5', '10', '10', '5.21828182846'])
    expect(sheet('E = 200 GPa', 'E*0.001', '2 E')).toEqual(['200 GPa', '200 MPa', '400 GPa'])
  })

  it('scientific notation still reads as a power of ten with E stored', () => {
    expect(sheet('E = 5', '1E3', '2.5E-3 + 1', '3e2')).toEqual(['5', '1000', '1.0025', '300'])
    expect(sheet('E = 200 GPa', '1E3', '2E3 MPa / E')).toEqual(['200 GPa', '1000', '0.01'])
  })

  it('E = hc/λ', () => {
    expect(sheet('h = 6.626e-34', 'c = 3e8', 'lambda = 500e-9', 'E = h*c/lambda').at(-1)).toBe('3.9756e-19')
  })

  it('an unassigned E is still e, and E = mc² still isolates', () => {
    expect(sheet('E*2')).toEqual(['5.43656365692'])
    expect(sheet('E = mc^2, isolate c')).toEqual(['c = ±sqrt(E/m)'])
  })
})

describe('moments, loads per length and densities read in the typed units', () => {
  it('a force times a length is a moment', () => {
    expect(shown('50 lbf * 12 in')).toBe('600 lbf·in')
    expect(shown('1000 N * 500 mm')).toBe('500 N·m')
  })

  it('a load per length', () => {
    expect(shown('12 kN / 3 m')).toBe('4 kN/m')
    expect(shown('100 lbf / 2 ft')).toBe('50 lbf/ft')
  })

  it('a density, and a density times a depth', () => {
    expect(shown('1 lb / 1 in^3')).toBe('1 lb/in³')
    expect(shown('62.4 lb/ft^3 * 10 ft')).toBe('624 lb/ft²')
  })

  it('US work is ft·lbf and US heat is BTU', () => {
    expect(shown('1/2 * 3 lb * (4 ft/s)^2')).toBe('0.745942804118 ft·lbf')
    expect(shown('1 hp * 1 hr')).toBe('2544.43357907 BTU')
    expect(shown('5 lb * 32 ft/s^2')).toBe('4.97295202745 lbf')
  })
})

describe('speed and motion', () => {
  it('a distance over a time', () => {
    expect(shown('10 m / 2 s')).toBe('5 m/s')
    expect(shown('100 ft / 4 s')).toBe('25 ft/s')
    expect(shown('10 mi / 2 hr')).toBe('5 mph')
    expect(shown('100 km / 2 hr')).toBe('50 km/h')
    expect(shown('3 mm / 2 s')).toBe('1.5 mm/s')
    expect(shown('3 in / 1 min')).toBe('3 in/min')
  })

  it('a speed times a time is a distance in the same system', () => {
    expect(shown('5 m/s * 10 s')).toBe('50 m')
    expect(shown('60 mph * 2 hr')).toBe('120 mi')
    expect(shown('32 ft/s^2 * 2 s')).toBe('64 ft/s')
    expect(sheet('v = 10 m/s', 't = 3 s', 'v*t', 'v/t')).toEqual(['10 m/s', '3 s', '30 m', '3.33333333333 m/s²'])
    expect(sheet('v = 60 mph', 't = 2 hr', 'v*t')).toEqual(['60 mph', '2 hr', '120 mi'])
  })
})

describe('a stored quantity keeps the unit it was typed in', () => {
  it('does not store the other system’s counterpart', () => {
    expect(sheet('weight = 2 kg', 'price = 3', 'price*weight')).toEqual(['2 kg', '3', '6 kg'])
    expect(sheet('d = 5 cm', 'd * 2')).toEqual(['5 cm', '10 cm'])
  })

  it('a lone quantity on its own line still shows its counterpart', () => {
    expect(shown('5 mm')).toBe('0.196850393701 in')
  })

  it('a compound answer reads back as itself when stored', () => {
    expect(sheet('w = 12 kN / 3 m', 'w * 2')).toEqual(['4 kN/m', '8 kN/m'])
    expect(sheet('T = 50 lbf * 12 in', 'T / 2')).toEqual(['600 lbf·in', '300 lbf·in'])
    expect(sheet('rho = 1 lb / 1 in^3', 'rho * 2 in^3')).toEqual(['1 lb/in³', '2 lbs'])
  })
})
