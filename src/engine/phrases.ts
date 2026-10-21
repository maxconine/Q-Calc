// natural-language phrases soulvercore answers on the mac, for hosts without it.
// every pattern matches the whole line; anything looser stays blank

import {
  convertCurrency,
  currencyWord,
  type CurrencyRates,
  formatMoney,
  type Money,
  NUM,
  parseMoney,
  parseNumber,
} from './currency'
import { evaluateLine } from './evaluate'
import { DEFAULT_SIG_FIGS, formatNumber } from './format'
import { answerTimePhrase } from './phraseTime'

export type PhraseAnswer = { display: string; n?: number }
export type PhraseOptions = { now?: Date; rates?: CurrencyRates | null; sigFigs?: number }

const PCT = String.raw`(${NUM})\s*%`
const LABEL = String.raw`(?:\s+(?:for\s+)?(?:lunch|dinner|breakfast|brunch|drinks|coffee))?`

function normalize(expr: string): string {
  return expr
    .trim()
    .toLowerCase()
    .replace(/[−–]/g, '-')
    .replace(/\s+/g, ' ')
    .replace(/\s*[?=]$/, '')
    .replace(/^(?:what is|what's|whats)\s+/, '')
    .replace(/(\d)\s*(?:percentage|per\s?cent|percent)\b/g, '$1%')
    .replace(/\b(?:percentage|per\s?cent|percent)\b/g, '%')
    .replace(/\bplus\b/g, '+')
    .replace(/\bminus\b/g, '-')
    .trim()
}

function sameCurrency(a: Money, b: Money): string | undefined | null {
  if (a.currency && b.currency && a.currency !== b.currency) return null
  return a.currency ?? b.currency
}

function amount(n: number, currency: string | undefined, sigFigs: number): PhraseAnswer | null {
  if (!Number.isFinite(n)) return null
  if (!currency) return { display: formatNumber(n, sigFigs), n }
  const display = formatMoney(n, currency)
  return { display, n: Number(display.replace(/[^\d.-]/g, '')) }
}

function match(s: string, pattern: string): RegExpMatchArray | null {
  return s.match(new RegExp(`^${pattern}$`))
}

// `$60 + 18% tip`, `$10 for lunch + 15% tip`, `60 with a 20% tip`
function tipTotal(s: string): Money | null {
  const m = match(s, String.raw`(.+?)${LABEL}\s*(?:\+|with(?: a)?|and(?: a)?)\s*${PCT}\s+tip`)
  if (!m) return null
  const base = parseMoney(m[1])
  const p = parseNumber(m[2])
  return base && p != null ? { n: base.n * (1 + p / 100), currency: base.currency } : null
}

// a quantity with a unit (`50 kg`, `3.5 km`), which the percent phrases take as well as money. the unit stays as
// typed (the engine would answer a bare quantity in the default unit); the engine only says it is one
function quantity(text: string): { n: number; unit: string } | null {
  const m = /^(-?\d*\.?\d+(?:e[+-]?\d+)?)\s*([a-zµ°][a-zµ°²³/^0-9]*(?: [a-zµ°][a-zµ°²³/^0-9]*)?)$/i.exec(text.trim())
  if (!m) return null
  const v = evaluateLine(text).value
  return v?.kind === 'number' && v.unit ? { n: Number(m[1]), unit: m[2]! } : null
}

// the base scaled by `factor`: money as money, a quantity in its own unit
function scaledBase(text: string, factor: number, sigFigs: number): PhraseAnswer | null {
  const money = parseMoney(text)
  if (money) return amount(money.n * factor, money.currency, sigFigs)
  const q = quantity(text)
  if (!q) return null
  const n = q.n * factor
  return { display: `${formatNumber(n, sigFigs)} ${q.unit}`, n }
}

function percentPhrase(s: string, sigFigs: number): PhraseAnswer | null {
  let m = match(s, String.raw`${PCT} off (.+)`)
  if (m) {
    const p = parseNumber(m[1])
    if (p == null || p > 100) return null
    return scaledBase(m[2], 1 - p / 100, sigFigs)
  }
  m = match(s, String.raw`${PCT} (on|of) (.+)`)
  if (m) {
    const p = parseNumber(m[1])
    if (p == null) return null
    return scaledBase(m[3], m[2] === 'on' ? 1 + p / 100 : p / 100, sigFigs)
  }
  m = match(s, String.raw`(.+?)${LABEL}\s*([+-])\s*${PCT}`)
  if (m) {
    const p = parseNumber(m[3])
    if (p == null) return null
    return scaledBase(m[1], m[2] === '+' ? 1 + p / 100 : 1 - p / 100, sigFigs)
  }
  // part and whole, in any of the three usual orders
  const asked = match(s, String.raw`what % of (.+?) is (.+)`)
  const stated = match(s, String.raw`(.+?) is what % of (.+)`) ?? match(s, String.raw`(.+?) as (?:an? )?% of (.+)`)
  const ratio = asked ? { whole: asked[1], part: asked[2] } : stated ? { whole: stated[2], part: stated[1] } : null
  if (ratio) {
    const whole = parseMoney(ratio.whole)
    const part = parseMoney(ratio.part)
    if (!whole || !part || whole.n === 0 || sameCurrency(whole, part) === null) return null
    return percent((part.n / whole.n) * 100, sigFigs)
  }
  m = match(s, String.raw`% (?:change|difference) from (.+?) to (.+)`)
  if (m) {
    const from = parseMoney(m[1])
    const to = parseMoney(m[2])
    if (!from || !to || from.n === 0 || sameCurrency(from, to) === null) return null
    return percent(((to.n - from.n) / from.n) * 100, sigFigs)
  }
  return null
}

// `60 split 3 ways`, `$60 / 3 people`, `$60 + 18% tip split 3 ways`
function splitPhrase(s: string, sigFigs: number): PhraseAnswer | null {
  const m =
    match(s, String.raw`(.+?) split (?:(\d+) ways|(?:between|among) (\d+)(?: people)?)(?: per person)?`) ??
    match(s, String.raw`(.+?) (?:/|÷|divided by) (\d+) (?:people|persons|ways)(?: per person)?`) ??
    match(s, String.raw`(.+?) (?:/|÷|divided by) (\d+) per person`) ??
    match(s, String.raw`(.+?) for (\d+) people per person`)
  if (!m) return null
  const ways = Number(m[2] ?? m[3])
  if (!Number.isInteger(ways) || ways < 1 || ways > 1_000_000) return null
  const head = parseMoney(m[1]) ?? tipTotal(m[1])
  return head ? amount(head.n / ways, head.currency, sigFigs) : null
}

// `ans` would have to guess between 25 and 0.25, so a percentage carries no number
function percent(n: number, sigFigs: number): PhraseAnswer | null {
  return Number.isFinite(n) ? { display: `${formatNumber(n, sigFigs)}%` } : null
}

// `$20 in euros`, `20 usd to gbp`, `£5 as yen`
function conversionPhrase(s: string, rates: CurrencyRates | null | undefined): PhraseAnswer | null {
  const m = s.match(/^(.+?)\s+(?:in|to|as|into)\s+(.+)$/)
  if (!m) return null
  const to = currencyWord(m[2], true)
  if (!to) return null
  const from = parseMoney(m[1], true)
  if (!from?.currency) return null
  const n = from.currency === to ? from.n : convertCurrency(from.n, from.currency, to, rates)
  return n == null ? null : amount(n, to, DEFAULT_SIG_FIGS)
}

export function answerPhrase(expr: string, options: PhraseOptions = {}): PhraseAnswer | null {
  const s = normalize(expr)
  if (!s || s.length > 120) return null
  const sigFigs = options.sigFigs ?? DEFAULT_SIG_FIGS
  // first: `7 plus 2` would otherwise read as an amount of money and stop here
  const said = wordMath(s, expr, sigFigs)
  if (said) return said

  const money = parseMoney(s)
  if (money?.currency) return amount(money.n, money.currency, sigFigs)
  if (money) return null

  const tip = tipTotal(s)
  if (tip) return amount(tip.n, tip.currency, sigFigs)
  return (
    splitPhrase(s, sigFigs) ??
    percentPhrase(s, sigFigs) ??
    conversionPhrase(s, options.rates) ??
    answerTimePhrase(s, options.now ?? new Date())
  )
}

// arithmetic said in words: `7 plus 2`, `10 divided by 4`, `3 squared`, `2 to the power of 10`, `square root of 81`,
// `half of 50`, `double 21`, `5 choose 2`. the words become symbols, and the line answers only when what they make
// is a plain number; `plus` and `minus` are symbols already (normalize)
const WHOLE_WORDS: Array<[RegExp, string]> = [
  [/^(\S+) choose (\S+)$/, 'nCr($1, $2)'],
  [/^(?:the )?square of (.+)$/, '($1)^2'],
  [/^(?:the )?cube of (.+)$/, '($1)^3'],
  [/^half of (.+)$/, '(1/2)*($1)'],
  [/^(?:a|one) third of (.+)$/, '(1/3)*($1)'],
  [/^(?:a|one) quarter of (.+)$/, '(1/4)*($1)'],
  [/^(?:double|twice) (.+)$/, '2*($1)'],
  [/^triple (.+)$/, '3*($1)'],
]
const OPERATOR_WORDS: Array<[RegExp, string]> = [
  [/\b(?:the )?square root of\b/g, 'sqrt '],
  [/\b(?:the )?cube root of\b/g, 'cbrt '],
  [/\b(?:to the power of|raised to the power of|raised to)\b/g, '^'],
  [/\bto the (\d+)(?:st|nd|rd|th)(?: power)?\b/g, '^$1'],
  [/\bsquared\b/g, '^2'],
  [/\bcubed\b/g, '^3'],
  [/\b(?:divided by|over)\b/g, '/'],
  [/\b(?:multiplied by|times)\b/g, '*'],
  [/\b(?:added to)\b/g, '+'],
  [/\bmodulo\b/g, 'mod'],
]

// only a line that said one of these: anything typed in symbols is js's to answer, with the user's own settings
const SAID = /\b(?:plus|minus|divided|over|times|multiplied|added|squared|cubed|power|raised|root|modulo|half|third|quarter|double|twice|triple|choose|square|cube|\d+(?:st|nd|rd|th))\b/i

function wordMath(s: string, raw: string, sigFigs: number): PhraseAnswer | null {
  if (!SAID.test(raw)) return null
  let text = s
  for (const [re, to] of WHOLE_WORDS) text = text.replace(re, to)
  for (const [re, to] of OPERATOR_WORDS) text = text.replace(re, to)
  if (/[a-z]{2,}/.test(text.replace(/\b(?:sqrt|cbrt|nCr|mod)\b/gi, ''))) return null
  const v = evaluateLine(text, { sigFigs }).value
  if (v?.kind !== 'number' || v.unit || !Number.isFinite(v.n)) return null
  return { display: formatNumber(v.n, sigFigs), n: v.n }
}
