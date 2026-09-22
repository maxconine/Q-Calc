import { describe, expect, it } from 'vitest'
import { evaluateLine } from './evaluate'
import { exactForm, wantsExactForm } from './simplify'

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

describe('wantsExactForm', () => {
  it.each(['sin(30)', 'cos(pi/6)', 'tan(45)', 'csc(90)', 'sec(60)', 'cot(30)', 'arcsin(0.5)', 'arccos(0.5)', 'arctan(1)', 'asin(1)', 'sin^-1(0.5)', 'sin90', '\\sin(90)', 'sqrt(12)', '2sqrt(3)', '√2', '\\sqrt{8}', 'sqrt12'])(
    'is true for %s',
    (expr) => {
      expect(wantsExactForm(expr)).toBe(true)
    },
  )

  it.each(['1/2', '2/3', 'pi', 'pi/2', '2*pi', '2+2', '90 deg', '2032mm to ft', 'log(10)', 'ln(e)', '2^8', 'sinh(0)', '5!', 'x = 5'])(
    'is false for %s',
    (expr) => {
      expect(wantsExactForm(expr)).toBe(false)
    },
  )
})

describe('evaluateLine exact only for trig and square roots', () => {
  it('omits a closed form for fractions, π, and unit conversions', () => {
    expect(evaluateLine('1/2').exact).toBeUndefined()
    expect(evaluateLine('pi/2').exact).toBeUndefined()
    expect(evaluateLine('2*pi').exact).toBeUndefined()
    expect(evaluateLine('90 deg').exact).toBeUndefined()
    expect(evaluateLine('2032mm to ft').exact).toBeUndefined()
  })

  it('still attaches a closed form to trig and square roots', () => {
    expect(evaluateLine('sin(30)').exact).toBe('1/2')
    expect(evaluateLine('sqrt(12)').exact).toBe('2sqrt(3)')
    expect(evaluateLine('√8').exact).toBe('2sqrt(2)')
  })
})
