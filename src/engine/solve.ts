import type { MathNode, SymbolNode } from 'mathjs'
import { formatNumber } from './format'
import { dependsOnAngle, findCriticalPoints, tidy, type GraphPoint } from './graph'
import { intGcd, math, namesPattern } from './math'
import { normalizeMathText } from './plainMath'
import {
  compileScientific,
  evalScientific,
  formatAsFraction,
  preprocessAscii,
  SCIENTIFIC_NAMES,
  type AngleMode,
  type ScientificContext,
} from './scientific'
import { exactForm, splitSquares } from './simplify'
import type { SolveInfo, SolveOutcome } from './types'

export type { SolveInfo, SolveOutcome }

export const MAX_SHOWN_ROOTS = 4
export const MAX_EVALS = 4000
// parallel test workers make a wall-clock limit flaky; the evaluation cap still holds there
const MAX_MS = import.meta.env?.MODE === 'test' ? Infinity : 20
/** Sign-change cells and near-misses refined on the numeric path, nearest 0 first. */
const MAX_REFINED = 12
const NOISE = 8 * Number.EPSILON

const UNKNOWN = /^(?:[A-Za-z]|θ)$/
const NAMES = SCIENTIFIC_NAMES.split('|')
const KNOWN = new Set([...NAMES.map((n) => n.toLowerCase()), 'e', 'i', 'nan'])
const CALL_SKIP = new Set(['pi', 'tau', 'inf', 'infinity', 'ans', 'mod'])
const CALLABLE = NAMES.filter((n) => !CALL_SKIP.has(n.toLowerCase()))

export interface Equation {
  lhs: string
  rhs: string
  forVar?: string
  /** `name = …`: an assignment unless the name comes back on the right and isn't stored. */
  assignVar?: string
  /** `solve …` or `… for v`: always an equation. */
  explicit: boolean
}

const SOLVE_CMD = /^solve\s+(.+)$/is
const FOR_VAR = /^(.+?)\s+for\s+([A-Za-z]|θ)$/s
const REJECT = /==|[<>!]=|[≤≥≠<>]/
const BARE_NAME = /^(?:[A-Za-z][A-Za-z0-9]*|θ)$/

/** Null unless the line is shaped like an equation: one lone `=`, or `solve …`. No evaluation. */
export function parseEquation(text: string): Equation | null {
  let s = text.trim()
  if (!s || /^graph(?:\s|$)/i.test(s)) return null
  const cmd = s.match(SOLVE_CMD)
  if (cmd) s = cmd[1]!.trim()
  const named = s.match(FOR_VAR)
  if (named) s = named[1]!.trim()
  const explicit = Boolean(cmd || named)
  if (REJECT.test(s)) return null
  const parts = s.split('=')
  if (parts.length === 1) return cmd ? { lhs: s, rhs: '0', forVar: named?.[2], explicit } : null
  if (parts.length !== 2) return null
  const lhs = parts[0]!.trim()
  const rhs = parts[1]!.trim()
  if (!lhs || !rhs) return null
  const assignVar = !explicit && BARE_NAME.test(lhs) && !KNOWN.has(lhs.toLowerCase()) ? lhs : undefined
  return { lhs, rhs, forVar: named?.[2], assignVar, explicit }
}

/** Cheap routing check: true for lines solve would read as an equation (`x = 5` is an assignment). */
export function isEquation(text: string): boolean {
  const eq = parseEquation(text)
  if (!eq) return false
  if (!eq.assignVar) return true
  return new RegExp(`(?<![A-Za-z_])${namesPattern([eq.assignVar])}(?![A-Za-z0-9_])`).test(eq.rhs)
}

/** `1,000x` is a thousand x; a call's arguments (`max(1,000x)`) are left alone. */
const GROUPED_BEFORE_LETTER = /(?<![A-Za-z_][A-Za-z0-9_]*\([^()]*|\[[^\][]*|[\d.,])\d{1,3}(?:,\d{3})+(?=[A-Za-zθ(])/g

function callPattern(fnNames: string[]): string {
  return namesPattern([...CALLABLE, ...fnNames])
}

/** `sin 2x` reads as sin(2)·x elsewhere in the engine; too ambiguous to solve. */
function ambiguousCall(side: string, fnNames: string[]): boolean {
  return new RegExp(`(?<![A-Za-z_])(?:${callPattern(fnNames)})\\s+\\d+(?:\\.\\d+)?\\s*(?:[A-Za-z]|θ)(?![A-Za-z])`).test(side)
}

function normalizeSide(side: string, fnNames: string[]): string {
  let s = normalizeMathText(side).replace(GROUPED_BEFORE_LETTER, (m) => m.replace(/,/g, ''))
  // `sin x` is sin(x), not sin·x; `sin x^2` stays unread
  s = s.replace(new RegExp(`(?<![A-Za-z_])(${callPattern(fnNames)})\\s+([A-Za-z]|θ)(?![A-Za-z0-9_(^!])`, 'g'), '$1($2)')
  // a single letter before `(` multiplies (`x(x-1)`, `2n(n+1)`) unless it's a user function
  return s.replace(/(?<![A-Za-z_])([A-Za-z]|θ)\s*\(/g, (m, l: string) => (fnNames.includes(l) ? m : `${l}*(`))
}

/** Identifiers on one side, after the preprocessing compile uses; null on a parse error. */
export function freeSymbols(side: string, ctx: ScientificContext): string[] | null {
  const fnNames = Object.keys(ctx.functions ?? {})
  let node: MathNode
  try {
    node = math.parse(preprocessAscii(normalizeSide(side, fnNames), fnNames))
  } catch {
    return null
  }
  const out = new Set<string>()
  node.traverse((n: MathNode, path: string, parent: MathNode | null) => {
    if (n.type === 'SymbolNode' && !(parent?.type === 'FunctionNode' && path === 'fn')) {
      const name = (n as SymbolNode).name
      // preprocessAscii spells θ as theta; the unknown keeps its symbol
      out.add(name === 'theta' && side.includes('θ') ? 'θ' : name)
    }
  })
  return [...out]
}

/** The letter to solve for, or null when the line isn't a solve. */
export function pickUnknown(eq: Equation, ctx: ScientificContext): string | null {
  const left = freeSymbols(eq.lhs, ctx)
  const right = freeSymbols(eq.rhs, ctx)
  if (!left || !right) return null
  const vars = ctx.variables ?? {}
  const fns = ctx.functions ?? {}
  if (eq.assignVar && (!right.includes(eq.assignVar) || eq.assignVar in vars)) return null
  const all = [...new Set([...left, ...right])]
  const free = all.filter((n) => !KNOWN.has(n.toLowerCase()) && !(n in vars) && !(n in fns))
  if (eq.forVar) {
    if (!all.includes(eq.forVar) || free.some((n) => n !== eq.forVar)) return null
    return eq.forVar
  }
  if (free.length === 1) return UNKNOWN.test(free[0]!) ? free[0]! : null
  if (free.length) return null
  // a stored letter is shadowed: after x = 5, 2x + 3 = 11 still solves for x
  const stored = all.filter((n) => n in vars && UNKNOWN.test(n))
  return stored.length === 1 ? stored[0]! : null
}

type Fn = (x: number) => number | null
type Exact = { n: number; d: number } | string

/** `sure` roots are pinned to full precision; a touching root only to about 8 digits. */
interface Root {
  x: number
  sure: boolean
  exact?: Exact
}

export interface Solved {
  info: SolveInfo
  /** One per shown root, or none at all. */
  exact?: Exact[]
  /** A quadratic's `(p ± q·sqrt(r))/d`. */
  closedForm?: string
  /** Roots known only to about 8 digits. */
  loose?: boolean[]
  evals: number
}

const OVER_BUDGET = Symbol('solve budget')

class Budget {
  evals = 0
  private readonly start = performance.now()
  tick(): void {
    this.evals++
    if (this.evals > MAX_EVALS) throw OVER_BUDGET
    if (this.evals % 64 === 0 && performance.now() - this.start > MAX_MS) throw OVER_BUDGET
  }
}

function compileSide(side: string, v: string, ctx: ScientificContext, budget: Budget): Fn | null {
  // the engine reads θ as theta
  const c = compileScientific(side, ctx, v === 'θ' ? 'theta' : v)
  if (!c) return null
  return (x) => {
    budget.tick()
    const r = c(x)
    return r && r.kind === 'number' && Number.isFinite(r.n) ? r.n : null
  }
}

/** Full solve; null means not a solve (the line falls through, usually to a blank). */
export function solveEquation(text: string, ctx: ScientificContext & { rationalize?: boolean } = {}): Solved | null {
  const eq = parseEquation(text)
  if (!eq) return null
  const fnNames = Object.keys(ctx.functions ?? {})
  if (ambiguousCall(eq.lhs, fnNames) || ambiguousCall(eq.rhs, fnNames)) return null
  const v = pickUnknown(eq, ctx)
  if (!v) return null
  const budget = new Budget()
  try {
    const lhs = normalizeSide(eq.lhs, fnNames)
    const rhs = normalizeSide(eq.rhs, fnNames)
    const L = compileSide(lhs, v, ctx, budget)
    const R = compileSide(rhs, v, ctx, budget)
    if (!L || !R) return null
    const found = new Solver(L, R, lhs, rhs, v, ctx).run()
    if (!found) return null
    return { ...found, evals: budget.evals }
  } catch {
    return null
  }
}

type Found = Omit<Solved, 'evals'>
type Poly = { c: number[]; maxY: number }

const NODES = [-2, -1, 0, 1, 2]
// the far points catch functions that only look polynomial near 0 (exp(x/1e6))
const CHECKS = [0.37, -2.9, 7.3, 31.7, -113.5, 1234.5, -98765.4, 3.21e6]

/** Degree ≤ 4 coefficients, ascending, or null when the side isn't a polynomial. */
function fitPolynomial(S: Fn): Poly | null {
  const ys = NODES.map(S)
  if (ys.some((y) => y == null)) return null
  const [a, b, c0, d, e] = ys as number[] as [number, number, number, number, number]
  const c = [
    c0,
    (a - 8 * b + 8 * d - e) / 12,
    (-a + 16 * b - 30 * c0 + 16 * d - e) / 24,
    (-a + 2 * b - 2 * d + e) / 12,
    (a - 4 * b + 6 * c0 - 4 * d + e) / 24,
  ]
  const maxY = Math.max(...ys.map((y) => Math.abs(y!)))
  for (let k = 1; k <= 4; k++) if (Math.abs(c[k]!) * 2 ** k <= 1e-13 * maxY) c[k] = 0
  for (const t of CHECKS) {
    const y = S(t)
    if (y == null) return null
    let p = 0
    let mag = Math.abs(y)
    for (let k = 0; k <= 4; k++) {
      const term = c[k]! * t ** k
      p += term
      mag += Math.abs(term)
    }
    if (Math.abs(y - p) > 1e-9 * mag) return null
  }
  return { c, maxY }
}

function horner(c: number[], x: number): number {
  let y = 0
  for (let k = c.length - 1; k >= 0; k--) y = y * x + c[k]!
  return y
}

function degreeOf(c: number[]): number {
  let n = c.length - 1
  while (n > 0 && c[n] === 0) n--
  return n
}

function toFrac(x: number, maxDen = 1000): { n: number; d: number } | null {
  if (x === 0) return { n: 0, d: 1 }
  for (let d = 1; d <= maxDen; d++) {
    const n = Math.round(x * d)
    if (Math.abs(n / d - x) <= 1e-11 * Math.abs(x)) return { n, d }
  }
  return null
}

function ratio(n: number, d: number): { n: number; d: number } {
  const g = intGcd(n, d) * (d < 0 ? -1 : 1)
  return { n: n / g, d: d / g }
}

/** Whole coefficients with the same ratios, or null when one isn't a small fraction. */
function integerCoeffs(c: number[]): number[] | null {
  const fr = c.map((x) => toFrac(x))
  if (fr.some((f) => !f)) return null
  let lcm = 1
  for (const f of fr) {
    lcm = (lcm * f!.d) / intGcd(lcm, f!.d)
    if (!Number.isSafeInteger(lcm)) return null
  }
  const ints = fr.map((f) => (f!.n * lcm) / f!.d)
  return ints.every(Number.isSafeInteger) ? ints : null
}

/** Real roots of a fitted polynomial, used only as breakpoints between monotone stretches. */
function fittedRoots(c: number[], lo: number, hi: number): number[] {
  const n = degreeOf(c)
  if (n === 0) return []
  if (n === 1) {
    const x = -c[0]! / c[1]!
    return x > lo && x < hi ? [x] : []
  }
  const crit = fittedRoots(
    c.slice(1, n + 1).map((v, k) => v * (k + 1)),
    lo,
    hi,
  )
  const breaks = [lo, ...crit, hi]
  const out: number[] = []
  for (let i = 1; i < breaks.length; i++) {
    let a = breaks[i - 1]!
    let b = breaks[i]!
    let ya = horner(c, a)
    const yb = horner(c, b)
    if (ya === 0 && i > 1) out.push(a)
    if (Math.sign(ya) * Math.sign(yb) >= 0) continue
    for (let k = 0; k < 200; k++) {
      const m = (a + b) / 2
      if (m <= a || m >= b) break
      const ym = horner(c, m)
      if (Math.sign(ym) === Math.sign(ya)) {
        a = m
        ya = ym
      } else b = m
    }
    out.push((a + b) / 2)
  }
  return out
}

class Solver {
  private readonly f: Fn
  private readonly mode: AngleMode
  private poly: { L: Poly; R: Poly } | null = null
  private readonly L: Fn
  private readonly R: Fn
  private readonly lhs: string
  private readonly rhs: string
  private readonly v: string
  private readonly ctx: ScientificContext & { rationalize?: boolean }

  constructor(L: Fn, R: Fn, lhs: string, rhs: string, v: string, ctx: ScientificContext & { rationalize?: boolean }) {
    this.L = L
    this.R = R
    this.lhs = lhs
    this.rhs = rhs
    this.v = v
    this.ctx = ctx
    this.f = (x) => {
      const a = L(x)
      if (a == null) return null
      const b = R(x)
      return b == null ? null : a - b
    }
    this.mode = ctx.angleMode ?? 'deg'
  }

  run(): Found | null {
    const pl = fitPolynomial(this.L)
    const pr = pl && fitPolynomial(this.R)
    if (pl && pr) {
      this.poly = { L: pl, R: pr }
      const out = this.polynomial()
      if (out) return out
      this.poly = null
    }
    return this.numeric()
  }

  private found(outcome: SolveOutcome, roots: Root[] = [], more = false, closedForm?: string): Found {
    const shown = pickShown(roots)
    const info: SolveInfo = { variable: this.v, roots: shown.map((r) => r.x), outcome }
    if (more || roots.length > shown.length) info.more = true
    const out: Found = { info }
    if (closedForm) out.closedForm = closedForm
    if (shown.length && shown.every((r) => r.exact != null)) out.exact = shown.map((r) => r.exact!)
    if (shown.some((r) => !r.sure)) out.loose = shown.map((r) => !r.sure)
    return out
  }

  /** How big the evaluation's rounding can be at x. */
  private scale(x: number): number {
    if (this.poly) {
      let s = 0
      for (let k = 0; k <= 4; k++) s += (Math.abs(this.poly.L.c[k]!) + Math.abs(this.poly.R.c[k]!)) * Math.abs(x) ** k
      return s
    }
    return Math.max(Math.abs(this.L(x) ?? 0), Math.abs(this.R(x) ?? 0))
  }

  /** Sign change to adjacent floats; null for a pole or jump. */
  private bisect(a: number, ya: number, b: number, yb: number): number | null {
    const { f } = this
    let lo = a
    let hi = b
    let ylo = ya
    for (let k = 0; k < 200; k++) {
      const mid = (lo + hi) / 2
      if (mid <= lo || mid >= hi) break
      const ym = f(mid)
      if (ym == null) return null
      if (ym === 0) return mid
      if (Math.sign(ym) === Math.sign(ylo)) {
        lo = mid
        ylo = ym
      } else hi = mid
    }
    const flo = f(lo)
    const fhi = f(hi)
    if (flo == null || fhi == null) return null
    const [x, y] = Math.abs(flo) <= Math.abs(fhi) ? [lo, flo] : [hi, fhi]
    if (Math.abs(y) > 1e-6 * Math.max(Math.abs(ya), Math.abs(yb))) return null
    const tol = Math.abs(y) + NOISE * this.scale(x)
    return tidy(x, (c) => {
      if (c < a || c > b) return false
      const yc = f(c)
      return yc != null && Math.abs(yc) <= tol
    })
  }

  /** A candidate from a formula checked on the real equation: bracketed and pinned, or touching zero. */
  private settle(x: number): Root | null {
    const y = this.f(x)
    if (y == null) return null
    if (y === 0) return { x, sure: true }
    const h = 1e-9 * Math.max(Math.abs(x), 1e-6)
    for (const [a, b] of [
      [x - h, x],
      [x, x + h],
    ] as const) {
      const ya = a === x ? y : this.f(a)
      const yb = b === x ? y : this.f(b)
      if (ya == null || yb == null) continue
      if (ya === 0) return { x: a, sure: true }
      if (yb === 0) return { x: b, sure: true }
      if (Math.sign(ya) !== Math.sign(yb)) {
        const r = this.bisect(a, ya, b, yb)
        return r == null ? null : { x: r, sure: true }
      }
    }
    return Math.abs(y) <= NOISE * this.scale(x) ? { x, sure: true } : null
  }

  /** Keeps an exact candidate only if it is at least as good a root as the number found. */
  private accept(root: Root, exact: Exact, value: number): Root {
    // a pinned root that moves is a coincidence, however flat the curve is there
    if (root.sure && Math.abs(value - root.x) > 1e-12 * Math.abs(root.x)) return root
    const fx = Math.abs(this.f(root.x) ?? Infinity)
    const fc = this.f(value)
    if (fc == null || Math.abs(fc) > Math.max(fx, NOISE * this.scale(value))) return root
    return { x: value, sure: true, exact }
  }

  private exactRoot(root: Root): Root {
    // past 1e4 a radical or fraction is never nicer than the number, and finding one gets slow
    const s = Math.abs(root.x) <= 1e4 ? exactForm(root.x, { rationalize: this.ctx.rationalize }) : String(Math.round(root.x))
    if (!s || /\d{7}/.test(s.replace(/^-?\d+$/, ''))) return root
    const frac = s.match(/^(-?\d+)(?:\/(\d+))?$/)
    if (frac) {
      const n = Number(frac[1])
      const d = Number(frac[2] ?? 1)
      return this.accept(root, ratio(n, d), n / d)
    }
    const c = evalScientific(s, { angleMode: this.mode })
    if (!c || c.kind !== 'number' || !Number.isFinite(c.n)) return root
    return this.accept(root, s, c.n)
  }

  private settleAll(cands: Array<{ x: number; exact?: { n: number; d: number } }>): Root[] | null {
    const out: Root[] = []
    for (const cand of cands) {
      const root = this.settle(cand.x)
      if (!root) return null
      out.push(cand.exact ? this.accept(root, cand.exact, cand.exact.n / cand.exact.d) : this.exactRoot(root))
    }
    return dedupe(out)
  }

  private polynomial(): Found | null {
    const { L, R } = this.poly!
    const d = L.c.map((a, k) => {
      const b = R.c[k]!
      const tol = k === 0 ? 1e-12 * Math.max(Math.abs(a), Math.abs(b)) : (1e-14 * (L.maxY + R.maxY)) / 2 ** k
      return Math.abs(a - b) <= tol ? 0 : a - b
    })
    const n = degreeOf(d)
    if (n === 0) return this.found(d[0] === 0 ? 'all' : 'contradiction')
    const ints = integerCoeffs(d.slice(0, n + 1))
    if (n === 1) {
      const exact = ints ? ratio(-ints[0]!, ints[1]!) : undefined
      const roots = this.settleAll([{ x: exact ? exact.n / exact.d : -d[0]! / d[1]!, exact }])
      return roots && this.found('roots', roots)
    }
    if (n === 2) return this.quadratic(d, ints)
    // cubic and quartic: every real root is inside the Cauchy bound, one per monotone stretch
    const bound = 1 + Math.max(...d.slice(0, n).map((c) => Math.abs(c / d[n]!)))
    const crit = fittedRoots(
      d.slice(1, n + 1).map((c, k) => c * (k + 1)),
      -bound,
      bound,
    )
    const breaks = [-bound, ...crit, bound]
    const ys = breaks.map(this.f)
    if (ys.some((y) => y == null)) return null
    const cands: number[] = []
    for (let i = 0; i < breaks.length; i++) {
      const y = ys[i]!
      if (i > 0 && i < breaks.length - 1 && Math.abs(y) <= NOISE * this.scale(breaks[i]!)) cands.push(breaks[i]!)
      if (i === 0 || y === 0 || ys[i - 1] === 0 || Math.sign(y) === Math.sign(ys[i - 1]!)) continue
      const r = this.bisect(breaks[i - 1]!, ys[i - 1]!, breaks[i]!, y)
      if (r == null) return null
      cands.push(r)
    }
    const roots = this.settleAll(cands.map((x) => ({ x })))
    if (!roots) return null
    return this.found(roots.length ? 'roots' : 'none', roots)
  }

  private quadratic(d: number[], ints: number[] | null): Found | null {
    const [c, b, a] = d as [number, number, number]
    if (ints) {
      const [C, B, A] = ints as [number, number, number]
      const D = B * B - 4 * A * C
      const size = B * B + 4 * Math.abs(A * C)
      // an integer discriminant is exact, unless it is so small next to its parts that a snapped coefficient could flip it
      if (Number.isSafeInteger(size) && (D === 0 || Math.abs(D) * 1e10 >= size)) {
        if (D < 0) return this.found('none')
        if (D === 0) {
          const r = ratio(-B, 2 * A)
          const roots = this.settleAll([{ x: r.n / r.d, exact: r }])
          return roots && this.found('roots', roots)
        }
        const s = Math.round(Math.sqrt(D))
        if (s * s === D) {
          const cands = [ratio(-B - s, 2 * A), ratio(-B + s, 2 * A)].map((r) => ({ x: r.n / r.d, exact: r }))
          const roots = this.settleAll(cands)
          return roots && this.found('roots', roots)
        }
        const roots = this.settleAll(stableRoots(A, B, C).map((x) => ({ x })))
        if (!roots) return null
        const closed = D <= 1e10 ? this.closedForm(A, B, D, roots) : undefined
        return this.found('roots', roots, false, closed)
      }
    }
    // how far fitting noise in the coefficients could move the discriminant
    const { L, R } = this.poly!
    const noise = 32 * Number.EPSILON * (L.maxY + R.maxY) * (2 * Math.abs(b) + 4 * Math.abs(a) + 4 * Math.abs(c))
    const D = b * b - 4 * a * c
    if (D < -noise) return this.found('none')
    const cands = Math.abs(D) <= noise ? [-b / (2 * a)] : stableRoots(a, b, c)
    const roots = this.settleAll(cands.map((x) => ({ x })))
    return roots && this.found('roots', roots)
  }

  /** `(p ± q·sqrt(r))/d`, kept only when both of its values are as good as the roots found. */
  private closedForm(A: number, B: number, D: number, roots: Root[]): string | undefined {
    if (roots.length !== 2) return undefined
    const { coeff: k, rad } = splitSquares(D)
    let p = -B
    let q = k
    let d = 2 * A
    if (d < 0) {
      p = -p
      d = -d
    }
    const g = intGcd(intGcd(p, q), d)
    p /= g
    q /= g
    d /= g
    const values = [(p - q * Math.sqrt(rad)) / d, (p + q * Math.sqrt(rad)) / d].sort((x, y) => x - y)
    const ok = roots.every((r, i) => {
      const fc = this.f(values[i]!)
      const fx = Math.abs(this.f(r.x) ?? Infinity)
      return fc != null && Math.abs(fc) <= 4 * Math.max(fx, NOISE * this.scale(values[i]!))
    })
    if (!ok) return undefined
    const term = `${q === 1 ? '' : q}sqrt(${rad})`
    if (p === 0) return d === 1 ? `±${term}` : `±${term}/${d}`
    return d === 1 ? `${p} ± ${term}` : `(${p} ± ${term})/${d}`
  }

  private numeric(): Found | null {
    const { mode } = this
    const period = mode === 'deg' ? 360 : 2 * Math.PI
    const periodic = this.periodic(period)
    const xs = periodic ? linear(0, period, 401) : WIDE_GRID
    const sizes: number[] = []
    const pts: GraphPoint[] = xs.map((x) => {
      const a = this.L(x)
      const b = a == null ? null : this.R(x)
      sizes.push(Math.max(Math.abs(a ?? 0), Math.abs(b ?? 0)))
      return { x, y: a == null || b == null ? null : a - b }
    })
    const defined = pts.filter((p) => p.y != null)
    if (!defined.length) return null
    // zero next to the sides' own size, so tan(x) - x near 0 isn't mistaken for flat
    const small = (i: number) => pts[i]?.y != null && Math.abs(pts[i]!.y!) <= 1e-12 * sizes[i]!
    if (pts.every((p, i) => p.y == null || small(i))) return defined.length === pts.length ? this.found('all') : null
    const roots: Root[] = []
    type Job = { at: number; crossing: boolean; run: () => Root | null }
    const jobs: Job[] = []
    const cross = (a: GraphPoint, b: GraphPoint) =>
      jobs.push({ at: Math.min(Math.abs(a.x), Math.abs(b.x)), crossing: true, run: () => this.crossing(a.x, a.y!, b.x, b.y!) })
    // a sample counts by its sign unless it is (near) zero; `live` is defined and not zero
    const live = (i: number) => pts[i]?.y != null && !small(i)
    for (let i = 0; i < pts.length; i++) {
      const p = pts[i]!
      if (small(i)) {
        let j = i
        while (j + 1 < pts.length && small(j + 1)) j++
        // zero over a stretch is an interval (floor(x) = 3), which isn't shown; a short stretch
        // is one flat root that float can't resolve (tan(x) = x near 0)
        const zeros = pts.slice(i, j + 1).filter((q) => q.y === 0)
        const reach = Math.max(1, Math.abs(pts[i]!.x), Math.abs(pts[j]!.x))
        const flatZero = zeros.length > 1 && zeros[zeros.length - 1]!.x - zeros[0]!.x > 1e-6 * reach
        if (flatZero || pts[j]!.x - pts[i]!.x > 1e-3 * reach) return null
        const opposite = live(i - 1) && live(j + 1) && Math.sign(pts[i - 1]!.y!) !== Math.sign(pts[j + 1]!.y!)
        if (opposite) cross(pts[i - 1]!, pts[j + 1]!)
        else {
          let k = i
          for (let m = i; m <= j; m++) if (Math.abs(pts[m]!.y!) < Math.abs(pts[k]!.y!) || (pts[m]!.y === pts[k]!.y && Math.abs(pts[m]!.x) < Math.abs(pts[k]!.x))) k = m
          roots.push({ x: pts[k]!.x, sure: false })
        }
        i = j
        continue
      }
      if (!live(i)) continue
      const prev = pts[i - 1]
      if (live(i - 1) && Math.sign(prev!.y!) !== Math.sign(p.y!)) cross(prev!, p)
      if (live(i - 1) && live(i + 1)) {
        const next = pts[i + 1]!
        const s = Math.sign(p.y!)
        const low = Math.abs(p.y!)
        const rise = Math.abs(prev!.y!) - low + Math.abs(next.y!) - low
        const dips = Math.sign(prev!.y!) === s && Math.sign(next.y!) === s && low < Math.abs(prev!.y!) && low < Math.abs(next.y!)
        // only a dip steep enough that it could reach zero between the samples
        if (dips && low <= rise) jobs.push({ at: Math.abs(p.x), crossing: false, run: () => this.touching(prev!, p, next) })
      }
    }
    jobs.sort((a, b) => a.at - b.at)
    for (const job of jobs.slice(0, MAX_REFINED)) {
      const r = job.run()
      if (r) roots.push(r)
    }
    roots.push(...this.domainEdges(pts))
    const inWindow = periodic ? roots.filter((r) => r.x < period * (1 - 1e-9)) : roots
    const all = dedupe(inWindow.map((r) => this.exactRoot(r)))
    const skipped = jobs.slice(MAX_REFINED).some((j) => j.crossing)
    return this.found(all.length ? 'roots' : 'noneFound', all, skipped)
  }

  private crossing(a: number, ya: number, b: number, yb: number): Root | null {
    const x = this.bisect(a, ya, b, yb)
    return x == null ? null : { x, sure: true }
  }

  /** A curve that dips toward zero between samples might touch it (sin(x) = 1). */
  private touching(prev: GraphPoint, cur: GraphPoint, next: GraphPoint): Root | null {
    const best = findCriticalPoints([prev, cur, next], this.f)[0]
    if (!best) return null
    if (Math.abs(best.y) > NOISE * Math.max(1, this.scale(best.x))) return null
    return { x: best.x, sure: false }
  }

  /** Where the curve stops being defined, bisected to adjacent floats (sqrt(x - 0.3) = 0). */
  private domainEdges(pts: GraphPoint[]): Root[] {
    const edges: Array<[GraphPoint, GraphPoint]> = []
    for (let i = 1; i < pts.length; i++) {
      if ((pts[i - 1]!.y == null) !== (pts[i]!.y == null)) edges.push([pts[i - 1]!, pts[i]!])
    }
    edges.sort((p, q) => Math.abs(p[0].x) - Math.abs(q[0].x))
    const out: Root[] = []
    for (const [a, b] of edges.slice(0, 4)) {
      let def = a.y == null ? b.x : a.x
      let undef = a.y == null ? a.x : b.x
      for (let k = 0; k < 80; k++) {
        const m = (def + undef) / 2
        if (m === def || m === undef) break
        if (this.f(m) == null) undef = m
        else def = m
      }
      const y = this.f(def)
      if (y != null && Math.abs(y) <= NOISE * Math.max(1, this.scale(def))) out.push({ x: def, sure: true })
    }
    return out
  }

  /** Trig in the angle mode that repeats each turn is shown for one turn: [0, 360) or [0, 2π). */
  private periodic(period: number): boolean {
    const { f } = this
    const probes = [0.37, 1.9, -4.3, 7.7, 23.1]
    const repeats = probes.every((t) => {
      const a = f(t)
      const b = f(t + period)
      if (a == null || b == null) return a === b
      return Math.abs(a - b) <= 1e-9 * Math.max(1, Math.abs(a))
    })
    if (!repeats) return false
    const other: ScientificContext = { ...this.ctx, angleMode: this.mode === 'deg' ? 'rad' : 'deg' }
    const budget = new Budget()
    const L2 = compileSide(this.lhs, this.v, other, budget)
    const R2 = compileSide(this.rhs, this.v, other, budget)
    if (!L2 || !R2) return false
    const g: Fn = (x) => {
      const a = L2(x)
      const b = R2(x)
      return a == null || b == null ? null : a - b
    }
    return dependsOnAngle(f, g)
  }
}

function linear(lo: number, hi: number, n: number): number[] {
  return Array.from({ length: n }, (_, i) => lo + (hi - lo) * (i / (n - 1)))
}

/** ±10 finely, then log-spaced out to ±1e7 and in to ±1e-9. */
const WIDE_GRID: number[] = (() => {
  const xs = new Set(linear(-10, 10, 401))
  for (let k = 1; k <= 96; k++) {
    const v = 10 ** (1 + k / 16)
    xs.add(v).add(-v)
  }
  for (let k = 1; k <= 36; k++) {
    const v = 10 ** (-k / 4)
    xs.add(v).add(-v)
  }
  return [...xs].sort((a, b) => a - b)
})()

function stableRoots(a: number, b: number, c: number): number[] {
  const D = b * b - 4 * a * c
  const q = -(b + (b < 0 ? -1 : 1) * Math.sqrt(Math.max(0, D))) / 2
  if (q === 0) return [0]
  return [q / a, c / q].sort((x, y) => x - y)
}

function dedupe(roots: Root[]): Root[] {
  const sorted = [...roots].sort((a, b) => a.x - b.x)
  const out: Root[] = []
  for (const r of sorted) {
    const last = out[out.length - 1]
    // a touching root is only good to about 8 digits
    const tol = last?.sure && r.sure ? 1e-12 : 1e-7
    if (last && Math.abs(r.x - last.x) <= tol * Math.max(Math.abs(r.x), Math.abs(last.x))) {
      if (!last.sure && r.sure) out[out.length - 1] = r
      continue
    }
    out.push(r)
  }
  return out
}

/** The ones nearest 0, in ascending order. */
function pickShown(roots: Root[]): Root[] {
  if (roots.length <= MAX_SHOWN_ROOTS) return roots
  const near = [...roots].sort((a, b) => Math.abs(a.x) - Math.abs(b.x)).slice(0, MAX_SHOWN_ROOTS)
  return near.sort((a, b) => a.x - b.x)
}

const MESSAGES: Record<Exclude<SolveOutcome, 'roots' | 'all'>, string> = {
  none: 'no real solution',
  contradiction: 'no solution',
  noneFound: 'no solution found',
}

function terminates(d: number): boolean {
  let x = Math.abs(d)
  while (x % 2 === 0) x /= 2
  while (x % 5 === 0) x /= 5
  return x === 1
}

/** Display strings: values only, the `x =` label is the UI's. */
export function formatSolve(
  s: Solved,
  opts: { sigFigs: number; fractionMode?: boolean },
): { display: string; exact?: string; message?: boolean } {
  const { info } = s
  if (info.outcome === 'all') return { display: `true for all ${info.variable}`, message: true }
  if (info.outcome !== 'roots') return { display: MESSAGES[info.outcome], message: true }
  const roots = info.roots
  const more = info.more ? ', …' : ''
  const pair =
    roots.length === 2 && !info.more && roots[0]! < 0 && Math.abs(roots[0]! + roots[1]!) <= 1e-12 * roots[1]!
  let whole = roots.length === 1 || pair ? opts.sigFigs : Math.min(opts.sigFigs, 6)
  // close roots get the digits that tell them apart (1 and 1.000001 aren't "1, 1")
  const distinct = (f: number) => new Set(roots.map((r) => formatNumber(r, f))).size === roots.length
  while (whole < opts.sigFigs && !distinct(whole)) whole++
  const figs = (i: number) => (s.loose?.[i] ? Math.min(whole, 6) : whole)
  const decimal = (r: number, i: number) => formatNumber(r, figs(i))
  const shown = (r: number, i: number) => (opts.fractionMode ? formatAsFraction(r) : null) ?? decimal(r, i)
  const display = pair ? `±${shown(roots[1]!, 1)}` : roots.map(shown).join(', ') + more
  let exact = s.closedForm
  if (!exact && s.exact) {
    const parts = s.exact.map((e, i) => {
      if (typeof e === 'string') return e
      if (e.d === 1) return String(e.n)
      return opts.fractionMode || !terminates(e.d) ? `${e.n}/${e.d}` : decimal(roots[i]!, i)
    })
    exact = pair ? `±${parts[1]}` : parts.join(', ') + more
  }
  return exact && exact !== display ? { display, exact } : { display }
}
