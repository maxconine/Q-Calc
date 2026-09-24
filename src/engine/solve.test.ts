import { describe, expect, it } from 'vitest'
import { evaluateLine, evaluateSheet, stripTrailingEquals } from './evaluate'
import { isEquation, MAX_EVALS, parseEquation, pickUnknown, solveEquation } from './solve'
import type { EvaluateOptions } from './types'

const F = { f: { params: ['t'], body: 't^2-2' } }

function line(text: string, opts: EvaluateOptions = {}) {
  return evaluateLine(text, { angleMode: 'deg', ...opts })
}

describe('solve: answers', () => {
  const cases: Array<[number, string, EvaluateOptions, string, string, string | undefined]> = [
    [1, '2x+3=11', {}, 'x', '4', undefined],
    [2, '2x + 3 = 11', {}, 'x', '4', undefined],
    [3, 'x^2=2', {}, 'x', '±1.41421356237', '±sqrt(2)'],
    [4, 'x^2-5x+6=0', {}, 'x', '2, 3', undefined],
    [5, 'solve x^2-5x+6', {}, 'x', '2, 3', undefined],
    [6, '3x = 12 for x', {}, 'x', '4', undefined],
    [7, '3x = 1', {}, 'x', '0.333333333333', '1/3'],
    [8, '3x = 1', { fractionMode: true }, 'x', '1/3', undefined],
    [9, 'x/3 = 1/2', {}, 'x', '1.5', undefined],
    [10, '0.1x = 0.3', {}, 'x', '3', undefined],
    [11, '2(x+1) = 8', {}, 'x', '3', undefined],
    [12, 'x(x-1) = 6', {}, 'x', '-2, 3', undefined],
    [13, 'n(n+1) = 12', {}, 'n', '-4, 3', undefined],
    [14, 'x^2 - 6x + 7 = 0', {}, 'x', '1.58579, 4.41421', '3 ± sqrt(2)'],
    [15, '2x^2 = 1', {}, 'x', '±0.707106781187', '±sqrt(2)/2'],
    [16, 'x^2 - x - 1 = 0', {}, 'x', '-0.618034, 1.61803', '(1 ± sqrt(5))/2'],
    [17, 'x^2+2x+1=0', {}, 'x', '-1', undefined],
    [18, '(x-0.3)^2 = 0', {}, 'x', '0.3', undefined],
    [19, '3x^2 - 2x = 0', {}, 'x', '0, 0.666667', '0, 2/3'],
    [20, 'x^2 = 1/4', {}, 'x', '±0.5', undefined],
    [21, 'x^2 = pi', {}, 'x', '±1.77245385091', undefined],
    [22, 'x^2 = 1e12', {}, 'x', '±1000000', undefined],
    [23, 'x/3 = 1e9', {}, 'x', '3000000000', undefined],
    [24, 'x^3 - 6x^2 + 11x - 6 = 0', {}, 'x', '1, 2, 3', undefined],
    [25, 'x^3 = 2', {}, 'x', '1.25992104989', undefined],
    [26, 'x^4 - 5x^2 + 4 = 0', {}, 'x', '-2, -1, 1, 2', undefined],
    [27, 'x^6 = 64', {}, 'x', '±2', undefined],
    [28, 'x^5 - x - 1 = 0', {}, 'x', '1.16730397826', undefined],
    [29, '2^x = 8', {}, 'x', '3', undefined],
    [30, 'e^x = 5', {}, 'x', '1.60943791243', undefined],
    [31, 'ln(x) = 1', {}, 'x', '2.71828182846', undefined],
    [32, '1.05^n = 2', {}, 'n', '14.2066990829', undefined],
    [33, 'sqrt(x) = 3', {}, 'x', '9', undefined],
    [34, 'x = sqrt(x + 6)', {}, 'x', '3', undefined],
    [35, 'abs(x-1) = 2', {}, 'x', '-1, 3', undefined],
    [36, 'x! = 120', {}, 'x', '5', undefined],
    [37, 'x = 2x - 5', {}, 'x', '5', undefined],
    [38, 'x + 15% = 230', {}, 'x', '200', undefined],
    [39, 'sin(x) = 0.5', {}, 'x', '30, 150', undefined],
    [40, 'sin(x) = 0.5', { angleMode: 'rad' }, 'x', '0.523599, 2.61799', 'pi/6, 5*pi/6'],
    [41, 'sin x = 0.5', {}, 'x', '30, 150', undefined],
    [42, 'sin(θ) = 0.5', {}, 'θ', '30, 150', undefined],
    [43, 'tan(x) = 1', {}, 'x', '45, 225', undefined],
    [44, 'cos(x) = 0.5', {}, 'x', '60, 300', undefined],
    [45, 'sin(x) = 1', {}, 'x', '90', undefined],
    [46, 'sin(2x) = sin(x)', {}, 'x', '0, 60, 180, 300', undefined],
    [47, 'cos(x) = x', { angleMode: 'rad' }, 'x', '0.739085133215', undefined],
    [48, 'exp(-x) = x', { angleMode: 'rad' }, 'x', '0.56714329041', undefined],
    [49, 'x*sin(x) = 1', { angleMode: 'rad' }, 'x', '-2.7726, -1.11416, 1.11416, 2.7726, …', undefined],
    [50, 'pi r^2 = 10', {}, 'r', '±1.78412411615', undefined],
    [51, '1/x = 2', {}, 'x', '0.5', undefined],
    [52, 'x^{2}=2', {}, 'x', '±1.41421356237', '±sqrt(2)'],
    [53, '1,000x = 5,000', {}, 'x', '5', undefined],
    [54, 'x·3 = 12', {}, 'x', '4', undefined],
    [55, '2x+3=11', { variables: { x: 5 } }, 'x', '4', undefined],
    [56, 'x + y = 10', { variables: { x: 3 } }, 'y', '7', undefined],
    [57, 'x + y = 10 for y', { variables: { x: 3, y: 1 } }, 'y', '7', undefined],
    [58, '2f(x) = 6', { functions: F }, 'x', '±2.2360679775', '±sqrt(5)'],
    [59, 'solve f(x) = 3', { functions: F }, 'x', '±2.2360679775', '±sqrt(5)'],
    // the literal reading, though m could be metres
    [60, '2m = 10', {}, 'm', '5', undefined],
  ]
  it.each(cases)('%i: %s', (_n, text, opts, variable, display, exact) => {
    const r = line(text, opts)
    expect(r.kind).toBe('solve')
    expect(r.solve?.variable).toBe(variable)
    expect(r.solve?.outcome).toBe('roots')
    expect(r.display).toBe(display)
    expect(r.exact).toBe(exact)
  })

  it('never stores the root', () => {
    const vars = { x: 5 }
    const rows = evaluateSheet(['2x+3=11', 'x'], { angleMode: 'deg', variables: vars })
    expect(rows[0]!.display).toBe('4')
    expect(rows[1]!.display).toBe('5')
    expect(vars).toEqual({ x: 5 })
  })

  it('marks the rest of a long list', () => {
    const r = line('x*sin(x) = 1', { angleMode: 'rad' })
    expect(r.solve?.more).toBe(true)
    expect(r.solve?.roots).toHaveLength(4)
  })

  it('keeps the root at full precision for ans', () => {
    expect(line('x^3 = 2').value?.n).toBeCloseTo(Math.cbrt(2), 15)
    expect(line('3x = 1').value?.n).toBe(1 / 3)
    expect(line('sin(x) = 1', { angleMode: 'rad' }).value?.n).toBe(Math.PI / 2)
  })
})

describe('solve: messages', () => {
  const cases: Array<[number, string, string, string]> = [
    [61, 'x^2 = -1', 'no real solution', 'none'],
    [62, 'x^2 + 1e-10 = 0', 'no real solution', 'none'],
    [63, 'x + 1 = x + 2', 'no solution', 'contradiction'],
    [64, '2(x+1) = 2x+2', 'true for all x', 'all'],
    [65, 'sin(x) = 2', 'no solution found', 'noneFound'],
    [66, 'sqrt(x) = -1', 'no solution found', 'noneFound'],
    [67, '1/x = 0', 'no solution found', 'noneFound'],
  ]
  it.each(cases)('%i: %s', (_n, text, display, outcome) => {
    const r = line(text)
    expect(r.kind).toBe('solve')
    expect(r.display).toBe(display)
    expect(r.solve?.outcome).toBe(outcome)
    expect(r.value?.kind).toBe('text')
  })
})

describe('solve: not a solve', () => {
  it('68-74: keeps assignments and definitions', () => {
    expect(line('x = 5')).toMatchObject({ kind: 'assignment', display: '5' })
    expect(line('rent = 1850')).toMatchObject({ kind: 'assignment', display: '1850' })
    expect(line('x = x + 1', { variables: { x: 5 } })).toMatchObject({ kind: 'assignment', display: '6' })
    expect(line('x = 2x - 5', { variables: { x: 3 } })).toMatchObject({ kind: 'assignment', display: '1' })
    expect(line('y = 3x + 1')).toMatchObject({ kind: 'assignment', display: '' })
    expect(line('f(x) = x^2').kind).toBe('function')
    expect(line('f(x) = 3').kind).toBe('function')
  })

  const blanks: Array<[number, string, EvaluateOptions]> = [
    [75, 'a*b = 6', {}],
    [77, 'x + y = 10', {}],
    [78, 'x + y = 10', { variables: { x: 3, y: 1 } }],
    [79, 'total = total + 5', {}],
    [80, '2 kg * a = 10 N', {}],
    [81, '3 cm = 12', {}],
    [82, 'x == 3', {}],
    [83, 'x <= 3', {}],
    [84, 'x = 5 = 5', {}],
    [85, '= 5', {}],
    [86, '2+3=5', {}],
    [88, '2x+3=', {}],
    [89, 'x^2 = 4 ± 0.1', {}],
    [90, 'floor(x) = 3', {}],
    [91, 'x^2', {}],
    [92, 'solve', {}],
    [92, 'solve ', {}],
    [93, 'x^2 = a for x', {}],
  ]
  it.each(blanks)('%i: %s is blank', (_n, text, opts) => {
    const r = line(text, opts)
    expect(r.kind).not.toBe('solve')
    expect(r.display).toBe('')
  })

  it('76: xy = 6 stays what it was (an assignment to xy)', () => {
    expect(line('xy = 6').kind).toBe('assignment')
  })

  it('87: one trailing = is dropped', () => {
    expect(line('2+3=').display).toBe('5')
    expect(line('2+3 = ').display).toBe('5')
    expect(line('2+3==').display).toBe('')
    expect(line('x =', { variables: { x: 7 } }).display).toBe('7')
    expect(stripTrailingEquals('3 !=')).toBe('3 !=')
    expect(stripTrailingEquals('3 <=')).toBe('3 <=')
    expect(stripTrailingEquals('=')).toBe('=')
  })

  it('reads sin 2x as too ambiguous to solve', () => {
    expect(line('sin 2x = 0.5').kind).not.toBe('solve')
  })
})

describe('solve: guarding against wrong numbers', () => {
  it('does not treat a near-polynomial as one', () => {
    // looks quadratic near 0; a fitted quadratic would say "no real solution"
    expect(line('exp(x/1e6) = 0.4').display).toBe('-916290.731874')
    // looks linear everywhere it's sampled; the true root is beyond the search
    expect(line('exp(x*1e-12) = 2').display).toBe('no solution found')
  })

  it('does not round a coefficient to a fraction', () => {
    const r = line('0.333333333334x = 1')
    expect(r.display).toBe('2.99999999999')
    expect(r.exact).toBeUndefined()
  })

  it('tells tiny terms from nothing', () => {
    expect(line('x + 1e-13 = x').display).toBe('no solution')
    expect(line('1e-13x + 1 = 1').display).toBe('0')
    expect(line('x*(1+1e-13) = x').display).toBe('0')
  })

  it('keeps exact forms nice', () => {
    expect(line('exp(x/1e6) = 0.4').exact).toBeUndefined()
    expect(line('sqrt(x - 0.3) = 0').display).toBe('0.3')
  })

  it('gives close roots the digits that tell them apart', () => {
    expect(line('(x-1)(x-1.000001)=0').display).toBe('1, 1.000001')
    expect(line('x^2 = 1e-20').display).toBe('±1e-10')
  })

  it('reads a root float can only see as flat', () => {
    expect(line('tan(x) = x', { angleMode: 'rad' }).display).toBe('-7.72525, -4.49341, 0, 4.49341, …')
    expect(line('sin(x) = x', { angleMode: 'rad' }).display).toBe('0')
  })

  it('handles repeated roots', () => {
    expect(line('(x-1)^2(x-2) = 0').display).toBe('1, 2')
    expect(line('x^4 = 0').display).toBe('0')
    expect(line('x = x').display).toBe('true for all x')
  })
})

describe('parseEquation and pickUnknown', () => {
  it('splits the forms', () => {
    expect(parseEquation('2x+3=11')).toMatchObject({ lhs: '2x+3', rhs: '11', explicit: false })
    expect(parseEquation('solve x^2-4')).toMatchObject({ lhs: 'x^2-4', rhs: '0', explicit: true })
    expect(parseEquation('3x = 12 for x')).toMatchObject({ lhs: '3x', rhs: '12', forVar: 'x', explicit: true })
    expect(parseEquation('x = 2x - 5')?.assignVar).toBe('x')
    expect(parseEquation('x^2 - 4 for x')).toBeNull()
    expect(parseEquation('graph y = x')).toBeNull()
    expect(parseEquation('x != 3')).toBeNull()
    expect(parseEquation('x ≤ 3')).toBeNull()
  })

  it('picks the one free letter, or a stored one', () => {
    const pick = (text: string, variables?: Record<string, number>) => pickUnknown(parseEquation(text)!, { variables })
    expect(pick('2x = 10')).toBe('x')
    expect(pick('2x = 10', { x: 5 })).toBe('x')
    expect(pick('x + y = 10', { x: 3 })).toBe('y')
    expect(pick('x + y = 10', { x: 3, y: 1 })).toBeNull()
    expect(pick('e^x = 5')).toBe('x')
    expect(pick('3 cm = 12')).toBeNull()
    expect(pick('x = 5')).toBeNull()
    expect(pick('x = 2x - 5', { x: 3 })).toBeNull()
    expect(pick('x = 2x - 5 for x', { x: 3 })).toBe('x')
  })

  it('102: isEquation', () => {
    for (const t of ['2x+3=11', 'solve x', 'x = 2x - 5', '2+3=5']) expect(isEquation(t), t).toBe(true)
    for (const t of ['x = 5', '2+3', 'graph y = x', '2+3=', 'rent = 1850']) expect(isEquation(t), t).toBe(false)
  })
})

describe('solve in a sheet', () => {
  it('100: ans carries a single root', () => {
    expect(evaluateSheet(['3x = 12', 'ans * 2'])[1]!.display).toBe('8')
  })

  it('101: the letter is not stored', () => {
    expect(evaluateSheet(['x^2 = 4', 'x'])[1]!.display).toBe('')
  })

  it('takes ans from before a list of roots', () => {
    expect(evaluateSheet(['5', 'x^2 = 4', 'ans + 1'])[2]!.display).toBe('6')
  })
})

describe('solve budget', () => {
  it('stays under the evaluation cap and is quick', () => {
    solveEquation('x^3 = 2', { angleMode: 'deg' })
    for (const [text, angleMode] of [
      ['e^x = 5', 'deg'],
      ['sin(x) = 0.5', 'deg'],
      ['cos(x) = x', 'rad'],
      ['x*sin(x) = 1', 'rad'],
    ] as const) {
      let best = Infinity
      for (let i = 0; i < 5; i++) {
        const t = performance.now()
        const s = solveEquation(text, { angleMode })
        best = Math.min(best, performance.now() - t)
        expect(s, text).not.toBeNull()
        expect(s!.evals, text).toBeLessThanOrEqual(MAX_EVALS)
      }
      expect(best, text).toBeLessThan(10)
    }
  })
})

describe('single-equation solve', () => {
  it('a linear equation uses a stored coefficient', () => {
    const rows = evaluateSheet(['a = 2', 'a*x = 10'])
    expect(rows[1]?.display).toBe('5')
    expect(rows[1]?.solve?.variable).toBe('x')
    expect(rows[1]?.solve?.roots).toEqual([5])
  })

  it('solves for the named unknown when two letters appear', () => {
    const r = line('x + y = 5 for y', { variables: { x: 2 } })
    expect(r.solve?.variable).toBe('y')
    expect(r.solve?.roots?.[0]).toBeCloseTo(3, 10)
  })

  it.each([
    ['x^2 = 0', '0'],
    ['-3x = 9', '-3'],
    ['2^x = 8', '3'],
    ['2^x = 1', '0'],
    ['(x-2)^2 = 0', '2'],
    ['x^2 = 9', '±3'],
  ])('%s → %s', (text, display) => {
    const r = line(text)
    expect(r.display).toBe(display)
    expect(r.solve?.variable).toBe('x')
  })

  it('a negative square has no real root', () => {
    expect(line('x^2 = -4').display).toBe('no real solution')
    expect(line('x^2 + 4 = 0').solve?.outcome).toBe('none')
  })

  it('an identity and a contradiction stay messages', () => {
    expect(line('2x = x+x').display).toBe('true for all x')
    expect(line('0*x = 1').display).toMatch(/no solution/)
  })

  it('degrees: sin(x) = 1/2 includes 30', () => {
    const r = line('sin(x) = 0.5')
    expect(r.solve?.roots).toContain(30)
    expect(r.solve?.roots?.every((n) => Math.abs(Math.sin((n * Math.PI) / 180) - 0.5) < 1e-9)).toBe(true)
  })

  it('a bare assignment is not a solve', () => {
    expect(line('x = 5').kind).toBe('assignment')
    expect(line('x = 5').solve).toBeUndefined()
  })
})
