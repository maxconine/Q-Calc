// money said the way people type it into a calculator: `$45.50 + $12.25`, `3 x $4.99`, `$100 + 8.25% tax`,
// `my rent is $1500 + utilities $200`, a rate times an amount (`$30/hr * 40 hr`, `10 nights at $120`), and
// shorthand numbers (`$50k / 12`, `1.5k + 500`, `5 dozen`). what soulvercore answered on the mac.
// a line only counts with money or a shorthand in it, so plain maths stays the engine's

import { currencyWord, NUM, parseNumber } from './currency'

export type Sum = { n: number; currency?: string }

const SYMBOLS: Record<string, string> = { $: 'USD', '€': 'EUR', '£': 'GBP', '¥': 'JPY', '₹': 'INR' }
// the scale after a number. `m` and `b` are a million and a billion only on money (`$1.5m`), since `5 m` is metres;
// `k` only touching the number (`1.5k`), since `5 k` could be kelvin
const SCALES: Record<string, number> = {
  k: 1e3,
  thousand: 1e3,
  m: 1e6,
  mm: 1e6,
  mn: 1e6,
  million: 1e6,
  b: 1e9,
  bn: 1e9,
  billion: 1e9,
  dozen: 12,
}
const SCALE = String.raw`(?:(k|mm|mn|m|bn|b)\b|\s*(thousand|million|billion|dozen)\b)?`
// money or a number: an optional symbol before, the number, an optional scale, an optional symbol after
const VALUE = String.raw`([$€£¥₹])?\s?(${NUM})${SCALE}\s?([$€£¥₹])?`

const CONVERTS = new Set(['in', 'to', 'into', 'as'])

type Token = { kind: 'value'; n: number; currency?: string; scaled: boolean } | { kind: 'pct'; p: number } | { kind: 'op'; op: string }

function value(m: RegExpExecArray | RegExpMatchArray, at = 1): { n: number; currency?: string; scaled: boolean } | null {
  const [pre, num, scale, dozen, post] = [m[at], m[at + 1], m[at + 2], m[at + 3], m[at + 4]]
  if (pre && post) return null
  const n = parseNumber(num ?? '')
  if (n == null) return null
  const k = scale ?? dozen
  const sym = pre ?? post
  if (scale && scale !== 'k' && !sym) return null
  return { n: n * (k ? SCALES[k]! : 1), currency: sym ? SYMBOLS[sym] : undefined, scaled: Boolean(k) }
}

// the line as values, percentages and operators; any other word is a label and drops out
function tokens(s: string): Token[] | null {
  const out: Token[] = []
  const re = new RegExp(String.raw`\s*(?:(${NUM})\s*%|${VALUE}|([+\-*/×÷x])|([a-z']+))`, 'gy')
  let at = 0
  for (;;) {
    re.lastIndex = at
    const m = re.exec(s)
    if (!m || m[0].length === 0) break
    at = re.lastIndex
    if (m[1] != null) {
      const p = parseNumber(m[1])
      if (p == null) return null
      out.push({ kind: 'pct', p })
    } else if (m[3] != null) {
      const v = value(m, 2)
      if (!v) return null
      out.push({ kind: 'value', ...v })
    } else if (m[7] != null) {
      out.push({ kind: 'op', op: m[7] === '×' || m[7] === 'x' ? '*' : m[7] === '÷' ? '/' : m[7] })
    } else if (m[8] === 'x' || m[8] === 'times') {
      out.push({ kind: 'op', op: '*' })
    } else if (m[8] && (CONVERTS.has(m[8]) || currencyWord(m[8], true))) {
      // `$20 in euros`, `100 usd to eur`: a conversion or a currency said in words, which isn't a sum
      return null
    }
    // any other word is a label
  }
  return s.slice(at).trim() ? null : out
}

// left to right: * and / bind first; `+ 10%` and `- 10%` scale everything before them, as a calculator tape does
function evaluate(list: Token[]): Sum | null {
  let currency: string | undefined
  const money = (c: string | undefined): boolean => {
    if (!c) return true
    if (currency && currency !== c) return false
    currency = c
    return true
  }
  let total = 0
  let sign = 1
  let i = 0
  let first = true
  while (i < list.length) {
    let t = list[i]!
    if (!first) {
      if (t.kind !== 'op' || (t.op !== '+' && t.op !== '-')) return null
      sign = t.op === '+' ? 1 : -1
      t = list[++i]!
      if (!t) return null
    } else if (t.kind === 'op' && (t.op === '-' || t.op === '+')) {
      sign = t.op === '-' ? -1 : 1
      t = list[++i]!
      if (!t) return null
    }
    first = false
    if (t.kind === 'pct') {
      // a percentage only scales what came before it
      if (i === 0) return null
      total *= 1 + (sign * t.p) / 100
      i++
      continue
    }
    if (t.kind !== 'value' || !money(t.currency)) return null
    let term = t.n
    let termMoney = Boolean(t.currency)
    i++
    while (i < list.length) {
      const op = list[i]!
      if (op.kind !== 'op' || (op.op !== '*' && op.op !== '/')) break
      const next = list[i + 1]
      if (!next || next.kind !== 'value' || !money(next.currency)) return null
      if (op.op === '*') {
        if (termMoney && next.currency) return null
        term *= next.n
        termMoney ||= Boolean(next.currency)
      } else {
        if (next.n === 0 || (!termMoney && next.currency)) return null
        term /= next.n
        // money over money is a plain ratio
        if (termMoney && next.currency) termMoney = false
      }
      i += 2
    }
    total += sign * term
  }
  return Number.isFinite(total) ? { n: total, currency } : null
}

// `$30/hr * 40 hr`, `$15 per hour * 8 hours`, `$5 per day for 30 days`, `10 nights at $120`, `3 tickets at $25 each`
function rateTimes(s: string): Sum | null {
  const per = s.match(new RegExp(String.raw`^${VALUE}\s*(?:/|per|an?)\s*[a-z]+\s*(?:\*|x|×|times|for)\s*(${NUM})(?:\s*[a-z]+)?$`))
  if (per) {
    const v = value(per)
    const k = parseNumber(per[6] ?? '')
    return v?.currency && k != null ? { n: v.n * k, currency: v.currency } : null
  }
  const at = s.match(new RegExp(String.raw`^(${NUM})\s+[a-z]+\s+(?:at|@)\s+${VALUE}(?:\s*(?:/|per|an?)\s*[a-z]+|\s+each)?$`))
  if (at) {
    const k = parseNumber(at[1] ?? '')
    const v = value(at, 2)
    return v?.currency && k != null ? { n: v.n * k, currency: v.currency } : null
  }
  return null
}

/** Money arithmetic or a shorthand number, or null when the line has neither money nor a shorthand in it. */
export function moneyMath(s: string): Sum | null {
  const rate = rateTimes(s)
  if (rate) return rate
  const list = tokens(s)
  if (!list?.length) return null
  const values = list.filter((t) => t.kind === 'value')
  if (!values.some((t) => t.currency || t.scaled)) return null
  // `$5 $6`: two amounts with nothing between them aren't a sum
  for (let i = 1; i < list.length; i++) if (list[i]!.kind === 'value' && list[i - 1]!.kind === 'value') return null
  return evaluate(list)
}
