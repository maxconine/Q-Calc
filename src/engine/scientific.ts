import type { MathNode } from 'mathjs'
import { formatNumber, num, textVal } from './format'
import { intGcd, math, namesPattern } from './math'
import type { UserFunction, Value } from './types'

/** Cap on list sizes from `[a...b]`, `random(N)` and `randint(lo, hi, count)`. */
export const MAX_LIST_ALLOC = 10_000

export type AngleMode = 'deg' | 'rad'

export const SCIENTIFIC_NAMES =
  'sqrt|cbrt|nthroot|nthRoot|sin|cos|tan|csc|sec|cot|asin|acos|atan|atan2|arcsin|arccos|arctan|arctan2|arccsc|arcsec|arccot|sinh|cosh|tanh|csch|sech|coth|asinh|acosh|atanh|arsinh|arcosh|artanh|arcsinh|arccosh|arctanh|arccsch|arcsech|arccoth|acsch|asech|acoth|ln|log|log2|log10|exp|abs|sign|floor|ceil|round|clamp|min|max|mean|median|mad|std|stdev|stdevp|var|varp|sum|total|length|count|quartile|quantile|corr|gcd|gcf|hcf|lcm|mod|hypot|factorial|nCr|nPr|combinations|permutations|randint|rand|random|re|im|real|imag|conj|arg|range|inclusiveRange|zeta|catalan|pi|tau|inf|infinity|ans'

const NAME_RE = new RegExp(`\\b(${SCIENTIFIC_NAMES})\\b`, 'gi')
const HAS_NAME_RE = new RegExp(`\\b(${SCIENTIFIC_NAMES})\\b`, 'i')

function chop(n: number): number {
  if (!Number.isFinite(n)) return n
  if (Math.abs(n) < 1e-12) return 0
  return n
}

function isMatrix(x: unknown): x is { toArray: () => unknown } {
  return !!x && typeof x === 'object' && 'toArray' in x && typeof (x as { toArray: unknown }).toArray === 'function'
}

/** Flattens arrays and mathjs matrices; an undefined entry stays NaN, so `mean(1, 1/0)` can't quietly skip it. */
function nums(args: unknown[]): number[] {
  const out: number[] = []
  const walk = (x: unknown): void => {
    if (Array.isArray(x)) {
      x.forEach(walk)
      return
    }
    if (isMatrix(x)) {
      walk(x.toArray())
      return
    }
    out.push(Number(x))
  }
  args.forEach(walk)
  return out
}

function avg(xs: number[]): number {
  return xs.reduce((s, x) => s + x, 0) / xs.length
}

function varianceOf(xs: number[], population: boolean): number {
  const m = avg(xs)
  const d = population ? xs.length : xs.length - 1
  if (d <= 0) return Number.NaN
  return xs.reduce((s, x) => s + (x - m) ** 2, 0) / d
}

function medianOf(s: number[]): number {
  if (!s.length || s.some(Number.isNaN)) return Number.NaN
  const m = (s.length - 1) / 2
  return (s[Math.floor(m)]! + s[Math.ceil(m)]!) / 2
}

function tukeyQuartile(data: number[], q: number): number {
  if (![0, 1, 2, 3, 4].includes(q) || data.some(Number.isNaN)) return Number.NaN
  const s = [...data].sort((a, b) => a - b)
  if (q === 0 || q === 4) return s.length ? s[q === 0 ? 0 : s.length - 1]! : Number.NaN
  if (q === 2) return medianOf(s)
  const n = s.length
  const lower = n % 2 === 0 ? s.slice(0, n / 2) : s.slice(0, (n - 1) / 2)
  const upper = n % 2 === 0 ? s.slice(n / 2) : s.slice((n + 1) / 2)
  return q <= 1 ? medianOf(lower) : medianOf(upper)
}

function quantileLinear(data: number[], p: number): number {
  if (!(p >= 0 && p <= 1) || data.some(Number.isNaN)) return Number.NaN
  const s = [...data].sort((a, b) => a - b)
  if (!s.length) return Number.NaN
  const idx = (s.length - 1) * p
  const lo = Math.floor(idx)
  const hi = Math.ceil(idx)
  if (lo === hi) return s[lo]!
  return s[lo]! * (hi - idx) + s[hi]! * (idx - lo)
}

function pearson(a: number[], b: number[]): number {
  const n = Math.min(a.length, b.length)
  if (n < 2) return Number.NaN
  const ma = avg(a.slice(0, n))
  const mb = avg(b.slice(0, n))
  let num = 0
  let da = 0
  let db = 0
  for (let i = 0; i < n; i++) {
    const xa = a[i]! - ma
    const xb = b[i]! - mb
    num += xa * xb
    da += xa * xa
    db += xb * xb
  }
  const den = Math.sqrt(da * db)
  return den === 0 ? Number.NaN : num / den
}

function intLcm(a: number, b: number): number {
  if (a === 0 || b === 0) return 0
  return Math.abs(a * b) / intGcd(a, b)
}

// exact n/d for x, or null when x isn't a fraction with a small denominator (like π or √2)
function asRatio(x: number): [number, number] | null {
  for (let d = 1; d <= 10000; d++) {
    const n = Math.round(x * d)
    if (Math.abs(n / d - x) <= 1e-12 * Math.max(1, Math.abs(x))) return [n, d]
  }
  return null
}

// gcd(a/b, c/d) = gcd(a, c) / lcm(b, d) and lcm(a/b, c/d) = lcm(a, c) / gcd(b, d); non-fractions give NaN
function ratioFold(args: unknown[], kind: 'gcd' | 'lcm'): number {
  const ratios = nums(args).map(asRatio)
  if (!ratios.length || ratios.some((r) => r == null)) return Number.NaN
  const [n, d] = (ratios as [number, number][]).reduce(([n1, d1], [n2, d2]) =>
    kind === 'gcd' ? [intGcd(n1, n2), intLcm(d1, d2)] : [intLcm(n1, n2), intGcd(d1, d2)],
  )
  return n / d
}

/** Half away from zero, on the decimal digits as written: `round(1.005, 2)` is 1.01 and `round(-2.5)` is -3. */
function roundHalfAway(x: number, digits = 0): number {
  if (!Number.isInteger(digits)) return Number.NaN
  if (!Number.isFinite(x)) return x
  const shift = (v: number, k: number) => {
    const [m, e = '0'] = String(v).split('e')
    return Number(`${m}e${Number(e) + k}`)
  }
  return Math.sign(x) * shift(Math.round(shift(Math.abs(x), digits)), -digits)
}

function toRad(x: number, mode?: AngleMode): number {
  return mode === 'rad' ? x : (x * Math.PI) / 180
}

function fromRad(x: number, mode?: AngleMode): number {
  return mode === 'rad' ? x : (x * 180) / Math.PI
}

// sin and cos at a multiple of π/2 miss 0 by float noise that grows with the angle; a tiny angle's tiny sine is real
function sinCos(x: number, mode?: AngleMode): [number, number] {
  const r = toRad(x, mode)
  const noise = 1e-14 * Math.abs(r)
  const snap = (v: number) => (Math.abs(v) <= noise ? 0 : v)
  return [snap(Math.sin(r)), snap(Math.cos(r))]
}

/** Within a few float steps of an integer, which is all `0.29*100` misses 29 by. */
function nearInteger(x: number): number | null {
  const r = Math.round(x)
  return Math.abs(x - r) <= 4 * Number.EPSILON * Math.max(1, Math.abs(r)) ? r : null
}

/** Non-negative remainder; `x mod 0` is undefined. */
function modulo(a: number, b: number): number {
  if (b === 0) return Number.NaN
  // past 2^53 the float's low digits are gone, so there is no honest answer
  if (Math.abs(a) > Number.MAX_SAFE_INTEGER || Math.abs(b) > Number.MAX_SAFE_INTEGER) throw new Error('inexact mod')
  const m = Math.abs(b)
  const r = ((a % b) + m) % m
  // `1 mod 0.1` lands one float step short of 0.1
  return nearInteger(a / b) != null ? 0 : r
}

/** `b^e mod m` in exact integers. */
function powmod(b: number, e: number, m: number): number {
  if (![b, e, m].every(Number.isSafeInteger)) throw new Error('inexact mod')
  if (m === 0) return Number.NaN
  const mod = BigInt(Math.abs(m))
  let base = BigInt(b) % mod
  let out = 1n % mod
  for (let k = BigInt(e); k > 0n; k >>= 1n) {
    if (k & 1n) out = (out * base) % mod
    base = (base * base) % mod
  }
  return Number(out)
}

/** A whole `3^100 mod 7` term only, not `2*3^100 mod 7` or `3^100 mod 7^2`. */
const POW_MOD_RE = /(?<=(?:^|[(,+]|[\w)]\s*-)\s*)(\d+)\s*\^\s*(\d+)\s*mod\s*(\d+)(?![\d.]|\s*[(^!])/g

// `7 mod 3` parses as mathjs's own operator, so it needs the same convention
math.import({ mod: modulo }, { override: true })

// mathjs's own nCr overflows inside (NaN for nCr(1000, 500)) even when the answer fits a float
function combinationsFloat(n: number, k: number): number {
  const m = Math.min(k, n - k)
  // with n >= 2m the answer is at least 2^m, past any float
  if (m > 1100) return Infinity
  let r = 1
  for (let i = 1; i <= m; i++) r = (r * (n - m + i)) / i
  return r
}

function combinationsSafe(n: number, k: number): number {
  const v = Number(math.combinations(n, k))
  return Number.isNaN(v) ? combinationsFloat(n, k) : v
}

/** nCr / nPr: 0 when choosing more than there are, undefined for non-integers or negatives. */
function choose(f: (n: number, k: number) => number): (n: number, k: number) => number {
  return (n, k) => {
    if (![n, k].every(Number.isInteger) || n < 0 || k < 0) return Number.NaN
    return k > n ? 0 : Number(f(n, k))
  }
}

/** n! for integers, Γ(n+1) for other reals; negative integers are undefined. */
function factorial(n: number): number {
  if (Number.isInteger(n)) {
    if (n < 0) return Number.NaN
    if (n > 170) return Infinity
    return Number(math.factorial(n))
  }
  if (Number.isNaN(n)) return Number.NaN
  if (n === Infinity) return Infinity
  return Number(math.gamma(n + 1))
}

const WRAP_SKIP = new Set(['pi', 'tau', 'inf', 'infinity', 'ans', 'e', 'mod', 'catalan'])

/** sin^-1(x), cos^(-1)(x), tan⁻¹(x). Longer names first so sinh^-1 isn't read as sin. */
const INVERSE_POWER_FNS: [string, string][] = [
  ['csch', 'acsch'],
  ['sech', 'asech'],
  ['coth', 'acoth'],
  ['sinh', 'asinh'],
  ['cosh', 'acosh'],
  ['tanh', 'atanh'],
  ['csc', 'acsc'],
  ['sec', 'asec'],
  ['cot', 'acot'],
  ['sin', 'asin'],
  ['cos', 'acos'],
  ['tan', 'atan'],
]

const INVERSE_POWER_RES: [RegExp, string][] = INVERSE_POWER_FNS.map(([fn, inv]) => [
  new RegExp(`\\b${fn}\\s*(?:\\^\\s*(?:-1|\\(\\s*-1\\s*\\))|⁻¹)`, 'gi'),
  inv,
])

function rewriteInversePower(expr: string): string {
  let s = expr
  for (const [re, inv] of INVERSE_POWER_RES) s = s.replace(re, inv)
  return s
}

type CallableNames = { key: string; names: string[]; implicitRe: RegExp }
let callableCache: CallableNames | null = null

/** Cached per user-function list. */
function callableNames(extraNames: string[]): CallableNames {
  const key = extraNames.join('|')
  if (callableCache?.key === key) return callableCache
  const names = [...SCIENTIFIC_NAMES.split('|'), ...extraNames]
    .filter((n) => !WRAP_SKIP.has(n.toLowerCase()))
    .sort((a, b) => b.length - a.length)
  const implicitRe = new RegExp(`(\\d)(\\s*)(${names.join('|')})(?![A-Za-z_])`, 'gi')
  callableCache = { key, names, implicitRe }
  return callableCache
}

/** The argument of a bare function call (`sin 30`, `sin 2pi`); a power stays inside, so `log 10^3` is 3. */
const BARE_ATOM_RE =
  /^(?:pi|tau|e|\d+(?:\.\d+)?(?:e[+-]?\d+)?(?:\*(?:pi|tau|\(pi\)|\(tau\))(?![A-Za-z0-9_]))?)(?:\s*\^\s*(?:-?\d+(?:\.\d+)?(?![\d.])|\([^()]*\)))*/i

export function wrapBareFunctions(expr: string, extraNames: string[] = []): string {
  const { names } = callableNames(extraNames)
  let i = 0
  let out = ''
  while (i < expr.length) {
    const ch = expr[i]!
    const prev = expr[i - 1]
    const startIdent = /[A-Za-z]/.test(ch) && (i === 0 || !/[A-Za-z0-9_]/.test(prev ?? ''))
    if (startIdent) {
      const rest = expr.slice(i)
      const lower = rest.toLowerCase()
      const name = names.find((n) => {
        if (!lower.startsWith(n.toLowerCase())) return false
        const next = rest[n.length]
        return !next || !/[A-Za-z_]/.test(next)
      })
      if (name) {
        let j = i + name.length
        while (expr[j] === ' ') j++
        if (expr[j] !== '(') {
          const m = expr.slice(j).match(BARE_ATOM_RE)
          if (m) {
            out += `${name}(${m[0]})`
            i = j + m[0].length
            continue
          }
        }
        out += expr.slice(i, i + name.length)
        i += name.length
        continue
      }
    }
    out += ch
    i++
  }
  return out
}

/** ×, middle dot, dot operator and bullet operator from pasted math. */
const TYPESET_MUL = /[×·⋅∙]/g

export function rewriteTypesetMul(s: string): string {
  return s.replace(TYPESET_MUL, '*').replace(/(?<![\\A-Za-z])dot(?![A-Za-z])/gi, '*')
}

/** Typed "p i" or "p·i" is the constant π. */
export function stitchConstants(s: string): string {
  return s
    .replace(/\\imaginaryI\b/gi, 'i')
    .replace(/\\mathrm\{i\}/gi, 'i')
    .replace(/\bp(?:\s*(?:\*|·|⋅|\\cdot)\s*|\s+)i\b/gi, '(pi)')
    .replace(/(\d+(?:\.\d+)?)(\s*)\(pi\)/gi, '$1*$2(pi)')
    .replace(/(\d+(?:\.\d+)?)(pi|tau)\b/gi, '$1*$2')
}

/** Where the additive left side of `pos` begins: after the nearest unmatched `(` or same-level comma, else 0. */
function groupStart(s: string, pos: number): number {
  let depth = 0
  for (let i = pos - 1; i >= 0; i--) {
    const ch = s[i]
    if (ch === ')') depth++
    else if (ch === '(') {
      if (depth === 0) return i + 1
      depth--
    } else if (ch === ',' && depth === 0) return i + 1
  }
  return 0
}

const PERCENT_TERM_RE = /([+-])\s*(\d+(?:\.\d+)?)\s*%(?=\s*(?:[-+),]|$))/g
const ONLY_PERCENTS_RE = /^[\s+-]*(?:\d+(?:\.\d+)?\s*%[\s+-]*)+$/

/** `a ± b%` means `a * (1 ± b/100)`, where `a` is the whole additive left side (`200 + 15%` = 230). */
function rewritePercentAdd(expr: string): string {
  let s = expr
  PERCENT_TERM_RE.lastIndex = 0
  for (let m = PERCENT_TERM_RE.exec(s); m; m = PERCENT_TERM_RE.exec(s)) {
    const start = groupStart(s, m.index)
    const left = s.slice(start, m.index)
    const skip =
      !left.trim() || /[-+*/^(,]\s*$/.test(left) || /\d[eE]$/.test(left) || ONLY_PERCENTS_RE.test(left)
    if (skip) continue
    const put = `((${left.trim()})*(1${m[1]}${m[2]}/100))`
    s = s.slice(0, start) + put + s.slice(m.index + m[0].length)
    PERCENT_TERM_RE.lastIndex = start + put.length
  }
  return s
}

const SUPERSCRIPT_DIGITS = '⁻⁰¹²³⁴⁵⁶⁷⁸⁹'

export function preprocessAscii(expr: string, extraNames: string[] = []): string {
  // `√x` roots a single letter; `√2` and `√(x + 1)` already read right
  let s = expr.replace(/([√∛])([A-Za-z])(?![A-Za-z0-9_(])/g, '$1($2)')
  s = rewriteTypesetMul(s).replace(/°/g, ' deg').replace(/÷/g, '/').replace(/−/g, '-').replace(/π/g, '(pi)').replace(/τ/g, '(tau)').replace(/∞/g, 'Infinity').replace(/√/g, 'sqrt').replace(/∛/g, 'cbrt').replace(/θ/g, 'theta')
  s = s.replace(/ζ/g, 'zeta').replace(/[⁻⁰¹²³⁴⁵⁶⁷⁸⁹]+/g, (m) => `^(${[...m].map((c) => SUPERSCRIPT_DIGITS.indexOf(c) - 1).map((d) => (d < 0 ? '-' : d)).join('')})`)
  s = s.replace(/(?<![\d)\]!])\|([^|]+)\|/g, 'abs($1)')
  s = s.replace(/\*\*/g, '^')
  s = s.replace(POW_MOD_RE, 'powmod($1,$2,$3)')
  s = stitchConstants(s)
  s = s.replace(/(\d+(?:\.\d+)?)\s*%\s*of\b/gi, '($1/100)*')
  s = rewritePercentAdd(s)
  s = s.replace(/(\d+(?:\.\d+)?)\s*%/g, '($1/100)')
  s = s.replace(/\barcsin\b/g, 'asin').replace(/\barccos\b/g, 'acos').replace(/\barctan\b/g, 'atan')
  s = s.replace(/\barccsc\b/g, 'acsc').replace(/\barccot\b/g, 'acot')
  s = s.replace(/\barcsec(?=\s*\()/g, 'asec')
  s = s.replace(/\barctan2\b/g, 'atan2')
  s = rewriteInversePower(s)
  s = s.replace(/\barcsinh\b|\barsinh\b/g, 'asinh')
  s = s.replace(/\barccosh\b|\barcosh\b/g, 'acosh')
  s = s.replace(/\barctanh\b|\bartanh\b/g, 'atanh')
  s = s.replace(/\barccsch\b|\barcsch\b/g, 'acsch')
  s = s.replace(/\barcsech\b/g, 'asech')
  s = s.replace(/\barccoth\b/g, 'acoth')
  s = s.replace(/\bnthroot\b/g, 'nthRoot')
  s = s.replace(/((?:\([^()]*\)|\d+(?:\.\d+)?))\s*nCr\s*((?:\([^()]*\)|\d+(?:\.\d+)?))/gi, 'combinations($1,$2)')
  s = s.replace(/((?:\([^()]*\)|\d+(?:\.\d+)?))\s*nPr\s*((?:\([^()]*\)|\d+(?:\.\d+)?))/gi, 'permutations($1,$2)')
  s = s.replace(/\bnCr\s*\(/g, 'combinations(')
  s = s.replace(/\bnPr\s*\(/g, 'permutations(')
  // `n` can be a user function; `count` is reserved, so it always counts
  if (!extraNames.includes('n')) s = s.replace(/\bn\s*\(/g, 'length(')
  s = s.replace(/\bcount\s*\(/g, 'length(')
  s = s.replace(/\[([^\][]*?)\s*\.\.\.\s*([^\][]*?)\]/g, 'inclusiveRange($1,$2)')
  s = s.replace(/\breal\b/g, 're').replace(/\bimag\b/g, 'im')
  s = s.replace(/\blog_(\d+(?:\.\d+)?)\s*\(([^)]+)\)/g, 'log($2, $1)')
  s = s.replace(callableNames(extraNames).implicitRe, '$1*$2$3')
  s = wrapBareFunctions(s, extraNames)
  // after wrapping, so bare `log 1000` is base 10 too
  s = rewriteLog10(s)
  s = rewriteFactorial(s)
  return s
}

/** One-argument `log(...)` is base 10, however its argument nests (`log(max(10, 100))`). */
function rewriteLog10(expr: string): string {
  let s = expr
  const re = /\blog\(/g
  for (let m = re.exec(s); m; m = re.exec(s)) {
    const open = m.index + 3
    let depth = 0
    let commas = 0
    let close = -1
    for (let i = open; i < s.length && close < 0; i++) {
      if (s[i] === '(' || s[i] === '[') depth++
      else if (s[i] === ')' || s[i] === ']') {
        if (--depth === 0) close = i
      } else if (s[i] === ',' && depth === 1) commas++
    }
    if (close < 0 || commas > 0) continue
    s = `${s.slice(0, m.index)}log10${s.slice(open)}`
    re.lastIndex = m.index + 5
  }
  return s
}

function assertListAlloc(count: number): void {
  if (!Number.isFinite(count) || count < 0 || count > MAX_LIST_ALLOC) {
    throw new Error('list allocation exceeds limit')
  }
}

/** Index of the bracket that opens the group closing at `close`, or -1. */
function openingIndex(s: string, close: number, openCh: string, closeCh: string): number {
  let depth = 0
  for (let i = close; i >= 0; i--) {
    if (s[i] === closeCh) depth++
    else if (s[i] === openCh && --depth === 0) return i
  }
  return -1
}

function rewriteFactorial(expr: string): string {
  let s = expr
  for (let guard = 0; guard < 32; guard++) {
    let idx = -1
    for (let i = 0; i < s.length; i++) {
      if (s[i] === '!' && s[i + 1] !== '=') idx = i
    }
    if (idx < 0) break
    let j = idx - 1
    while (j >= 0 && s[j] === ' ') j--
    if (j < 0) break
    let start = j
    if (s[j] === ')') {
      start = openingIndex(s, j, '(', ')')
      // `sqrt(4)!` takes the whole call, not just its parentheses
      let k = start
      while (k > 0 && /[A-Za-z0-9_]/.test(s[k - 1]!)) k--
      while (k < start && /\d/.test(s[k]!)) k++
      if (k < start) start = k
    } else if (s[j] === ']') {
      start = openingIndex(s, j, '[', ']')
    } else if (/[0-9.]/.test(s[j]!)) {
      // a sign belongs to the number only inside an exponent (`1e+5!`), never `2+3!`
      while (start > 0 && (/[0-9.eE]/.test(s[start - 1]!) || (/[+-]/.test(s[start - 1]!) && /\d[eE]$/.test(s.slice(0, start - 1))))) start--
    } else if (/[A-Za-z_]/.test(s[j]!)) {
      while (start > 0 && /[A-Za-z0-9_]/.test(s[start - 1]!)) start--
    } else {
      break
    }
    if (start < 0) break
    const operand = s.slice(start, j + 1)
    s = `${s.slice(0, start)}factorial(${operand})${s.slice(idx + 1)}`
  }
  return s
}

const MAX_LIST_SHOWN = 16

function formatList(values: number[]): string {
  if (values.length <= MAX_LIST_SHOWN) {
    return `[${values.map((v) => formatNumber(v)).join(', ')}]`
  }
  const head = values.slice(0, 6).map((v) => formatNumber(v))
  const tail = values.slice(-4).map((v) => formatNumber(v))
  return `[${head.join(', ')}, …, ${tail.join(', ')}]`
}

function snapInt(n: number): number {
  if (!Number.isFinite(n) || n === 0) return n
  const r = Math.round(n)
  if (r === 0) return n
  const err = Math.abs(n - r)
  if (err <= Number.EPSILON * Math.max(1, Math.abs(r))) return r
  return n
}

function fromMathjs(v: unknown, overflowOk: boolean): Value | null {
  if (typeof v === 'number') {
    if (Number.isNaN(v)) return textVal('undefined')
    return num(snapInt(v))
  }
  if (typeof v === 'boolean') return num(v ? 1 : 0)
  if (v && typeof v === 'object' && 're' in v && 'im' in v) {
    const re = Number((v as { re: number }).re)
    const im = Number((v as { im: number }).im)
    if (Math.abs(im) > 1e-12) return textVal('undefined')
    if (!Number.isFinite(re)) return textVal('undefined')
    return num(chop(re))
  }
  if (Array.isArray(v) || isMatrix(v)) {
    return textVal(formatList(nums([v]).map((n) => (Number.isFinite(n) || overflowOk ? n : Number.NaN))))
  }
  return null
}

export type ScientificContext = {
  ans?: number
  angleMode?: AngleMode
  variables?: Record<string, number>
  functions?: Record<string, UserFunction>
}

function prepare(text: string, ctx: ScientificContext): { expr: string; scope: Record<string, unknown> } | null {
  const expr = preprocessChecked(text, ctx)
  if (expr == null) return null
  const scope: Record<string, unknown> = {
    ...ctx.variables,
    // with no previous answer `ans` stays unknown (blank), not 0
    ...(ctx.ans !== undefined && { ans: ctx.ans }),
    ...builtinScope(ctx.angleMode),
  }
  const fns = ctx.functions ?? {}
  for (const [name, def] of Object.entries(fns)) scope[name] = userFunction(def, ctx, fns)
  return { expr, scope }
}

export function preprocessChecked(text: string, ctx: ScientificContext): string | null {
  let src = text.trim()
  if (!src) return null
  const fns = ctx.functions ?? {}
  const fnNames = Object.keys(fns)
  const vars = ctx.variables ?? {}
  // a variable followed by `(` multiplies (`n(2+3)`), unless a function has the same name
  const varNames = Object.keys(vars).filter((n) => !(n in fns))
  if (varNames.length) {
    src = src.replace(new RegExp(`(?<![A-Za-z0-9_])(${namesPattern(varNames)})\\s*\\(`, 'g'), '$1*(')
  }
  let expr = preprocessAscii(src, fnNames)
  if (ctx.ans !== undefined) expr = expr.replace(/\bans\b/gi, `(${ctx.ans})`)

  const allowNames = [...Object.keys(vars), ...fnNames]
  const allowRe = allowNames.length
    ? new RegExp(`(?<![A-Za-z_])(?:${namesPattern(allowNames)})(?![A-Za-z0-9_])`, 'gi')
    : null
  const leftover = expr
    .replace(NAME_RE, '')
    .replace(/\d+(?:\.\d+)?e[+-]?\d+/gi, '')
    .replace(allowRe ?? /$^/, '')
  const named = HAS_NAME_RE.test(src) || HAS_NAME_RE.test(expr)
  if (!named && !allowRe && !/\d|pi|tau|ans|\be\b/i.test(expr)) return null
  if (/[a-zA-Z_$€£¥₹#?=\\]/.test(leftover.replace(/[iIeE]/g, ''))) {
    if (!/^[\d\s+\-*/^().,eE!|[\]:]+$/.test(expr) && !named) return null
  }

  // `2:30` is a clock reading (h:mm or m:ss, ambiguous), never a mathjs range
  if (expr.includes(':')) return null
  // `pi = 3` would be a mathjs assignment, `3!!` reads as a double factorial as often as (3!)!,
  // and `17 % 5` is a remainder to a programmer, not 17% times 5
  if (/(?<![=<>!])=(?!=)/.test(expr) || /!\s*!/.test(src) || /%\s*[\d.(]/.test(src)) return null
  return expr
}

const gcd = (...a: unknown[]) => ratioFold(a, 'gcd')
const combinations = choose(combinationsSafe)
const permutations = choose(math.permutations)

const ANGLE_FNS = new Set(['sin', 'cos', 'tan', 'csc', 'sec', 'cot'])

function hasUnit(x: unknown): boolean {
  if (math.isUnit(x)) return true
  if (Array.isArray(x)) return x.some(hasUnit)
  return isMatrix(x) && hasUnit(x.toArray())
}

// mathjs reads `90 deg` or `5 m` inside a call as a unit; only trig takes one (as an angle), anything else is blank
function unitSafe(name: string, fn: (...a: unknown[]) => unknown, mode: AngleMode | undefined) {
  return (...args: unknown[]) => {
    if (!args.some(hasUnit)) return fn(...args)
    const [angle] = args
    if (ANGLE_FNS.has(name) && args.length === 1 && math.isUnit(angle)) return fn(angle.toNumber(mode === 'rad' ? 'rad' : 'deg'))
    throw new Error('unit in a scalar function')
  }
}

export function builtinScope(mode: AngleMode | undefined): Record<string, unknown> {
  const scope = scalarScope(mode)
  for (const [name, v] of Object.entries(scope)) {
    if (typeof v === 'function') scope[name] = unitSafe(name, v as (...a: unknown[]) => unknown, mode)
  }
  return scope
}

function scalarScope(mode: AngleMode | undefined): Record<string, unknown> {
  const inverseTrig = {
    asin: (x: number) => (x < -1 || x > 1 ? Number.NaN : fromRad(Math.asin(x), mode)),
    acos: (x: number) => (x < -1 || x > 1 ? Number.NaN : fromRad(Math.acos(x), mode)),
    atan: (x: number) => fromRad(Math.atan(x), mode),
    atan2: (y: number, x: number) => fromRad(Math.atan2(y, x), mode),
    acsc: (x: number) => (Math.abs(x) < 1 ? Number.NaN : fromRad(Math.asin(1 / x), mode)),
    asec: (x: number) => (Math.abs(x) < 1 ? Number.NaN : fromRad(Math.acos(1 / x), mode)),
    acot: (x: number) => fromRad(x === 0 ? Math.PI / 2 : Math.atan(1 / x), mode),
  }
  return {
    e: Math.E,
    pi: Math.PI,
    catalan: 0.915965594177219,
    zeta: (x: number) => Number(math.zeta(x)),
    tau: Math.PI * 2,
    ln: (x: number) => (x <= 0 ? Number.NaN : Math.log(x)),
    log2: (x: number) => (x <= 0 ? Number.NaN : Math.log2(x)),
    log10: (x: number) => (x <= 0 ? Number.NaN : Math.log10(x)),
    log: math.log,
    exp: Math.exp,
    sqrt: (x: number) => (x < 0 ? Number.NaN : Math.sqrt(x)),
    cbrt: Math.cbrt,
    nthroot: math.nthRoot,
    nthRoot: math.nthRoot,
    abs: Math.abs,
    sign: Math.sign,
    floor: (x: number) => nearInteger(x) ?? Math.floor(x),
    ceil: (x: number) => nearInteger(x) ?? Math.ceil(x),
    round: roundHalfAway,
    clamp: (x: number, lo: number, hi: number) => {
      const a = Number(lo)
      const b = Number(hi)
      const v = Number(x)
      if (![v, a, b].every(Number.isFinite)) return Number.NaN
      const min = Math.min(a, b)
      const max = Math.max(a, b)
      return Math.min(Math.max(v, min), max)
    },
    min: (...a: unknown[]) => Math.min(...nums(a)),
    max: (...a: unknown[]) => Math.max(...nums(a)),
    mean: (...a: unknown[]) => avg(nums(a)),
    median: (...a: unknown[]) => medianOf([...nums(a)].sort((x, y) => x - y)),
    stdev: (...a: unknown[]) => Math.sqrt(varianceOf(nums(a), false)),
    stdevp: (...a: unknown[]) => Math.sqrt(varianceOf(nums(a), true)),
    var: (...a: unknown[]) => varianceOf(nums(a), false),
    varp: (...a: unknown[]) => varianceOf(nums(a), true),
    mad: (...a: unknown[]) => {
      const xs = nums(a)
      const m = avg(xs)
      return xs.reduce((p, c) => p + Math.abs(c - m), 0) / xs.length
    },
    total: (...a: unknown[]) => nums(a).reduce((p, c) => p + c, 0),
    length: (...a: unknown[]) => nums(a).length,
    quartile: (...a: unknown[]) => {
      const xs = nums(a)
      const q = xs.pop()
      return tukeyQuartile(xs, q ?? 2)
    },
    quantile: (...a: unknown[]) => {
      const xs = nums(a)
      const p = xs.pop()
      return quantileLinear(xs, p ?? 0.5)
    },
    corr: (...a: unknown[]) => {
      if (a.length === 2) {
        const [xs, ys] = [nums([a[0]]), nums([a[1]])]
        return xs.length === ys.length ? pearson(xs, ys) : Number.NaN
      }
      const xs = nums(a)
      const mid = xs.length / 2
      return Number.isInteger(mid) ? pearson(xs.slice(0, mid), xs.slice(mid)) : Number.NaN
    },
    random: (...a: unknown[]) => {
      const xs = nums(a)
      if (xs.length === 0) return Math.random()
      if (xs.length === 1 && xs[0]! >= 2 && Number.isInteger(xs[0])) {
        assertListAlloc(xs[0]!)
        return Array.from({ length: xs[0]! }, () => Math.random())
      }
      if (xs.length >= 2) return xs[0]! + Math.random() * (xs[1]! - xs[0]!)
      return Math.random() * xs[0]!
    },
    randint: (...a: unknown[]) => {
      const xs = nums(a)
      if (!xs.every(Number.isInteger)) return Number.NaN
      const [a0 = 0, a1 = a0, count] = xs
      const lo = Math.min(a0, a1)
      const hi = Math.max(a0, a1)
      const one = () => lo + Math.floor(Math.random() * (hi - lo + 1))
      if (count == null) return one()
      assertListAlloc(count)
      return Array.from({ length: count }, one)
    },
    gcd,
    gcf: gcd,
    hcf: gcd,
    lcm: (...a: unknown[]) => ratioFold(a, 'lcm'),
    mod: modulo,
    powmod,
    factorial,
    combinations,
    permutations,
    nCr: combinations,
    nPr: permutations,
    inclusiveRange: (a: number, b: number) => {
      const start = Number(a)
      const end = Number(b)
      if (![start, end].every(Number.isFinite)) throw new Error('invalid range')
      const step = start <= end ? 1 : -1
      const count = Math.floor(Math.abs(end - start)) + 1
      assertListAlloc(count)
      const out: number[] = []
      for (let i = start; step > 0 ? i <= end : i >= end; i += step) out.push(i)
      return out
    },
    csc: (x: number) => {
      const [s] = sinCos(x, mode)
      return s === 0 ? Number.NaN : 1 / s
    },
    sec: (x: number) => {
      const [, c] = sinCos(x, mode)
      return c === 0 ? Number.NaN : 1 / c
    },
    cot: (x: number) => {
      const [s, c] = sinCos(x, mode)
      return s === 0 ? Number.NaN : c / s
    },
    sin: (x: number) => sinCos(x, mode)[0],
    cos: (x: number) => sinCos(x, mode)[1],
    tan: (x: number) => {
      const [s, c] = sinCos(x, mode)
      return c === 0 ? Number.NaN : s / c
    },
    ...inverseTrig,
    arcsin: inverseTrig.asin,
    arccos: inverseTrig.acos,
    arctan: inverseTrig.atan,
    arctan2: inverseTrig.atan2,
    arccsc: inverseTrig.acsc,
    arcsec: inverseTrig.asec,
    arccot: inverseTrig.acot,
    sinh: Math.sinh,
    cosh: Math.cosh,
    tanh: Math.tanh,
    csch: (x: number) => (x === 0 ? Number.NaN : 1 / Math.sinh(x)),
    sech: (x: number) => 1 / Math.cosh(x),
    coth: (x: number) => (x === 0 ? Number.NaN : 1 / Math.tanh(x)),
    asinh: Math.asinh,
    acosh: (x: number) => (x < 1 ? Number.NaN : Math.acosh(x)),
    atanh: (x: number) => (x <= -1 || x >= 1 ? Number.NaN : Math.atanh(x)),
    acsch: (x: number) => (x === 0 ? Number.NaN : Math.asinh(1 / x)),
    asech: (x: number) => (x <= 0 || x > 1 ? Number.NaN : Math.acosh(1 / x)),
    acoth: (x: number) => (Math.abs(x) <= 1 ? Number.NaN : Math.atanh(1 / x)),
  }
}

function userFunction(def: UserFunction, ctx: ScientificContext, fns: Record<string, UserFunction>) {
  return (...args: unknown[]) => {
    if (args.some(hasUnit)) throw new Error('unit in a user function')
    const localVars: Record<string, number> = { ...ctx.variables }
    def.params.forEach((p, i) => {
      const n = Number(args[i])
      localVars[p] = Number.isFinite(n) ? n : Number.NaN
    })
    const result = evalScientific(def.body, { ans: ctx.ans, angleMode: ctx.angleMode, variables: localVars, functions: fns })
    if (!result || result.kind === 'text') return Number.NaN
    return result.n
  }
}

/** Only factorials and counts may overflow to ∞; any other infinity (`1/0`) is undefined. */
function finish(v: unknown, expr: string): Value | null {
  const overflowOk = /!|factorial|combinations|permutations/i.test(expr)
  const out = fromMathjs(v, overflowOk)
  if (out?.kind === 'number' && !Number.isFinite(out.n) && !overflowOk) return textVal('undefined')
  return out
}

export function evalScientific(text: string, ctx: ScientificContext = {}): Value | null {
  const prep = prepare(text, ctx)
  if (!prep) return null
  try {
    return finish(math.evaluate(prep.expr, prep.scope), prep.expr)
  } catch {
    return null
  }
}

/** Same results as evalScientific with `variable` set, but compiled once for graph sampling. */
export function compileScientific(
  text: string,
  ctx: ScientificContext,
  variable: string,
): ((value: number) => Value | null) | null {
  const variables: Record<string, number> = { ...ctx.variables, [variable]: 0 }
  const prep = prepare(text, { ...ctx, variables })
  if (!prep) return null
  let code: { evaluate: (scope: Record<string, unknown>) => unknown }
  try {
    code = math.parse(prep.expr).compile()
  } catch {
    return null
  }
  const { expr, scope } = prep
  return (value) => {
    variables[variable] = value
    scope[variable] = value
    try {
      return finish(code.evaluate(scope), expr)
    } catch {
      return null
    }
  }
}

/** The tree compileScientific evaluates, with the scope it evaluates in, for walking its shape. */
export function parseScientific(
  text: string,
  ctx: ScientificContext,
  variable: string,
): { node: MathNode; scope: Record<string, unknown> } | null {
  const prep = prepare(text, { ...ctx, variables: { ...ctx.variables, [variable]: 0 } })
  if (!prep) return null
  try {
    return { node: math.parse(prep.expr), scope: prep.scope }
  } catch {
    return null
  }
}

export function formatAsFraction(n: number, maxDen = 10_000): string | null {
  if (!Number.isFinite(n)) return null
  const sign = n < 0 ? '-' : ''
  const x = Math.abs(n)
  if (Math.abs(x - Math.round(x)) < 1e-12) return `${sign}${Math.round(x)}`
  let bestN = 1
  let bestD = 1
  let bestErr = Infinity
  for (let d = 1; d <= maxDen; d++) {
    const nm = Math.round(x * d)
    const err = Math.abs(x - nm / d)
    if (err < bestErr - 1e-18) {
      bestErr = err
      bestN = nm
      bestD = d
      if (err < 1e-15) break
    }
  }
  // only exact-looking values; an approximation like 355/113 for π stays decimal
  if (bestErr > 1e-9 * Math.max(1, x)) return null
  const g = intGcd(bestN, bestD)
  return `${sign}${bestN / g}/${bestD / g}`
}
