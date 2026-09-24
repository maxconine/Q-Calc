// adversarial coverage for ghost-word completion, on top of completion.test.ts
import { describe, expect, it } from 'vitest'
import { completionFor } from './completion'

describe('completionFor: function completion', () => {
  it('a short prefix shared by several functions picks the ranked first', () => {
    // COMPLETION_FUNCTIONS lists arcsin before arccos and arctan
    expect(completionFor('ar')).toBe('csin(')
  })
  it('finishes a longer prefix uniquely', () => {
    expect(completionFor('permu')).toBe('tations(')
    expect(completionFor('combin')).toBe('ations(')
    expect(completionFor('hyp')).toBe('ot(')
  })
  it('a whole function name is quiet even though a longer one shares its prefix', () => {
    // sin and cos are themselves known names, so sinh/cosh are never offered from them
    expect(completionFor('sin')).toBe('')
    expect(completionFor('cos')).toBe('')
  })
})

describe('completionFor: unit completion', () => {
  it('finishes a distinctive unit prefix', () => {
    expect(completionFor('5 kilob')).toBe('yte')
    expect(completionFor('2 mega')).toBe('byte')
    expect(completionFor('3 hors')).toBe('epower')
  })
  it('after "to" a unit is preferred over a function of the same prefix', () => {
    expect(completionFor('5 kg to ce')).toBe('ntimeter')
  })
})

describe('completionFor: user names take priority in both positions', () => {
  it('a user variable beats a built-in unit prefix', () => {
    expect(completionFor('5 met', { variables: ['method'] })).toBe('hod')
  })
  it('a user function beats a built-in function prefix', () => {
    expect(completionFor('sq', { functions: ['sqft'] })).toBe('ft(')
  })
})

describe('completionFor: quiet cases', () => {
  it.each(['pi', 'e', 'in', 'to', 'as', 'of', 'and', 'or', 'mod', 'kg', 'cm', '', 'a', '5 + '])(
    '%s gives nothing',
    (t) => {
      expect(completionFor(t)).toBe('')
    },
  )
  it('a whole word that is also a prefix of a longer one is still quiet if it stands alone', () => {
    expect(completionFor('meter')).toBe('')
  })
  it('an unknown gibberish prefix is quiet', () => {
    expect(completionFor('xqzz')).toBe('')
  })
  it('a prefix word already claimed as a whole word by the user does not re-offer itself', () => {
    expect(completionFor('mass', { variables: ['mass'] })).toBe('')
  })
})
