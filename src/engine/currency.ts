// money parsing and conversion for the phrase layer; rates are always handed in, never fetched here

// one unit of `base` buys `rates[code]` of each currency
export type CurrencyRates = { base: string; rates: Record<string, number> }

export type Money = { n: number; currency?: string }

// only major codes: a wider list starts colliding with unit names
const CODES = new Set([
  'USD', 'EUR', 'GBP', 'JPY', 'CAD', 'AUD', 'CHF', 'CNY', 'INR', 'MXN', 'NZD', 'SEK', 'NOK', 'DKK',
  'HKD', 'SGD', 'KRW', 'BRL', 'ZAR', 'PLN', 'CZK', 'HUF', 'TRY', 'ILS', 'THB',
])

const SYMBOLS: Record<string, string> = { $: 'USD', '€': 'EUR', '£': 'GBP', '¥': 'JPY', '₹': 'INR' }
const SHOWN_SYMBOL: Record<string, string> = { USD: '$', EUR: '€', GBP: '£', JPY: '¥', INR: '₹' }
const NO_DECIMALS = new Set(['JPY', 'KRW', 'HUF'])

const WORDS: Record<string, string> = {
  dollar: 'USD',
  dollars: 'USD',
  euro: 'EUR',
  euros: 'EUR',
  yen: 'JPY',
  yuan: 'CNY',
  renminbi: 'CNY',
  rupee: 'INR',
  rupees: 'INR',
  sterling: 'GBP',
}
// pounds are also a weight, so they only count as money next to another currency
const POUNDS: Record<string, string> = { pound: 'GBP', pounds: 'GBP' }

export const NUM = String.raw`(?:\d{1,3}(?:,\d{3})+|\d+)(?:\.\d+)?|\.\d+`

export function parseNumber(s: string): number | null {
  const t = s.trim()
  if (!new RegExp(`^(?:${NUM})$`).test(t)) return null
  const n = Number(t.replace(/,/g, ''))
  return Number.isFinite(n) ? n : null
}

export function currencyWord(s: string, pounds = false): string | null {
  const t = s.trim()
  if (SYMBOLS[t]) return SYMBOLS[t]
  const up = t.toUpperCase()
  if (/^[a-z]{3}$/i.test(t) && CODES.has(up)) return up
  const low = t.toLowerCase().replace(/^us\s+(?=dollars?$)/, '')
  return WORDS[low] ?? (pounds ? (POUNDS[low] ?? null) : null)
}

// `80`, `$80`, `€20`, `20€`, `20 usd`, `20 dollars`
export function parseMoney(s: string, pounds = false): Money | null {
  const t = s.trim()
  const pre = t.match(new RegExp(`^([$€£¥₹])\\s?(${NUM})$`))
  if (pre) {
    const n = parseNumber(pre[2])
    return n == null ? null : { n, currency: SYMBOLS[pre[1]] }
  }
  const post = t.match(new RegExp(`^(${NUM})\\s?([$€£¥₹])$`))
  if (post) {
    const n = parseNumber(post[1])
    return n == null ? null : { n, currency: SYMBOLS[post[2]] }
  }
  const worded = t.match(new RegExp(`^(${NUM})\\s+([a-z]+(?:\\s[a-z]+)?)$`, 'i'))
  if (worded) {
    const n = parseNumber(worded[1])
    const currency = currencyWord(worded[2], pounds)
    return n == null || !currency ? null : { n, currency }
  }
  const n = parseNumber(t)
  return n == null ? null : { n }
}

function rateOf(code: string, table: CurrencyRates): number | null {
  if (code === table.base.toUpperCase()) return 1
  const r = table.rates[code] ?? table.rates[code.toLowerCase()]
  return typeof r === 'number' && Number.isFinite(r) && r > 0 ? r : null
}

export function convertCurrency(amount: number, from: string, to: string, table: CurrencyRates | null | undefined): number | null {
  if (!table || !Number.isFinite(amount)) return null
  const a = rateOf(from.toUpperCase(), table)
  const b = rateOf(to.toUpperCase(), table)
  if (a == null || b == null) return null
  const out = (amount / a) * b
  return Number.isFinite(out) ? out : null
}

export function isRatesTable(v: unknown): v is CurrencyRates {
  if (!v || typeof v !== 'object') return false
  const o = v as Record<string, unknown>
  return typeof o.base === 'string' && Boolean(o.rates) && typeof o.rates === 'object'
}

export function formatMoney(n: number, currency: string): string {
  const dp = NO_DECIMALS.has(currency) ? 0 : 2
  const fixed = Math.abs(n).toFixed(dp)
  const sign = n < 0 && Number(fixed) !== 0 ? '-' : ''
  const sym = SHOWN_SYMBOL[currency]
  return sym ? `${sign}${sym}${fixed}` : `${sign}${fixed} ${currency}`
}
