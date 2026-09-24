import { describe, expect, it } from 'vitest'
import { evaluateLine } from '../engine/evaluate'
import { COMPLETION_FUNCTIONS, COMPLETION_UNITS, completionFor } from './completion'

describe('completionFor', () => {
  it('finishes function names with an open paren', () => {
    expect(completionFor('sq')).toBe('rt(')
    expect(completionFor('2 + co')).toBe('s(')
    expect(completionFor('2sq')).toBe('rt(')
    expect(completionFor('fac')).toBe('torial(')
  })

  it('finishes unit names', () => {
    expect(completionFor('5 kilo')).toBe('gram')
    expect(completionFor('12 kg in po')).toBe('und')
    expect(completionFor('3 tab')).toBe('lespoon')
    expect(completionFor('20 cel')).toBe('sius')
  })

  it('prefers a unit after a number and a function elsewhere', () => {
    expect(completionFor('ce')).toBe('il(')
    expect(completionFor('5 ce')).toBe('ntimeter')
    expect(completionFor('5 si')).toBe('n(')
  })

  it('offers user names first', () => {
    expect(completionFor('ma', { variables: ['mass'] })).toBe('ss')
    expect(completionFor('2 * are', { functions: ['area'] })).toBe('a(')
  })

  it('stays quiet on whole words and short or unknown ones', () => {
    for (const t of ['s', 'sin', 'pi', '5 mi', '5 kg', 'x', '5 kg in', '5 kg as', 'zzq', '2 + ', '']) {
      expect(completionFor(t)).toBe('')
    }
  })

  it('only offers names the engine reads', () => {
    for (const f of COMPLETION_FUNCTIONS) expect(evaluateLine(`${f}(4, 2)`).display || evaluateLine(`${f}(4)`).display).not.toBe('')
    for (const u of COMPLETION_UNITS) expect(evaluateLine(`2 ${u}`).value?.unit, u).toBeTruthy()
  })
})
