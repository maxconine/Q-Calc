import { describe, expect, it } from 'vitest'
import { evaluateLine } from '../engine/evaluate'
import { blankReason } from './blankReason'

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
