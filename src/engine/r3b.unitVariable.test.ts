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
    expect(rows[2]!.value?.unit).toBe('lbs')
    expect(rows[2]!.value?.n).toBeCloseTo(3 * 4.4092452437, 6)
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
