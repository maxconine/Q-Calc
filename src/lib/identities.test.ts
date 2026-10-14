import { describe, expect, it } from 'vitest'
import { evaluateSheet } from '../engine/evaluate'
import { evaluateLine } from '../engine'
import { CALCULUS_SHEET, CONVERSION_SHEET, IDENTITY_SHEET, identityLabel, identityRuns, identitySheetFor, isIdentityCommand } from './identities'

const items = (sheet: typeof IDENTITY_SHEET) => sheet.sections.flatMap((s) => s.groups.flatMap((g) => g.items))
const VARS = { x: 0.7, y: 0.3, a: 1.7, b: 2.3, m: 2, n: 3, r: 1.5 }
const SETUP = Object.entries(VARS).map(([k, v]) => `${k} = ${v}`)
// the whole engine, as the bar would read a pasted side, with the letters set on earlier lines
// the first = splits the sides; Σ's own k = 0..n stays on the right
const sides = (copy: string) => {
  const at = copy.indexOf(' = ')
  return [copy.slice(0, at), copy.slice(at + 3)] as const
}
const value = (text: string) => evaluateSheet([...SETUP, text], { angleMode: 'rad' }).at(-1)?.value?.n ?? Number.NaN

describe('identity sheet commands', () => {
  it('opens the identities on its words', () => {
    for (const t of ['identities', 'identity', 'Trig identities', 'trig identity', ' log identities ', 'algebra  identities', 'trigonometric identities'])
      expect(identitySheetFor(t)).toBe(IDENTITY_SHEET)
  })

  it('opens the calculus sheet on its own words', () => {
    for (const t of ['derivatives', 'derivative', 'integrals', 'integral', 'calculus', 'common derivatives', 'Calculus'])
      expect(identitySheetFor(t)).toBe(CALCULUS_SHEET)
  })

  it('leaves calculus lines and other words alone', () => {
    for (const t of ['', 'derivative of x^2', 'integral of x^2 from 0 to 1', 'identity matrix', 'trig', 'periodic', 'sin(x)', '2 in to cm', 'convert', 'metric ton', 'unit', 'kilo'])
      expect(isIdentityCommand(t)).toBe(false)
  })
})

describe('identity text', () => {
  it('raises and lowers the marked runs', () => {
    expect(identityRuns('sin^{2}x + cos^{2}x = 1')).toEqual([
      { text: 'sin' },
      { text: '2', shift: 'sup' },
      { text: 'x + cos' },
      { text: '2', shift: 'sup' },
      { text: 'x = 1' },
    ])
    expect(identityRuns('b^{log_{b}x} = x')).toEqual([
      { text: 'b' },
      { text: 'log', shift: 'sup' },
      { text: 'b', shift: 'supsub' },
      { text: 'x', shift: 'sup' },
      { text: ' = x' },
    ])
    expect(identityLabel('log_{b}1 = 0')).toBe('logb1 = 0')
  })

  it('leaves no markup in any line', () => {
    for (const item of [...items(IDENTITY_SHEET), ...items(CALCULUS_SHEET)])
      expect(identityLabel(item.show)).not.toMatch(/[\^_{}]/)
  })

  it('has every section asked for', () => {
    expect(IDENTITY_SHEET.sections.map((s) => s.title)).toEqual(['Trig', 'Logs and exponents', 'Algebra'])
    expect(CALCULUS_SHEET.sections.map((s) => s.title)).toEqual(['Derivatives', 'Integrals'])
  })
})

describe('identity copies', () => {
  // each side in the bar's own syntax, worked out by the calculator itself: a typo in either breaks the equality
  it('hold in the calculator, both sides', () => {
    for (const item of items(IDENTITY_SHEET)) {
      if (item.copy.includes('±')) continue
      const [left, right] = sides(item.copy)
      expect(value(left), item.copy).toBeCloseTo(value(right), 9)
    }
  })

  it('square the half angles', () => {
    const half = items(IDENTITY_SHEET).filter((i) => i.copy.includes('±'))
    expect(half).toHaveLength(2)
    for (const item of half) {
      const [left, right] = sides(item.copy)
      expect(value(`(${left})^2`), item.copy).toBeCloseTo(value(`(${right.replace('±', '')})^2`), 9)
    }
  })
})

describe('conversion sheet', () => {
  const groups = CONVERSION_SHEET.sections.flatMap((s) => s.groups)
  const prefixes = CONVERSION_SHEET.sections[0]!.groups.flatMap((g) => g.items)

  it('writes each prefix out as a name, a symbol, a power and its digits', () => {
    const row = (name: string) => prefixes.find((p) => p.lead?.[0] === name)!
    expect(row('kilo')).toEqual({ lead: ['kilo', 'k'], show: '10^{3}', copy: '10^3', note: '1 000' })
    expect(row('deka').note).toBe('10')
    expect(row('hecto').note).toBe('100')
    expect(row('peta').note).toBe('1 000 000 000 000 000')
    expect(row('centi').note).toBe('0.01')
    expect(row('micro')).toEqual({ lead: ['micro', 'μ'], show: '10^{−6}', copy: '10^-6', note: '0.000 001' })
    expect(row('pico').note).toBe('0.000 000 000 001')
    expect(row('quetta').note).toBe('nonillion')
    expect(prefixes).toHaveLength(24)
  })

  it('has digits that are the power they sit beside', () => {
    for (const p of prefixes) {
      if (!/\d/.test(p.note!)) continue
      expect(Number(p.note!.replace(/ /g, '')), p.lead![0]).toBeCloseTo(evaluateLine(p.copy).value!.n, 20)
    }
  })

  // each factor is checked against the calculator's own conversion, to the digits shown
  it('agrees with the calculator, row by row', () => {
    const rows = groups.filter((g) => g.title !== 'Temperature').flatMap((g) => g.items).filter((i) => !i.lead)
    expect(rows.length).toBeGreaterThan(80)
    for (const item of rows) {
      const [left, right] = sides(item.copy)
      const [, amount, unit] = right.match(/^(\S+) (.+)$/)!
      const got = evaluateLine(`${left} to ${unit}`).value?.n ?? Number.NaN
      const want = evaluateLine(amount!).value?.n ?? Number.NaN
      expect(Math.abs(got / want - 1), item.copy).toBeLessThan(1e-5)
    }
  })

  it('has temperature formulas the calculator agrees with', () => {
    const n = (text: string) => evaluateLine(text).value?.n ?? Number.NaN
    expect(n('37 C to F')).toBeCloseTo(37 * 9 / 5 + 32, 9)
    expect(n('98.6 F to C')).toBeCloseTo((98.6 - 32) * 5 / 9, 9)
    expect(n('37 C to K')).toBeCloseTo(37 + 273.15, 9)
    expect(n('300 K to R')).toBeCloseTo(300 * 9 / 5, 9)
  })

  it('copies nothing twice in a group, so each row is its own key', () => {
    for (const g of groups) expect(new Set(g.items.map((i) => i.copy)).size, g.title).toBe(g.items.length)
  })
})
