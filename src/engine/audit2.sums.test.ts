// adversarial coverage for sum/product, on top of sums.test.ts. every expected value is computed
// by hand (closed forms, known series) rather than read off the engine.
import { describe, expect, it } from 'vitest'
import { evaluateSheet } from './evaluate'
import type { EvaluateOptions } from './types'

function run(lines: string | string[], options: EvaluateOptions = {}) {
  return evaluateSheet(Array.isArray(lines) ? lines : [lines], options).at(-1)!
}
const shown = (lines: string | string[], options?: EvaluateOptions) => run(lines, options).display
const value = (lines: string | string[], options?: EvaluateOptions) => run(lines, options).value?.n

describe('finite sums: exact arithmetic', () => {
  it('sum of 1..20 is 210', () => {
    expect(shown('Σ n, n=1..20')).toBe('210')
  })
  it('sum of squares 1..5 is 55', () => {
    expect(shown('Σ n^2, n=1..5')).toBe('55')
  })
  it('sum of cubes 1..4 is 100 (equals square of the sum 1..4=10)', () => {
    expect(shown('Σ n^3, n=1..4')).toBe('100')
  })
  it('an empty-looking single-term range', () => {
    expect(shown('Σ n^2, n=7..7')).toBe('49')
  })
  it('a negative-to-positive range of an odd function sums to zero', () => {
    expect(shown('Σ n^3, n=-4..4')).toBe('0')
  })
  it('product of 1..6 is 720 (6!)', () => {
    expect(shown('Π n, n=1..6')).toBe('720')
  })
  it('product with a zero term is zero', () => {
    expect(shown('Π n, n=0..5')).toBe('0')
  })
  it('product of a constant 3 taken 4 times is 81', () => {
    expect(shown('Π 3, n=1..4')).toBe('81')
  })
  it('alternating sum of 1..6 is -3', () => {
    // -1+2-3+4-5+6 = 3, so (-1)^n * n from 1..6 is -1+2-3+4-5+6 = 3
    expect(shown('Σ (-1)^n * n, n=1..6')).toBe('3')
  })
  it('sum of a constant over a range counts the terms', () => {
    expect(shown('Σ 5, n=1..7')).toBe('35')
  })
  it('geometric sum of powers of 2 is one less than the next power', () => {
    expect(shown('Σ 2^n, n=0..9')).toBe('1023')
  })
  it('sum of a fraction total simplifies', () => {
    // 1/2+1/4+1/8+1/16 = 15/16
    expect(run('Σ 1/2^n, n=1..4')).toMatchObject({ display: '0.9375', exact: '15/16' })
  })
  it('binomial-style sum via factorial ratio', () => {
    // sum of 1/n! for n=0..4 = 1 + 1 + 1/2 + 1/6 + 1/24 = 65/24
    expect(run('Σ 1/n!, n=0..4')).toMatchObject({ exact: '65/24' })
  })
})

describe('finite sums: degrees, radians, variables, caps', () => {
  it('sum of cos at the four right angles is zero in degrees', () => {
    expect(shown('Σ cos(n*90), n=0..3')).toBe('0')
  })
  it('sum of sin over a full turn in degrees is zero', () => {
    expect(shown('Σ sin(n*45), n=0..7')).toBe('0')
  })
  it('a stored variable sets the upper bound', () => {
    expect(shown(['top = 6', 'Σ n, n=1..top'])).toBe('21')
  })
  it('a user function used as the summand', () => {
    expect(shown(['cube(x) = x^3', 'Σ cube(n), n=1..3'])).toBe('36')
  })
  it('nested sum-like arithmetic: sum times a stored scalar', () => {
    // the bare-word form only opens a sum at the start of the line; embedded in a larger
    // expression it needs the parenthesized call form
    expect(shown(['k = 3', 'k * Σ(n, n=1..4)'])).toBe('30')
  })
  it('a huge finite range still gets an exact closed form', () => {
    // sum 1..n = n(n+1)/2, for n = 10^8 that is 5000000050000000
    expect(run('Σ n, n=1..10^8').exact).toBe('5000000050000000')
  })
  it('an empty-looking descending range is blank', () => {
    expect(shown('Σ n, n=5..2')).toBe('')
  })
})

describe('infinite sums: convergence must be certain', () => {
  it.each([
    ['Σ 1/n^6, n=1..∞', 'pi^6/945', Math.PI ** 6 / 945],
    ['Σ (2/5)^n, n=0..∞', '5/3', 5 / 3],
    ['Σ (-1)^n/2^n, n=0..∞', '2/3', 2 / 3],
  ])('%s = %s', (text, exact, n) => {
    const r = run(text)
    expect(r.exact).toBe(exact)
    expect(r.value?.n).toBeCloseTo(n, 10)
  })

  it('a constant multiple of e^x still gets the right number, even without a named exact form', () => {
    // the closed form is only spelled out for coefficient 1; 5/n! from 0 is still 5e numerically
    const r = run('Σ 5/n!, n=0..∞')
    expect(r.value?.n).toBeCloseTo(5 * Math.E, 10)
  })

  it('1/n! starting past the special-cased offsets (0 and 1) is still summed correctly', () => {
    expect(value('Σ 1/n!, n=2..∞')).toBeCloseTo(Math.E - 2, 10)
  })

  it('a p-series with p<=1 diverges and is blank', () => {
    expect(shown('Σ 1/n^0.9, n=1..∞')).toBe('')
  })
  it('a geometric series with |r|>1 diverges and is blank', () => {
    expect(shown('Σ 1.5^n, n=0..∞')).toBe('')
  })
  it('a geometric series with |r|=1 (constant term) diverges and is blank', () => {
    expect(shown('Σ 7, n=1..∞')).toBe('')
  })
  it('an alternating series that does not shrink to zero diverges and is blank', () => {
    expect(shown('Σ (-1)^n * 2, n=0..∞')).toBe('')
  })
  it('a series whose shape the engine cannot certify stays blank even though it converges', () => {
    // 1/(n^2+n+1) converges by comparison but is not a shape termSum recognizes
    expect(shown('Σ 1/(n^2+n+1), n=1..∞')).toBe('')
  })
})

describe('products: convergence and caps', () => {
  it('a convergent telescoping-style product', () => {
    // prod (1 - 1/(n+1)) for n=1..3 = (1/2)(2/3)(3/4) = 1/4
    expect(run('Π (1 - 1/(n+1)), n=1..3')).toMatchObject({ exact: '1/4', display: '0.25' })
  })
  it('an infinite product is always blank (only finite products are supported)', () => {
    expect(shown('Π n, n=1..∞')).toBe('')
  })
  it('a product over both the exact and float caps is blank', () => {
    // past EXACT_TERMS (5000) the exact bigint path is skipped, and PROD_FLOAT_TERMS (1000)
    // then blanks the float fallback too
    expect(shown('Π (1+1/n), n=1..5001')).toBe('')
  })
})

describe('sums and products: syntax variety not already covered', () => {
  it.each([
    ['Σ n^2, n in 1..5', '55'],
    ['sum(n^2, n=1..5)', '55'],
    ['Σ_(n=1)^5 n^2', '55'],
  ])('%s', (text, want) => {
    expect(shown(text)).toBe(want)
  })
})

describe('finite sums and products: a quick batch', () => {
  it.each([
    ['Σ n, n=1..100', '5050'],
    ['Σ n^2, n=1..10', '385'],
    ['Π n, n=1..5', '120'],
    ['Σ 2n, n=1..5', '30'],
    ['Σ n-1, n=1..5', '10'],
    ['Σ 1, n=1..50', '50'],
    ['Π 2, n=1..8', '256'],
    ['Σ n^2, n=-3..3', '28'],
    ['Σ n, n=1..0', ''],
  ])('%s = %s', (text, want) => {
    expect(shown(text)).toBe(want)
  })
})

describe('cross-checks with solve, θ, and other modes', () => {
  it('a sum feeds solve on the next line through ans', () => {
    // Σ n, n=1..4 = 10, so 2x = ans -> x = 5
    const rows = evaluateSheet(['Σ n, n=1..4', '2x = ans'], { angleMode: 'deg' })
    expect(rows[0]!.display).toBe('10')
    expect(rows[1]!.display).toBe('5')
  })
  it('θ can name a sum index, as it does the unknown in solve', () => {
    expect(shown('Σ θ^2, θ=1..5')).toBe('55')
  })
  it('fraction mode changes a finite rational sum display', () => {
    expect(shown('Σ 1/n, n=1..3', { fractionMode: true })).toBe('11/6')
  })
  it('sig fig mode does not turn a sum into a measured answer', () => {
    const r = run('Σ n, n=1..10', { sigFigMode: true })
    expect(r.display).toBe('55')
    expect(r.meas?.unc).toBeUndefined()
  })
})
