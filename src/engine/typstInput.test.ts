import { describe, expect, it } from 'vitest'
import { expectNum, shown } from './audit.helpers'
import { evaluateLine } from './evaluate'
import { latexToAscii } from './plainMath'
import { typstToAscii } from './typstInput'

describe('typst input', () => {
  it('leaves ordinary calculator text alone', () => {
    expect(typstToAscii('sin(90)')).toBe('sin(90)')
    expect(typstToAscii('2*3+4')).toBe('2*3+4')
    expect(typstToAscii('$10 for lunch + 15% tip')).toBe('$10 for lunch + 15% tip')
    expectNum('sin(90)', 1)
    expectNum('2*3+4', 10)
  })

  it.each([
    ['$frac(1, 2)$', 0.5],
    ['frac(1, 2)+frac(1, 3)', 5 / 6],
    ['frac(frac(1, 2), 2)', 0.25],
    ['$sqrt(16)+2^3$', 12],
    ['root(3, 8)', 2],
    ['root(4, 81)', 3],
    ['binom(6, 2)', 15],
    ['2 times 3', 6],
    ['8 div 2', 4],
    ['2 dot.op 4', 8],
    ['$sin(90)$', 1],
    ['log(8, base: 2)', 3],
    ['ln(e)', 1],
    ['bold(2+3)', 5],
    ['attach(3, t: 2)', 9],
    ['lr(|-3|)', 3],
    ['$pi/2$', Math.PI / 2],
    ['limits(sum)_(n=1)^10 n', 55],
    ['$sum_(n=1)^10 n^2$', 385],
    ['product_(k=1)^4 k', 24],
    ['$product_(k=1)^5 k$', 120],
  ])('%s', (text, want) => expectNum(text, want))

  it('evaluates a typst integral', () => {
    expectNum('integral_0^1 x^2 dif x', 1 / 3)
    expectNum('$integral_0^(1) x^2 dif x$', 1 / 3)
    expectNum('integral_(0)^1 2x dif x', 1)
  })

  it('evaluates a typst derivative and limit', () => {
    expect(shown('frac(dif, dif x) x^2')).toBe(shown('d/dx x^2'))
    expect(shown('$frac(dif, dif x) x^3$')).toBe('3x²')
    expectNum('lim_(x arrow.r 0) sin(x)/x', 1, { angleMode: 'rad' })
    expectNum('$lim_(x -> 0) (1-cos(x))/x^2$', 0.5, { angleMode: 'rad' })
  })

  it('reads a repeating decimal and a conjugate', () => {
    expect(shown('0.overline(3)')).toBe(shown('1/3'))
    expect(shown('$0.1overline(6)$')).toBe(shown('1/6'))
    expectNum('re(overline(2+3i))', 2)
    expectNum('im(overline(2+3i))', -3)
  })

  it('does not turn plus-minus or a subscript into a different value', () => {
    expect(shown('5 plus.minus 2')).toBe(shown('5±2'))
    expect(shown('$5 plus.minus 2$')).toBe(shown('5±2'))
    expect(shown('$x_1 + 1$')).toBe('')
    expect(shown('mat(1, 2; 3, 4)')).toBe('')
  })

  it('solves an equation written in typst', () => {
    expect(shown('2x = frac(1, 2)')).toBe(shown('2x = 1/2'))
  })
})

function line(text: string, opts: { angleMode?: 'deg' | 'rad' } = {}) {
  return evaluateLine(text, { angleMode: 'deg', ...opts })
}

describe('plain math, LaTeX, and Typst', () => {
  it.each([
    ['\\frac{1}{2}+\\frac{1}{3}', 5 / 6],
    ['\\sqrt{16}', 4],
    ['\\sqrt[3]{8}', 2],
    ['\\sin(90)', 1],
    ['\\binom{5}{2}', 10],
    ['\\ln(e)', 1],
    ['\\left(\\frac{1}{2}\\right)', 0.5],
    ['1,000 + 1', 1001],
    ['1,000,000', 1_000_000],
    ['3.00 × 10^8', 3e8],
    ['what is 2 + 2?', 4],
    ['what is sin(90)?', 1],
    ['$root(3, 27)$', 3],
    ['$2^3$', 8],
    ['max(1,200)', 200],
  ])('%s', (text, want) => {
    const r = line(text)
    expect(r.value?.n, r.display).toBeCloseTo(want, 10)
  })

  it('a repeating decimal matches the fraction', () => {
    expect(line('0.\\overline{3}').value!.n).toBeCloseTo(line('1/3').value!.n, 12)
    expect(line('$0.overline(9)$').value!.n).toBeCloseTo(1, 12)
  })

  it('a subscript is not a power, so the line stays blank', () => {
    expect(line('x_{1}+1').display).toBe('')
    expect(latexToAscii('x_{1}+1')).toContain('_')
  })

  it('LaTeX pm is the uncertainty mark', () => {
    expect(line('(5.0 \\pm 0.1)*2').display).toBe('10.0 ± 0.2')
  })

  it('a limit written with \\infty is the same limit', () => {
    const latex = line('lim x->\\infty 1/x', { angleMode: 'rad' })
    const plain = line('lim x->∞ 1/x', { angleMode: 'rad' })
    expect(latex.display).toBe('0')
    expect(latex.value!.n).toBeCloseTo(plain.value!.n, 12)
  })

  it('ordinary text is not rewritten as Typst', () => {
    expect(typstToAscii('2+2')).toBe('2+2')
    expect(typstToAscii('sin(90)')).toBe('sin(90)')
    expect(line('2+2').display).toBe('4')
  })

  it('Typst frac and a product agree with the ascii forms', () => {
    expect(line('$frac(1, 2)+frac(1, 3)$').value!.n).toBeCloseTo(5 / 6, 12)
    expect(line('product_(k=1)^4 k').display).toBe('24')
  })
})
