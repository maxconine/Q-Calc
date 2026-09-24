import { describe, expect, it } from 'vitest'
import { ELEMENTS, gridCell, hasStandardWeight, insertableMass } from './elements'
import { isPeriodicCommand } from './periodic'

const PERIOD_STARTS = [1, 3, 11, 19, 37, 55, 87, 119]

// where an element sits, worked out from period lengths alone
function expectedPlace(n: number): { period: number; group: number | null } {
  const period = PERIOD_STARTS.findIndex((start, i) => n >= start && n < PERIOD_STARTS[i + 1]!) + 1
  const idx = n - PERIOD_STARTS[period - 1]!
  const len = PERIOD_STARTS[period]! - PERIOD_STARTS[period - 1]!
  if (len === 2) return { period, group: idx === 0 ? 1 : 18 }
  if (len === 8) return { period, group: idx < 2 ? idx + 1 : idx + 11 }
  if (len === 18) return { period, group: idx + 1 }
  if (idx < 2) return { period, group: idx + 1 }
  if (idx < 17) return { period, group: null }
  return { period, group: idx - 13 }
}

const mass = (e: { mass: string }) => Number(e.mass.replace(/[[\]]/g, ''))

describe('periodic table data', () => {
  it('has 118 elements numbered 1 to 118 in order', () => {
    expect(ELEMENTS).toHaveLength(118)
    ELEMENTS.forEach((e, i) => expect(e.n).toBe(i + 1))
  })

  it('has unique, well formed symbols and names', () => {
    expect(new Set(ELEMENTS.map((e) => e.symbol)).size).toBe(118)
    expect(new Set(ELEMENTS.map((e) => e.name)).size).toBe(118)
    for (const e of ELEMENTS) {
      expect(e.symbol).toMatch(/^[A-Z][a-z]?$/)
      expect(e.name).toMatch(/^[A-Z][a-z]+$/)
    }
  })

  it('puts every element in its period and group', () => {
    for (const e of ELEMENTS) expect({ n: e.n, period: e.period, group: e.group }).toEqual({ n: e.n, ...expectedPlace(e.n) })
  })

  it('lays the table out on the 18 column grid without overlaps', () => {
    const cells = ELEMENTS.map(gridCell)
    expect(new Set(cells.map((c) => `${c.row},${c.col}`)).size).toBe(118)
    for (const c of cells) {
      expect(c.col).toBeGreaterThanOrEqual(1)
      expect(c.col).toBeLessThanOrEqual(18)
      expect([1, 2, 3, 4, 5, 6, 7, 9, 10]).toContain(c.row)
    }
    expect(gridCell(ELEMENTS[56]!)).toEqual({ row: 9, col: 3 })
    expect(gridCell(ELEMENTS[70]!)).toEqual({ row: 9, col: 17 })
    expect(gridCell(ELEMENTS[88]!)).toEqual({ row: 10, col: 3 })
    expect(gridCell(ELEMENTS[102]!)).toEqual({ row: 10, col: 17 })
    expect(gridCell(ELEMENTS[71]!)).toEqual({ row: 6, col: 4 })
  })

  it('writes masses as a weight to at most 3 decimals, or a bracketed mass number', () => {
    for (const e of ELEMENTS) expect(e.mass).toMatch(/^(\d+\.\d{1,3}|\[\d+\])$/)
  })

  it('brackets exactly the elements with no standard atomic weight', () => {
    const bracketed = ELEMENTS.filter((e) => !hasStandardWeight(e)).map((e) => e.n)
    const expected = [43, 61, 84, 85, 86, 87, 88, 89]
    for (let n = 93; n <= 118; n++) expected.push(n)
    expect(bracketed).toEqual(expected)
  })

  it('increases in mass except at the known inversions', () => {
    const lighter = new Set([19, 28, 53, 91, 93, 95, 106])
    const equal = new Set([97, 118])
    for (let i = 1; i < ELEMENTS.length; i++) {
      const prev = mass(ELEMENTS[i - 1]!)
      const next = ELEMENTS[i]!
      const m = mass(next)
      if (lighter.has(next.n)) expect(m, next.symbol).toBeLessThan(prev)
      else if (equal.has(next.n)) expect(m, next.symbol).toBe(prev)
      else expect(m, next.symbol).toBeGreaterThan(prev)
    }
  })

  it('matches well known masses', () => {
    const known: Record<string, string> = {
      H: '1.008',
      He: '4.003',
      Li: '6.94',
      C: '12.011',
      N: '14.007',
      O: '15.999',
      F: '18.998',
      Na: '22.990',
      Mg: '24.305',
      Al: '26.982',
      Si: '28.085',
      P: '30.974',
      S: '32.06',
      Cl: '35.45',
      Ar: '39.95',
      K: '39.098',
      Ca: '40.078',
      Fe: '55.845',
      Cu: '63.546',
      Zn: '65.38',
      Br: '79.904',
      Ag: '107.87',
      I: '126.90',
      Xe: '131.29',
      Pt: '195.08',
      Au: '196.97',
      Hg: '200.59',
      Pb: '207.2',
      U: '238.03',
      Rn: '[222]',
      Pu: '[244]',
    }
    const bySymbol = new Map(ELEMENTS.map((e) => [e.symbol, e.mass]))
    for (const [symbol, m] of Object.entries(known)) expect(bySymbol.get(symbol), symbol).toBe(m)
  })

  it('inserts the plain number', () => {
    const find = (s: string) => ELEMENTS.find((e) => e.symbol === s)!
    expect(insertableMass(find('Na'))).toBe('22.990')
    expect(insertableMass(find('Tc'))).toBe('98')
    expect(insertableMass(find('Og'))).toBe('294')
  })
})

describe('periodic command', () => {
  it('reads periodic and periodic table', () => {
    for (const t of ['periodic', 'periodic table', ' Periodic  Table ', 'PERIODIC']) expect(isPeriodicCommand(t), t).toBe(true)
  })

  it('leaves other input alone', () => {
    for (const t of ['periodi', 'periodic t', 'periodic tables', 'the periodic table', 'periodic 2', '']) {
      expect(isPeriodicCommand(t), t).toBe(false)
    }
  })
})
