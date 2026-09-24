// adversarial coverage for solve, on top of solve.test.ts. every expected value below is worked
// out by hand from the equation itself (factoring, the quadratic formula, or known trig angles),
// never copied from what the engine prints.
import { describe, expect, it } from 'vitest'
import { evaluateLine, evaluateSheet } from './evaluate'
import type { EvaluateOptions } from './types'

function line(text: string, opts: EvaluateOptions = {}) {
  return evaluateLine(text, { angleMode: 'deg', ...opts })
}

describe('solve: linear with huge and tiny coefficients', () => {
  const cases: Array<[string, EvaluateOptions, string, string | undefined]> = [
    ['4x - 8 = 0', {}, '2', undefined],
    ['-x - 5 = 0', {}, '-5', undefined],
    ['x/100 = 4', {}, '400', undefined],
    ['x*3000000 = 9000000', {}, '3', undefined],
    ['0.000001x = 0.000004', {}, '4', undefined],
    ['5000000x = 1', {}, '2e-7', undefined],
    ['3000000x = 1', {}, '3.33333333333e-7', '1/3000000'],
  ]
  it.each(cases)('%s', (text, opts, display, exact) => {
    const r = line(text, opts)
    expect(r.kind).toBe('solve')
    expect(r.solve?.outcome).toBe('roots')
    expect(r.display).toBe(display)
    expect(r.exact).toBe(exact)
  })
})

describe('solve: quadratics', () => {
  const cases: Array<[string, string, string | undefined]> = [
    ['x^2 - 7x + 12 = 0', '3, 4', undefined],
    ['x^2 + 5x + 6 = 0', '-3, -2', undefined],
    ['2x^2 - 8 = 0', '±2', undefined],
    ['3x^2 - 27 = 0', '±3', undefined],
    ['x^2 - x - 6 = 0', '-2, 3', undefined],
    ['2x^2 - 3x - 5 = 0', '-1, 2.5', undefined],
    ['9x^2 - 3x - 2 = 0', '-0.333333, 0.666667', '-1/3, 2/3'],
    ['1e8*x^2 = 4', '±0.0002', undefined],
  ]
  it.each(cases)('%s', (text, display, exact) => {
    const r = line(text)
    expect(r.kind).toBe('solve')
    expect(r.solve?.outcome).toBe('roots')
    expect(r.display).toBe(display)
    expect(r.exact).toBe(exact)
  })
})

describe('solve: cubic and quartic, near-together and overflowing roots', () => {
  it('a cubic with two close roots keeps them distinct', () => {
    // roots are exactly 2, 2.0001 and 5 by construction, not by trusting the fit
    expect(line('(x-2)*(x-2.0001)*(x-5) = 0').display).toBe('2, 2.0001, 5')
  })

  it('a negative leading coefficient still finds the same roots', () => {
    expect(line('-(x-1)*(x-4)*(x-9) = 0').display).toBe('1, 4, 9')
  })

  it('a quartic with exactly four real roots shows all of them', () => {
    const r = line('(x+3)*(x+1)*(x-2)*(x-5)=0')
    expect(r.display).toBe('-3, -1, 2, 5')
    expect(r.solve?.more).toBeFalsy()
  })

  it('a quintic with five roots keeps the four nearest zero and marks more', () => {
    const r = line('(x+10.3)*(x+5)*(x-1)*(x-2)*(x-3) = 0')
    expect(r.display).toBe('-5, 1, 2, 3, …')
    expect(r.solve?.more).toBe(true)
  })
})

describe('solve: trig in degrees and radians, tangent roots', () => {
  const deg: Array<[string, string]> = [
    ['cos(2x) = 0', '45, 135, 225, 315'],
    ['tan(x) = -1', '135, 315'],
    ['tan(2x) = 1', '22.5, 112.5, 202.5, 292.5'],
    ['tan(x) = 0', '0, 180'],
  ]
  it.each(deg)('%s (deg)', (text, display) => {
    const r = line(text)
    expect(r.display).toBe(display)
  })

  it('sin(3x) = 0 in degrees has six roots in the window; the four nearest zero are shown', () => {
    const r = line('sin(3x) = 0')
    expect(r.display).toBe('0, 60, 120, 180, …')
    expect(r.solve?.more).toBe(true)
  })

  it('cos(2x) = 0 in radians', () => {
    const r = line('cos(2x) = 0', { angleMode: 'rad' })
    expect(r.display).toBe('0.785398, 2.35619, 3.92699, 5.49779')
  })

  it('tan(x) = 1 in radians', () => {
    const r = line('tan(x) = 1', { angleMode: 'rad' })
    expect(r.display).toBe('0.785398, 3.92699')
  })
})

describe('solve: for v, stored variables, shadowing, user functions', () => {
  it('solves for a named variable amid other stored ones', () => {
    const r = line('y = m*x + b for m', { variables: { y: 10, x: 2, b: 4 } })
    expect(r.solve?.variable).toBe('m')
    expect(r.display).toBe('3')
  })

  it('solve keyword combined with for', () => {
    const r = line('solve x^2 - 9 for x')
    expect(r.display).toBe('±3')
  })

  it('a stored letter is shadowed by the equation and left untouched afterward', () => {
    const rows = evaluateSheet(['x = 5', 'x^2 = x + 6', 'x'], { angleMode: 'deg' })
    expect(rows[1]!.display).toBe('-2, 3')
    expect(rows[2]!.display).toBe('5')
  })

  it('a stored variable feeds a linear solve', () => {
    const rows = evaluateSheet(['a = 2', 'a*x + 1 = 9'], { angleMode: 'deg' })
    expect(rows[1]!.display).toBe('4')
  })

  it('a user function combined with another call of itself', () => {
    const r = line('g(x) - g(2) = 10', { functions: { g: { params: ['t'], body: '3*t+1' } } })
    // g(x) = 3x+1, g(2) = 7, so 3x+1-7=10 -> 3x=16 -> x=16/3
    expect(r.display).toBe('5.33333333333')
    expect(r.exact).toBe('16/3')
  })
})

describe('solve: messages, new equations', () => {
  const cases: Array<[string, string, string]> = [
    ['x^2 + 2x + 5 = 0', 'no real solution', 'none'],
    ['5x + 2 = 5x + 9', 'no solution', 'contradiction'],
    ['3(x+2) = 3x + 6', 'true for all x', 'all'],
    ['sqrt(x) = -3', 'no solution found', 'noneFound'],
    ['exp(x) = -5', 'no solution found', 'noneFound'],
    ['abs(x) = -5', 'no solution found', 'noneFound'],
  ]
  it.each(cases)('%s', (text, display, outcome) => {
    const r = line(text)
    expect(r.display).toBe(display)
    expect(r.solve?.outcome).toBe(outcome)
  })
})

describe('solve: trailing = combined with other syntax', () => {
  it('a trailing = with the solve keyword', () => {
    expect(line('solve x^2-16 =').display).toBe('±4')
  })
  it('a trailing = with a for clause', () => {
    expect(line('3x=12 for x =').display).toBe('4')
  })
})

describe('solve: a batch of quick linear equations', () => {
  it.each([
    ['6x = 42', '7'],
    ['x - 9 = 0', '9'],
    ['-2x + 6 = 0', '3'],
    ['10 - x = 3', '7'],
    ['3x + 1 = 2x + 9', '8'],
    ['7 - 2x = 1', '3'],
    ['x/4 + 1 = 6', '20'],
    ['2(x - 3) = 4', '5'],
    ['5 - x = x - 1', '3'],
    ['4x = -20', '-5'],
    ['-x/2 = 6', '-12'],
    ['3(2x + 1) = 21', '3'],
  ])('%s -> x = %s', (text, x) => {
    expect(line(text).display).toBe(x)
  })
})

describe('solve: a batch of quick perfect-square quadratics', () => {
  it.each([
    ['x^2 - 9 = 0', '±3'],
    ['x^2 - 16x + 64 = 0', '8'],
    ['x^2 + 6x + 9 = 0', '-3'],
    ['4x^2 - 25 = 0', '±2.5'],
    ['x^2 - 10x + 21 = 0', '3, 7'],
    ['x^2 + x - 20 = 0', '-5, 4'],
  ])('%s', (text, display) => {
    expect(line(text).display).toBe(display)
  })
})

describe('solve: rationalize option', () => {
  // BUG: quadratics with a non-perfect-square discriminant build their exact form through
  // Solver.closedForm, which always writes `±sqrt(r)/d` and never consults ctx.rationalize.
  // The `rationalize: false` option does change the exact form for a single linear/cubic
  // root (via exactForm -> asRadical), and it does change answerForms's own closed-form
  // offer for the same number (see audit2.answerForms.test.ts), but for this quadratic path
  // it is silently ignored. input: '2x^2 = 1' with rationalize:false, expected exact
  // '±1/sqrt(2)', actual '±sqrt(2)/2' (same as the default).
  it('rationalize:false should un-rationalize a quadratic closed form too', () => {
    expect(line('2x^2 = 1', { rationalize: false }).exact).toBe('±1/sqrt(2)')
  })
})

describe('solve: things that must not solve', () => {
  it('assignments and units stay their own kind', () => {
    expect(line('price = 10')).toMatchObject({ kind: 'assignment', display: '10' })
    // solve is skipped whenever ± is present, even with a for clause
    const pm = line('x = 5 ± 2')
    expect(pm.kind).not.toBe('solve')
    expect(pm.display).toBe('5 ± 2')
  })

  const blanks: Array<[string, EvaluateOptions]> = [
    ['y == 10', {}],
    ['x > 3', {}],
    ['x >= 3', {}],
    ['weight kg = 5', {}],
    ['the x = 5', {}],
    ['5 m = x', {}],
  ]
  it.each(blanks)('%s is blank, not a solve', (text, opts) => {
    const r = line(text, opts)
    expect(r.kind).not.toBe('solve')
    expect(r.display).toBe('')
  })
})
