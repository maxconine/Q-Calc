import { describe, expect, it } from 'vitest'
import { afterTyping } from '../lib/bounds'
import { flattenPastedText, prettyTokens, spliceText } from './QuickInput'

/** Types `text` one keystroke at a time with the caret always at the end, mirroring the real input. */
function typeKeys(text: string, keepWords = false): string {
  let value = ''
  for (const ch of text) {
    const raw = value + ch
    value = prettyTokens(raw, raw.length, keepWords)
  }
  return prettyTokens(value, undefined, keepWords)
}

/** Types `text` one keystroke at a time through the same two-step pipeline QuickInput's commit() uses
 * (prettyTokens, then afterTyping for a real keystroke), always appending at the end. */
function typeKeysWithBoundStep(text: string): { text: string; caret: number } {
  let value = ''
  let caret = 0
  for (const ch of text) {
    const raw = value.slice(0, caret) + ch + value.slice(caret)
    const at = caret + 1
    const before = prettyTokens(raw.slice(0, at), at)
    value = prettyTokens(raw, at)
    caret = Math.min(before.length, value.length)
    const slot = afterTyping(value, caret)
    if (slot) ({ text: value, caret } = slot)
  }
  return { text: value, caret }
}

describe('typing a longer word through the whole pipeline stays a word', () => {
  // int, sum and prod are bound signs; a longer word built on top of them must not get
  // mistaken for the start of a ∫/Σ/Π limit as extra letters land at the caret
  it.each(['integer', 'interest', 'integral', 'international', 'interval', 'summary', 'summation', 'product', 'producer', 'produce'])(
    '%s',
    (word) => {
      expect(typeKeysWithBoundStep(word)).toEqual({ text: word, caret: word.length })
    },
  )
})

describe('caret in the middle while typing', () => {
  it('a letter glued right after theta makes it a longer word, same as at the end', () => {
    expect(prettyTokens('thetax', 6)).toBe('thetax')
    expect(prettyTokens('xtheta', 6)).toBe('xtheta')
  })

  it('a token before the caret is already complete and converts, even mid-string', () => {
    // 'theta' then '+1' typed after it: the caret sits after the '1', so theta is no
    // longer trailing text and converts, same as it does once settled without a caret
    expect(prettyTokens('theta+1', 7)).toBe('θ+1')
  })

  it('typing inside an already-converted symbol does not resurrect the word', () => {
    // '√(4)' with the caret after '(' and a digit typed there
    const value = '√(4)'
    const caret = 2
    const raw = value.slice(0, caret) + '1' + value.slice(caret)
    expect(prettyTokens(raw, caret + 1)).toBe('√(14)')
  })

  it('a caret before a settled word does not stop it from converting (it is already complete)', () => {
    expect(prettyTokens('sqrt', 0)).toBe('√')
    expect(prettyTokens('pi', 0)).toBe('π')
  })

  it('inserting a letter before a trailing sqrt keeps the tail waiting on its own caret', () => {
    // 'x' + caret + 'sqrt': the sqrt half is still trailing text after the caret, so with a
    // caret defined it is treated as "after" text and converts immediately (sqrt is at-once)
    expect(prettyTokens('xsqrt', 1)).toBe('x√')
  })
})

describe('backspacing through a converted symbol', () => {
  it('sqrt collapses one character at a time without reviving the word', () => {
    let value = prettyTokens('sqrt(2)')
    const seen: string[] = []
    while (value) {
      value = prettyTokens(value.slice(0, -1), value.length - 1)
      seen.push(value)
    }
    expect(seen).toEqual(['√(2', '√(', '√', ''])
  })

  it('backspacing sum down to nothing never leaves a stray Σ or the bare word', () => {
    let value = prettyTokens('sum(1,2)')
    const seen: string[] = []
    while (value) {
      value = prettyTokens(value.slice(0, -1), value.length - 1)
      seen.push(value)
    }
    expect(seen.at(-1)).toBe('')
    expect(seen).not.toContain('sum')
  })

  it('backspacing off the last letter of a longer word converts what remains', () => {
    // 'into' backspaced to 'int' becomes the sign, matching a fresh 'int' typed alone
    expect(prettyTokens('into'.slice(0, 3), 3)).toBe('∫')
  })
})

describe('pasting a whole string at once', () => {
  it('converts every symbol in a multi-word paste in one pass', () => {
    const pasted = flattenPastedText('sqrt(2) + pi - theta*dot')
    expect(prettyTokens(pasted, pasted.length)).toBe('√(2) + π - θ*dot')
  })

  it('flattens embedded newlines before converting', () => {
    const pasted = flattenPastedText('sum n^2,\nn = 1..10')
    expect(prettyTokens(pasted, pasted.length)).toBe('Σ n^2, n = 1..10')
  })

  it('a pasted ± shortcut converts the same as typing it', () => {
    const pasted = flattenPastedText('10 +/- 0.7')
    expect(prettyTokens(pasted, pasted.length)).toBe('10 ± 0.7')
  })

  it('a pasted ans keeps reading the last answer', () => {
    const pasted = flattenPastedText('ans*2\nans^2')
    expect(prettyTokens(pasted, pasted.length)).toBe('ans*2 ans^2')
  })
})

describe('± shortcuts beyond the basics', () => {
  it.each([
    ['5+/-2', '5±2'],
    ['5+.-2', '5±2'],
    ['5-/+2', '5∓2'],
    ['5~2', '5±2'],
    ['5 plus minus 2', '5 ± 2'],
    ['5 minus plus 2', '5 ∓ 2'],
    ['5 plusminus 2', '5 ± 2'],
  ])('%s -> %s', (typed, shown) => {
    expect(typeKeys(typed)).toBe(shown)
  })

  it('+- alone never becomes ± no matter how it is typed', () => {
    expect(typeKeys('5+-3')).toBe('5+-3')
    expect(typeKeys('5 +- 3')).toBe('5 +- 3')
    expect(prettyTokens('5+-3', 4)).toBe('5+-3')
  })

  it('keepWords still converts the shortcut forms, since only the word form is a "word"', () => {
    expect(typeKeys('5+/-2', true)).toBe('5±2')
    expect(typeKeys('5~2', true)).toBe('5±2')
    expect(typeKeys('5 plus minus 2', true)).toBe('5 plus minus 2')
    expect(typeKeys('5 minus plus 2', true)).toBe('5 minus plus 2')
  })
})

describe('spliceText clamps out-of-range offsets', () => {
  it('never throws and always clamps into the string', () => {
    expect(spliceText('abc', 'X', -5, 100)).toEqual({ next: 'X', cursor: 1 })
    expect(spliceText('abc', 'X', 5, 1)).toEqual({ next: 'abcX', cursor: 4 })
  })
})
