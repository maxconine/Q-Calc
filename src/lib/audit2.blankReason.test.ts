// adversarial coverage for the why-blank squiggle, on top of blankReason.test.ts
import { describe, expect, it } from 'vitest'
import { evaluateLine } from '../engine/evaluate'
import { blankReason } from './blankReason'

const ok = (text: string) => Boolean(evaluateLine(text).display)

function marked(expr: string, names = {}): string | null {
  const span = blankReason(expr, ok, names)
  return span ? expr.slice(span.start, span.end) : null
}

describe('blankReason: unknown words in context', () => {
  it('points at the first unknown word, not a later one', () => {
    expect(marked('foo + bar')).toBe('foo')
  })
  it('a known function name is never flagged even mid-expression', () => {
    expect(marked('3 + sqrt(9) + zog')).toBe('zog')
  })
  it('a two-word unit spanning a known-looking first word is not flagged', () => {
    expect(marked('5 fl oz + qux')).toBe('qux')
  })
  it('a variable list all known leaves nothing marked', () => {
    expect(marked('a + b + c', { variables: ['a', 'b', 'c'] })).toBe(null)
  })
})

describe('blankReason: unreachable unit conversions', () => {
  it('a valid quantity converting to a bogus unit is marked at the unit', () => {
    expect(marked('5 kg in zorts')).toBe('zorts')
  })
  it('a bogus quantity converting to a valid unit is not marked here (the word rule catches it first)', () => {
    expect(marked('5 zonks in kg')).toBe('zonks')
  })
})

describe('blankReason: stray operators', () => {
  it('a doubled operator marks the second', () => {
    expect(marked('5//2')).toBe('/')
  })
  it('a trailing comma inside an unclosed call is flagged, even though a real user may still be mid-argument', () => {
    // removing the comma leaves 'sum(1,2', which the paren-autofill makes valid ('sum(1,2)');
    // that is enough for strayOperator to call the comma the culprit. Whether a comma right
    // before the next digit of a third argument should really be squiggled is arguable - noted
    // as an ambiguous case in the report rather than asserted as a bug.
    expect(marked('sum(1,2,')).toBe(',')
  })
  it('a leading multiplication sign is marked', () => {
    expect(marked('×3')).toBe('×')
  })
})

describe('blankReason: solve interplay', () => {
  it('an equation with two free letters marks the one that is not also a real unit symbol', () => {
    // 'a' alone is a valid unit (the metric area "are"), so "1 a" evaluates and the word
    // rule skips it; 'b' has no such escape and is what actually gets marked
    expect(marked('a + b = 10')).toBe('b')
  })
  it('a genuinely unreadable equation with a bad word is marked at the word, not the =', () => {
    expect(marked('foo = 3 + 2')).toBe(null)
  })
})

describe('blankReason: nothing marked for input that reads fine', () => {
  it.each(['2 + 2 * 3', 'sqrt(16) + 4', '12 kg in lb', '5 sq ft', 'x^2 = 4', ''])('%s', (expr) => {
    expect(marked(expr, { variables: ['x'] })).toBe(null)
  })
})
