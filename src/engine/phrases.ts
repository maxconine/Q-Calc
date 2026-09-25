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

function percentPhrase(s: string, sigFigs: number): PhraseAnswer | null {
  let m = match(s, String.raw`${PCT} off (.+)`)
  if (m) {
    const p = parseNumber(m[1])
    const base = parseMoney(m[2])
    if (p == null || !base || p > 100) return null
    return amount(base.n * (1 - p / 100), base.currency, sigFigs)
  }
  m = match(s, String.raw`${PCT} (on|of) (.+)`)
  if (m) {
    const p = parseNumber(m[1])
    const base = parseMoney(m[3])
    if (p == null || !base) return null
    return amount(base.n * (m[2] === 'on' ? 1 + p / 100 : p / 100), base.currency, sigFigs)
  }
  m = match(s, String.raw`(.+?)${LABEL}\s*([+-])\s*${PCT}`)
  if (m) {
    const base = parseMoney(m[1])
    const p = parseNumber(m[3])
    if (!base || p == null) return null
    const n = base.n * (m[2] === '+' ? 1 + p / 100 : 1 - p / 100)
    return amount(n, base.currency, sigFigs)
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
