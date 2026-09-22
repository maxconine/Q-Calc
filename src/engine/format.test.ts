import { describe, expect, it } from 'vitest'
import { evaluateLine } from './evaluate'
import { formatNumber } from './format'

describe('significant figures', () => {
  it('rounds π to the requested significant figures', () => {
    expect(formatNumber(Math.PI, 4)).toBe('3.142')
    expect(formatNumber(Math.PI, 6)).toBe('3.14159')
  })

  it('keeps small exact decimals like unit conversions', () => {
    expect(formatNumber(50.8, 12)).toBe('50.8')
    expect(formatNumber(0.0508, 12)).toBe('0.0508')
  })

  it('does not throw on extreme magnitudes', () => {
    expect(() => formatNumber(1e-320, 12)).not.toThrow()
    expect(() => formatNumber(1e308, 12)).not.toThrow()
    expect(() => formatNumber(Number.NaN, 12)).not.toThrow()
    expect(() => formatNumber(Number.POSITIVE_INFINITY, 12)).not.toThrow()
  })

  it('applies significant figures to live results', () => {
    expect(evaluateLine('pi', { sigFigs: 4 }).display).toBe('3.142')
    expect(evaluateLine('2 in', { sigFigs: 3 }).display).toBe('50.8 mm')
  })

  it('keeps full precision on the numeric value while rounding only the display', () => {
    const r = evaluateLine('pi', { sigFigs: 4 })
    expect(r.display).toBe('3.142')
    expect(r.value?.n).toBe(Math.PI)
    const chained = evaluateLine('ans * 2', { ans: r.value?.n, sigFigs: 4 })
    expect(chained.display).toBe(formatNumber(Math.PI * 2, 4))
    expect(chained.value?.n).toBe(Math.PI * 2)
  })

  const nums = [
    0, 1, -1, 2, 10, 12.5, 50.8, 0.0508, 0.001, 3.141592653589793, 2.718281828, 99.9, 100, 123.456, 0.333333, 1e-5, 1e6, 1e11, 9.99e-7, 6.02e23, -50.8, -0.125,
    1.2345, 9, 16, 25, 36, 49, 64, 81, 0.5, 0.25, 0.125, 7.5, 8.75, 1024, 2048, 4096, 1.5e-8, 2.5e9,
  ]
  it.each(nums.flatMap((n) => [2, 3, 4, 6, 8, 12].map((figs) => ({ n, figs }))))(
    'formatNumber($n, $figs) is stable',
    ({ n, figs }) => {
      expect(() => formatNumber(n, figs)).not.toThrow()
      const s = formatNumber(n, figs)
      expect(typeof s).toBe('string')
      expect(s.length).toBeGreaterThan(0)
    },
  )
})
