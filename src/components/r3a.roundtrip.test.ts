import { describe, expect, it } from 'vitest'
import { evaluateSheet } from '../engine/evaluate'
import { afterTyping, boundKey, wordToSign } from '../lib/bounds'
import { prettyTokens } from './QuickInput'

/** Types `keys` the way QuickInput sends them to the input: plain strings go through
 * prettyTokens + afterTyping character by character, `{ key }` objects go through boundKey
 * (falling back to wordToSign for Tab/→, and to plain insertion for Backspace/space). */
function typeKeys(keys: (string | { key: string })[]): { text: string; caret: number } {
  let text = ''
  let caret = 0
  for (const k of keys) {
    if (typeof k !== 'string') {
      const edit = boundKey(text, caret, k.key) ?? (k.key === 'Tab' || k.key === 'ArrowRight' ? wordToSign(text, caret) : null)
      if (edit) ({ text, caret } = edit)
      else if (k.key === 'Backspace' && caret > 0) {
        text = text.slice(0, caret - 1) + text.slice(caret)
        caret--
      } else if (k.key === ' ') {
        text = text.slice(0, caret) + ' ' + text.slice(caret)
        caret++
      }
      continue
    }
    for (const ch of k) {
      const raw = text.slice(0, caret) + ch + text.slice(caret)
      const at = caret + 1
      const before = prettyTokens(raw.slice(0, at), at)
      text = prettyTokens(raw, at)
      caret = Math.min(before.length, text.length)
      const slot = afterTyping(text, caret)
      if (slot) ({ text, caret } = slot)
    }
  }
  return { text, caret }
}

const answer = (line: string) => evaluateSheet([line], { angleMode: 'rad' })[0]

describe('bounds typed alongside word symbols evaluate the same as the fully plain form', () => {
  it('an integral with a pi upper limit typed through int, then →, then pi', () => {
    const tab = { key: 'Tab' }
    const r = typeKeys(['int0', tab, 'pi/2 ', 'cos(x)'])
    expect(r.text).toBe('∫_0^π/2 cos(x)')
    expect(answer(r.text)?.display).toBe('1')
    expect(answer(r.text)?.display).toBe(answer('∫0..π/2 cos(x)')?.display)
  })

  it('a sum whose body uses sqrt and theta-like variable names, typed end to end', () => {
    const right = { key: 'ArrowRight' }
    const r = typeKeys(['sum1', right, '4 ', 'sqrt(n)'])
    expect(r.text).toBe('Σ_1^4 √(n)')
    expect(answer(r.text)?.display).toBe(answer('sum sqrt(n), n=1..4')?.display)
    expect(answer(r.text)?.display).not.toBe('')
  })

  it('a bound typed with keepWords stays plain text', () => {
    let value = ''
    for (const ch of 'int0..1 x^2') {
      value = prettyTokens(value + ch, value.length + 1, true)
    }
    expect(value).toBe('int0..1 x^2')
  })

  // decision needed: calculus.ts's Desmos-style shorthand (`0..1`, `_0^1`) only recognizes the
  // ∫ symbol, not the word `int` (see calculus.ts's `if (!text.startsWith('∫')) return null`);
  // a keepWords user who types the shorthand by hand gets a blank answer, and only the
  // parenthesized `int(x^2, 0, 1)` form works for them. Fixing this means calculus.ts should also
  // read the word form, which is calculus's file, not this agent's to decide or touch.
  it('a keepWords bound typed with the Desmos shorthand answers like the symbol', () => {
    const value = 'int0..1 x^2'
    expect(answer(value)?.display).toBe(answer('∫0..1 x^2')?.display)
  })
})

describe('a full ± measurement typed through the shortcuts evaluates like the word form', () => {
  it.each(['10+/-1', '10+.-1', '10~1'])('%s', (typed) => {
    let value = ''
    for (const ch of typed) value = prettyTokens(value + ch, value.length + 1)
    const plain = value
    let wordForm = ''
    for (const ch of '10 plus minus 1') wordForm = prettyTokens(wordForm + ch, wordForm.length + 1)
    expect(answer(plain)?.display).toBe(answer(wordForm)?.display)
    expect(answer(plain)?.display).not.toBe('')
  })
})
