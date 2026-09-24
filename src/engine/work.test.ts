import { afterEach, describe, expect, it, vi } from 'vitest'
import { evaluateSheet } from './evaluate'

const SHEETS = [
  ['5/sqrt(41)'],
  ['0.1 + 0.2'],
  ['x = 3', 'sqrt(2)x'],
  ['theta = 30', 'sin(theta)'],
  ['sum(1/k^2, k, 1, 100)'],
  ['sum(1/k^2, k, 1, 200000)'],
  ['sum(0.5^n, n, 0, inf)'],
  ['prod(1+1/k, k, 1, 5000)'],
  ['x^3 - 2x = 5'],
  ['sin(x) = x/3'],
  ['integral of sin(x)/x from 1 to 50'],
  ['f(t) = t^2 + sin(t)', 'integral of f(x) from 0 to 3'],
  [Array.from({ length: 50 }, (_, i) => `sin(${i}.3)`).join('+')],
]

const answers = () => SHEETS.map((lines) => evaluateSheet(lines).map((r) => [r.display, r.exact, r.value]))

afterEach(() => vi.restoreAllMocks())

describe('answers never depend on how busy the machine is', () => {
  it('gives the same answers when every clock read jumps a second', () => {
    const calm = answers()
    let t = 0
    vi.spyOn(performance, 'now').mockImplementation(() => (t += 1000))
    vi.spyOn(Date, 'now').mockImplementation(() => (t += 1000))
    expect(answers()).toEqual(calm)
  })
})
