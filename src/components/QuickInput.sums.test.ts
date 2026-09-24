import { describe, expect, it } from 'vitest'
import { evaluateSheet } from '../engine/evaluate'
import { mergeLiveAnswer } from '../lib/nativeEval'
import { prettyTokens } from './QuickInput'

function typeKeys(text: string, keepWords = false): string {
  let value = ''
  for (const ch of text) {
    const raw = value + ch
    value = prettyTokens(raw, raw.length, keepWords)
  }
  return prettyTokens(value, undefined, keepWords)
}

function outcome(lines: string[], options: { angleMode?: 'deg' | 'rad'; sigFigMode?: boolean; fractionMode?: boolean } = {}) {
  const last = evaluateSheet(lines, options).at(-1)!
  return { display: last.display, exact: last.exact, n: last.value?.n, meas: last.meas }
}

describe('sum and prod become Σ and Π', () => {
  it.each([
    ['sum n^2, n = 1..10', 'Σ n^2, n = 1..10'],
    ['sum(1, 2, 3)', 'Σ(1, 2, 3)'],
    ['2sum(n, n=1..3)', '2Σ(n, n=1..3)'],
    ['prod(2, 3)', 'Π(2, 3)'],
    ['sum_{n=1}^{10} n', 'Σ_{n=1}^{10} n'],
    ['summary', 'summary'],
    ['sums', 'sums'],
    ['product', 'product'],
    ['produce', 'produce'],
    ['x_sum', 'x_sum'],
    ['sum2', 'Σ2'],
    ['\\sum_{n=1}^{3} n', '\\sum_{n=1}^{3} n'],
    ['\\prod_{n=1}^{3} n', '\\prod_{n=1}^{3} n'],
  ])('%s → %s', (typed, shown) => {
    expect(typeKeys(typed)).toBe(shown)
  })

  it('turns into Σ at once, and back into a word if it grows into summary', () => {
    expect(prettyTokens('sum', 3)).toBe('Σ')
    expect(typeKeys('summary')).toBe('summary')
    expect(prettyTokens('sum ', 4)).toBe('Σ ')
  })

  it('keeps the words when words are kept', () => {
    expect(typeKeys('sum n, n=1..3', true)).toBe('sum n, n=1..3')
    expect(typeKeys('prod(2, 3)', true)).toBe('prod(2, 3)')
  })
})

const SAME_ANSWER: string[][] = [
  ['sum(1, 2, 3)'],
  ['sum([1, 2, 3])'],
  ['sum 5'],
  ['sum(sqrt(2), sqrt(8))'],
  ['2sum(1, 2)'],
  ['prod(2, 3, 4)'],
  ['sum n^2, n = 1..10'],
  ['sum(n^2, n, 1, 10)'],
  ['sum 1/n, n = 1..10'],
  ['sum 1/n^2, n = 1..inf'],
  ['sum (-1)^(n+1)/n, n = 1..inf'],
  ['sum 1/n, n = 1..inf'],
  ['sum sin(n*30), n = 1..12'],
  ['sum sqrt(n), n = 1..20'],
  ['sum pi/n^2, n = 1..inf'],
  ['prod n, n = 1..10'],
  ['prod (1 - 1/n^2), n = 2..100'],
  ['sum_{n=1}^{10} n^2'],
  ['2sum(n, n=1..10) + prod(n, n=1..3)'],
  ['sum from n = 1 to 10 of n^2'],
  ['m = 10', 'sum k, k = 1..m'],
  ['f(x) = x^2', 'sum f(n), n = 1..5'],
  ['s = sum n, n = 1..4', 's*2'],
]

describe('Σ gives the same answer as sum', () => {
  it.each(SAME_ANSWER)('%s', (...lines) => {
    const pretty = lines.map((l) => typeKeys(l))
    expect(pretty).not.toEqual(lines)
    expect(outcome(pretty)).toEqual(outcome(lines))
  })

  it('in radians, fractions and sig figs too', () => {
    for (const options of [{ angleMode: 'rad' as const }, { fractionMode: true }, { sigFigMode: true }]) {
      for (const lines of SAME_ANSWER) {
        expect(outcome(lines.map((l) => typeKeys(l)), options), lines.join(' ; ')).toEqual(outcome(lines, options))
      }
    }
  })

  it('answers the sums it reads', () => {
    expect(outcome([typeKeys('sum n^2, n = 1..10')]).display).toBe('385')
    expect(outcome([typeKeys('prod(2, 3, 4)')]).display).toBe('24')
    expect(outcome([typeKeys('sum 1/n^2, n = 1..inf')]).exact).toBe('pi^2/6')
    // the ? sheet line
    expect(outcome([typeKeys('sum n, n=1..9')]).display).toBe('45')
  })
})

describe('the mac app', () => {
  it('never lets soulver answer a Σ line', () => {
    const native = { expr: 'Σ n, n = 1..10^20', display: '42', n: 42 }
    expect(mergeLiveAnswer('Σ n, n = 1..10^20', '', undefined, native)).toEqual({ display: '', n: undefined })
    const from = { ...native, expr: 'sum from n = 1 to 3 of n' }
    expect(mergeLiveAnswer('sum from n = 1 to 3 of n', '6', 6, from)).toEqual({ display: '6', n: 6 })
  })
})
