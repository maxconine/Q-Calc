import { describe, expect, it } from 'vitest'
import { evaluateLine } from '../engine/evaluate'
import { completionFor } from './completion'

describe('completion is not fooled by "as" and "per", which are not conversion words here', () => {
  it('prefers a function after as/per, since the engine does not convert units with them', () => {
    expect(evaluateLine('5 kg as lb').value?.unit).not.toBe('lb')
    expect(evaluateLine('5 kg per lb').display).toBe('')
    expect(completionFor('5 kg as ce')).toBe('il(')
    expect(completionFor('5 kg per ce')).toBe('il(')
  })

  it('still prefers the unit after the words that really do convert', () => {
    expect(evaluateLine('5 kg to lb').value?.unit).toBe('lbs')
    expect(evaluateLine('5 kg in lb').value?.unit).toBe('lbs')
    expect(completionFor('5 kg to ce')).toBe('ntimeter')
    expect(completionFor('5 kg in ce')).toBe('ntimeter')
  })
})

describe('completion around punctuation and unicode already in the field', () => {
  it('a converted symbol right before the word still counts as a word boundary', () => {
    expect(completionFor('π+sq')).toBe('rt')
    expect(completionFor('θ*co')).toBe('s(')
  })

  it('a short or already-whole word offers nothing, even right after a symbol', () => {
    expect(completionFor('π+x')).toBe('')
    expect(completionFor('√sin')).toBe('')
  })
})

describe('user-provided names still rank first over built-ins that would otherwise match', () => {
  it('a user function shadows a built-in with the same prefix', () => {
    expect(completionFor('si', { functions: ['sigma'] })).toBe('gma(')
  })

  it('a user variable that is also a real unit name completes to no-thing extra, since it is already whole', () => {
    expect(completionFor('meter', { variables: ['meter'] })).toBe('')
  })
})
