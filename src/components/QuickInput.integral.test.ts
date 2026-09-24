import { describe, expect, it } from 'vitest'
import { evaluateSheet } from '../engine/evaluate'
import { prettyTokens } from './QuickInput'

function typeKeys(text: string, keepWords = false): string {
  let value = ''
  for (const ch of text) {
    const raw = value + ch
    value = prettyTokens(raw, raw.length, keepWords)
  }
  return prettyTokens(value, undefined, keepWords)
}

const shown = (line: string) => evaluateSheet([line], { angleMode: 'rad' })[0]?.display

describe('int becomes ∫', () => {
  it('turns into the symbol as you type', () => {
    expect(typeKeys('int0..1 x^2')).toBe('∫0..1 x^2')
    expect(typeKeys('int(x^2, 0, 1)')).toBe('∫(x^2, 0, 1)')
    expect(typeKeys('int x^2 from 0 to 1')).toBe('∫ x^2 from 0 to 1')
  })

  it('gives the same answer as the word', () => {
    for (const t of ['int(x^2, 0, 1)', 'int(t^2, t, 0, 1)', 'int0..pi sin(x)', 'int_0^1 x^2 dx']) {
      expect(shown(typeKeys(t))).toBe(shown(t.replace(/int/, '∫')))
      expect(shown(typeKeys(t))).not.toBe('')
    }
    expect(shown(typeKeys('int x^2 from 0 to 1'))).toBe('0.333333333333')
  })

  it('leaves longer words alone', () => {
    for (const t of ['integral of x^2 from 0 to 1', 'interest', 'print', 'into']) {
      expect(typeKeys(t)).toBe(t)
    }
  })

  it('stays a word when words are kept as text', () => {
    expect(typeKeys('int(x^2, 0, 1)', true)).toBe('int(x^2, 0, 1)')
  })
})
