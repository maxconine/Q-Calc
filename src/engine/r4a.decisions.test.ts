// open behaviour questions from homework sessions; each it.fails is the recommended answer, waiting on the owner
import { describe, expect, it } from 'vitest'
import { evaluateSheet } from './evaluate'

function last(lines: string[]) {
  return evaluateSheet(lines).at(-1)!
}

describe('decisions from homework sessions', () => {
  // today `h = 10 m` after `m = 2 kg` is 10 × m = 44 lbs; recommended: blank, since m right after a number reads as metres
  it.fails('a stored quantity named like a unit, right after a number, is blank', () => {
    expect(last(['m = 2 kg', 'h = 10 m']).display).toBe('')
  })

  // today `E = 5` is blank (E is reserved as e) and then `E*2` is 2e = 5.44; recommended: E is a free name
  it.fails('E can hold a value, as in E = hc/λ', () => {
    expect(last(['E = 5', 'E*2']).value?.n).toBe(10)
  })

  // today `2 T` is 2 tonnes; recommended: T is tesla (t stays tonne), so F = qvB works in physics 2
  it.fails('T is tesla', () => {
    expect(last(['2 T * 3 A * 1 m']).display).toBe('6 N')
  })

  // today `q1 = 2e-6 C` is 32.0000036 °F; recommended: a charge-sized C with no temperature context is coulombs
  it.fails('a tiny amount of C in an assignment is coulombs', () => {
    expect(last(['q1 = 2e-6 C']).value?.unitId).toBe('coulomb')
  })

  // today `P(t)=1000` silently replaces P with a constant; recommended: with P already defined, solve P(t) = 1000 for t
  it.fails('P(t)=1000 solves when P is already a function', () => {
    expect(last(['P(t) = 100 e^(0.5t)', 'P(t)=1000']).value?.n).toBeCloseTo(2 * Math.log(10), 9)
  })
})
