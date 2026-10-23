// a unit times a variable: `5 m * x` and `weight * price` used to go blank
import { describe, expect, it } from 'vitest'
import { evaluateSheet } from './evaluate'

function shown(lines: string[]): string {
  return evaluateSheet(lines).at(-1)!.display
}

describe('a unit literal times a plain variable', () => {
  it('multiplies either order', () => {
    expect(shown(['x = 3', '5 m * x'])).toBe('15 m')
    expect(shown(['x = 3', 'x * 5 m'])).toBe('15 m')
    expect(shown(['x = 3', '5 kg * x'])).toBe('15 kg')
  })

  it('divides too', () => {
    expect(shown(['x = 2', '10 m / x'])).toBe('5 m')
  })
})

describe('a variable holding a quantity times a plain variable', () => {
  it('multiplies either order', () => {
    const rows = evaluateSheet(['price = 3', 'weight = 2 kg', 'price * weight'])
    // the stored 2 kg stays kg, not its lbs counterpart
    expect(rows[2]!.value?.unit).toBe('kg')
    expect(rows[2]!.value?.n).toBeCloseTo(6, 9)
    expect(shown(['price = 3', 'weight = 2 kg', 'weight * price'])).toBe(shown(['price = 3', 'weight = 2 kg', 'price * weight']))
  })

  it('still prefers a variable over the unit it collides with', () => {
    // n is also the newton symbol; the stored variable must win, unaffected by this fix
    const rows = evaluateSheet(['n = 5', 'n*2'])
    expect(rows[1]?.value?.n).toBe(10)
    expect(rows[1]?.value?.unit).toBeUndefined()
  })

  it('leaves an undefined name blank rather than guessing', () => {
    expect(shown(['5 m * q'])).toBe('')
  })
})

// `ans mm to in` used to go blank: a plain ans takes a unit the way `x mm` does
describe('a plain ans given a unit', () => {
  it('converts', () => {
    expect(shown(['25.4', 'ans mm to in'])).toBe('1 in')
    expect(shown(['25.4', 'ans mm in in'])).toBe('1 in')
    expect(shown(['3', 'ans hr to min'])).toBe('180 min')
    expect(shown(['2', 'ans atm to Pa'])).toBe('202650 Pa')
    expect(evaluateSheet(['5', 'ans kJ to cal']).at(-1)!.value?.n).toBeCloseTo(5000 / 4.184, 9)
  })

  it('reads an offset scale', () => {
    expect(shown(['100', 'ans °C to °F'])).toBe('212 °F')
    expect(shown(['100', 'ans degC to degF'])).toBe('212 °F')
  })

  it('stands as that quantity', () => {
    expect(shown(['2', 'ans kg'])).toBe('2 kg')
    expect(shown(['2', 'ans N m'])).toBe('2 N·m')
    expect(shown(['5', '2 ans mm'])).toBe('10 mm')
  })

  it('works from an ans handed in', () => {
    expect(evaluateSheet(['ans mm to in'], { ans: 25.4 })[0]!.display).toBe('1 in')
  })

  it('leaves plain ans maths alone', () => {
    expect(shown(['7', 'ans mod 3'])).toBe('1')
    expect(shown(['2', 'ans pi'])).toBe(shown(['2 pi']))
    expect(shown(['255', 'ans in hex'])).toBe('0xFF')
    expect(shown(['ans mm'])).toBe('')
  })
})

describe('an ans with a unit given another of its kind', () => {
  it('converts rather than squaring', () => {
    expect(shown(['5 m', 'ans m'])).toBe('5 m')
    expect(shown(['25.4 mm', 'ans in'])).toBe('1 in')
    expect(shown(['5 m', 'ans ft to in'])).toBe(shown(['5 m to in']))
    expect(shown(['100 °C', 'ans K'])).toBe('373.15 K')
  })

  it('chains a temperature, which used to go blank', () => {
    expect(shown(['100 °C', 'ans to K'])).toBe('373.15 K')
    expect(shown(['212 °F', 'ans to °C'])).toBe('100 °C')
  })

  it('still multiplies by a unit of another kind', () => {
    expect(evaluateSheet(['5 m', 'ans s']).at(-1)!.value?.unit).toMatch(/·s$/)
  })
})
