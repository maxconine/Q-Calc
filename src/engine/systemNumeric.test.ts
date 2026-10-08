import { describe, expect, it } from 'vitest'
import { evaluateSheet } from './evaluate'
import { parseSystemCall, solveLive } from './system'

const live = (lines: string[], angleMode: 'deg' | 'rad' = 'deg') => solveLive(lines, undefined, angleMode)?.display ?? null
const call = (line: string, angleMode: 'deg' | 'rad' = 'deg') => evaluateSheet([line], { angleMode })[0]!.display

describe('nonlinear systems, solved numerically', () => {
  it.each([
    [['x^2 + y^2 = 4', 'x^2 - y = 2'], '(x, y) = (-1.73205080757, 1) or (x, y) = (0, -2) or (x, y) = (1.73205080757, 1)'],
    [['e^x = y', 'x + y = 2'], 'x = 0.442854401002, y = 1.557145599'],
    [['ln(x) + y = 1', 'x * y = 1'], 'x = 1, y = 1'],
    [['log(x) = y', 'x + y = 12'], 'x = 10.9601822252, y = 1.03981777484'],
    [['x^3 + y = 10', 'x * y = 3'], '(x, y) = (0.300818880182, 9.97277829829) or (x, y) = (2.04337252327, 1.46816107481)'],
    [['x^2 + y^2 = 1', 'x = y'], '(x, y) = (-0.707106781187, -0.707106781187) or (x, y) = (0.707106781187, 0.707106781187)'],
    [['(x - 1)^2 + y^2 = 0', 'x = y + 1'], 'x = 1, y = 0'],
  ])('%j', (lines, shown) => {
    expect(live(lines)).toBe(shown)
  })

  it('reads trig in the angle mode', () => {
    expect(live(['sin(x) + cos(y) = 1', 'x + y = 90'], 'deg')).toBe('x = 30, y = 60')
    // sin(x) - x is flat to the third order at 0; it still lands on (0, 1)
    expect(live(['sin(x) + y = 1', 'x + y = 1'], 'rad')).toBe('x = 0, y = 1')
  })

  it('works at any scale', () => {
    expect(live(['x^2 = 1e-18', 'y = 2*x'])).toBe('x = 1e-9, y = 2e-9')
    expect(live(['x*y = 1e12', 'x + y = 3e6'])).toBe('(x, y) = (381966.01125, 2618033.98875) or (x, y) = (2618033.98875, 381966.01125)')
  })

  it('carries units', () => {
    expect(live(['x^2 * y = 2 m^3', 'x + y = 3 m'])).toBe(
      '(x, y) = (-0.732050807569 m, 3.73205080757 m) or (x, y) = (1 m, 2 m) or (x, y) = (2.73205080757 m, 0.267949192431 m)',
    )
  })

  it('shows the nearest few of many, with a …', () => {
    expect(live(['x*y*z = 6', 'x + y + z = 6', 'x^2 + y^2 + z^2 = 14'])).toMatch(/^\(x, y, z\) = \(1, 2, 3\) or .* or …$/)
  })

  it('says when the search finds nothing', () => {
    expect(live(['x^2 + y^2 = 1', 'x^2 + y^2 = 4'])).toBe('no solution found')
  })

  it('keeps the exact answers it had', () => {
    expect(live(['x + y = 3', 'x - y = 1'])).toBe('x = 2, y = 1')
    expect(live(['x^2 + y^2 = 25', 'x + y = 7'])).toBe('(x, y) = (3, 4) or (x, y) = (4, 3)')
    expect(live(['d + e = 5', 'd - e = 1'])).toBe('d = 3, e = 2')
  })
})

describe('solve({…}, x = …, y = …)', () => {
  it('reads the equations and the guesses', () => {
    expect(parseSystemCall('solve({x^2 + y^2 = 4, x^2 - y = 2}, x = 1, y = -1)')).toEqual({
      equations: ['x^2 + y^2 = 4', 'x^2 - y = 2'],
      guessText: { x: '1', y: '-1' },
    })
    expect(parseSystemCall('solve({e^x = y; x + y = 2})')).toEqual({ equations: ['e^x = y', 'x + y = 2'], guessText: {} })
    expect(parseSystemCall('solve({max(x, y) = 3, x + y = 4}, x, y)')?.equations).toEqual(['max(x, y) = 3', 'x + y = 4'])
    expect(parseSystemCall('solve(x^2 = 4, x)')).toBeNull()
  })

  it('gives the solution the guesses lead to', () => {
    expect(call('solve({x^2 + y^2 = 4, x^2 - y = 2}, x = 1, y = 1)')).toBe('x = 1.73205080757, y = 1')
    expect(call('solve({x^2 + y^2 = 4, x^2 - y = 2}, x = -1, y = 1)')).toBe('x = -1.73205080757, y = 1')
    expect(call('solve({x^2 + y^2 = 4, x^2 - y = 2}, x = 0.1, y = -3)')).toBe('x = 0, y = -2')
  })

  it('gives every solution without guesses', () => {
    expect(call('solve({x^2 + y^2 = 4, x^2 - y = 2})')).toBe('(x, y) = (-1.73205080757, 1) or (x, y) = (0, -2) or (x, y) = (1.73205080757, 1)')
    expect(call('solve({x + y = 3, x - y = 1})')).toBe('x = 2, y = 1')
  })

  it('takes guesses with units', () => {
    expect(call('solve({x^2 * y = 2 m^3, x + y = 3 m}, x = 2 m, y = 1 m)')).toBe('x = 2.73205080757 m, y = 0.267949192431 m')
    expect(call('solve({d = v * t, v = 2 m/s, t = 3 s}, d, v, t)')).toBe('d = 6 m, v = 2 m/s, t = 3 s')
  })

  it('says so when there is nothing to find', () => {
    expect(call('solve({x^2 + y^2 = 1, x + y = 5}, x = 1, y = 1)')).toBe('no solution')
  })
})
