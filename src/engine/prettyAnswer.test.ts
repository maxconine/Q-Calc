import { describe, expect, it } from 'vitest'
import { insertableAnswer } from '../lib/answer'
import { prettyAnswer } from './format'

const M = '−'

describe('prettyAnswer', () => {
  it('uses a real minus sign', () => {
    expect(prettyAnswer('-3')).toBe(`${M}3`)
    expect(prettyAnswer('-0.5')).toBe(`${M}0.5`)
    expect(prettyAnswer('-∞')).toBe(`${M}∞`)
    expect(prettyAnswer('-1e+30')).toBe(`${M}1e+30`)
    expect(prettyAnswer('1e-9')).toBe(`1e${M}9`)
  })

  it('groups the integer part only', () => {
    expect(prettyAnswer('3000000000')).toBe('3,000,000,000')
    expect(prettyAnswer('1234567.891234')).toBe('1,234,567.891234')
    expect(prettyAnswer('1234')).toBe('1,234')
    expect(prettyAnswer('999')).toBe('999')
    expect(prettyAnswer('-4050.19685039 ft')).toBe(`${M}4,050.19685039 ft`)
    expect(prettyAnswer('1.18059162072e+21')).toBe('1.18059162072e+21')
  })

  it('regroups soulver output and leaves ambiguous commas alone', () => {
    expect(prettyAnswer('1,234,567.5 USD')).toBe('1,234,567.5 USD')
    expect(prettyAnswer('-$1,234.50')).toBe(`${M}$1,234.50`)
    expect(prettyAnswer('1,5')).toBe('1,5')
    expect(prettyAnswer('1.234,5')).toBe('1.234,5')
  })

  it('handles exact forms but not words, dates or ranges', () => {
    expect(prettyAnswer('-2sqrt(3)')).toBe(`${M}2sqrt(3)`)
    expect(prettyAnswer('π - 1')).toBe(`π ${M} 1`)
    expect(prettyAnswer('-1/2')).toBe(`${M}1/2`)
    expect(prettyAnswer('2026-09-22')).toBe('2026-09-22')
    expect(prettyAnswer('well-known')).toBe('well-known')
    expect(prettyAnswer('10-15')).toBe('10-15')
    expect(prettyAnswer('0x1F')).toBe('0x1F')
    expect(prettyAnswer('undefined')).toBe('undefined')
    expect(prettyAnswer('')).toBe('')
  })

  it('keeps measured answers intact', () => {
    expect(prettyAnswer('-5.00 ± 0.02 m')).toBe(`${M}5.00 ± 0.02 m`)
  })

  it('never changes what copy and insert produce', () => {
    for (const raw of ['-3', '3000000000', '-1234.5', '1e-9']) {
      expect(insertableAnswer(raw)).toBe(raw)
      expect(Number(insertableAnswer(prettyAnswer(raw).replace(M, '-')))).toBe(Number(raw))
    }
  })
})
