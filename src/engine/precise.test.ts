import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest'
import { evaluateLine, evaluateSheet } from './evaluate'
import { formatNumber, num } from './format'
import { verdict } from './precise'
import { evalScientific } from './scientific'

// a loaded test machine must not trip the 5 ms keystroke budget and fall back to the float answer
beforeAll(() => {
  vi.spyOn(performance, 'now').mockReturnValue(0)
})
afterAll(() => {
  vi.restoreAllMocks()
})

const show = (text: string, angleMode: 'deg' | 'rad' = 'deg') => evaluateLine(text, { angleMode }).display

describe('truthful digits', () => {
  it('drops float artefacts', () => {
    expect(show('0.1+0.2-0.3')).toBe('0')
    expect(show('(1e8+0.1)-1e8')).toBe('0.1')
    expect(show('(1+1e-15)^(1e15)')).toBe('2.71828182846')
    expect(show('1e16 + 1 - 1e16')).toBe('1')
    expect(show('sin(1e-13)', 'rad')).toBe('1e-13')
    expect(show('sqrt(2)^2 - 2')).toBe('0')
    expect(show('0.3 mod 0.1')).toBe('0')
    expect(show('ln(1 + 1e-15)')).toBe('1e-15')
    expect(show('exp(1e-15) - 1')).toBe('1e-15')
  })

  it('stores the true value, not the artefact', () => {
    expect(evaluateLine('1.1*1.1').value?.n).toBe(1.21)
    expect(evaluateLine('sin(30)').value?.n).toBe(0.5)
  })

  it('reads a jump at the exact value, not the float one', () => {
    expect(show('floor((0.1+0.7)*10)')).toBe('8')
    expect(show('floor(sin(30)*2)')).toBe('1')
    expect(show('round(sin(30))')).toBe('1')
    expect(show('ceil(2470 / (1e8)^5)')).toBe('1')
  })

  it('keeps true zeros and near-zeros apart', () => {
    expect(show('sin(pi)', 'rad')).toBe('0')
    expect(show('sin(180)')).toBe('0')
    expect(show('cos(90)')).toBe('0')
    expect(show('sin(1e-13)')).toBe('1.74532925199e-15')
    expect(show('sin(mod(pi, 12))', 'rad')).toBe('0')
  })

  it('follows variables, ans and user functions', () => {
    const sheet = evaluateSheet(['a = 0.1 + 0.2', 'a - 0.3', 'f(x) = x*3', 'f(0.1) - 0.3', 'ans + 0.1 - 0.1'])
    expect(sheet.map((r) => r.display)).toEqual(['0.3', '0', 'f(x) = x*3', '0', '0'])
  })

  it('only trusts a computed input as far as its last bit', () => {
    // a = 0.333…3 is 1/3 to the last bit; taking its digits as exact would show -1e-16
    const sheet = evaluateSheet(['a = 1/3', 'a*3 - 1', '(a + 1e8) - 1e8'])
    expect(sheet.map((r) => r.display)).toEqual(['0.333333333333', '0', '0.333333333333'])
    expect(evaluateLine('ans*3 - 1', { ans: 1 / 3 }).display).toBe('0')
  })

  it('leaves what it cannot verify alone', () => {
    const cases: [string, string][] = [
      ['random()', 'unsupported'],
      ['gcd(12, 18)', 'unsupported'],
      ['mean([1, 2, 3])', 'unsupported'],
      ['3.5!', 'unsupported'],
      ['sin(1e16)', 'unsupported'],
    ]
    for (const [text, reason] of cases) {
      const f = evalScientific(text)!
      expect(verdict(text, f, {}), text).toEqual({ kind: 'kept', reason })
    }
    expect(verdict('1/3', num(1 / 3), {}, -1)).toEqual({ kind: 'kept', reason: 'budget' })
  })

  it('never corrects a value that means something else', () => {
    // the engine's answer and the precise one must describe the same expression
    expect(verdict('2+2', num(5), {})).toEqual({ kind: 'kept', reason: 'meaning' })
  })

  it('leaves units on the float path', () => {
    expect(evaluateLine('0.1 m + 0.2 m').value?.unit).toBe('m')
  })
})

describe('truthful digits fuzz', () => {
  let seed = 20260922
  const rnd = () => (seed = (Math.imul(seed, 1103515245) + 12345) & 0x7fffffff) / 0x7fffffff
  const int = (lo: number, hi: number) => lo + Math.floor(rnd() * (hi - lo + 1))

  type Frac = [bigint, bigint]
  const gcd = (a: bigint, b: bigint): bigint => (b ? gcd(b, a % b) : a < 0n ? -a : a)
  const norm = ([n, d]: Frac): Frac => {
    if (d < 0n) [n, d] = [-n, -d]
    const g = gcd(n, d) || 1n
    return [n / g, d / g]
  }

  function literal(): { text: string; v: Frac } {
    const digits = String(int(1, 99999))
    const places = int(0, 4)
    const padded = digits.padStart(places + 1, '0')
    const text = places ? `${padded.slice(0, -places)}.${padded.slice(-places)}` : digits
    return { text, v: norm([BigInt(digits), 10n ** BigInt(places)]) }
  }

  function gen(depth: number): { text: string; v: Frac } | null {
    if (depth === 0 || rnd() < 0.3) return literal()
    const a = gen(depth - 1)
    const b = gen(depth - 1)
    if (!a || !b) return null
    const [an, ad] = a.v
    const [bn, bd] = b.v
    switch (int(0, 4)) {
      case 0:
        return { text: `(${a.text} + ${b.text})`, v: norm([an * bd + bn * ad, ad * bd]) }
      case 1:
        return { text: `(${a.text} - ${b.text})`, v: norm([an * bd - bn * ad, ad * bd]) }
      case 2:
        return { text: `(${a.text} * ${b.text})`, v: norm([an * bn, ad * bd]) }
      case 3:
        return bn ? { text: `(${a.text} / ${b.text})`, v: norm([an * bd, ad * bn]) } : null
      default: {
        const k = int(2, 3)
        return { text: `(${a.text})^${k}`, v: norm([an ** BigInt(k), ad ** BigInt(k)]) }
      }
    }
  }

  /** 30 correct digits of an exact fraction, then the nearest double. */
  function toNumber([n, d]: Frac): number {
    if (n === 0n) return 0
    const neg = n < 0n
    let a = neg ? -n : n
    let exp = 0
    while (a / d >= 10n ** 30n) {
      d *= 10n
      exp++
    }
    while (a / d < 10n ** 29n) {
      a *= 10n
      exp--
    }
    return Number(`${neg ? '-' : ''}${a / d}e${exp}`)
  }

  it('shows the exact digits of exact arithmetic', () => {
    let checked = 0
    let floatWrong = 0
    for (let i = 0; i < 1500; i++) {
      const e = gen(int(2, 4))
      if (!e) continue
      const truth = toNumber(e.v)
      if (!Number.isFinite(truth) || Math.abs(truth) > 1e300 || (truth !== 0 && Math.abs(truth) < 1e-300)) continue
      const r = evaluateLine(e.text)
      if (r.value?.kind !== 'number') continue
      checked++
      const f = evalScientific(e.text)
      if (f && formatNumber(f.n) !== formatNumber(truth)) floatWrong++
      expect(r.display, e.text).toBe(formatNumber(truth))
    }
    expect(checked).toBeGreaterThan(1000)
    expect(floatWrong).toBeGreaterThan(0)
  })

  it('agrees with the engine on meaning', () => {
    const fns = ['sin', 'cos', 'tan', 'asin', 'atan', 'sqrt', 'ln', 'log10', 'exp', 'abs', 'floor', 'round', 'sinh', 'cbrt']
    let numeric = 0
    let meaning = 0
    for (let i = 0; i < 1500; i++) {
      const f1 = fns[int(0, fns.length - 1)]!
      const f2 = fns[int(0, fns.length - 1)]!
      const x = literal().text
      const y = literal().text
      const text = `${f1}(${x} / ${y}) + ${f2}(${y} * 0.01)`
      const angleMode = rnd() < 0.5 ? 'deg' : 'rad'
      const v = evalScientific(text, { angleMode })
      if (v?.kind !== 'number' || !Number.isFinite(v.n)) continue
      numeric++
      const out = verdict(text, v, { angleMode }, 10_000)
      if (out.kind === 'kept' && out.reason === 'meaning') meaning++
    }
    expect(numeric).toBeGreaterThan(800)
    expect(meaning).toBe(0)
  })
})
