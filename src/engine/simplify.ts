import { intGcd } from './math'

const NICE_DEN = new Set([2, 3, 4, 5, 6, 8, 12])

const NESTED: ReadonlyArray<readonly [string, number]> = [
  ['(sqrt(6)-sqrt(2))/4', (Math.sqrt(6) - Math.sqrt(2)) / 4],
  ['(sqrt(6)+sqrt(2))/4', (Math.sqrt(6) + Math.sqrt(2)) / 4],
  ['-(sqrt(6)-sqrt(2))/4', -(Math.sqrt(6) - Math.sqrt(2)) / 4],
  ['-(sqrt(6)+sqrt(2))/4', -(Math.sqrt(6) + Math.sqrt(2)) / 4],
  ['2-sqrt(3)', 2 - Math.sqrt(3)],
  ['2+sqrt(3)', 2 + Math.sqrt(3)],
  ['-(2-sqrt(3))', -(2 - Math.sqrt(3))],
  ['-(2+sqrt(3))', -(2 + Math.sqrt(3))],
]

// an exact form claims every digit the decimal shows, so it may only differ by float noise
function tol(x: number): number {
  return 1e-11 + 1e-14 * Math.abs(x)
}

function close(a: number, b: number): boolean {
  return Math.abs(a - b) <= tol(b)
}

function toFraction(x: number, maxDen: number, eps: number): { n: number; d: number } | null {
  if (!Number.isFinite(x)) return null
  const sign = x < 0 ? -1 : 1
  const abs = Math.abs(x)
  let bestN = 1
  let bestD = 1
  let bestErr = Infinity
  for (let d = 1; d <= maxDen; d++) {
    const nm = Math.round(abs * d)
    const err = Math.abs(abs - nm / d)
    if (err < bestErr) {
      bestErr = err
      bestN = nm
      bestD = d
      if (err < 1e-16) break
    }
  }
  if (bestErr > eps) return null
  const g = intGcd(bestN, bestD)
  return { n: sign * (bestN / g), d: bestD / g }
}

function snapInteger(n: number): number | null {
  if (!Number.isFinite(n)) return null
  const r = Math.round(n)
  if (close(n, r) && Math.abs(r) < 1e12) return r
  return null
}

function formatPi(num: number, den: number): string {
  const sign = num < 0 ? '-' : ''
  const n = Math.abs(num)
  if (n === 0) return '0'
  const coeff = n === 1 ? '' : `${n}*`
  return `${sign}${coeff}pi${den === 1 ? '' : `/${den}`}`
}

export function splitSquares(n: number): { coeff: number; rad: number } {
  let rad = Math.round(Math.abs(n))
  if (rad <= 0) return { coeff: 0, rad: 1 }
  let coeff = 1
  for (let i = 2; i * i <= rad; i++) {
    const sq = i * i
    while (rad % sq === 0) {
      rad /= sq
      coeff *= i
    }
  }
  return { coeff, rad }
}

function formatRadicalTerm(coeff: number, rad: number): string {
  if (rad === 1) return String(coeff)
  const core = `sqrt(${rad})`
  return coeff === 1 ? core : `${coeff}${core}`
}

function formatRadical(sign: string, coeff: number, rad: number, den: number): string {
  const g = intGcd(coeff, den)
  coeff /= g
  den /= g
  const core = formatRadicalTerm(coeff, rad)
  return den === 1 ? `${sign}${core}` : `${sign}${core}/${den}`
}

function formatUnrationalized(
  sign: string,
  num: { coeff: number; rad: number },
  den: { coeff: number; rad: number },
): string {
  const g = intGcd(num.coeff, den.coeff)
  const nc = num.coeff / g
  const dc = den.coeff / g
  const n = formatRadicalTerm(nc, num.rad)
  if (dc === 1 && den.rad === 1) return `${sign}${n}`
  const d = formatRadicalTerm(dc, den.rad)
  const wrap = dc !== 1 && den.rad !== 1
  return `${sign}${n}/${wrap ? `(${d})` : d}`
}

function asPiMultiple(n: number): string | null {
  const f = toFraction(n / Math.PI, 24, tol(n / Math.PI))
  if (!f) return null
  if (f.n === 0) return '0'
  return formatPi(f.n, f.d)
}

function asNestedRadical(n: number): string | null {
  for (const [exact, v] of NESTED) {
    if (close(n, v)) return exact
  }
  return null
}

function asRadical(n: number, rationalize: boolean): string | null {
  const sign = n < 0 ? '-' : ''
  const x = Math.abs(n)
  const f = toFraction(x * x, 256, 2 * tol(x * x))
  if (!f || f.n <= 0) return null
  if (rationalize) {
    const split = splitSquares(f.n * f.d)
    if (split.rad === 1) return null
    return formatRadical(sign, split.coeff, split.rad, f.d)
  }
  const num = splitSquares(f.n)
  const den = splitSquares(f.d)
  if (num.rad === 1 && den.rad === 1) return null
  return formatUnrationalized(sign, num, den)
}

function asNiceFraction(n: number): string | null {
  const f = toFraction(n, 12, tol(n))
  if (!f || !NICE_DEN.has(f.d)) return null
  if (f.n === 0) return '0'
  return `${f.n}/${f.d}`
}

export type ExactFormOptions = { rationalize?: boolean }

const TRIG_OR_SQRT =
  /√|\\sqrt|(?:^|[^A-Za-z_])(?:sqrt|arcsin|arccos|arctan2|arctan|arccsc|arcsec|arccot|asin|acos|atan2|atan|acsc|asec|acot|sin|cos|tan|csc|sec|cot)(?![A-Za-z_])/i

/** The closed form is only shown beside the decimal for trig and square-root expressions. */
export function wantsExactForm(expr: string): boolean {
  return TRIG_OR_SQRT.test(expr)
}

/** Null when no closed form is nicer than the decimal. */
export function exactForm(n: number, options: ExactFormOptions = {}): string | null {
  if (!Number.isFinite(n)) return null
  const asInt = snapInteger(n)
  if (asInt !== null) return String(asInt)
  // past this any big enough radicand or numerator fits the tolerance by chance
  if (Math.abs(n) >= 1e4) return null
  return asNestedRadical(n) || asPiMultiple(n) || asRadical(n, options.rationalize !== false) || asNiceFraction(n)
}

export function dualLabel(exact: string | undefined, display: string): string {
  if (exact && exact !== display) return `${exact} ≈ ${display}`
  return display
}
