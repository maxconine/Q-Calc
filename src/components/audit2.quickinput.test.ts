// adversarial coverage for word->symbol conversion (prettyTokens) and the keepWords setting,
// on top of QuickInput.test.ts, QuickInput.symbols.test.ts and QuickInput.sums.test.ts
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

describe('prettyTokens: theta word boundaries', () => {
  it('a digit right after theta blocks the conversion (it could be a variable name)', () => {
    expect(typeKeys('theta2')).toBe('theta2')
  })
  it('a letter right after theta blocks it too', () => {
    expect(typeKeys('thetas')).toBe('thetas')
  })
  it('a backslash before theta keeps it as LaTeX source', () => {
    expect(prettyTokens('\\theta')).toBe('\\theta')
  })
  it('converts cleanly inside a larger expression', () => {
    expect(typeKeys('sqrt(pi)+theta')).toBe('√(π)+θ')
  })
})

describe('prettyTokens: caret holds the trailing word for every pattern, not just sqrt', () => {
  it('theta waits at the caret', () => {
    expect(prettyTokens('theta', 5)).toBe('theta')
    expect(prettyTokens('theta ', 6)).toBe('θ ')
  })
  it('cbrt converts at the caret, since no longer word starts with it', () => {
    expect(prettyTokens('cbrt', 4)).toBe('∛')
    expect(prettyTokens('cbrt(', 5)).toBe('∛(')
  })
  it('prod converts at the caret', () => {
    expect(prettyTokens('prod', 4)).toBe('Π')
    expect(prettyTokens('prod(', 5)).toBe('Π(')
  })
})

describe('prettyTokens: case-insensitive word matching', () => {
  it('uppercase PI still becomes π', () => {
    expect(typeKeys('2*PI')).toBe('2*π')
  })
  it('mixed-case Infty still becomes infinity', () => {
    expect(typeKeys('Infty')).toBe('∞')
  })
})

describe('prettyTokens: two known words fused with no separator do not partially convert', () => {
  it('sum immediately followed by theta converts both', () => {
    expect(typeKeys('sumtheta')).toBe('Σθ')
  })
})

describe('prettyTokens: dot shortcut multiplication between digits', () => {
  it('a bare "dot" between two numbers becomes *', () => {
    expect(typeKeys('4dot5')).toBe('4*5')
  })
  it('gives the same answer as writing * directly', () => {
    expect(evaluateSheet([typeKeys('4dot5')]).at(-1)!.display).toBe(evaluateSheet(['4*5']).at(-1)!.display)
  })
})

describe('prettyTokens: several ± shortcuts in one line, order-independent', () => {
  it('mixes -/+, +/- and ~ correctly in sequence', () => {
    expect(typeKeys('5+/-2-/+3~1', true)).toBe('5±2∓3±1')
  })
})

describe('keepWords: sum/theta stay as text but shortcuts still convert', () => {
  it('a whole expression with theta and sum is untouched apart from +/-', () => {
    expect(typeKeys('theta + sum(1,2) +/- 1', true)).toBe('theta + sum(1,2) ± 1')
  })
})
