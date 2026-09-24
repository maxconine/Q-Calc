import { describe, expect, it } from 'vitest'
import { evaluateSheet } from './evaluate'

const last = (lines: string[], opts = {}) => evaluateSheet(lines, { angleMode: 'rad', ...opts }).at(-1)!.display

describe('a higher derivative needs the lower ones to exist at the point', () => {
  it.each([
    [['d²/dx² abs(x) at 0'], ''],
    [['f(x) = abs(x)', "f''(0)"], ''],
    [['d²/dx² abs(x - 1) at 1'], ''],
    [['d²/dx² x^(4/3) at 0'], ''],
    [['d²/dx² abs(x) at 1'], '0'],
    [['d³/dx³ x^3 at 0'], '6'],
    [['d²/dx² sqrt(x) at 1'], '-0.25'],
    [['f(x) = x^3', "f'''(2)"], '6'],
  ])('%j → %j', (lines, shown) => {
    expect(last(lines)).toBe(shown)
  })
})

describe('a bare function name is not the integration variable', () => {
  it.each([
    [['f(x) = x^2', '∫0..3 f'], ''],
    [['f(x) = x^2', '∫0..3 f(x)'], '9'],
    [['g(t) = t^3', '∫0..1 g(u) du'], '0.25'],
    [['x = 2', '∫0..x x dx'], '2'],
    [['t = 3', '∫0..t t dt'], '4.5'],
  ])('%j → %j', (lines, shown) => {
    expect(last(lines)).toBe(shown)
  })
})

describe('decisions', () => {
  // fraction mode snaps within 1e-9 so a typed 0.333333333 reads as 1/3; a computed value that close snaps too
  it.fails('a computed value 1e-10 off a fraction stays decimal', () => {
    expect(last(['1/3 + 1e-10'], { fractionMode: true })).toBe('0.333333333433')
  })

  it.fails('a solve root 1e-13 off a fraction stays decimal in fraction mode', () => {
    expect(last(['3x = 1 + 1e-12'], { fractionMode: true })).toBe('0.333333333334')
  })
})

describe('known gaps', () => {
  // the pole search splits a cell where one side flips sign; with everything on one side nothing flips
  it.fails('tan(x) - x = 0 in degrees finds the roots near ±90', () => {
    expect(last(['tan(x) - x = 0'], { angleMode: 'deg' })).toMatch(/89\.3/)
  })
})
