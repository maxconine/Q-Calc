// behaviour the owner hasn't decided yet, and known gaps; each it.fails flips once it's settled
import { describe, expect, it } from 'vitest'
import { evaluateSheet } from './evaluate'

const last = (lines: string[], opts = {}) => evaluateSheet(lines, { angleMode: 'rad', ...opts }).at(-1)!.display

describe('open decisions', () => {
  // today the ± quietly drops and a bare 4 shows, as if exact
  it('a ± variable through a step ± does not model keeps its ± or goes blank', () => {
    expect(['', '4.0 ± 0.3']).toContain(last(['x = 2.0 ± 0.1', '2^x']))
  })

  it('a ± variable into a solve keeps its ± or goes blank', () => {
    expect(['', '1.5 ± 0.5']).toContain(last(['x = 3 ± 1', '2y = x']))
  })

  it('a typed ± is never rounded', () => {
    expect(last(['2.0 ± 0.35'])).toBe('2.00 ± 0.35')
    expect(last(['10 ± 0.123'])).toBe('10.000 ± 0.123')
    expect(last(['10 ± 1'])).toBe('10 ± 1')
  })

  // m and s as a function's own parameters, or ms as milliseconds
  it("a function's parameters win over a unit their letters spell", () => {
    expect(last(['f(m,s)=ms', 'f(2,3)'])).toBe('6')
  })
})
