import { describe, expect, it } from 'vitest'
import { prettyTokens } from '../components/QuickInput'
import { evaluateLine, evaluateSheet } from '../engine/evaluate'
import type { EvaluateOptions } from '../engine/types'
import { insertableAnswer } from './answer'

function valueOf(text: string, opts: EvaluateOptions = {}): number | undefined {
  const r = evaluateLine(text, opts)
  return r.value?.kind === 'number' ? r.value.n : undefined
}

/** Type `text` one key at a time with the caret at the end, as QuickInput does. */
function typeKeys(text: string): string {
  let value = ''
  for (const ch of text) {
    const raw = value + ch
    value = prettyTokens(raw, raw.length)
  }
  return prettyTokens(value)
}

describe('audit: prettyTokens leaves words alone', () => {
  it.each([
    ['spin', 'spin'],
    ['epic', 'epic'],
    ['api', 'api'],
    ['pint', 'pint'],
    ['pi2', 'π2'],
    ['info', 'info'],
    ['infinity', 'infinity'],
    ['thetas', 'thetas'],
    ['dots', 'dots'],
    ['\\cdot', '\\cdot'],
    ['cbrt2', '∛2'],
    ['answer', 'answer'],
    ['trans', 'trans'],
  ])('%s → %s', (typed, want) => {
    expect(prettyTokens(typed)).toBe(want)
    expect(typeKeys(typed)).toBe(want)
  })
})

describe('audit: prettyTokens replaces tokens', () => {
  it.each([
    ['pi', 'π'],
    ['Pi', 'π'],
    ['PI/2', 'π/2'],
    ['2pi', '2π'],
    ['inf', '∞'],
    ['theta', 'θ'],
    ['cbrt(27)', '∛(27)'],
    ['3 dot 4', '3 · 4'],
    ['5 +/- 1', '5 ± 1'],
    ['5 -/+ 1', '5 ∓ 1'],
    ['5 ~ 1', '5 ± 1'],
  ])('%s → %s', (typed, want) => {
    expect(prettyTokens(typed)).toBe(want)
    expect(typeKeys(typed)).toBe(want)
  })
})

describe('audit: what prettyTokens produces still evaluates the same', () => {
  it.each<[string, number]>([
    ['sin(pi/6)', 0.5],
    ['2pi', 2 * Math.PI],
    ['cbrt(27)', 3],
    ['cbrt(-8) + 1', -1],
    ['4 dot 5', 20],
    ['pi*2^2', 4 * Math.PI],
    ['1/inf', 0],
  ])('%s', (typed, want) => {
    const rad = { angleMode: 'rad' as const }
    const n = valueOf(typeKeys(typed), rad)
    expect(n, typeKeys(typed)).toBeCloseTo(want, 12)
  })

  it('a ± typed as +/- evaluates as a measurement', () => {
    expect(evaluateLine(typeKeys('(2.0 +/- 0.1) * 3')).display).toBe('6.0 ± 0.3')
  })

  // theta turns into θ, which the engine does not read, so a theta variable stops working
  it('a variable called theta still works after prettifying', () => {
    const out = evaluateSheet([typeKeys('theta = 30'), typeKeys('sin(theta)')])
    expect(out[1]!.value?.n).toBeCloseTo(0.5, 12)
  })
})

describe('audit: typed ans reads the last answer at full precision', () => {
  const after = (first: string, typed: string, opts: EvaluateOptions = {}) =>
    evaluateSheet([first, typeKeys(typed)], opts)[1]!
  it('a plain positive answer is fine', () => {
    expect(after('21', 'ans * 2').value?.n).toBe(42)
    expect(after('3', 'ans^2').value?.n).toBe(9)
    expect(after('4', '10 - ans').value?.n).toBe(6)
  })
  it('keeps full precision: 1/3 then ans*3 is 1', () => {
    expect(after('1/3', 'ans*3').display).toBe('1')
  })
  it('a measured answer keeps its ±', () => {
    expect(after('10.0 ± 0.7', 'ans * 2').display).toBe('20.0 ± 1.4')
  })
  it('ans^2 with ans = -3 is 9', () => {
    expect(after('0-3', 'ans^2').value?.n).toBe(9)
  })
  it('4/ans with ans 1/2 is 8', () => {
    expect(after('1/2', '4/ans').value?.n).toBe(8)
  })
  it('6/ans with ans 2sqrt(3) is sqrt(3)', () => {
    expect(after('2sqrt(3)', '6/ans').value?.n).toBeCloseTo(Math.sqrt(3), 12)
  })
  it('1/ans with ans pi/6 is 6/pi', () => {
    expect(after('pi/6', '1/ans', { angleMode: 'rad' }).value?.n).toBeCloseTo(6 / Math.PI, 12)
  })
  it('ans^2 with ans = 3 cm keeps the unit', () => {
    const r = after('3 cm', 'ans^2')
    expect(r.value?.unit).toMatch(/²/)
  })
})

describe('audit: an inserted answer reads back as the same value', () => {
  it.each<[string, EvaluateOptions?]>([
    ['5 ft to cm'],
    ['1 mi to km'],
    ['100 C to F'],
    ['6.022e23 * 2'],
    ['1e-9 * 3'],
    ['2^0.5'],
    ['1/8', { fractionMode: true }],
    ['-7/3', { fractionMode: true }],
  ])('%s', (text, opts) => {
    const first = evaluateLine(text, opts)
    const inserted = insertableAnswer(first.display, first.value?.n)
    // a bare unit answer shows its counterpart, so read it back in its own unit
    const unit = first.value?.unit
    const again = evaluateLine(unit ? `${inserted} to ${unit}` : inserted, opts)
    const want = Number(first.value!.n.toPrecision(12))
    expect(again.value?.n, `${text} → ${first.display} → ${inserted} → ${again.display}`).toBeCloseTo(want, 9)
  })

  it('an exact form inserts and evaluates to the same number', () => {
    for (const [text, opts] of [
      ['sin(15)', {}],
      ['sqrt(8)', {}],
      ['asin(0.5)', { angleMode: 'rad' as const }],
      ['tan(75)', {}],
    ] as const) {
      const first = evaluateLine(text, opts)
      expect(first.exact, text).toBeTruthy()
      const again = evaluateLine(insertableAnswer(first.exact!), opts)
      expect(again.value?.n, `${text} → ${first.exact}`).toBeCloseTo(first.value!.n, 12)
    }
  })
})
