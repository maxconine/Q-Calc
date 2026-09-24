import { describe, expect, it } from 'vitest'
import { derivativeAt, gaussKronrod, integrate, justified, limit } from './calcNumeric'

describe('justified', () => {
  it('keeps the digits the error allows, less one, at most 13', () => {
    expect(justified(1.5707963158666323, 3.9e-8)).toBe(1.570796)
    expect(justified(0.3333333333333333, 1e-16)).toBe(0.3333333333333)
    expect(justified(2.0000000003, 1e-9)).toBe(2)
  })

  it('zero when the value is lost in the error at the problem scale, else nothing', () => {
    expect(justified(3e-18, 1e-17, 1)).toBe(0)
    expect(justified(3e-4, 1e-4, 1)).toBeNull()
    expect(justified(1.23, 0.05)).toBeNull()
    expect(justified(Number.NaN, 0)).toBeNull()
  })
})

describe('integrate', () => {
  const cases: Array<[string, (x: number) => number, number, number, number]> = [
    ['x²', (x) => x * x, 0, 1, 1 / 3],
    ['1/√x', (x) => 1 / Math.sqrt(x), 0, 1, 2],
    ['1/√(1-x²)', (x) => 1 / Math.sqrt(1 - x * x), 0, 1, Math.PI / 2],
    ['ln x ln(1-x)', (x) => Math.log(x) * Math.log(1 - x), 0, 1, 2 - Math.PI ** 2 / 6],
    ['e^-x on [0, ∞)', (x) => Math.exp(-x), 0, Infinity, 1],
    ['gaussian on ℝ', (x) => Math.exp(-x * x), -Infinity, Infinity, Math.sqrt(Math.PI)],
    ['e^x on (-∞, 0]', Math.exp, -Infinity, 0, 1],
    ['|x - 0.3|', (x) => Math.abs(x - 0.3), 0, 1, 0.29],
    ['step', (x) => (x < 0.7 ? 0 : 1), 0, 1, 0.3],
    ['narrow spike', (x) => Math.exp(-1e4 * (x - 0.3) ** 2), 0, 1, Math.sqrt(Math.PI) / 100],
  ]

  it.each(cases)('%s: the error bound covers the truth', (_, f, a, b, truth) => {
    const r = integrate(f, a, b)
    expect(r).not.toBeNull()
    expect(Math.abs(r!.value - truth)).toBeLessThanOrEqual(r!.err + 1e-15)
    expect(r!.err).toBeLessThan(1e-6)
  })

  it('reverses and collapses', () => {
    expect(integrate((x) => x, 1, 0)!.value).toBe(-0.5)
    expect(integrate((x) => x, 2, 2)).toEqual({ value: 0, err: 0 })
  })

  it('null when there is no finite answer', () => {
    expect(integrate((x) => 1 / x, 0, 1)).toBeNull()
    expect(integrate((x) => 1 / x, 1, Infinity)).toBeNull()
    expect(integrate(Math.sin, 0, Infinity)).toBeNull()
    expect(integrate((x) => Math.sqrt(x - 2), 0, 1)).toBeNull()
    expect(integrate((x) => x, Number.NaN, 1)).toBeNull()
  })

  it('gauss–kronrod on its own splits toward trouble', () => {
    const r = gaussKronrod((x) => Math.abs(x - 1 / 3), 0, 1)
    expect(Math.abs(r!.value - 5 / 18)).toBeLessThan(1e-10)
  })
})

describe('limit', () => {
  it('extrapolates, and checks both sides', () => {
    expect(limit((x) => Math.sin(x) / x, 0, 0)!.value).toBe(1)
    expect(limit((x) => (1 - Math.cos(x)) / (x * x), 0, 0)!.value).toBeCloseTo(0.5, 12)
    expect(limit((x) => Math.abs(x) / x, 0, 0)).toBeNull()
    expect(limit((x) => Math.abs(x) / x, 0, 1)!.value).toBe(1)
    expect(limit((x) => (1 + 1 / x) ** x, Infinity, 0)!.value).toBeCloseTo(Math.E, 10)
  })

  it('reports infinities only when they grow from the start', () => {
    expect(limit((x) => 1 / x, 0, 1)!.value).toBe(Infinity)
    expect(limit((x) => 1 / (x * x), 0, 0)!.value).toBe(Infinity)
    expect(limit((x) => 1 / x, 0, 0)).toBeNull()
    const r = limit((x) => (Math.cos(x) - 1 + (x * x) / 2) / x ** 4, 0, 0)
    if (r) expect(r.value).toBeCloseTo(1 / 24, 6)
  })
})

describe('derivativeAt', () => {
  it('has a small error on smooth functions', () => {
    const d = derivativeAt(Math.sin, 1)!
    expect(Math.abs(d.value - Math.cos(1))).toBeLessThan(1e-10)
    expect(d.err).toBeLessThan(1e-9)
  })
})
