import { describe, expect, it } from 'vitest'
import { TYPST_PREAMBLE, copiedEquation, toTypstMath, typstAnswer, typstDocument } from './typstMath'

describe('copiedEquation', () => {
  it('keeps the typed equation until Typst copy is on', () => {
    expect(copiedEquation('√(2)+1', false)).toBe('√(2)+1')
    expect(copiedEquation('√(2)+1', true)).toBe('sqrt(2) + 1')
    expect(copiedEquation('2^8', true)).toBe('2 ^ 8')
  })
})

describe('toTypstMath', () => {
  it('typesets ordinary calculations', () => {
    expect(toTypstMath('sin(90)')).toBe('sin(90)')
    expect(toTypstMath('sqrt(2)')).toBe('sqrt(2)')
    expect(toTypstMath('2^8')).toBe('2 ^ 8')
    expect(toTypstMath('1/2')).toBe('1 / 2')
    expect(toTypstMath('π')).toBe('pi')
    expect(toTypstMath('√(2)')).toBe('sqrt(2)')
    expect(toTypstMath('2 * 3')).toBe('2 ast 3')
    expect(toTypstMath('2 dot 3')).toBe('2 dot.op 3')
    expect(toTypstMath('2 · 3')).toBe('2 dot.op 3')
  })

  it('a decimal point is a point, and a lone one still finishes', () => {
    expect(toTypstMath('.')).toBe('.')
    expect(toTypstMath('2+.')).toBe('2 + .')
    expect(toTypstMath('.5')).toBe('.5')
    expect(toTypstMath('1.5')).toBe('1.5')
  })

  it('uses the preamble macros for the matching names', () => {
    expect(toTypstMath('bu + bv')).toBe('#bu + #bv')
    expect(toTypstMath('arctan(x)')).toBe('(#tan1)(x)')
    expect(toTypstMath('±')).toBe('#pm')
    expect(toTypstMath('ip(x, y)')).toBe('#ip($x$, $y$)')
    expect(toTypstMath('nCr(5, 2)')).toBe('binom(5, 2)')
  })

  it('typesets a sum and an integral', () => {
    expect(toTypstMath('Σ n^2, n=1..10')).toBe('sum_(n = 1)^(10) n ^ 2')
    expect(toTypstMath('Σ_x=2^5 2')).toBe('sum_(x = 2)^(5) 2')
    expect(toTypstMath('Σ_n=1^10 n^2')).toBe('sum_(n = 1)^(10) n ^ 2')
    expect(toTypstMath('Σ_{n=1}^{10} n')).toBe('sum_(n = 1)^(10) n')
    expect(toTypstMath('Π_n=1^5 n')).toBe('product_(n = 1)^(5) n')
    expect(toTypstMath('∫(x^2, 0, 1)')).toBe('integral_(0)^(1) x ^ 2')
    expect(toTypstMath('∫_23')).toBe('integral_(23)')
    expect(toTypstMath('∫_0^1 x^2')).toBe('integral_(0)^(1) x ^ 2')
    expect(toTypstMath('∫_0^{} x')).toBe('integral_(0) x')
    expect(toTypstMath('∫0..1 x')).toBe('integral_(0)^(1) x')
    expect(toTypstMath('∫_{L_0}^L (dl)/l')).toBe('integral_(L_(0))^(L) (dl) / l')
    expect(toTypstMath('∫_0^1 x^2 dx')).toBe('integral_(0)^(1) x ^ 2 dif x')
    expect(toTypstMath('∫_0^π/2 cos(x) dx')).toBe('integral_(0)^(pi / 2) cos(x) dif x')
    expect(toTypstMath('∫(x^2 dx, 0, 1)')).toBe('integral_(0)^(1) x ^ 2 dif x')
    expect(toTypstMath('∫(x^2, x, 0, 1)')).toBe('integral_(0)^(1) x ^ 2 dif x')
    expect(toTypstMath('int_0^1 x^2 dx')).toBe('integral_(0)^(1) x ^ 2 dif x')
    expect(toTypstMath('sum_{n=1}^{10} n^2')).toBe('sum_(n = 1)^(10) n ^ 2')
    expect(toTypstMath('prod_n=1^5 n')).toBe('product_(n = 1)^(5) n')
    expect(toTypstMath('2*(Σ_n=1^10 n^2)+1')).toBe('2 ast (sum_(n = 1)^(10) n ^ 2) + 1')
    expect(copiedEquation('∫_0^1 x^2 dx', true)).toBe('integral_(0)^(1) x ^ 2 dif x')
    expect(copiedEquation('Σ_n=1^10 n^2', true)).toBe('sum_(n = 1)^(10) n ^ 2')
  })

  it('wraps a document that imports the preamble and includes the answer', () => {
    const doc = typstDocument('x^2', '#1d1d1f', '4')
    expect(doc).toContain('#import "/macros.typ": *')
    expect(doc).toContain('$ x ^ 2 = 4 $')
    expect(doc).not.toContain(TYPST_PREAMBLE)
    expect(typstDocument('   ', '#1d1d1f')).toBeNull()
    expect(typstAnswer('sqrt(2)/2', '0.707')).toBe('sqrt(2)/2 ≈ 0.707')
    expect(typstAnswer(undefined, '4')).toBe('4')
  })
})
