import { describe, expect, it } from 'vitest'
import { evaluateLine } from './evaluate'
import { exactForm } from './simplify'

describe('exactForm rationalize', () => {
  it('rationalizes a radical denominator by default', () => {
    expect(exactForm(5 / Math.sqrt(41))).toBe('5sqrt(41)/41')
    expect(exactForm(1 / Math.sqrt(2))).toBe('sqrt(2)/2')
    expect(exactForm(2 / Math.sqrt(3))).toBe('2sqrt(3)/3')
  })

  it('keeps a radical in the denominator when rationalize is off', () => {
    expect(exactForm(5 / Math.sqrt(41), { rationalize: false })).toBe('5/sqrt(41)')
    expect(exactForm(1 / Math.sqrt(2), { rationalize: false })).toBe('1/sqrt(2)')
    expect(exactForm(2 / Math.sqrt(3), { rationalize: false })).toBe('2/sqrt(3)')
    expect(exactForm(-5 / Math.sqrt(41), { rationalize: false })).toBe('-5/sqrt(41)')
  })

  it('still simplifies square factors when not rationalizing', () => {
    expect(exactForm(Math.sqrt(12), { rationalize: false })).toBe('2sqrt(3)')
    expect(exactForm(5 / Math.sqrt(8), { rationalize: false })).toBe('5/(2sqrt(2))')
  })
})

describe('evaluateLine rationalize', () => {
  it('shows 5/sqrt(41) unrationalized when that setting is off', () => {
    expect(evaluateLine('5/sqrt(41)').exact).toBe('5sqrt(41)/41')
    expect(evaluateLine('5/sqrt(41)', { rationalize: false }).exact).toBe('5/sqrt(41)')
  })

  it('leaves tan(30) as 1/sqrt(3) when not rationalizing', () => {
    expect(evaluateLine('tan(30)').exact).toBe('sqrt(3)/3')
    expect(evaluateLine('tan(30)', { rationalize: false }).exact).toBe('1/sqrt(3)')
  })
})
