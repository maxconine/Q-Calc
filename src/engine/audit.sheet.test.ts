import { describe, expect, it } from 'vitest'
import { expectValue, last, line } from './audit.helpers'
import { evaluateSheet } from './evaluate'

describe('audit: variables across lines', () => {
  it.each<[string[], number]>([
    [['x = 5', 'x * 2'], 10],
    [['x = 3', 'y = 4', 'sqrt(x^2 + y^2)'], 5],
    [['a = 2', 'a = a + 1', 'a'], 3],
    [['x = 5', 'x = x * 2', 'x + 1'], 11],
    [['x = 4', '2x'], 8],
    [['x = 2', 'x(3)'], 6],
    [['r = 2', 'pi r^2'], 4 * Math.PI],
    [['x = 3', 'x!'], 6],
    [['n = 5', 'n(n+1)/2'], 15],
    [['m = 3', 'm * 2'], 6],
    [['s = 10', 's / 2'], 5],
    [['c = 3', 'c^2'], 9],
    [['h = 2', 'h * 3'], 6],
    [['g = 9.8', 'g * 2'], 19.6],
    [['a = 2', 'b = 3', 'a*b + a'], 8],
    [['rate = 0.05', 'years = 10', '1000 * (1 + rate)^years'], 1000 * 1.05 ** 10],
    [['x = -3', 'x^2'], 9],
    [['x = -3', '-x'], 3],
    [['x = 2', 'x^x^x'], 16],
    [['width = 3', 'height = 4', 'width * height'], 12],
    [['x1 = 2', 'x2 = 5', 'x2 - x1'], 3],
    [['x = 10', 'y = x / 4', 'y'], 2.5],
  ])('%j → %d', (lines, want) => {
    expectValue(last(lines), want)
  })

  it('pi and e cannot be reassigned', () => {
    expectValue(last(['pi = 3', '2 * pi']), 2 * Math.PI)
    expectValue(last(['e = 5', 'e']), Math.E)
  })

  it('an undefined variable is blank, not zero', () => {
    expect(line('q * 2').display).toBe('')
    expect(last(['x = 5', 'y * 2']).display).toBe('')
  })

  it('a variable does not leak into another sheet', () => {
    evaluateSheet(['zz = 5'])
    expect(line('zz * 2').display).toBe('')
  })
})

describe('audit: ans', () => {
  it.each<[string[], number]>([
    [['2+3', 'ans * 2'], 10],
    [['5', 'ans^2'], 25],
    [['3', '4', 'ans + ans'], 8],
    [['x = 10', 'ans'], 10],
    [['5', '1/0', 'ans'], 5],
    [['10', 'ans / 4', 'ans * 2'], 5],
    [['2', 'sqrt(ans + 2)'], 2],
    [['7', 'ANS + 1'], 8],
  ])('%j → %d', (lines, want) => {
    expectValue(last(lines), want)
  })

  it('with no previous answer, ans is blank', () => {
    expect(line('ans').display).toBe('')
    expect(line('ans + 1').display).toBe('')
  })

  it('an ans passed in from history is used', () => {
    expectValue(line('ans * 3', { ans: 7 }), 21)
    expect(['', '10']).toContain(line('2ans', { ans: 5 }).display)
    expectValue(line('2 ans', { ans: 5 }), 10)
  })
})

describe('audit: user functions', () => {
  it.each<[string[], number]>([
    [['f(x) = x^2 + 1', 'f(3)'], 10],
    [['f(x) = 3x + 1', 'f(2)'], 7],
    [['g(a, b) = a*b + 1', 'g(2, 3)'], 7],
    [['f(x, y) = x - y', 'f(5, 3)'], 2],
    [['f(x) = 2x', 'f(f(3))'], 12],
    [['k = 10', 'f(x) = x + k', 'f(1)'], 11],
    [['f(x) = sin(x)', 'f(30)'], 0.5],
    [['area(r) = pi r^2', 'area(2)'], 4 * Math.PI],
    [['f(x) = x!', 'f(4)'], 24],
    [['f(x) = x + 1', 'f(x) = x + 2', 'f(1)'], 3],
    [['f(x) = x^2', '2f(3)'], 18],
    [['f(x) = x^2', 'f(3) + f(4)'], 25],
    [['f(x) = x^2', 'f(-3)'], 9],
    [['x = 10', 'f(x) = x^2', 'f(2)'], 4],
    [['x = 10', 'f(y) = x + y', 'f(1)'], 11],
    [['sq(x) = x*x', 'sq(sq(2))'], 16],
    [['f(x) = 2x', 'g(x) = f(x) + 1', 'g(3)'], 7],
    [['f(x) = x^2', 'f(2)!'], 24],
    [['f(t) = 5t^2', 'f(2)'], 20],
    [['f(x) = sqrt(x)', 'f(9) + f(16)'], 7],
    [['f(x) = log(x)', 'f(1000)'], 3],
    [['f(x) = x mod 3', 'f(10)'], 1],
  ])('%j → %d', (lines, want) => {
    expectValue(last(lines), want)
  })

  it('uses the angle mode of the sheet', () => {
    expectValue(last(['f(x) = cos(x)', 'f(pi)'], { angleMode: 'rad' }), -1)
  })

  it('division by zero inside a function is undefined', () => {
    expect(last(['f(x) = 1/x', 'f(0)']).display).toBe('undefined')
  })

  it('a function that calls itself forever gives no number and does not throw', () => {
    const d = last(['f(x) = f(x) + 1', 'f(1)']).display
    expect(['', 'undefined']).toContain(d)
  }, 20_000)

  it('a function on a free variable nobody set is blank or undefined', () => {
    const d = last(['f(x) = x + zz', 'f(1)']).display
    expect(['', 'undefined']).toContain(d)
  })
})

describe('audit: fraction mode', () => {
  const frac = { fractionMode: true }
  it.each([
    ['1/3 + 1/6', '1/2'],
    ['0.75', '3/4'],
    ['2/4', '1/2'],
    ['1/3', '1/3'],
    ['5', '5'],
    ['-3/4', '-3/4'],
    ['1/7', '1/7'],
    ['0.1 + 0.2', '3/10'],
    ['1.5', '3/2'],
    ['22/7', '22/7'],
    ['1/3 * 3', '1'],
    ['2/3 - 1/6', '1/2'],
    ['0.125', '1/8'],
    ['1/2 + 1/3 + 1/6', '1'],
    ['3/4 / 3', '1/4'],
    ['-0.5', '-1/2'],
  ])('%s → %s', (text, want) => {
    expect(line(text, frac).display).toBe(want)
  })

  it.each([
    ['pi', Math.PI],
    ['sqrt(2)', Math.SQRT2],
    ['0.333333', 0.333333],
    ['e', Math.E],
  ])('%s stays a decimal', (text, want) => {
    const r = line(text, frac)
    expect(r.display).not.toContain('/')
    expectValue(r, want)
  })
})

describe('audit: exact forms beside the decimal', () => {
  it.each([
    ['sqrt(8)', '2sqrt(2)'],
    ['sqrt(12)', '2sqrt(3)'],
    ['sqrt(50)', '5sqrt(2)'],
    ['sqrt(1/2)', 'sqrt(2)/2'],
    ['sqrt(2)*sqrt(3)', 'sqrt(6)'],
    ['sqrt(0.3)', 'sqrt(30)/10'],
    ['1/sqrt(3)', 'sqrt(3)/3'],
    ['sin(60)', 'sqrt(3)/2'],
    ['cos(45)', 'sqrt(2)/2'],
    ['sin(15)', '(sqrt(6)-sqrt(2))/4'],
    ['cos(15)', '(sqrt(6)+sqrt(2))/4'],
    ['tan(30)', 'sqrt(3)/3'],
    ['tan(75)', '2+sqrt(3)'],
    ['sin(30)', '1/2'],
    ['cos(120)', '-1/2'],
    ['sin(225)', '-sqrt(2)/2'],
  ])('%s → %s', (text, want) => {
    expect(line(text).exact).toBe(want)
  })

  it.each([
    ['asin(1)', 'pi/2'],
    ['acos(-1)', 'pi'],
    ['atan(1)', 'pi/4'],
    ['acos(0.5)', 'pi/3'],
    ['asin(-0.5)', '-pi/6'],
    ['atan2(1, -1)', '3*pi/4'],
  ])('%s in radians → %s', (text, want) => {
    expect(line(text, { angleMode: 'rad' }).exact).toBe(want)
  })

  it('leaves the radical in the denominator when rationalize is off', () => {
    expect(line('1/sqrt(3)', { rationalize: false }).exact).toBe('1/sqrt(3)')
  })

  it('no exact form for a plain arithmetic answer', () => {
    expect(line('1/3').exact).toBeUndefined()
    expect(line('2 + 2').exact).toBeUndefined()
  })

  // the closed-form matchers accept a 1e-8 to 1e-9 relative miss, so a value that visibly differs
  // from the closed form in its shown 12 digits still gets it, e.g. "sqrt(3) ≈ 1.73205081046"
  it('sqrt(3.000000005) is not labelled sqrt(3)', () => {
    expect(line('sqrt(3.000000005)').exact).not.toBe('sqrt(3)')
  })
  it('sqrt(2) + 1e-10 is not labelled sqrt(2)', () => {
    expect(line('sqrt(2) + 1e-10').exact).not.toBe('sqrt(2)')
  })
  it('sqrt(1.000000001) is not labelled 1', () => {
    expect(line('sqrt(1.000000001)').exact).not.toBe('1')
  })
  it('asin(0.5000000001) in radians is not labelled pi/6', () => {
    expect(line('asin(0.5000000001)', { angleMode: 'rad' }).exact).not.toBe('pi/6')
  })
  it('sin(30) + 1e-10 is not labelled 1/2', () => {
    expect(line('sin(30) + 1e-10').exact).not.toBe('1/2')
  })
})
