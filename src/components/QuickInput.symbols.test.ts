import { describe, expect, it } from 'vitest'
import { evaluateSheet } from '../engine/evaluate'
import { buildGraph } from '../engine/graph'
import { defaultSettings, mergeSettings, settingsEqual } from '../lib/settings'
import { prettyTokens } from './QuickInput'

function typeKeys(text: string, keepWords = false): string {
  let value = ''
  for (const ch of text) {
    const raw = value + ch
    value = prettyTokens(raw, raw.length, keepWords)
  }
  return prettyTokens(value, undefined, keepWords)
}

function outcome(lines: string[], angleMode: 'deg' | 'rad' = 'deg') {
  const last = evaluateSheet(lines, { angleMode }).at(-1)!
  return { display: last.display, exact: last.exact, n: last.value?.n, error: last.error }
}

describe('typed words become symbols', () => {
  it.each([
    ['sqrt(2)', '√(2)'],
    ['sqrt2', '√2'],
    ['2sqrt(3)', '2√(3)'],
    ['sqrt 2', '√ 2'],
    ['sqrt(sqrt(16))', '√(√(16))'],
    ['1/sqrt(2)+pi', '1/√(2)+π'],
    ['cbrt(27)', '∛(27)'],
    ['theta = 30', 'θ = 30'],
    ['\\sqrt{2}', '\\sqrt{2}'],
    ['\\pi', '\\pi'],
    ['sqrts', '√s'],
    ['pint', 'pint'],
    ['infinity', 'infinity'],
  ])('%s → %s', (typed, shown) => {
    expect(typeKeys(typed)).toBe(shown)
  })

  it('turns sqrt and cbrt into symbols the moment the word is typed', () => {
    expect(prettyTokens('sqrt', 4)).toBe('√')
    expect(prettyTokens('2cbrt', 5)).toBe('2∛')
    expect(prettyTokens('sqrt(', 5)).toBe('√(')
  })

  it('converts pi, int and sum at once and gives the word back if it grows', () => {
    expect(prettyTokens('pi', 2)).toBe('π')
    expect(prettyTokens('int', 3)).toBe('∫')
    expect(prettyTokens('sum', 3)).toBe('Σ')
    for (const w of ['pint', 'picofarad', 'integral of x^2 from 0 to 1', 'interest', 'infinity', 'info', 'summary', 'product']) {
      expect(typeKeys(w)).toBe(w)
    }
  })

  it('theta still waits a keystroke, since thetas and theta2 are names', () => {
    expect(prettyTokens('theta', 5)).toBe('theta')
  })
})

const SAME_ANSWER: string[][] = [
  ['sqrt(2)'],
  ['sqrt2'],
  ['sqrt 2'],
  ['2sqrt(3)'],
  ['2 sqrt 3'],
  ['sqrt(2)^2'],
  ['sqrt(8)'],
  ['5/sqrt(41)'],
  ['sqrt(12)/sqrt(3)'],
  ['sqrt(2)*sqrt(8)'],
  ['sqrt(sqrt(16))'],
  ['sqrt(1/2)'],
  ['sqrt(2.25)'],
  ['sqrt(-1)'],
  ['-sqrt(4)'],
  ['2^sqrt(4)'],
  ['e^sqrt(4)'],
  ['sqrt(4)!'],
  ['sqrt(4)%'],
  ['sqrt2pi'],
  ['sqrt(2) + pi'],
  ['sqrt(16 m^2)'],
  ['sqrt(9) km'],
  ['sin(pi/4)^2 + sqrt(3)/2'],
  ['10 ± sqrt(4)'],
  ['x = 9', 'sqrt(x)'],
  ['x = 3', 'sqrt(2)x'],
  ['f(x) = sqrt(x)', 'f(16)'],
  ['cbrt(27)'],
  ['2cbrt(8)'],
  ['theta = 30', 'sin(theta)'],
  ['theta = 30', '2theta'],
  ['theta = 30', 'sqrt(theta)'],
]

describe('the symbol gives the same answer as the word', () => {
  it.each(SAME_ANSWER)('%s', (...lines) => {
    const pretty = lines.map((l) => typeKeys(l))
    expect(pretty).not.toEqual(lines)
    expect(outcome(pretty)).toEqual(outcome(lines))
  })

  it('in radians too', () => {
    expect(outcome([typeKeys('sin(pi/3) + sqrt(3)')], 'rad')).toEqual(outcome(['sin(pi/3) + sqrt(3)'], 'rad'))
  })

  it('answers theta once it has become θ', () => {
    expect(outcome([typeKeys('theta = 30'), typeKeys('sin(theta)')]).n).toBeCloseTo(0.5)
  })

  it('graphs the same curve', () => {
    for (const word of ['graph sqrt(x)', 'graph sqrt(x^2+1)', 'graph 2sqrt(x) - cbrt(x)']) {
      expect(buildGraph(typeKeys(word))?.points).toEqual(buildGraph(word)?.points)
    }
  })
})

describe('keeping typed words as text', () => {
  it.each([
    ['sqrt(2)', 'sqrt(2)'],
    ['2pi', '2pi'],
    ['theta = 30', 'theta = 30'],
    ['cbrt(8)', 'cbrt(8)'],
    ['1/inf', '1/inf'],
    ['4 dot 1', '4 dot 1'],
    ['10 +/- 0.7', '10 ± 0.7'],
    ['10 -/+ 0.7', '10 ∓ 0.7'],
    ['10 ~ 0.7', '10 ± 0.7'],
  ])('%s → %s', (typed, shown) => {
    expect(typeKeys(typed, true)).toBe(shown)
  })

  it('leaves ans as a word', () => {
    expect(typeKeys('ans*2', true)).toBe('ans*2')
  })

  it('gives the same answer as the symbols', () => {
    for (const word of ['sqrt(2)', '2pi', 'cbrt(27)', '4 dot 1', '10 +/- 0.7']) {
      expect(outcome([typeKeys(word, true)])).toEqual(outcome([typeKeys(word)]))
    }
  })
})

describe('keepWords setting', () => {
  it('is off by default and round-trips', () => {
    expect(defaultSettings().keepWords).toBe(false)
    const on = mergeSettings({ keepWords: true }, defaultSettings())
    expect(on.keepWords).toBe(true)
    expect(mergeSettings({}, on).keepWords).toBe(true)
    expect(settingsEqual(on, defaultSettings())).toBe(false)
  })
})
