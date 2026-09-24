// adversarial coverage for prettyAnswer (minus sign + digit grouping display), on top of
// prettyAnswer.test.ts
import { describe, expect, it } from 'vitest'
import { prettyAnswer } from './format'

const M = '−'

describe('prettyAnswer: grouping an ungrouped currency amount', () => {
  it('adds thousands separators to a plain currency answer', () => {
    expect(prettyAnswer('-$1234567.89')).toBe(`${M}$1,234,567.89`)
    expect(prettyAnswer('£1234567')).toBe('£1,234,567')
  })
})

describe('prettyAnswer: combined sign and grouping across the decimal and exponent', () => {
  it('groups the integer part and prettifies both minus signs at once', () => {
    expect(prettyAnswer('-12345.6e-10')).toBe(`${M}12,345.6e${M}10`)
  })
})

describe('prettyAnswer: edge values', () => {
  it('negative zero still gets the real minus sign', () => {
    expect(prettyAnswer('-0')).toBe(`${M}0`)
  })
  it('a leading minus before pi (not just a mid-expression one) converts', () => {
    expect(prettyAnswer('-π + 1')).toBe(`${M}π + 1`)
  })
})

describe('prettyAnswer: a leading minus before e or i is missed by the math-only fallback', () => {
  it('sqrt gets the pretty minus for a leading minus', () => {
    expect(prettyAnswer('-sqrt(4)')).toBe(`${M}sqrt(4)`)
  })

  // BUG: the MATH_ONLY replace's lookahead `[\s\d(π√s]` whitelists the start of
  // "sqrt" (the literal 's') and pi and parens/digits/space, but not 'e' or 'i'. So a leading
  // minus directly before 'e' or 'i' is left as a plain ascii hyphen, unlike every other
  // math-only case (pi, sqrt, a digit, a paren). input: '-e^2', expected: '−e^2', actual: '-e^2'.
  it('bare e does not get the pretty minus, unlike sqrt or pi', () => {
    expect(prettyAnswer('-e^2')).toBe(`${M}e^2`)
  })

  // same root cause as above. input: '-i', expected: '−i', actual: '-i'.
  it('bare i does not get the pretty minus either', () => {
    expect(prettyAnswer('-i')).toBe(`${M}i`)
  })
})
