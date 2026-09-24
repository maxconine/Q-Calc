// adversarial worksheet-style inputs across units, uncertainty, chem and sums.
// all of these already worked correctly; kept as regression coverage for the area's edges.
import { describe, expect, it } from 'vitest'
import { evaluateLine, evaluateSheet } from './evaluate'

const shown = (text: string) => evaluateLine(text).display

describe('bits, bytes and case-sensitive prefixes', () => {
  it('Gb is gigabit, GB is decimal gigabyte', () => {
    expect(shown('10 Gb in GB')).toBe('1.25 GB')
    expect(shown('10 GB in Gb')).toBe('80 Gb')
  })

  it('binary prefixes convert against the decimal ones', () => {
    expect(shown('1 Gib in GB')).toBe('1.073741824 GB')
  })

  it('reads mPa and MPa, Mg and mg, apart by case alone', () => {
    expect(shown('5 mPa')).toBe('5 mPa')
    expect(evaluateLine('5 MPa').value?.unit).toBe('ksi')
    expect(shown('5 Mg')).toBe('5 t')
    expect(evaluateLine('5 mg').value?.unit).toBe('gr')
  })
})

describe('Cal, cal and everyday temperature edges', () => {
  it('keeps the food calorie and the small calorie distinct', () => {
    expect(shown('2000 Cal to kJ')).toBe('8368 kJ')
    expect(shown('500 cal to Cal')).toBe('0.5 kcal')
  })

  it('0 K is absolute zero in every scale', () => {
    expect(shown('0 K in C')).toBe('-273.15 °C')
    expect(shown('0 K in F')).toBe('-459.67 °F')
  })
})

describe('uncertainty through units and powers', () => {
  it('propagates a relative uncertainty through a product with units', () => {
    expect(shown('2.0 ± 0.05 cm * 3.0 ± 0.05 cm')).toBe('0.00060 ± 0.00003 m²')
  })

  it('keeps two figures only when the calculated ± leads with a 1', () => {
    expect(shown('(10 ± 1)^2')).toBe('100 ± 20')
    expect(shown('5.0 ± 0.14')).toBe('5.00 ± 0.14')
  })
})

describe('chemistry from a worksheet', () => {
  it('balances combustion and redox equations', () => {
    expect(shown('C3H8 + O2 -> CO2 + H2O')).toBe('C₃H₈ + 5 O₂ → 3 CO₂ + 4 H₂O')
    expect(shown('KMnO4 + HCl -> KCl + MnCl2 + H2O + Cl2')).toBe('2 KMnO₄ + 16 HCl → 2 KCl + 2 MnCl₂ + 8 H₂O + 5 Cl₂')
  })

  it('balances an ionic equation with charge', () => {
    expect(shown('Fe^2+ + Cl2 -> Fe^3+ + Cl^-')).toBe('2 Fe²⁺ + Cl₂ → 2 Fe³⁺ + 2 Cl⁻')
  })
})

describe('sums over a huge or infinite range', () => {
  it('finds the closed form for a classic series', () => {
    expect(evaluateSheet(['Σ 1/n^2, n=1..infinity']).at(-1)!.exact).toBe('pi^2/6')
  })

  it('stays blank for a divergent series rather than guessing', () => {
    expect(shown('Σ 1/n, n=1..infinity')).toBe('')
  })
})
