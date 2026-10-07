import { describe, expect, it } from 'vitest'
import { evaluateSheet } from './evaluate'
import { solveCall } from './solve'

const answer = (...lines: string[]) => evaluateSheet(lines).at(-1)!

describe('solve(eq, x)', () => {
  it.each([
    ['solve(x^2 = 4, x)', 'solve x^2 = 4 for x'],
    ['solve(x^2 = 4)', 'solve x^2 = 4'],
    ['solve( 2x + 3 = 7 , x )', 'solve 2x + 3 = 7 for x'],
    ['solve(max(a, 2) = 3, a)', 'solve max(a, 2) = 3 for a'],
    ['solve(a) + 1', 'solve(a) + 1'],
    ['solve(x = 2, 3)', 'solve(x = 2, 3)'],
  ])('reads %s as %s', (typed, words) => {
    expect(solveCall(typed)).toBe(words)
  })

  it('solves for the variable named', () => {
    expect(answer('solve(x^2 = 4, x)').display).toBe('±2')
    expect(answer('solve(a*b = 6, b)').display).toBe('b = 6/a')
    expect(answer('solve(y = m*x + b, x)').display).toBe('x = (y - b)/m')
  })
})

describe('equations with units', () => {
  it.each([
    'solve(0.5 = e ^ (- t / 0.384 "milliseconds"), t)',
    'solve(0.5 = e^(-t/0.384 milliseconds), t)',
    'solve(0.5 = e^(-t/0.384 ms), t)',
    'solve 0.5 = e^(-t/0.384 ms) for t',
    '0.5 = e^(-t/0.384 ms)',
  ])('%s is a time, in the ms typed', (line) => {
    const r = answer(line)
    expect(r.kind).toBe('solve')
    expect(r.solve?.variable).toBe('t')
    expect(r.display).toBe('0.266168517335 ms')
  })

  it.each([
    ['solve(10 m = v * 2 s, v)', '5 m/s'],
    ['solve(5 kg * a = 20 N, a)', '4 m/s²'],
    ['x^2 = 9 m^2', '±3 m'],
    ['solve(20 m = 0.5 * 9.8 m/s^2 * t^2, t)', '±2.0203050891 s'],
  ])('%s is %s', (line, shown) => {
    expect(answer(line).display).toBe(shown)
  })

  it('keeps a quantity whole in a formula, so its unit is not read as letters', () => {
    expect(answer('solve(F = m * 9.8 m/s^2, m)').display).toBe('m = F/(9.8 m/s^2)')
    expect(answer('solve(v^2 = 2 * 9.8 m/s^2 * h, h)').display).toBe('h = v^2/(2(9.8 m/s^2))')
  })

  it('carries the unit on through ans', () => {
    expect(answer('solve(0.5 = e^(-t/0.384 ms), t)', 'ans * 2').display).toBe('0.53233703467 ms')
  })

  it('leaves an assignment with a unit an assignment', () => {
    expect(answer('x = 5 m').kind).toBe('assignment')
  })
})

describe('solve(eq, x = guess)', () => {
  const rad = (line: string) => evaluateSheet([line], { angleMode: 'rad' })[0]!

  it.each([
    ['solve(x ^ 2 - cos(x) = 0, x = 0.2)', '0.824132312303'],
    ['solve(x ^ 2 - cos(x) = 0, x = -9)', '-0.824132312303'],
    ['solve(x^2 = 4, x = -5)', '-2'],
    ['solve(sin(x) = 0.5, x = 2)', '2.61799387799'],
    ['solve(sin(x) = 0.5, x = pi/4)', '0.523598775598'],
    ['solve(e^x = 3x, x = 0)', '0.619061286736'],
    ['solve(x^3 - 2x - 5 = 0, x = 2)', '2.09455148154'],
    ['solve x^2 - cos(x) = 0 for x near 0.2', '0.824132312303'],
  ])('%s starts from the guess and gives %s', (line, shown) => {
    const r = rad(line)
    expect(r.kind).toBe('solve')
    expect(r.display).toBe(shown)
  })

  it('still says when there is nothing to find', () => {
    expect(rad('solve(x^2 = -1, x = 1)').display).toBe('no real solution')
  })

  it('takes a guess with units', () => {
    expect(answer('solve(0.5 = e^(-t/0.384 ms), t = 0.1 ms)').display).toBe('0.266168517335 ms')
    expect(answer('solve(x^2 = 9 m^2, x = -1 m)').display).toBe('-3 m')
  })

  it('reads the guess out of the call', () => {
    expect(solveCall('solve(x^2 - cos(x) = 0, x = 0.2)')).toBe('solve x^2 - cos(x) = 0 for x near 0.2')
    expect(solveCall('solve(f(a, b) = 0, a = -1)')).toBe('solve f(a, b) = 0 for a near -1')
  })
})
