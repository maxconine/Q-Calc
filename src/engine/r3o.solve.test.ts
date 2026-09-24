import { describe, expect, it } from 'vitest'
import { evaluateSheet } from './evaluate'

const last = (lines: string[], opts = {}) => evaluateSheet(lines, { angleMode: 'rad', ...opts }).at(-1)!.display

describe('solve with a huge or tiny constant', () => {
  it.each([
    ['2x = 1e20', '5e+19'],
    ['3x = 6e18', '2e+18'],
    ['x/1e20 = 3', '3e+20'],
    ['x/6.022e23 = 2', '1.2044e+24'],
    ['x^2 = 1e16', '±100000000'],
    ['x^3 = 1e20', '4641588.83361'],
    ['x^4 = 1e20', '±100000'],
    ['x^2 = 1e-30', '±1e-15'],
    ['x^2 = 6.6e-34', '±2.56904651573e-17'],
  ])('%s → %s', (text, shown) => {
    expect(last([text])).toBe(shown)
  })

  it('never claims none or all from a term lost next to a huge constant', () => {
    expect(['', 'no solution found']).toContain(last(['x + 1e20 = 2e20']))
    expect(['', 'no solution found']).toContain(last(['x + 1e20 = 1e20']))
  })

  it('still finds real contradictions and identities', () => {
    expect(last(['0.1x*3 + 1 = 0.3x'])).toBe('no solution')
    expect(last(['0.1x*3 = 0.3x'])).toBe('true for all x')
    expect(last(['(x+1)^2 = x^2 + 2x + 1'])).toBe('true for all x')
    expect(last(['(x-pi)^2 = 0'])).toBe('3.14159265359')
  })
})
