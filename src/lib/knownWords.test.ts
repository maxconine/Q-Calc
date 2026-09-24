import { describe, expect, it } from 'vitest'
import { knownWordSpans } from './knownWords'

const lit = (text: string, fns?: string[], ans = true) => knownWordSpans(text, fns, ans).map((s) => text.slice(s.start, s.end))

describe('words that light up', () => {
  it('lights whole function names and ans', () => {
    expect(lit('2sin(30) + log 100')).toEqual(['sin', 'log'])
    expect(lit('ans*2')).toEqual(['ans'])
    expect(lit('ans*2', [], false)).toEqual([])
    expect(lit('sqrt(2) + ln(e) + atan2(1,1) + log10(5)')).toEqual(['sqrt', 'ln', 'atan2', 'log10'])
    expect(lit('sinh(1)')).toEqual(['sinh'])
  })

  it('lets go once the word grows into something else', () => {
    expect(lit('cost')).toEqual([])
    expect(lit('sine')).toEqual([])
    expect(lit('answer')).toEqual([])
    expect(lit('x + y')).toEqual([])
  })

  it('lights the function in a glued name', () => {
    expect(lit('logx=2')).toEqual(['log'])
    expect(lit('sinθ')).toEqual(['sin'])
    expect(lit('sintheta')).toEqual(['sin'])
  })

  it('knows your own functions', () => {
    expect(lit('area(3)', ['area'])).toEqual(['area'])
    expect(lit('area(3)')).toEqual([])
  })
})
