import { describe, expect, it } from 'vitest'
import { evaluateSheet } from '../engine/evaluate'
import { evaluateLine } from '../engine'
import { completionFor } from './completion'
import {
  CALCULUS_SHEET,
  CONSTANTS_SHEET,
  CONVERSION_SHEET,
  IDENTITY_SHEET,
  identityLabel,
  identityRuns,
  identitySheetFor,
  identityTargetFor,
  isIdentityCommand,
} from './identities'

const items = (sheet: typeof IDENTITY_SHEET) => sheet.sections.flatMap((s) => s.groups.flatMap((g) => g.items))
const VARS = { x: 0.7, y: 0.3, a: 1.7, b: 2.3, m: 2, n: 3, r: 1.5, p: 0.9, q: 1.3, d: 0.4 }
const SETUP = Object.entries(VARS).map(([k, v]) => `${k} = ${v}`)
// the whole engine, as the bar would read a pasted side, with the letters set on earlier lines
// the first = splits the sides; Σ's own k = 0..n stays on the right, and a line that starts with Σ splits at its last
const sides = (copy: string) => {
  const at = copy.startsWith('Σ') ? copy.lastIndexOf(' = ') : copy.indexOf(' = ')
  return [copy.slice(0, at), copy.slice(at + 3)] as const
}
const value = (text: string, more: string[] = []) => evaluateSheet([...SETUP, ...more, text], { angleMode: 'rad' }).at(-1)?.value?.n ?? Number.NaN
const section = (sheet: typeof IDENTITY_SHEET, title: string) => sheet.sections.find((s) => s.title === title)!
// a partial fraction's constants, A = …, are the bar's own assignments
const isConstant = (copy: string) => /^[ABC] = /.test(copy)

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
    for (const t of ['', 'derivative of x^2', 'integral of x^2 from 0 to 1', 'identity matrix', 'trig 2', 'idt matrix', 'constant', 'constants of x', 'periodic', 'sin(x)', '2 in to cm', 'convert', 'metric ton', 'unit', 'kilo'])
      expect(isIdentityCommand(t)).toBe(false)
  })
})

describe('identity sheet parts', () => {
  const opens = (text: string) => {
    const t = identityTargetFor(text)
    return t && [t.sheet.id, t.sheet.sections[t.section]!.title]
  }

  it('opens a sheet at the part asked for', () => {
    expect(opens('trig')).toEqual(['identities', 'Trig'])
    expect(opens('idt trig')).toEqual(['identities', 'Trig'])
    expect(opens('identities partial fractions')).toEqual(['identities', 'Partial fractions'])
    expect(opens('idt pf')).toEqual(['identities', 'Partial fractions'])
    expect(opens('partial fractions')).toEqual(['identities', 'Partial fractions'])
    expect(opens('Identity  Summation')).toEqual(['identities', 'Summation'])
    expect(opens('idt sums')).toEqual(['identities', 'Summation'])
    expect(opens('idt logs')).toEqual(['identities', 'Logs and exponents'])
    // integrals live on the calculus sheet, so that's where integration goes
    expect(opens('idt integration')).toEqual(['calculus', 'Integrals'])
    expect(opens('identities integrals')).toEqual(['calculus', 'Integrals'])
    expect(opens('idt derivatives')).toEqual(['calculus', 'Derivatives'])
    expect(opens('idt constants')).toEqual(['constants', 'CODATA 2022'])
    expect(opens('identities constants')).toEqual(['constants', 'CODATA 2022'])
    expect(opens('idt')).toEqual(['identities', 'Trig'])
  })

  it('says the part in the bar', () => {
    expect(identityTargetFor('idt summation')!.hint).toBe('identity sheet: summation')
    expect(identityTargetFor('identities')!.hint).toBe('identity sheet')
    expect(identityTargetFor('constants')!.hint).toBe('physical constants')
  })

  it('opens the constants on their words', () => {
    for (const t of ['constants', 'physical constants', 'Physical Constants', 'fundamental constants', 'codata', 'constants table'])
      expect(identitySheetFor(t), t).toBe(CONSTANTS_SHEET)
  })

  it('completes the part being typed', () => {
    expect(completionFor('idt in')).toBe('tegration')
    expect(completionFor('identities pa')).toBe('rtial fractions')
    expect(completionFor('identities partial f')).toBe('ractions')
    expect(completionFor('idt su')).toBe('mmation')
    expect(completionFor('idt con')).toBe('stants')
    expect(completionFor('idt tr')).toBe('ig')
    expect(completionFor('identities trig')).toBe('')
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
    expect(IDENTITY_SHEET.sections.map((s) => s.title)).toEqual(['Trig', 'Logs and exponents', 'Algebra', 'Partial fractions', 'Summation'])
    expect(CALCULUS_SHEET.sections.map((s) => s.title)).toEqual(['Derivatives', 'Integrals'])
  })
})

describe('identity copies', () => {
  // each side in the bar's own syntax, worked out by the calculator itself: a typo in either breaks the equality
  it('hold in the calculator, both sides', () => {
    for (const item of items(IDENTITY_SHEET)) {
      if (item.copy.includes('±') || isConstant(item.copy)) continue
      // a sum to ∞ needs |r| < 1
      const more = item.note === '|r| < 1' ? ['r = 0.5'] : []
      // the forms hold with their own group's constants
      const group = section(IDENTITY_SHEET, 'Partial fractions').groups.find((g) => g.items.includes(item))
      if (group) more.push(...group.items.filter((i) => isConstant(i.copy)).map((i) => i.copy))
      const [left, right] = sides(item.copy)
      expect(value(left, more), item.copy).toBeCloseTo(value(right, more), 9)
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

describe('integrals', () => {
  // each antiderivative's slope, by a central difference, is its integrand: a wrong sign or factor shows
  it('differentiate back to what they integrate', () => {
    const rows = section(CALCULUS_SHEET, 'Integrals').groups.flatMap((g) => g.items).filter((i) => / dx = .* \+ C$/.test(i.copy))
    expect(rows.length).toBeGreaterThan(20)
    for (const item of rows) {
      const [, f, F] = item.copy.match(/^∫ (.+) dx = (.+) \+ C$/)!
      const at = (x: number) => value(F!.replace(/(?<![a-z])x(?![a-z])/g, `(${x})`))
      const h = 1e-5
      expect((at(0.7 + h) - at(0.7 - h)) / (2 * h), item.copy).toBeCloseTo(value(f!), 5)
    }
  })
})

describe('constants sheet', () => {
  const rows = items(CONSTANTS_SHEET)
  const row = (symbol: string) => rows.find((r) => r.lead![0] === symbol)!

  it('has every codata row, each with its symbol and a quiet note', () => {
    expect(rows).toHaveLength(54)
    expect(CONSTANTS_SHEET.sections.every((s) => s.wide)).toBe(true)
    for (const r of rows) {
      expect(r.lead).toHaveLength(1)
      expect(r.note).toMatch(/ · (exact|\d)/)
      expect(identityLabel(r.show, r.lead)).not.toMatch(/[\^_{}]/)
    }
    expect(new Set(rows.map((r) => r.copy)).size).toBe(54)
  })

  it('writes values as codata does, with the uncertainty and the power', () => {
    expect(row('G')).toEqual({
      lead: ['G'],
      show: '6.674 30(15) × 10^{−11} m^{3}·kg^{−1}·s^{−2}',
      copy: '6.67430e-11 m^3/(kg s^2)',
      note: 'Newtonian constant of gravitation · 2.2 × 10⁻⁵',
    })
    expect(row('c').show).toBe('299 792 458 m·s^{−1}')
    expect(row('c').note).toBe('speed of light in vacuum · exact')
    expect(row('g_{e}').copy).toBe('-2.00231930436092')
    expect(row('R_{∞}').show).toBe('10 973 731.568 157(12) m^{−1}')
    expect(row('α').note).toBe('fine-structure constant · 1.6 × 10⁻¹⁰')
    expect(identityLabel(row('m_{e}').show, row('m_{e}').lead)).toBe('me 9.109 383 713 9(28) × 10−31 kg')
  })

  // what a click copies is what the bar reads: the shown number, with a unit the bar takes for the right one
  it('copies values the bar reads back', () => {
    for (const r of rows) {
      const [n, ...unit] = r.copy.split(' ')
      const got = evaluateLine(unit.length ? `(${r.copy})/(1 ${unit.join(' ')})` : r.copy).value?.n ?? Number.NaN
      expect(Math.abs(got / Number(n) - 1), r.copy).toBeLessThan(1e-9)
    }
  })
})
