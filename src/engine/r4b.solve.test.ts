import { describe, expect, it } from 'vitest'
import { evaluateSheet } from './evaluate'

const run = (lines: string[], opts = {}) => evaluateSheet(lines, { angleMode: 'rad', ...opts }).at(-1)!
const last = (lines: string[], opts = {}) => run(lines, opts).display

describe('a root beside a pole in the same grid cell', () => {
  it('tan(x) = x in degrees finds the roots near ±89.36', () => {
    expect(last(['tan(x) = x'], { angleMode: 'deg' })).toMatch(/^-?[\d.]+, -89\.3588, 0, 89\.3588/)
  })

  it.each([
    [['tan(x) = 2x'], { angleMode: 'deg' }, /-89\.6806, 0, 89\.6806/],
    [['tan(x) = 100x'], {}, /-1\.5644, 0, 1\.5644/],
    [['tan(x) = 1e10'], {}, /-1\.5708, 1\.5708/],
    [['tan(x) = x'], {}, /-4\.49341, 0, 4\.49341/],
  ])('%j', (lines, opts, want) => {
    expect(last(lines, opts)).toMatch(want)
  })

  it('each root found is a root', () => {
    const r = run(['tan(x) = x'], { angleMode: 'deg' })
    for (const x of r.solve!.roots) {
      const t = Math.tan((x * Math.PI) / 180)
      expect(Math.abs(t - x)).toBeLessThan(1e-6 * Math.max(1, Math.abs(x)))
    }
  })
})

describe('a minimum that stays above zero is not a root', () => {
  it.each([
    [['x^2 + 1e-30 = 0']],
    [['sin(x)^2 + 1e-20 = 0']],
  ])('%j', (lines) => {
    expect(['no real solution', 'no solution found', '']).toContain(last(lines))
  })

  it.each([
    [['x sin(x) = 1'], '-2.7726, -1.11416, 1.11416, 2.7726, …'],
    [['sin(x)^2 = 0'], '0, 3.14159'],
    [['abs(sin(x)) = 0'], '0, 3.14159'],
    [['(e^x - 2)^2 = 0'], '0.693147'],
    [['(sin(x) - 0.5)^2 = 0'], '0.523599, 2.61799'],
    [['(ln(x) - 1)^2 = 0'], '2.71828'],
    [['sin(x) = 1'], '1.57079632679'],
  ])('%j still touches: %s', (lines, shown) => {
    expect(last(lines)).toBe(shown)
  })

  it('sin(x) = 1 in degrees still touches at 90', () => {
    expect(last(['sin(x) = 1'], { angleMode: 'deg' })).toBe('90')
  })
})

describe('a hole is not a root', () => {
  it.each([
    [['(x^2-1)/(x-1) = 2']],
    [['sin(x)/x = 1']],
    [['x/x = 2']],
  ])('%j', (lines) => {
    expect(['no solution', 'no solution found', '']).toContain(last(lines))
  })

  it('x ln(x) = 0 is 1, not a sliver past 0 where x ln(x) is undefined', () => {
    expect(last(['x ln(x) = 0'])).toBe('1')
  })

  it('0^0 is 1 here, so x^x = 1 keeps 0', () => {
    expect(last(['x^x = 1'])).toBe('0, 1')
  })

  it('sin(1/x) = 0 shows real roots, not slivers beside 0', () => {
    const r = run(['sin(1/x) = 0'])
    for (const x of r.solve!.roots) {
      expect(Math.abs(x)).toBeGreaterThan(1e-12)
      // 1/x near 1e9 is only good to about 1e-7
      expect(Math.abs(Math.sin(1 / x))).toBeLessThan(1e-4)
    }
  })
})

describe('a domain edge that is a root', () => {
  it.each([
    [['sqrt(x^2-2) = 0'], '±1.41421356237'],
    [['sqrt(2 - x^2) = 0'], '±1.41421356237'],
    [['sqrt(x - 0.3) = 0'], '0.3'],
    [['sqrt(x) = 0'], '0'],
    [['acos(x) = 0'], '1'],
    [['sqrt(4 - x^2) = 0'], '±2'],
  ])('%j → %s', (lines, shown) => {
    expect(last(lines)).toBe(shown)
  })

  it('an edge that misses zero is not a root', () => {
    expect(last(['sqrt(x - 1) + 1 = 1.5'])).toBe('1.25')
  })
})
