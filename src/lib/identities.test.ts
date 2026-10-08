import { describe, expect, it } from 'vitest'
import { evaluateSheet } from '../engine/evaluate'
import { CALCULUS_SHEET, IDENTITY_SHEET, identityLabel, identityRuns, identitySheetFor, isIdentityCommand } from './identities'

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
    for (const t of ['', 'derivative of x^2', 'integral of x^2 from 0 to 1', 'identity matrix', 'trig', 'periodic', 'sin(x)'])
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
