import { describe, expect, it } from 'vitest'
import { evaluateSheet } from './evaluate'
import { hurwitz } from './sums'
import type { EvaluateOptions } from './types'

function run(lines: string | string[], options: EvaluateOptions = {}) {
  return evaluateSheet(Array.isArray(lines) ? lines : [lines], options).at(-1)!
}

const shown = (lines: string | string[], options?: EvaluateOptions) => run(lines, options).display
const value = (lines: string | string[], options?: EvaluateOptions) => run(lines, options).value?.n

describe('the ways to write a sum', () => {
  it.each([
    'Σ n^2, n = 1..10',
    'Σ n^2, n=1...10',
    'Σ n^2, n = 1 to 10',
    'Σ n^2, n from 1 to 10',
    'Σ n^2, 1..10',
    'Σ(n^2, n = 1..10)',
    'Σ(n^2, n, 1, 10)',
    'Σ(n^2, n=1, 10)',
    'Σ(n^2, 1, 10)',
    'sum(n^2, n, 1, 10)',
    'sum n^2 for n = 1 to 10',
    'sum of n^2 for n from 1 to 10',
    'sum n^2 from n = 1 to 10',
    'sum from n = 1 to 10 of n^2',
    'Σ n = 1..10, n^2',
    'Σ_{n=1}^{10} n^2',
    'Σ_(n=1)^10 n^2',
    '\\sum_{n=1}^{10} n^2',
    '\\sum_{n=1}^{10} n^{2}',
    '∑ n^2, n = 1..10',
    'Σn^2, n=1..10',
  ])('%s', (line) => {
    expect(shown(line)).toBe('385')
  })

  it('keeps arithmetic after a parenthesized sum outside the sum', () => {
    expect(shown('(Σ_k=2^21 3k^2)+2-44')).toBe('9888')
    expect(shown('2*(Σ_k=1^4 k)-1')).toBe('19')
  })

  it('takes the one free letter as the index', () => {
    expect(shown('Σ k, 1..100')).toBe('5050')
    expect(shown('Σ 2^i, 0..10')).toBe('2047')
    expect(shown('Σ x*y, 1..3')).toBe('')
    expect(shown(['n = 4', 'Σ n^2, 1..3'])).toBe('')
  })

  it('stays blank while the range is half typed', () => {
    for (const line of ['Σ', 'Σ n^2', 'Σ n^2,', 'Σ n^2, n', 'Σ n^2, n =', 'Σ n^2, n = 1', 'Σ n^2, n = 1..', 'Σ n^2, n = 1 to', 'Σ(n^2, n = 1..10']) {
      expect(shown(line), line).toBe('')
    }
  })

  it('is blank when the range is unclear or empty', () => {
    expect(shown('Σ n, n = 10..1')).toBe('')
    expect(shown('Σ n, n = 1..10.5')).toBe('')
    expect(shown('Σ n, n = 1..10 + 5')).toBe('')
    expect(shown('Σ n, e = 1..3')).toBe('')
  })
})

describe('finite sums', () => {
  it('are exact for rationals', () => {
    expect(run('Σ 1/n, n=1..10')).toMatchObject({ display: '2.92896825397', exact: '7381/2520' })
    expect(run('Σ 0.1, n=1..10').display).toBe('1')
    expect(run('Σ 1/n, n=1..4', { fractionMode: true }).display).toBe('25/12')
    expect(shown('Σ (-1)^n, n=1..11')).toBe('-1')
    expect(shown('Σ n!, n=1..10')).toBe('4037913')
  })

  it('use a closed form for polynomials over huge ranges', () => {
    expect(run('Σ n^3, n=1..10^6').exact).toBe('250000500000250000000000')
    expect(value('Σ n^2, n=1..10^9')).toBeCloseTo(3.333333338333333e26, -12)
    expect(shown('Σ (2n+1), n=0..999999')).toBe(shown('10^12'))
    expect(shown('Σ n, n=-5..5')).toBe('0')
  })

  it('give up on huge ranges with no closed form', () => {
    const t = performance.now()
    expect(shown('Σ sin(n), n=1..10^7')).toBe('')
    expect(shown('Σ 1/n, n=1..10^9')).toBe('')
    expect(performance.now() - t).toBeLessThan(500)
  })

  it('honor degrees and radians', () => {
    expect(shown('Σ sin(n*30), n=1..12')).toBe('0')
    expect(shown('Σ cos(n*60), n=1..6')).toBe('0')
    expect(value('Σ sin(n), n=1..10', { angleMode: 'rad' })).toBeCloseTo(1.4111883712180104, 13)
    expect(shown('Σ sin(n), n=1..10', { angleMode: 'rad' })).toBe('1.41118837122')
  })

  it('see variables and functions', () => {
    expect(shown(['m = 10', 'Σ k, k=1..m'])).toBe('55')
    expect(shown(['a = 3', 'Σ a*n, n=1..4'])).toBe('30')
    expect(shown(['f(x) = x^2 + 1', 'Σ f(n), n=1..10'])).toBe('395')
    // the index shadows a variable of the same name
    expect(shown(['n = 5', 'Σ n^2, n=1..3'])).toBe('14')
    expect(shown(['s = Σ n, n=1..10', 's*2'])).toBe('110')
    expect(shown(['Σ n, n=1..10', 'ans + 1'])).toBe('56')
    expect(shown('Σ i^2, i=1..3')).toBe('14')
  })

  it('work inside a larger expression', () => {
    expect(shown('2Σ(n, n=1..10)')).toBe('110')
    expect(shown('Σ(1/n, n=1..3) + 1')).toBe('2.83333333333')
    expect(shown('sqrt(Σ(n^3, n=1..4))')).toBe('10')
    expect(shown('Σ(n, n=1..3) * Π(n, n=1..3)')).toBe('36')
    expect(shown('4 sum_(n=1)^10 n')).toBe('220')
    expect(shown('(2*sum_(n=1)^10 n)+3')).toBe('113')
    expect(shown('(sum_(n=1)^10 n)+2')).toBe('57')
    expect(shown('2 * sum n, n=1..10')).toBe('110')
    expect(shown('(sum n^2, n=1..4)+1')).toBe('31')
    expect(shown('(sum n, n=1..4)*(prod k, k=1..3)')).toBe('60')
    expect(shown('10 - sum_(k=1)^4 k')).toBe('0')
    expect(shown('sqrt(sum_(n=1)^3 n^3)')).toBe('6')
  })

  it('are undefined when a term is', () => {
    expect(shown('Σ 1/n, n=0..5')).toBe('undefined')
    expect(shown('Σ sqrt(n), n=-1..3')).toBe('undefined')
  })

  it('leave units blank', () => {
    expect(shown('Σ n cm, n=1..3')).toBe('')
  })
})

describe('products', () => {
  it('multiply the terms', () => {
    expect(shown('Π n, n=1..10')).toBe('3628800')
    expect(shown('prod(k, k, 1, 5)')).toBe('120')
    expect(run('Π (1 - 1/n^2), n=2..100')).toMatchObject({ exact: '101/200', display: '0.505' })
    expect(shown('Π (1+1/n), n=1..1000')).toBe('1001')
    expect(value('Π (1+1/n^2), n=1..500')).toBeCloseTo(3.669, 2)
  })

  it('are blank when infinite or out of range', () => {
    expect(shown('Π (1+1/n^2), n=1..∞')).toBe('')
    expect(shown('Π n, n=1..5000')).toBe('')
  })
})

describe('lists keep working', () => {
  it.each([
    ['Σ(1, 2, 3)', '6'],
    ['sum(1, 2, 3)', '6'],
    ['Σ([1, 2, 3])', '6'],
    ['Π(2, 3, 4)', '24'],
    ['prod(2, 3, 4)', '24'],
    ['3prod(2, 3)', '18'],
    ['Σ(1, 2, 3)*2', '12'],
  ])('%s → %s', (line, answer) => {
    expect(shown(line)).toBe(answer)
  })

  it('reads four numbers as a list when the second is a variable not in the body', () => {
    expect(shown(['x = 1', 'y = 2', 'sum(x, y, 1, 10)'])).toBe('14')
  })
})

describe('infinite sums', () => {
  it.each([
    ['Σ 1/n^2, n=1..∞', 'pi^2/6', Math.PI ** 2 / 6],
    ['Σ 1/n^4, n=1..∞', 'pi^4/90', Math.PI ** 4 / 90],
    ['Σ 1/n^2, n=2..∞', 'pi^2/6 - 1', Math.PI ** 2 / 6 - 1],
    ['Σ (-1)^(n+1)/n^2, n=1..∞', 'pi^2/12', Math.PI ** 2 / 12],
    ['Σ (-1)^n/n^2, n=1..∞', '-pi^2/12', -(Math.PI ** 2) / 12],
    ['Σ (-1)^(n+1)/n, n=1..∞', 'ln(2)', Math.LN2],
    ['Σ 1/(n 2^n), n=1..∞', 'ln(2)', Math.LN2],
    ['Σ 1/n!, n=0..∞', 'e', Math.E],
    ['Σ 1/n!, n=1..∞', 'e - 1', Math.E - 1],
    ['Σ 2^n/n!, n=0..∞', 'e^2', Math.E ** 2],
    ['Σ 1/2^n, n=1..∞', '1', 1],
    ['Σ (1/3)^n, n=0..∞', '3/2', 1.5],
    ['Σ 3*(-1/2)^n, n=0..inf', '2', 2],
    ['Σ 1/3^n + 1/2^n, n=0..∞', '7/2', 3.5],
  ])('%s = %s', (line, exact, n) => {
    const r = run(line)
    expect(r.exact).toBe(exact)
    expect(r.value?.n).toBeCloseTo(n, 13)
  })

  it.each([
    ['Σ 1/n^3, n=1..∞', 1.2020569031595942],
    ['Σ 1/n^1.5, n=1..∞', 2.612375348685488],
    ['Σ (-1)^(n+1)/sqrt(n), n=1..∞', 0.6048986434216304],
    ['Σ n/2^n, n=1..∞', 2],
    ['Σ n^2/2^n, n=1..∞', 6],
    ['Σ n^5*0.9^n, n=1..∞', 87723090],
    ['Σ 1/n!, n=3..∞', Math.E - 2.5],
    ['Σ 0.999^n/n^2, n=1..∞', 1.637022605276118],
  ])('%s ≈ %d', (line, n) => {
    expect(value(line)).toBeCloseTo(n, n > 1000 ? 4 : 12)
  })

  it('are blank when they diverge', () => {
    for (const line of ['Σ 1/n, n=1..∞', 'Σ 1, n=1..∞', 'Σ (-1)^n, n=0..∞', 'Σ 2^n, n=0..∞', 'Σ n, n=1..∞', 'Σ 1/sqrt(n), n=1..∞', 'Σ (-1)^n*n, n=1..∞']) {
      expect(shown(line), line).toBe('')
    }
  })

  it('are blank when convergence is not certain from the shape', () => {
    for (const line of ['Σ 1/(n^2+1), n=1..∞', 'Σ 1/(n(n+1)), n=1..∞', 'Σ sin(n)/n^2, n=1..∞', 'Σ 1/n^2, n=0..∞', 'Σ 1/ln(n), n=2..∞']) {
      expect(shown(line), line).toBe('')
    }
  })
})

describe('hurwitz zeta', () => {
  it('matches known values', () => {
    expect(hurwitz(2, 1)).toBeCloseTo(Math.PI ** 2 / 6, 15)
    expect(hurwitz(3, 1)).toBeCloseTo(1.2020569031595942, 15)
    expect(hurwitz(0.5, 1)).toBeCloseTo(-1.4603545088095868, 14)
    expect(hurwitz(2, 0.5)).toBeCloseTo(Math.PI ** 2 / 2, 14)
    expect(hurwitz(40, 1)).toBeCloseTo(1 + 2 ** -40, 15)
    expect(hurwitz(1, 1)).toBeNull()
  })
})

describe('sum and product bounds', () => {
  it.each([
    ['Σ n, n=4..4', '4'],
    ['Π n, n=5..5', '5'],
    ['Σ 0, n=1..100', '0'],
    ['Π 1, n=1..40', '1'],
    ['Π n, n=0..5', '0'],
    ['Σ n, n=-2..2', '0'],
    ['Σ n^2, n=-2..2', '10'],
    ['Σ (-1)^n * n, n=1..4', '2'],
    ['Π (n/(n+1)), n=1..9', '0.1'],
  ])('%s → %s', (text, display) => {
    expect(shown(text)).toBe(display)
  })

  it('a one-term geometric sum is the first term', () => {
    expect(shown('Σ 1/2^n, n=0..0')).toBe('1')
    expect(shown('Σ 1/2^n, n=0..3', { fractionMode: true })).toBe('15/8')
  })

  it('an empty or reversed range stays blank', () => {
    expect(shown('Σ n, n=5..4')).toBe('')
    expect(shown('Π n, n=3..1')).toBe('')
    expect(shown('Σ n^2, n=1..')).toBe('')
  })

  it('the index shadows a stored variable of the same name', () => {
    const rows = evaluateSheet(['k = 100', 'Σ k, k=1..4', 'k'])
    expect(rows[1]!.display).toBe('10')
    expect(rows[2]!.display).toBe('100')
  })

  it('a product through a zero term is zero even when a later term is undefined', () => {
    expect(shown('Π n, n=-1..2')).toBe('0')
  })

  it('an infinite geometric with ratio 1 is blank', () => {
    expect(shown('Σ 1^n, n=0..∞')).toBe('')
    expect(run('Σ (1/2)^n, n=0..∞').exact).toBe('2')
  })

  it('a sum of a user function of the index', () => {
    const rows = evaluateSheet(['f(t) = 2t+1', 'Σ f(n), n=0..3'])
    expect(rows[1]!.display).toBe('16')
  })
})
