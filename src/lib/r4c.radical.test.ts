import { describe, expect, it } from 'vitest'
import { evaluateSheet } from '../engine'
import { prettyAnswer } from '../engine/format'
import { exampleAnswer } from './onboarding'
import { answerParts, radicalAnswer } from './radical'

describe('solved roots read with a radical sign', () => {
  it('a ± or ∓ in front of the root no longer keeps sqrt( as text', () => {
    expect(radicalAnswer('±sqrt(2)')).toBe('±√2')
    expect(radicalAnswer('∓sqrt(3)')).toBe('∓√3')
    expect(radicalAnswer('±sqrt(6)/2')).toBe('±√6/2')
    expect(radicalAnswer('(3 ± sqrt(5))/2')).toBe('(3 ± √5)/2')
  })

  it('still leaves anything that is not plain math alone', () => {
    expect(radicalAnswer('±sqrt(x)')).toBe('±sqrt(x)')
  })
})

describe('answerParts', () => {
  it('splits a label and the ≈ off so each side can read its roots', () => {
    expect(answerParts('x = ±sqrt(2) ≈ ±1.41421356237')).toEqual(['x = ', '±sqrt(2)', ' ≈ ', '±1.41421356237'])
    expect(answerParts('2sqrt(2) ≈ 2.82842712475')).toEqual(['2sqrt(2)', ' ≈ ', '2.82842712475'])
    expect(answerParts('177.8 cm')).toEqual(['177.8 cm'])
    expect(answerParts('')).toEqual([])
  })

  it('the rotating examples read like the answers they stand for', () => {
    const read = (expr: string) => answerParts(prettyAnswer(exampleAnswer(evaluateSheet([expr])[0]))).map((p) => radicalAnswer(p)).join('')
    expect(read('√(8)')).toBe('2√2 ≈ 2.82842712475')
    expect(read('x^2 = 2')).toBe('x = ±√2 ≈ ±1.41421356237')
  })
})
