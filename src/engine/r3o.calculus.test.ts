import { describe, expect, it } from 'vitest'
import { evaluateSheet } from './evaluate'

const last = (lines: string[]) => evaluateSheet(lines, { angleMode: 'rad' }).at(-1)!.display

describe('a plain variable before its differential', () => {
  it.each([
    [['∫_0^1 x dx'], '0.5'],
    [['∫0..1 t dt'], '0.5'],
    [['∫_0^1 2x dx'], '1'],
    [['∫_0^3 t dt'], '4.5'],
    [['∫0..1 x*dx'], '0.5'],
    [['x=3', '∫_0^x t dt'], '4.5'],
    [['x=2', '∫_0^1 x t dt'], '1'],
  ])('%j → %s', (lines, shown) => {
    expect(last(lines)).toBe(shown)
  })
})

describe('no derivative where the function is undefined', () => {
  it.each([
    [['d/dx ln(x) at -1'], ''],
    [['d²/dx² ln(x) at -1'], ''],
    [['f(x)=ln(x)', "f'(-1)"], ''],
    [['d/dx ln(x) at 2'], '0.5'],
  ])('%j → %j', (lines, shown) => {
    expect(last(lines)).toBe(shown)
  })
})

describe('a limit that drifts away is not the value it started near', () => {
  it.each([
    [['lim x→0 (x + 1e-10)/x'], ''],
    [['lim x→0+ (x + 1e-10)/x'], ''],
    [['lim x→0 (1 - cos(x))/x^2'], '0.5'],
    [['lim x→0 (tan(x) - sin(x))/x^3'], '0.5'],
    [['lim x→∞ x sin(1/x)'], '1'],
  ])('%j → %j', (lines, shown) => {
    expect(last(lines)).toBe(shown)
  })
})

describe('an integral never misses a narrow bump and calls it 0', () => {
  it.each([
    [['∫0..1000 exp(-(x-737)^2)'], '1.7724538509'],
    [['∫0..1e6 exp(-x)'], '1'],
    [['∫0..10 exp(-1000(x-3.7)^2) + 1'], '10.0560499122'],
    [['∫0..1000 exp(-(x-737)^2) + exp(-x)'], '2.7724538509'],
    [['∫0..10 abs(x-3.3)'], '27.89'],
    [['∫0..1e10 exp(-x)'], ''],
    [['∫0..10 exp(-1e6 (x-3.7)^2)'], ''],
    [['∫-∞..∞ exp(-(x-50)^2)'], ''],
    [['∫0..∞ exp(-(x-1e4)^2)'], ''],
    [['∫0..1 floor(x)'], '0'],
    [['∫-1..1 x^3'], '0'],
    [['∫0..2pi sin(x)'], '0'],
  ])('%j → %j', (lines, shown) => {
    expect(last(lines)).toBe(shown)
  })
})
