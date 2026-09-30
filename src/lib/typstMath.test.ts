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
  it('writes the last answer as an upright word, so a chain like `+ 2` has a left side', () => {
    expect(toTypstMath('ans+ 2')).toBe('"ans" + 2')
    expect(toTypstMath('sin(ans)')).toBe('sin("ans")')
  })

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
    expect(toTypstMath('∫_{L_0}^L (dl)/l')).toBe('integral_(L_(0))^(L) (dif l) / l')
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

  it('writes words Typst does not know upright, so a unit or a name never fails the preview', () => {
    expect(toTypstMath('5 km')).toBe('5 "km"')
    expect(toTypstMath('2xy')).toBe('2 "xy"')
    expect(toTypstMath('x1 + x2')).toBe('x_(1) + x_(2)')
    expect(toTypstMath('alpha + Omega')).toBe('alpha + Omega')
    expect(toTypstMath('factorial(5)')).toBe('op("factorial")(5)')
    expect(toTypstMath('floor(2.5)')).toBe('floor(2.5)')
    expect(toTypstMath('arcsec(2)')).toBe('op("arcsec")(2)')
    expect(toTypstMath('gcd(4, 6)')).toBe('gcd(4, 6)')
  })

  it('keeps unicode letters, stray brackets and what follows them', () => {
    expect(toTypstMath('2α + ∂')).toBe('2 α + ∂')
    expect(toTypstMath('2+3)*4')).toBe('2 + 3 ) ast 4')
    expect(toTypstMath('[1, 2')).toBe('\\[ 1 , 2')
    expect(toTypstMath("f'(x)")).toBe("f ' (x)")
  })

  it('typesets e notation as a power of ten', () => {
    expect(toTypstMath('1.26765060023e+30')).toBe('1.26765060023 times 10^(30)')
    expect(toTypstMath('1e-20')).toBe('1 times 10^(-20)')
  })

  it('wraps a document that imports the preamble and includes the answer', () => {
    const doc = typstDocument('x^2', '4')
    expect(doc).toContain('#import "/macros.typ": *')
    expect(doc).toContain('$ x ^ 2 = 4 $')
    expect(doc).not.toContain(TYPST_PREAMBLE)
    expect(typstDocument('   ')).toBeNull()
    expect(typstDocument('x^2-4=0', '±2', 'x')).toContain('$ x ^ 2 - 4 = 0 quad arrow.r.double quad x = #pm 2 $')
    expect(typstDocument('sys', 'x = 1, y = 2')).toContain('quad arrow.r.double quad x = 1 , y = 2')
    expect(typstAnswer('sqrt(2)/2', '0.707')).toBe('sqrt(2)/2 ≈ 0.707')
    expect(typstAnswer(undefined, '4')).toBe('4')
  })
})

describe('matrices', () => {
  const m = (body: string) => `mat(delim: "[", ${body})`
  it('typesets every way a matrix is typed or answered', () => {
    expect(toTypstMath('[[1,2],[3,4]]')).toBe(m('1, 2; 3, 4'))
    expect(toTypstMath('[1,2;3,4]')).toBe(m('1, 2; 3, 4'))
    expect(toTypstMath('[19 22; 43 50]')).toBe(m('19, 22; 43, 50'))
    expect(toTypstMath('[1 2 3]')).toBe(m('1, 2, 3'))
    expect(toTypstMath('[1; 2; 3]')).toBe(m('1; 2; 3'))
    expect(toTypstMath('[3/5 -7/10; -1/5 2/5]')).toBe(m('3 / 5, - 7 / 10; - 1 / 5, 2 / 5'))
    expect(toTypstMath('[[sin(30), 1/2],[pi, x^2]]')).toBe(m('sin(30), 1 / 2; pi, x ^ 2'))
  })

  it('keeps a comma list a list', () => {
    expect(toTypstMath('[1,2,3]')).toBe('lr(\\[1 , 2 , 3\\])')
    expect(toTypstMath('[1 + 2]')).toBe('lr(\\[1 + 2\\])')
  })

  it('writes transpose and inverse as exponents', () => {
    const a = m('1, 2; 3, 4')
    expect(toTypstMath('[[1,2],[3,4]]ᵀ')).toBe(`${a} ^T`)
    expect(toTypstMath("[[1,2],[3,4]]'")).toBe(`${a}^T`)
    expect(toTypstMath('[[1,2],[3,4]] transpose')).toBe(`${a}^T`)
    expect(toTypstMath('[[1,2],[3,4]]⁻¹')).toBe(`${a} ^(-1)`)
    expect(toTypstMath('[[1,2],[3,4]]^-1')).toBe(`${a} ^(-1)`)
    expect(toTypstMath('inv([[1,2],[3,4]])')).toBe(`${a}^(-1)`)
    expect(toTypstMath('inverse of [[1,2],[3,4]]')).toBe(`${a}^(-1)`)
    expect(toTypstMath('transpose(A B)')).toBe('(A B)^T')
    expect(toTypstMath('2^-1')).toBe('2 ^(-1)')
  })

  it('prints the matrix functions as notation', () => {
    const a = m('1, 2; 3, 4')
    expect(toTypstMath('det([[1,2],[3,4]])')).toBe(`det ${a}`)
    expect(toTypstMath('det [[1,2],[3,4]]')).toBe(`det ${a}`)
    expect(toTypstMath('trace([[1,2],[3,4]])')).toBe(`op("tr") ${a}`)
    expect(toTypstMath('identity(3)')).toBe('I_(3)')
    expect(toTypstMath('dot([1,2],[3,4])')).toBe('lr(\\[1 , 2\\]) dot lr(\\[3 , 4\\])')
  })
})
