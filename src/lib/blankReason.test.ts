import { describe, expect, it } from 'vitest'
import { evaluateLine } from '../engine/evaluate'
import { blankReason, enterHint } from './blankReason'

const ok = (text: string) => Boolean(evaluateLine(text).display)

function marked(expr: string, names = {}): string | null {
  const span = blankReason(expr, ok, names)
  return span ? expr.slice(span.start, span.end) : null
}

describe('blankReason', () => {
  it('points at an unknown word', () => {
    expect(marked('x^2')).toBe('x')
    expect(marked('12 kg in lbz')).toBe('lbz')
    expect(marked('3 + foo * 2')).toBe('foo')
  })

  it('knows user names, functions and two-word units', () => {
    expect(marked('x^2', { variables: ['x'] })).toBe(null)
    expect(marked('sqrt(2) + y', { variables: ['y'] })).toBe(null)
    expect(marked('5 sq ft + zz')).toBe('zz')
  })

  it('points at a stray operator', () => {
    expect(marked('2+2==')).toBe('=')
    expect(marked('2+*3')).toBe('*')
    expect(marked('*3')).toBe('*')
  })

  it('marks a stray = only when solve declines the line', () => {
    const at = (expr: string) => blankReason(expr, ok)
    expect(marked('2+2=')).toBe(null)
    expect(marked('2x+3=11')).toBe(null)
    expect(marked('x^2 = -1')).toBe(null)
    expect(at('2x+3==11')).toEqual({ start: 5, end: 6 })
    expect(at('=2x+3=11')).toEqual({ start: 0, end: 1 })
    expect(marked('2x+3=11=')).toBe(null)
    expect(marked('=2+3')).toBe('=')
    expect(marked('x + y = 10')).toBe('x')
  })

  it('leaves an unfinished line alone', () => {
    expect(marked('2+')).toBe(null)
    expect(marked('sqrt(2)*')).toBe(null)
    expect(marked('12 15 9')).toBe(null)
  })

  it('says nothing about input that evaluates', () => {
    expect(marked('2+2')).toBe(null)
    expect(marked('12 kg in lb')).toBe(null)
    expect(marked('')).toBe(null)
  })
})

describe('enterHint', () => {
  const hint = (expr: string, extra: Partial<Parameters<typeof enterHint>[0]> = {}) =>
    enterHint({ expr, span: blankReason(expr, ok), ...extra })

  it('an empty bar asks for something to calculate', () => {
    expect(hint('')).toMatch(/type a calculation/)
    expect(hint('   ')).toMatch(/type a calculation/)
    expect(enterHint({ expr: '', span: null, history: true })).toMatch(/↑ brings back/)
  })

  it('names the word it does not know', () => {
    expect(hint('3 + foo * 2')).toBe('“foo” isn’t a unit, function or name I know')
  })

  it('a single letter is a variable with no value', () => {
    expect(hint('x^2')).toBe('x has no value yet · x = 5 sets it')
  })

  it('an equation with two unknowns points at systems', () => {
    expect(hint('x + y = 10')).toBe('2 unknowns: sys 2 solves equations together')
  })

  it('a unit it cannot convert to', () => {
    expect(hint('12 kg in lbz')).toBe('can’t convert that to lbz')
  })

  it('units that measure different things', () => {
    expect(hint('5 m to kg', { improper: true })).toBe('those units measure different things')
  })

  it('unfinished input and stray operators', () => {
    expect(hint('2 +')).toBe('unfinished: something goes after the +')
    expect(hint('2+*3')).toBe('check the extra *')
    expect(hint('2+2==')).toMatch(/=/)
  })

  it('a closing bracket with nothing to close', () => {
    expect(hint('2 + 3)')).toBe('a ) has no ( to match')
  })

  it('a bare graph and plain words', () => {
    expect(enterHint({ expr: 'graph', span: null, bareGraph: true })).toMatch(/graph x\^2/)
    expect(enterHint({ expr: 'what is the weather', span: null, naturalLanguage: true })).toMatch(/as math/)
  })

  it('never hands over the answer, and always says something', () => {
    for (const expr of ['3 + foo', 'x^2', '2 +', '2+*3', '12 kg in lbz', '%%%']) {
      const text = hint(expr)
      expect(text.length, expr).toBeGreaterThan(0)
      expect(text.length, expr).toBeLessThanOrEqual(60)
    }
  })
})
