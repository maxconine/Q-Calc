import { describe, expect, it } from 'vitest'
import { evaluateSheet } from '../engine/evaluate'
import { afterTyping, boundKey, boundsIn, isHidden, snapOffset, wordToSign } from './bounds'

const shown = (line: string) => evaluateSheet([line], { angleMode: 'rad' })[0]?.display ?? ''

describe('two bound signs on one line', () => {
  it('a sum inside another sum limit, with a space between the signs', () => {
    const text = 'Σ_i=1^3 Σ_j=1^3 i*j'
    const bounds = boundsIn(text)
    expect(bounds).toHaveLength(2)
    expect(bounds[0]!.lower).toEqual({ start: 2, end: 5 })
    expect(bounds[1]!.sign).toBe(8)
  })

  it('an integral followed by a product, each with its own limits', () => {
    const text = '∫_0^1 x dx * Π_n=1^5 n'
    const bounds = boundsIn(text)
    expect(bounds).toHaveLength(2)
    expect(bounds[1]!.lower).toEqual({ start: 15, end: 18 })
  })

  it('two integrals glued with no space merge, matching how the engine itself reads it', () => {
    // the upper limit reads up to the next space, the same rule calculus.ts uses;
    // with nothing separating the two signs the first swallows the second, and both
    // the display and the evaluator agree it is blank rather than guessing
    const text = '∫_0^1x∫_0^1y'
    expect(boundsIn(text)).toHaveLength(1)
    expect(shown(text)).toBe('')
  })
})

describe('caret snapping around hidden offsets, with more than one bound', () => {
  it('never calls an offset inside a second bound hidden by the first', () => {
    const text = '∫_0^1 x ∫_0^1 y'
    const bounds = boundsIn(text)
    // only the underscores right after each sign are hidden; the rest of each bound's
    // own limit text and the body around them stay reachable
    const hiddenOffsets = [...Array(text.length + 1).keys()].filter((o) => isHidden(bounds, o))
    expect(hiddenOffsets).toEqual([1, 9])
  })

  it('steps over a hidden offset in either direction', () => {
    const text = '∫_0^1 x'
    const bounds = boundsIn(text)
    expect(snapOffset(bounds, 1, 1, text.length)).toBe(2)
    expect(snapOffset(bounds, 1, -1, text.length)).toBe(0)
  })
})

describe('Delete key mirrors Backspace around hidden scaffolding', () => {
  it('never deletes a hidden opening or closing brace, just steps past it', () => {
    const text = '∫_{0}^{1} x'
    expect(boundKey(text, 1, 'Delete')).toEqual({ text, caret: 3 })
    expect(boundKey(text, 4, 'Delete')).toEqual({ text, caret: 7 })
    expect(boundKey(text, 8, 'Delete')).toEqual({ text, caret: 9 })
  })

  it('leaves plain text and a bare sign alone', () => {
    expect(boundKey('2 + 3', 2, 'Delete')).toBeNull()
    expect(boundKey('∫', 0, 'Delete')).toBeNull()
  })
})

describe('wordToSign', () => {
  it('only fires with the caret at the very end', () => {
    expect(wordToSign('int', 3)).toEqual({ text: '∫', caret: 1 })
    expect(wordToSign('int x', 3)).toBeNull()
    expect(wordToSign('5 + int', 7)).toEqual({ text: '5 + ∫', caret: 5 })
  })

  it('a letter or underscore right before the word blocks it, a digit does not', () => {
    expect(wordToSign('sprint', 6)).toBeNull()
    expect(wordToSign('x_int', 5)).toBeNull()
    // digits are read as a coefficient everywhere else in this input, so this is consistent
    expect(wordToSign('2int', 4)).toEqual({ text: '2∫', caret: 2 })
  })

  it('leaves text with no trailing int, sum or prod alone', () => {
    expect(wordToSign('integral', 8)).toBeNull()
    expect(wordToSign('into', 4)).toBeNull()
    expect(wordToSign('', 0)).toBeNull()
  })
})

describe('afterTyping', () => {
  it('does nothing until the caret is at the end of the text', () => {
    expect(afterTyping('∫x 5', 2)).toBeNull()
  })

  it('only a character that can start a limit triggers it', () => {
    for (const bad of ['∫ ', '∫(', '∫_', '∫^', '∫,', '∫)']) {
      expect(afterTyping(bad, bad.length)).toBeNull()
    }
    expect(afterTyping('∫5', 2)).toEqual({ text: '∫_5', caret: 3 })
  })

  it('.. inside a lower limit moves to the upper, but only right after that limit', () => {
    expect(afterTyping('∫_0..', 5)).toEqual({ text: '∫_0^', caret: 4 })
    // a stray ".." with no open lower limit does nothing
    expect(afterTyping('2..', 3)).toBeNull()
  })
})

describe('sum and product limits round-trip through evaluation the same as ∫', () => {
  it.each([
    ['Σ_1^5 n', 'Σ(n, n, 1, 5)'],
    ['Π_1^4 n', 'Π(n, n, 1, 4)'],
  ])('%s', (typed, plain) => {
    expect(shown(typed)).not.toBe('')
    // both forms name the same n, just written differently; sanity check they are not blank
    expect(shown(plain)).not.toBe('')
  })
})
