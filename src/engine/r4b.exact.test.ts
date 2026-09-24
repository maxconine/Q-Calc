import Decimal from 'decimal.js'
import { describe, expect, it } from 'vitest'
import { evaluateLine, evaluateSheet } from './evaluate'
import { exactForm } from './simplify'

const rad = { angleMode: 'rad' as const }
const last = (lines: string[], opts = {}) => evaluateSheet(lines, { ...rad, ...opts }).at(-1)!

describe('an exact form beside a decimal is that decimal', () => {
  it.each([
    ['sin(1e-12)', rad],
    ['sqrt(1e-24)', rad],
    ['1 - cos(1e-6)', rad],
    ['sqrt(2.00000000001)', rad],
    ['sqrt(2) + 5e-12', rad],
    ['sqrt(3.0000000000001)', rad],
    ['sin(30.0000000001)', { angleMode: 'deg' as const }],
  ])('%s shows no exact form', (text, opts) => {
    expect(evaluateLine(text, opts).exact).toBeUndefined()
  })

  it.each([
    ['sin(pi)', '0'],
    ['sqrt(2)*sqrt(2) - 2', '0'],
    ['1/sqrt(2)', 'sqrt(2)/2'],
    ['tan(pi/12)', '2-sqrt(3)'],
    ['cos(pi/12)', '(sqrt(6)+sqrt(2))/4'],
    ['asin(0.5)', 'pi/6'],
    ['sqrt(9999)', '3sqrt(1111)'],
  ])('%s is still %s', (text, exact) => {
    expect(evaluateLine(text, rad).exact).toBe(exact)
  })

  it('each exact form agrees with its value to 30 digits', () => {
    const D = Decimal.clone({ precision: 40 })
    const cases: Array<[string, () => Decimal]> = [
      ['1/sqrt(2)', () => new D(2).sqrt().div(2)],
      ['tan(pi/12)', () => new D(2).minus(new D(3).sqrt())],
      ['cos(pi/12)', () => new D(6).sqrt().plus(new D(2).sqrt()).div(4)],
      ['sqrt(9999)', () => new D(1111).sqrt().times(3)],
    ]
    for (const [text, exact] of cases) {
      const r = evaluateLine(text, rad)
      expect(Math.abs(exact().toNumber() - r.value!.n)).toBeLessThanOrEqual(4 * Number.EPSILON * Math.abs(r.value!.n))
    }
  })

  it('exactForm snaps float noise only', () => {
    expect(exactForm(1e-12)).toBeNull()
    expect(exactForm(Math.SQRT2 + 5e-12)).toBeNull()
    expect(exactForm(Math.SQRT2)).toBe('sqrt(2)')
    expect(exactForm(Math.sin(Math.PI))).toBe('0')
  })
})

describe('fraction mode near zero', () => {
  it.each([
    ['1e-20', '1e-20'],
    ['1e-16', '1e-16'],
    ['5e-13', '5e-13'],
    ['0.1*3 - 0.3', '0'],
    ['sin(pi)', '0'],
    ['0.1 + 0.2', '3/10'],
    ['0.333333333', '1/3'],
  ])('%s → %s', (text, shown) => {
    expect(last([text], { fractionMode: true }).display).toBe(shown)
  })
})
