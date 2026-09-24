import { describe, expect, it } from 'vitest'
import { evaluateCalculus, isCalculusInput, jobKey, knownClosedForm, rememberClosedForm } from './calculus'
import { evaluateLine, evaluateSheet } from './evaluate'
import type { EvaluateOptions } from './types'

const rad: EvaluateOptions = { angleMode: 'rad' }
const deg: EvaluateOptions = { angleMode: 'deg' }

function shown(text: string, options: EvaluateOptions = rad): string {
  return evaluateLine(text, options).display
}

function value(text: string, options: EvaluateOptions = rad): number {
  const r = evaluateLine(text, options)
  if (r.value?.kind !== 'number') throw new Error(`no number for ${text}: ${JSON.stringify(r.display)}`)
  return r.value.n
}

function close(actual: number, expected: number, rel = 1e-10): void {
  expect(Math.abs(actual - expected)).toBeLessThanOrEqual(rel * Math.max(1, Math.abs(expected)))
}

/** The shown digits must all be right: rounding the truth to as many digits gives the same text. */
function honest(text: string, truth: number, options: EvaluateOptions = rad): void {
  const r = evaluateLine(text, options)
  expect(r.value?.kind, text).toBe('number')
  const n = r.value!.n
  if (truth === 0) {
    expect(n, text).toBe(0)
    return
  }
  const digits = n.toPrecision(15).replace(/^-?0\.0*|\.|e.*$|^-/g, '').replace(/0+$/, '').length || 1
  expect(Number(truth.toPrecision(Math.min(15, Math.max(1, digits)))), text).toBe(Number(n.toPrecision(Math.min(15, Math.max(1, digits)))))
}

describe('derivatives: symbolic formulas', () => {
  it.each([
    ['d/dx x^2 sin(x)', '2x sin(x) + x² cos(x)'],
    ['d/dx x^3', '3x²'],
    ['d/dx sin(x)', 'cos(x)'],
    ['d/dx cos(x)', '-sin(x)'],
    ['d/dx tan(x)', 'sec(x)²'],
    ['d/dx sqrt(x)', '1/(2√x)'],
    ['d/dx ln(x)', '1/x'],
    ['d/dx log(x)', '1/(x ln(10))'],
    ['d/dx log(x, 2)', '1/(x ln(2))'],
    ['d/dx e^x', 'e^x'],
    ['d/dx exp(x)', 'e^x'],
    ['d/dx e^(2x)', '2 e^(2x)'],
    ['d/dx 2^x', '2^x ln(2)'],
    ['d/dx x^x', 'x^x·(ln(x) + 1)'],
    ['d/dx asin(x)', '1/√(1 - x²)'],
    ['d/dx atan(x)^2', '2 atan(x)/(x² + 1)'],
    ['d/dx 1/x', '-1/x²'],
    ['d/dx (x^2+1)^3', '6x·(x² + 1)²'],
    ['d/dx sinh(x)', 'cosh(x)'],
    ['d/dx 5', '0'],
    ['d/dx pi x', 'π'],
    ['d/dx(x^2)', '2x'],
    ['d/dx e^(x^2)', '2x e^(x²)'],
    ['d/dt t^3', '3t²'],
    ['derivative of x^3', '3x²'],
    ['derivative of t^2', '2t'],
    ['d²/dx² x^4', '12x²'],
    ['d^2/dx^2 sin(x)', '-sin(x)'],
    ['d³/dx³ x^4', '24x'],
    ['second derivative of x^3', '6x'],
  ])('%s → %s', (input, formula) => {
    expect(shown(input)).toBe(formula)
  })

  it('follows degree mode: sin of degrees has a π/180 chain factor', () => {
    expect(shown('d/dx sin(x)', deg)).toBe('π cos(x)/180')
    expect(shown('d/dx x^2 sin(x)', deg)).toBe('2x sin(x) + π x² cos(x)/180')
    expect(shown('d/dx asin(x)', deg)).toBe('180/(π √(1 - x²))')
    expect(shown('d/dx sin(2x)', deg)).toBe('π cos(2x)/90')
    // radians-only functions don't change
    expect(shown('d/dx x^3', deg)).toBe('3x²')
    expect(shown('d/dx e^x', deg)).toBe('e^x')
  })

  it('formulas read back as the same function', () => {
    const cases = ['d/dx x^2 sin(x)', 'd/dx sqrt(x)', 'd/dx (x^2+1)^3', 'd/dx 2^x', 'd/dx asin(x)', 'd/dx e^(x^2)', 'd/dx x^x', 'd/dx log(x)']
    for (const c of cases) {
      const formula = shown(c)
      const body = c.replace(/^d\/dx\s*/, '')
      for (const x of [0.3, 0.7, 0.9]) {
        const want = value(`d/dx ${body} at ${x}`)
        const got = evaluateSheet(['g(x) = ' + formula, `g(${x})`], rad)[1]!.value!.n
        close(got, want, 1e-9)
      }
    }
  })
})

describe('derivatives: at a point', () => {
  it.each([
    ['d/dx x^3 at 2', 12],
    ['d/dx x^3 at x = 2', 12],
    ['d/dx x^3 at x=2', 12],
    ['d/dt t^3 at t=2', 12],
    ['derivative of x^3 at 2', 12],
    ['derivative of sin(x) at pi', -1],
    ['second derivative of x^3 at 2', 12],
    ['d^2/dx^2 x^3 at 2', 12],
    ['d/dx x^3 at pi', 3 * Math.PI ** 2],
    ['d/dx ln(x) at 4', 0.25],
    ['d/dx floor(x) at 0.5', 0],
    ['d/dx x mod 3 at 1.5', 1],
  ])('%s → %d', (input, want) => {
    close(value(input), want)
  })

  it('degree mode at a point', () => {
    close(value('d/dx sin(x) at 60', deg), Math.PI / 360)
    close(value('d/dx cos(x) at 90', deg), -Math.PI / 180)
    close(value('d/dx atan(x) at 1', deg), 90 / Math.PI)
  })

  it('no derivative, no answer', () => {
    expect(shown('d/dx abs(x) at 0')).toBe('')
    expect(shown('d/dx 1/x at 0')).toBe('')
    expect(shown('d/dx sqrt(x) at -1')).toBe('')
    expect(shown('d/dx max(x, 1)')).toBe('')
  })

  it("f'(x) and f''(x) for user functions", () => {
    const rs = evaluateSheet(['f(x) = x^2', "f'(3)", "f'(x)", "f''(x)", "f'(t)", 'd/dx f(x)^2', "f'(2+1)"], rad)
    expect(rs.map((r) => r.display).slice(1)).toEqual(['6', '2x', '2', '2t', '4x³', '6'])
    const g = evaluateSheet(['g(x) = sin(x)', "g'(0)", "g'(x)"], deg)
    close(g[1]!.value!.n, Math.PI / 180)
    expect(g[2]!.display).toBe('π cos(x)/180')
  })

  it('known variables are constants, the variable of differentiation is not', () => {
    const rs = evaluateSheet(['a = 2', 'x = 3', 'd/dx a x^2', 'd/dx x^2', 'd/dx x^2 at 5', 'x^2'], rad)
    expect(rs.map((r) => r.display).slice(2)).toEqual(['4x', '2x', '10', '9'])
  })
})

describe('definite integrals', () => {
  it.each([
    ['∫0..1 x^2', 1 / 3],
    ['integral of x^2 from 0 to 1', 1 / 3],
    ['int(x^2, 0, 1)', 1 / 3],
    ['integral(x^2, 0, 1)', 1 / 3],
    ['∫(x^2, 0, 1)', 1 / 3],
    ['int(t^2, t, 0, 1)', 1 / 3],
    ['∫0..1 x^2 dx', 1 / 3],
    ['∫0..1 t^2 dt', 1 / 3],
    ['∫_0^1 x^3', 0.25],
    ['∫_{0}^{1} x^3', 0.25],
    ['∫ x^3 from 0 to 1', 0.25],
    ['integrate x^3 from 0 to 1', 0.25],
    ['integral from 0 to 1 of x^3', 0.25],
    ['∫0..pi sin(x)', 2],
    ['∫0..π sin(x)', 2],
    ['∫0..pi/2 cos(x)', 1],
    ['∫0..1(x+1)', 1.5],
    ['∫2..0 x', -2],
    ['∫0..0 x', 0],
    ['∫-1..1 x^3', 0],
    ['∫0..2pi sin(x)', 0],
    ['∫0..1 ln(x)ln(1-x)', 2 - Math.PI ** 2 / 6],
    ['∫0..1 atan(x)/x', 0.915965594177219],
    ['∫0..1 1/sqrt(x)', 2],
    ['∫0..1 x^-0.9', 10],
    ['∫0..1 1/sqrt(1-x^2)', Math.PI / 2],
    ['∫0..2 sqrt(4-x^2)', Math.PI],
    ['∫0..1 1/(x-2)', -Math.LN2],
    ['∫0..∞ e^(-x)', 1],
    ['∫0..inf e^(-x)', 1],
    ['∫0..infinity e^(-x)', 1],
    ['∫-∞..∞ e^(-x^2)', Math.sqrt(Math.PI)],
    ['∫-inf..inf 1/(1+x^2)', Math.PI],
    ['∫0..∞ 1/(1+x^2)', Math.PI / 2],
    ['∫-∞..0 e^x', 1],
    ['∫1..∞ 1/x^2', 1],
    ['∫0..∞ x^3/(e^x-1)', Math.PI ** 4 / 15],
    ['∫0..∞ e^(-x) sin(x)', 0.5],
    ['∫-1..1 abs(x)', 1],
    ['∫0..2 floor(x)', 1],
    ['∫0..1 floor(3x)', 1],
    ['∫0..1 e^(-10000(x-0.3)^2)', Math.sqrt(Math.PI) / 100],
    ['∫0..1000 sin(x)^2', 500 - Math.sin(2000) / 4],
    ['∫0..100 sin(x)', 1 - Math.cos(100)],
  ])('%s', (input, want) => {
    honest(input, want)
  })

  it('degree mode integrates the function the calculator actually evaluates', () => {
    close(value('∫0..90 sin(x)', deg), 180 / Math.PI)
    close(value('∫0..1 x^2', deg), 1 / 3)
  })

  it('blank when the integral diverges or does not settle', () => {
    for (const t of ['∫0..1 1/x', '∫-1..1 1/x', '∫1..2 1/(x-1.5)', '∫0..1 x^-1.1', '∫1..∞ 1/x', '∫0..∞ sin(x)', '∫0..∞ sin(x)/x', '∫0..1 sin(1/x)', '∫0..1 sqrt(x-2)']) {
      expect(shown(t), t).toBe('')
    }
  })

  it('blank for a second free variable or a missing piece', () => {
    for (const t of ['∫0..1 x y', '∫', '∫0..1', 'integral of x^2', 'integral of x^2 from 0', 'int(x^2, 0)', '∫0.. x']) {
      expect(shown(t), t).toBe('')
    }
  })

  it('uses variables, ans and user functions', () => {
    const rs = evaluateSheet(['a = 2', '∫0..1 a x^2', 'f(x) = x^2', '∫0..1 f(x)', '∫0..a x'], rad)
    close(rs[1]!.value!.n, 2 / 3)
    close(rs[3]!.value!.n, 1 / 3)
    close(rs[4]!.value!.n, 2)
    const chained = evaluateSheet(['y = ∫0..1 x^2', 'y*3', '∫0..1 x^2', 'ans*3'], rad)
    expect(chained.map((r) => r.display)).toEqual(['0.333333333333', '1', '0.333333333333', '1'])
  })

  it('fraction mode shows the fraction', () => {
    expect(shown('∫0..1 x^2', { ...rad, fractionMode: true })).toBe('1/3')
  })
})

describe('integrals inside a larger expression', () => {
  it.each([
    ['4*∫0..1 x^2', 4 / 3],
    ['4 ∫0..1 x^2', 4 / 3],
    ['4*∫_0^1 x^2 dx', 4 / 3],
    ['4 int_(2)^(4) 2 x dx', 48],
    ['(∫0..1 x^2)+1', 4 / 3],
    ['(∫0..1 x^2 dx)+1', 4 / 3],
    ['∫0..1 x^2 dx + 1', 4 / 3],
    ['∫0..1 x dx * 3', 1.5],
    ['2*int(x^2, 0, 1)', 2 / 3],
    ['int(x^2, 0, 1)+1', 4 / 3],
    ['sqrt(∫0..1 4x^3)', 1],
    ['(∫0..1 x dx)^2', 0.25],
    ['∫0..1 x dx * ∫0..2 x dx', 1],
    ['2 * integral of x^2 from 0 to 1', 2 / 3],
    ['(integral of x from 0 to 1) + 3', 3.5],
    ['10 - ∫0..2 x dx', 8],
    ['∫0..1 x^2 + 1', 4 / 3],
  ])('%s', (input, want) => {
    close(value(input), want)
  })

  it('a diverging integral inside a larger line stays blank', () => {
    expect(shown('4*∫0..1 1/x')).toBe('')
    expect(shown('∫0..1 x^2 dx + ∫0..1 1/x')).toBe('')
  })
})

describe('limits', () => {
  it.each([
    ['lim x->0 sin(x)/x', 1],
    ['lim x→0 sin(x)/x', 1],
    ['lim_(x->0) sin(x)/x', 1],
    ['lim_{x\\to 0} sin(x)/x', 1],
    ['limit x->0 sin(x)/x', 1],
    ['limit of sin(x)/x as x->0', 1],
    ['limit of sin(x)/x as x approaches 0', 1],
    ['lim x->0 (1-cos(x))/x^2', 0.5],
    ['lim x->0 (e^x-1)/x', 1],
    ['lim x->0 (e^x-1-x)/x^2', 0.5],
    ['lim x->0 (tan(x)-sin(x))/x^3', 0.5],
    ['lim x->0 (x - sin(x))/x^3', 1 / 6],
    ['lim x->0 (2^x-1)/x', Math.LN2],
    ['lim x->1 (x^2-1)/(x-1)', 2],
    ['lim x->3 (x^2-9)/(x-3)', 6],
    ['lim x->1 ln(x)/(x-1)', 1],
    ['lim x->2 x^2', 4],
    ['lim t->2 t^3', 8],
    ['lim x->∞ (1+1/x)^x', Math.E],
    ['lim x->inf (1+2/x)^x', Math.E ** 2],
    ['lim x->∞ x^2/(2x^2+1)', 0.5],
    ['lim x->∞ sqrt(x^2+x)-x', 0.5],
    ['lim x->∞ x sin(1/x)', 1],
    ['lim x->∞ x^(1/x)', 1],
    ['lim x->∞ sin(x)/x', 0],
    ['lim x->∞ ln(x)/x', 0],
    ['lim x->-∞ e^x', 0],
    ['lim x->inf 1/x', 0],
    ['lim x->0 x sin(1/x)', 0],
    ['lim x->0+ x^x', 1],
    ['lim x->0+ x ln(x)', 0],
    ['lim x->0^+ sqrt(x)', 0],
    ['lim x->0 x/abs(x)^0.5', 0],
    ['lim x->0+ abs(x)/x', 1],
    ['lim x->0- abs(x)/x', -1],
    ['lim x->1- sqrt(1-x)', 0],
  ])('%s', (input, want) => {
    honest(input, want)
  })

  it('infinite limits', () => {
    expect(shown('lim x->0+ 1/x')).toBe('∞')
    expect(shown('lim x→0⁺ 1/x')).toBe('∞')
    expect(shown('lim x->0- 1/x')).toBe('-∞')
    expect(shown('lim x->0⁻ 1/x')).toBe('-∞')
    expect(shown('lim x->0 1/x^2')).toBe('∞')
    expect(shown('lim x->∞ x')).toBe('∞')
    expect(shown('lim x->∞ -x^3')).toBe('-∞')
    expect(shown('lim x->0+ 1/sqrt(x)')).toBe('∞')
    expect(shown('lim x->∞ e^x/x^5')).toBe('∞')
    expect(shown('lim x->0+ e^(1/x)')).toBe('∞')
    expect(shown('lim x->pi/2- tan(x)')).toBe('∞')
    expect(shown('lim x->pi/2+ tan(x)')).toBe('-∞')
  })

  it('blank when the sides disagree or nothing settles', () => {
    for (const t of ['lim x->0 1/x', 'lim x->0 abs(x)/x', 'lim x->0 sin(1/x)', 'lim x->pi/2 tan(x)', 'lim x->0 ln(x)', 'lim x->0+ 1/ln(x)', 'lim x->0', 'lim x']) {
      expect(shown(t), t).toBe('')
    }
  })

  it('never lets roundoff pass for a limit: 1/24 or blank, not 0 or ∞', () => {
    const r = shown('lim x->0 (cos(x) - 1 + x^2/2)/x^4')
    if (r) close(Number(r), 1 / 24, 1e-6)
    const s = shown('lim x->0 (sin(x)-x+x^3/6)/x^5')
    if (s) close(Number(s), 1 / 120, 1e-6)
  })

  it('follows degree mode', () => {
    close(value('lim x->0 sin(x)/x', deg), Math.PI / 180)
    close(value('lim x->0 sin(x)/x', rad), 1)
  })
})

describe('must not trigger', () => {
  it('variables named d, dx, lim, int, integral, derivative', () => {
    const d = evaluateSheet(['d = 5', 'd/2', 'd*3', 'd/dx x^2'], rad)
    expect(d.map((r) => r.display)).toEqual(['5', '2.5', '15', '2x'])
    const dx = evaluateSheet(['dx = 2', 'd = 4', 'd/dx'], rad)
    expect(dx[2]!.display).toBe('2')
    const lim = evaluateSheet(['lim = 3', 'lim*2', 'lim + 1', 'lim'], rad)
    expect(lim.map((r) => r.display)).toEqual(['3', '6', '4', '3'])
    const int = evaluateSheet(['int = 4', 'int*2', 'int(2)', 'int + 1'], rad)
    expect(int.map((r) => r.display)).toEqual(['4', '8', '8', '5'])
    const words = evaluateSheet(['integral = 3', 'integral + 1', 'derivative = 2', 'derivative*3', 'limit = 7', 'limit - 1'], rad)
    expect(words.map((r) => r.display)).toEqual(['3', '4', '2', '6', '7', '6'])
  })

  it('a user function named int stays a function', () => {
    const rs = evaluateSheet(['int(x) = floor(x)', 'int(3.7)', 'int(x^2, 0, 1)'], rad)
    expect(rs[1]!.display).toBe('3')
    expect(rs[2]!.display).toBe('')
  })

  it('primes need a user function', () => {
    expect(evaluateCalculus("g'(2)", {})).toBeNull()
    expect(shown("g'(2)")).toBe('')
  })

  it('ordinary input is untouched', () => {
    for (const [t, want] of [
      ['5 d', '5 d'],
      ['d', '1 d'],
      ['2 + 3', '5'],
      ['sin(30)', '0.5'],
      ['sqrt(16)', '4'],
      ['int', ''],
      ['lim', ''],
    ] as const) {
      expect(shown(t, deg), t).toBe(want)
    }
    expect(evaluateCalculus('2 + 3')).toBeNull()
    expect(evaluateCalculus('d/2')).toBeNull()
    expect(evaluateCalculus('lim*2')).toBeNull()
    expect(evaluateCalculus('dist/dt')).toBeNull()
    expect(evaluateCalculus('d/dxy x')).toBeNull()
  })

  it('isCalculusInput marks calculus lines, even half typed', () => {
    for (const t of ['∫0..1 x^2', '∫0..', 'd/dx x^2', 'd/dx', 'lim x->0 sin(x)/x', 'lim x->', 'integral of x^2 from 0 to 1', 'integral of x^2 from', "f'(2)", 'derivative of x^2', 'limit of sin(x)/x as x->0']) {
      expect(isCalculusInput(t), t).toBe(true)
    }
    for (const t of ['2 + 3', 'lim*2', 'lim + 1', 'int*2', 'd/2', 'what is 5% of 20', '5 d', 'integral', 'from 2 to 3']) {
      expect(isCalculusInput(t), t).toBe(false)
    }
  })

  it('± input keeps its own path', () => {
    expect(shown('(5.0 ± 0.2) * 3', deg)).toBe('15.0 ± 0.6')
  })
})

describe('closed-form jobs', () => {
  it('a non-integer answer asks the worker; an integer or formula does not', () => {
    expect(evaluateLine('∫0..1 x^2', rad).closedForm?.kind).toBe('integral')
    expect(evaluateLine('lim x->∞ (1+1/x)^x', rad).closedForm?.kind).toBe('limit')
    expect(evaluateLine('d/dx sin(x) at 60', deg).closedForm?.kind).toBe('value')
    expect(evaluateLine('∫0..pi sin(x)', rad).closedForm).toBeUndefined()
    expect(evaluateLine('d/dx x^3 at 2', rad).closedForm).toBeUndefined()
    expect(evaluateLine('d/dx x^3', rad).closedForm).toBeUndefined()
    expect(evaluateLine('lim x->0+ 1/x', rad).closedForm).toBeUndefined()
    expect(evaluateLine('2/3', rad).closedForm).toBeUndefined()
  })

  it('degree mode jobs are in radians with the π/180 factor', () => {
    const job = evaluateLine('∫0..90 sin(x)', deg).closedForm!
    expect(JSON.stringify(job)).toContain('"name":"pi"')
    expect(job.rad).toBe(false)
  })

  it('a remembered answer becomes the exact form, synchronously', () => {
    const r = evaluateLine('∫0..3 x^2 - x/7', rad)
    const job = r.closedForm!
    expect(knownClosedForm(jobKey(job))).toBeUndefined()
    rememberClosedForm(job, '60/7')
    const again = evaluateLine('∫0..3 x^2 - x/7', rad)
    expect(again.exact).toBe('60/7')
    expect(again.closedForm).toBeUndefined()
    rememberClosedForm(evaluateLine('∫0..1 x^5 + x/9', rad).closedForm!, null)
    const none = evaluateLine('∫0..1 x^5 + x/9', rad)
    expect(none.exact).toBeUndefined()
    expect(none.closedForm).toBeUndefined()
  })

  it('no job for integrands the high-precision evaluator can not read', () => {
    expect(evaluateLine('∫0..2 floor(x) + x/3', rad).closedForm).toBeUndefined()
  })
})

describe('superscripts', () => {
  it('read as powers, so typeset answers read back', () => {
    expect(shown('2²')).toBe('4')
    expect(shown('3³ + 1')).toBe('28')
    expect(shown('2⁻¹')).toBe('0.5')
    expect(shown('10³')).toBe('1000')
    expect(shown('sin⁻¹(1)', deg)).toBe('90')
    expect(evaluateSheet(['f(x) = 3x² + 1', 'f(2)'], rad)[1]!.display).toBe('13')
  })
})

describe('more calculus', () => {
  it.each([
    ['d/dx x at 0', 1],
    ['d^3/dx^3 sin(x) at 0', -1],
    ['d^2/dx^2 sin(x) at 0', 0],
    ['d^4/dx^4 x^4 at 2', 24],
    ['d³/dx³ x^3 at 1', 6],
    ['d/dx abs(x) at 2', 1],
    ['d/dx abs(x) at -3', -1],
    ['∫0..4 3', 12],
    ['∫-2..-1 x', -1.5],
    ['∫1..e 1/x', 1],
    ['∫0..1 (x+1)^2', 7 / 3],
    ['lim x->7 4', 4],
    ['lim x->∞ (2x+1)/(x-3)', 2],
    ['lim x->0- e^(1/x)', 0],
    ['lim x->0 abs(x)', 0],
    ['lim x->∞ (3x^2-x)/(x^2+4)', 3],
  ])('%s', (text, want) => {
    expect(value(text)).toBeCloseTo(want, 8)
  })

  it('a higher derivative of sin in degrees carries π/180 each time', () => {
    const factor = Math.PI / 180
    expect(value('d^3/dx^3 sin(x) at 0', deg)).toBeCloseTo(-(factor ** 3), 8)
    expect(shown('d/dx tan(x)', deg)).toBe('π sec(x)²/180')
  })

  it('shows the formula when there is no point', () => {
    expect(shown('d^3/dx^3 sin(x)')).toBe('-cos(x)')
    expect(shown('d³/dx³ x^3')).toBe('6')
    expect(shown('d/dx (x+1)(x-1)')).toBe('2x')
    expect(shown('d/dx ln(x^2)')).toBe('2/x')
  })

  it('abs is |x|/x away from 0, and has no second derivative at the corner', () => {
    expect(value('d^2/dx^2 abs(x) at 2')).toBe(0)
    expect(shown('d/dx abs(x)')).toBe('|x|/x')
    expect(shown('d^2/dx^2 abs(x) at 0')).toBe('')
  })

  it('bounds and the body can use a stored value', () => {
    const rows = evaluateSheet(['a = 4', '∫0..a x', 'd/dx a*x^2 at 3'], rad)
    expect(rows[1]!.value!.n).toBeCloseTo(8, 10)
    expect(rows[2]!.value!.n).toBeCloseTo(24, 10)
  })

  it('a slow divergence stays blank; a steady one is signed', () => {
    expect(shown('lim x->0+ ln(x)')).toBe('')
    expect(shown('lim x->0 ln(x)')).toBe('')
    expect(shown('lim x->0+ 1/x')).toBe('∞')
    expect(shown('lim x->0- 1/x')).toBe('-∞')
  })

  it('an integral of a non-number stays blank', () => {
    expect(value('∫0..1 1/x^0.5 - 1/x^0.5')).toBeCloseTo(0, 8)
    expect(shown('∫0..1 sqrt(-x-1)')).toBe('')
  })
})
