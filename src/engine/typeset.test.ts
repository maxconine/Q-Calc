import { describe, expect, it } from 'vitest'
import { evaluateLine, evaluateSheet } from './evaluate'
import { latexToAscii, tryPlainMath } from './plainMath'

function n(text: string, angleMode: 'deg' | 'rad' = 'deg'): number {
  const r = evaluateLine(text, { angleMode })
  if (r.value == null || !Number.isFinite(r.value.n)) {
    throw new Error(`No numeric result for ${JSON.stringify(text)} → ${r.display} ${r.error ?? ''}`)
  }
  return r.value.n
}

function closeTo(actual: number, expected: number, eps = 1e-8): void {
  expect(Math.abs(actual - expected)).toBeLessThan(eps)
}

describe('Scientific functions and typeset input', () => {
  it('roots, powers, and fractions', () => {
    closeTo(n('sqrt(16)'), 4)
    closeTo(n('cbrt(27)'), 3)
    closeTo(n('nthroot(81, 4)'), 3)
    closeTo(n('2^8'), 256)
    closeTo(n('10^-3'), 0.001)
    closeTo(n('\\sqrt{16}+2^{3}'), 12)
    closeTo(n('\\sqrt[3]{8}'), 2)
    closeTo(n('\\frac{1}{2}+\\frac{1}{3}'), 5 / 6)
  })

  it('trig in degrees by default', () => {
    closeTo(n('sin(90)'), 1)
    closeTo(n('cos(0)'), 1)
    closeTo(n('tan(45)'), 1)
    closeTo(n('arcsin(1)'), 90)
    closeTo(n('csc(90)'), 1)
    closeTo(n('\\sin\\left(90\\right)'), 1)
  })

  it('trig in radians when asked', () => {
    closeTo(n('sin(pi/2)', 'rad'), 1)
    closeTo(n('cos(0)', 'rad'), 1)
    closeTo(n('arcsin(1)', 'rad'), Math.PI / 2)
  })

  it('logs, exp, factorial, combinatorics', () => {
    closeTo(n('ln(e)'), 1)
    closeTo(n('log(100)'), 2)
    closeTo(n('log_2(8)'), 3)
    closeTo(n('\\ln\\left(e\\right)'), 1)
    closeTo(n('\\log\\left(1000\\right)'), 3)
    closeTo(n('5!'), 120)
    closeTo(n('nCr(6,2)'), 15)
    closeTo(n('nPr(6,2)'), 30)
    closeTo(n('\\operatorname{nCr}\\left(6,2\\right)'), 15)
  })

  it('stats, abs, floor, complex pieces', () => {
    closeTo(n('abs(-12.5)'), 12.5)
    closeTo(n('\\left|-3\\right|'), 3)
    closeTo(n('mean(1,5,5,10)'), 5.25)
    closeTo(n('stdevp(1,5,5,10)'), Math.sqrt(10.1875))
    closeTo(n('floor(3.9)'), 3)
    closeTo(n('ceil(3.1)'), 4)
    closeTo(n('round(3.5)'), 4)
    closeTo(n('re(2+3i)'), 2)
    closeTo(n('im(2+3i)'), 3)
  })

  it('ans is the previous numeric result', () => {
    const r = evaluateSheet(['2+3', 'ans*4'])
    closeTo(r[0].value!.n, 5)
    closeTo(r[1].value!.n, 20)
  })

  it('ans can be supplied for a single line', () => {
    const r = evaluateLine('ans+1', { angleMode: 'deg', ans: 9 })
    expect(r.value?.n).toBe(10)
  })

  it('can seed variables without re-evaluating prior lines', () => {
    const r = evaluateLine('x*2 + y', { variables: { x: 4, y: 3 } })
    closeTo(r.value!.n, 11)
  })

  it('does not stringify huge ranges into giant displays', () => {
    const r = evaluateLine('[1...80]')
    expect(r.display.length).toBeLessThan(80)
    expect(r.display).toBe('[1, 2, 3, 4, 5, 6, …, 77, 78, 79, 80]')
  })

  it('treats typeset dots as multiplication', () => {
    closeTo(n('2 · 3'), 6)
    closeTo(n('2⋅3'), 6)
    closeTo(n('2∙4'), 8)
    closeTo(n('2 · 3 + 4'), 10)
    closeTo(n('(1+2)·4'), 12)
    closeTo(n('0.5 · 8'), 4)
    closeTo(n('3 · 10^2'), 300)
    closeTo(n('2 · \\frac{1}{2}'), 1)
    closeTo(n('\\sin\\left(2 · 45\\right)'), 1)
    closeTo(n('3 dot 4'), 12)
    closeTo(n('4dot1'), 4)
    closeTo(n('3 DOT 4'), 12)
    closeTo(n('2 dot 3 + 4'), 10)
    closeTo(n('(1+2) dot 4'), 12)
  })

  it('converts typeset latex to ascii', () => {
    expect(latexToAscii('\\sqrt{16}+2^{8}')).toContain('sqrt')
    expect(tryPlainMath('\\frac{3}{4}')?.n).toBeCloseTo(0.75)
  })

  it('hyperbolic, gcd, lcm, lists, and binom', () => {
    closeTo(n('sinh(0)'), 0)
    closeTo(n('asinh(0)'), 0)
    closeTo(n('gcd(8,12)'), 4)
    closeTo(n('lcm(4,6)'), 12)
    closeTo(n('n(1,5,5,10)'), 4)
    closeTo(n('length(1,5,5,10)'), 4)
    closeTo(n('total(1,2,3)'), 6)
    closeTo(n('median(1,2,3,4,5)'), 3)
    closeTo(n('quartile(1,2,3,4,5,2)'), 3)
    closeTo(n('\\binom{6}{2}'), 15)
    closeTo(n('\\operatorname{gcd}\\left(8,12\\right)'), 4)
  })

  it('random returns a finite number', () => {
    const r = n('random()')
    expect(r).toBeGreaterThanOrEqual(0)
    expect(r).toBeLessThan(1)
  })

  it.each(
    [
      ['1+1', 2, 'deg'],
      ['2*3+4', 10, 'deg'],
      ['sqrt(9)', 3, 'deg'],
      ['cbrt(8)', 2, 'deg'],
      ['nthroot(32,5)', 2, 'deg'],
      ['2^10', 1024, 'deg'],
      ['10^-2', 0.01, 'deg'],
      ['sin(0)', 0, 'deg'],
      ['cos(0)', 1, 'deg'],
      ['tan(0)', 0, 'deg'],
      ['arcsin(0)', 0, 'deg'],
      ['arccos(1)', 0, 'deg'],
      ['arctan(0)', 0, 'deg'],
      ['csc(90)', 1, 'deg'],
      ['sec(0)', 1, 'deg'],
      ['ln(1)', 0, 'deg'],
      ['log(10)', 1, 'deg'],
      ['log_2(4)', 2, 'deg'],
      ['6!', 720, 'deg'],
      ['nCr(5,1)', 5, 'deg'],
      ['nPr(5,1)', 5, 'deg'],
      ['abs(-7)', 7, 'deg'],
      ['floor(2.2)', 2, 'deg'],
      ['ceil(2.2)', 3, 'deg'],
      ['round(2.2)', 2, 'deg'],
      ['mean(1,2,3)', 2, 'deg'],
      ['median(1,2,3)', 2, 'deg'],
      ['total(1,2,3,4)', 10, 'deg'],
      ['length(1,2,3)', 3, 'deg'],
      ['gcd(9,12)', 3, 'deg'],
      ['lcm(3,9)', 9, 'deg'],
      ['sinh(0)', 0, 'deg'],
      ['cosh(0)', 1, 'deg'],
      ['tanh(0)', 0, 'deg'],
      ['asinh(0)', 0, 'deg'],
      ['re(4+5i)', 4, 'deg'],
      ['im(4+5i)', 5, 'deg'],
      ['\\sin\\left(0\\right)', 0, 'deg'],
      ['\\cos\\left(0\\right)', 1, 'deg'],
      ['\\sqrt{25}', 5, 'deg'],
      ['\\frac{2}{5}', 0.4, 'deg'],
      ['\\sqrt[3]{27}', 3, 'deg'],
      ['\\ln\\left(1\\right)', 0, 'deg'],
      ['\\log\\left(100\\right)', 2, 'deg'],
      ['\\binom{5}{2}', 10, 'deg'],
      ['\\operatorname{gcd}\\left(6,9\\right)', 3, 'deg'],
      ['n(1,2,3,4,5)', 5, 'deg'],
      ['stdevp(2,2,2)', 0, 'deg'],
      ['2^3+2^3', 16, 'deg'],
      ['10^3 / 10^2', 10, 'deg'],
      ['round(pi, 2)', 3.14, 'deg'],
      ['min(9,3,7)', 3, 'deg'],
      ['max(9,3,7)', 9, 'deg'],
      ['sign(-4)', -1, 'deg'],
      ['sign(4)', 1, 'deg'],
      ['exp(0)', 1, 'deg'],
      ['e^0', 1, 'deg'],
      ['pi/pi', 1, 'deg'],
      ['2(3+4)', 14, 'deg'],
      ['(2+3)(4)', 20, 'deg'],
      ['1/2+1/2', 1, 'deg'],
      ['3/4-1/4', 0.5, 'deg'],
      ['sqrt(2)^2', 2, 'deg'],
      ['8^(1/3)', 2, 'deg'],
      ['16^(1/4)', 2, 'deg'],
      ['log10(1000)', 3, 'deg'],
      ['log2(16)', 4, 'deg'],
      ['nCr(10,0)', 1, 'deg'],
      ['nPr(10,0)', 1, 'deg'],
      ['0!', 1, 'deg'],
      ['1!', 1, 'deg'],
      ['mod(10,4)', 2, 'deg'],
      ['\\left|-9\\right|', 9, 'deg'],
      ['sin(30)', 0.5, 'deg'],
      ['cos(60)', 0.5, 'deg'],
      ['tan(45)', 1, 'deg'],
      ['arcsin(0.5)', 30, 'deg'],
      ['asin(0.5)', 30, 'deg'],
      ['sin^-1(0.5)', 30, 'deg'],
      ['arccos(0.5)', 60, 'deg'],
      ['arctan(1)', 45, 'deg'],
      ['atan2(1,1)', 45, 'deg'],
      ['sin(pi/2)', 1, 'rad'],
      ['cos(pi)', -1, 'rad'],
      ['tan(pi/4)', 1, 'rad'],
      ['ln(e^2)', 2, 'deg'],
      ['log(1e6)', 6, 'deg'],
      ['cbrt(-8)', -2, 'deg'],
      ['nthroot(81,4)', 3, 'deg'],
      ['mean([2,4,6])', 4, 'deg'],
      ['total([10,20])', 30, 'deg'],
      ['abs(-pi)', Math.PI, 'deg'],
      ['floor(-1.2)', -2, 'deg'],
      ['ceil(-1.2)', -1, 'deg'],
      ['\\frac{3}{8}', 0.375, 'deg'],
      ['\\sqrt{49}', 7, 'deg'],
      ['\\sin\\left(30\\right)', 0.5, 'deg'],
      ['nCr(8,2)', 28, 'deg'],
      ['nPr(8,2)', 56, 'deg'],
      ['gcd(21,35)', 7, 'deg'],
      ['lcm(6,8)', 24, 'deg'],
      ['2^0', 1, 'deg'],
      ['e^1', Math.E, 'deg'],
    ] as Array<[string, number, 'deg' | 'rad']>,
  )('extra %s', (expr, expected, mode) => {
    closeTo(n(expr, mode), expected)
  })
})
