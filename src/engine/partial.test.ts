import { describe, expect, it } from 'vitest'
import { isCalculusInput } from './calculus'
import { evaluateLine, evaluateSheet } from './evaluate'
import type { EvaluateOptions } from './types'

const rad: EvaluateOptions = { angleMode: 'rad' }
const deg: EvaluateOptions = { angleMode: 'deg' }

function shown(text: string, options: EvaluateOptions = rad): string {
  return evaluateLine(text, options).display
}

describe('partial derivatives: other letters are constants', () => {
  it.each([
    ['∂/∂x x^2 y', '2xy'],
    ['∂/∂y x^2 y', 'x²'],
    ['d/dx x^2 y', '2xy'],
    ['∂/∂x sin(x y)', 'y cos(xy)'],
    ['∂/∂x e^(x y)', 'y e^(xy)'],
    ['∂/∂x ln(x^2 + y^2)', '2x/(x² + y²)'],
    ['∂/∂y x y^2 + x^3', '2xy'],
    ['∂/∂x (x + y)/(x - y)', '-2y/(x - y)²'],
    ['∂/∂t A sin(w t)', 'Aw cos(tw)'],
    ['∂/∂θ r sin(θ)', 'r cos(θ)'],
  ])('%s → %s', (input, formula) => {
    expect(shown(input)).toBe(formula)
  })

  it('takes higher and mixed partials, applied right to left', () => {
    expect(shown('∂²/∂x² x^3 y')).toBe('6xy')
    expect(shown('∂^2/∂x^2 x^3 y')).toBe('6xy')
    expect(shown('∂²/∂x∂y x^2 y^3')).toBe('6x y²')
    expect(shown('∂²/∂y∂x x^2 y^3')).toBe('6x y²')
    expect(shown('∂^2/∂x ∂y sin(x y)')).toBe(shown('∂^2/∂y ∂x sin(x y)'))
    expect(shown('∂³/∂x²∂y x^3 y^2')).toBe('12xy')
  })

  it('reads the derivative in words', () => {
    expect(shown('derivative of x^2 y with respect to y')).toBe('x²')
    expect(shown('partial derivative of x y^2 wrt y')).toBe('2xy')
    expect(shown('the partial derivative of x^2 y')).toBe('2xy')
  })

  it('puts a quotient over one denominator', () => {
    // D and L held constant; Wolfram: -v (v D + γ L)/(m (γ² + v²)^(3/2))
    const want = '-v·(Dv + Lγ)/(m·(v² + γ²)^(3/2))'
    expect(shown('∂/∂γ (-1/m (D γ - L v)/sqrt(γ^2 + v^2))')).toBe(want)
    expect(shown('∂/∂γ (-1/m (Dγ - Lv)/√(γ² + v²))')).toBe(want)
    expect(shown('d/dγ -1/m*(D*γ - L*v)/sqrt(γ^2 + v^2)')).toBe(want)
  })
})

describe('partial derivatives at a point', () => {
  it.each([
    ['∂/∂x x^2 y at x=1, y=2', 4],
    ['∂/∂x x^2 y at x = 1 and y = 2', 4],
    ['∂/∂x x^2 y at (x, y) = (1, 2)', 4],
    ['∂/∂y x^2 y^3 at (x, y) = (2, -1)', 12],
    ['∂²/∂x∂y x^2 y^3 at x=1, y=2', 24],
    ['∂/∂x sin(x y) at x = 0, y = pi', Math.PI],
  ])('%s → %d', (input, want) => {
    const r = evaluateLine(input, rad)
    expect(r.value?.kind).toBe('number')
    expect(r.value!.kind === 'number' && r.value!.n).toBeCloseTo(want, 10)
  })

  it('substitutes some letters and keeps the rest', () => {
    expect(shown('∂/∂x x^2 y at x=1')).toBe('2y')
    // √(v²) is |v|, not v
    expect(shown('∂/∂γ (-1/m (Dγ - Lv)/√(γ² + v²)) at γ = 0')).toBe('-D/(m |v|)')
  })

  it('is blank where the function or a lower derivative is undefined', () => {
    expect(shown('∂/∂x ln(x y) at x = 1, y = -1')).toBe('')
    expect(shown('∂/∂x sqrt(x) y at x = -1, y = 2')).toBe('')
  })

  it('is blank for a point it cannot read', () => {
    expect(shown('∂/∂x x^2 y at (1, 2)')).toBe('')
    expect(shown('∂/∂x x^2 y at z = 1')).toBe('')
    expect(shown('∂/∂x x^2 y at x = 1, x = 2')).toBe('')
  })
})

describe('partial derivatives on a sheet', () => {
  it('uses defined variables as numbers and functions of several letters', () => {
    const rs = evaluateSheet(['f(x, y) = x^2 y', '∂/∂x f(x, y)', '∂/∂y f(x, y) at x=2, y=3', 'y = 4', 'd/dx x^2 y'], rad)
    expect(rs.map((r) => r.display).slice(1)).toEqual(['2xy', '4', '4', '8x'])
  })

  it('follows degree mode', () => {
    expect(shown('∂/∂x sin(x y)', deg)).toBe('πy cos(xy)/180')
  })

  it('counts as calculus while typing', () => {
    for (const t of ['∂/∂x', '∂²/∂x∂y x y', 'partial derivative of x y']) expect(isCalculusInput(t), t).toBe(true)
  })

  it('leaves one-variable derivatives as they were', () => {
    expect(shown('d/dx x^2 sin(x)')).toBe('2x sin(x) + x² cos(x)')
    expect(shown('d/dx x^3 at 2')).toBe('12')
    expect(shown('derivative of t^2')).toBe('2t')
  })
})
