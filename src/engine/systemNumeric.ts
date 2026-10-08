import type { EvalFunction, MathNode } from 'mathjs'
import { math } from './math'

export type NumericOptions = {
  angleMode?: 'deg' | 'rad'
  /** A starting value per unknown; with one, the solution Newton's method reaches from it, and only that one. */
  guess?: Record<string, number>
}

const DEG = Math.PI / 180

// the calculator's own names: ln is natural, log is base ten unless given a base
const CALCULATOR_FNS: Record<string, (...a: number[]) => number> = {
  ln: (x) => Math.log(x),
  log: (x, base) => (base == null ? Math.log10(x) : Math.log(x) / Math.log(base)),
  log10: (x) => Math.log10(x),
  log2: (x) => Math.log2(x),
}

// in degree mode the trig functions read and give degrees, as everywhere else in the calculator
function angleScope(mode: NumericOptions['angleMode']): Record<string, (...a: number[]) => number> {
  const radians = { arcsin: Math.asin, arccos: Math.acos, arctan: Math.atan }
  if (mode !== 'deg') return { ...CALCULATOR_FNS, ...radians }
  return {
    ...CALCULATOR_FNS,
    arcsin: (x) => Math.asin(x) / DEG,
    arccos: (x) => Math.acos(x) / DEG,
    arctan: (x) => Math.atan(x) / DEG,
    sin: (x) => Math.sin(x * DEG),
    cos: (x) => Math.cos(x * DEG),
    tan: (x) => Math.tan(x * DEG),
    sec: (x) => 1 / Math.cos(x * DEG),
    csc: (x) => 1 / Math.sin(x * DEG),
    cot: (x) => 1 / Math.tan(x * DEG),
    asin: (x) => Math.asin(x) / DEG,
    acos: (x) => Math.acos(x) / DEG,
    atan: (x) => Math.atan(x) / DEG,
  }
}

const MAX_SHOWN = 4
const MAX_EVALS = 40_000

/**
 * Solutions of a square nonlinear system, `x^2 + y^2 = 4` with `x^2 - y = 2`, found numerically. Each start runs
 * damped Newton (Levenberg–Marquardt, so a flat or badly scaled step can't throw it off); without a guess a fixed
 * spread of starts collects the distinct solutions, nearest the origin first. Null when nothing converges.
 */
export function solveNumericSystem(
  diffs: MathNode[],
  variables: string[],
  opts: NumericOptions = {},
): { solutions: number[][]; more: boolean } | null {
  const n = variables.length
  if (!n || diffs.length !== n) return null
  let fns: EvalFunction[]
  try {
    fns = diffs.map((d) => d.compile())
  } catch {
    return null
  }
  const base = angleScope(opts.angleMode)
  let evals = 0
  const F = (x: number[]): number[] | null => {
    if (++evals > MAX_EVALS) return null
    const scope: Record<string, unknown> = { ...base }
    variables.forEach((v, i) => (scope[v] = x[i]))
    const out: number[] = []
    for (const f of fns) {
      let r: unknown
      try {
        r = f.evaluate(scope)
      } catch {
        return null
      }
      // a complex value (`sqrt(-1)`) has no real place in a solution
      if (typeof r !== 'number' || !Number.isFinite(r)) return null
      out.push(r)
    }
    return out
  }

  const guess = opts.guess
  const start = guess && variables.every((v) => Number.isFinite(guess[v])) ? variables.map((v) => guess[v]!) : null
  if (start) {
    const x = solveFrom(F, start)
    if (x) return { solutions: [snap(F, x)], more: false }
  }

  const found: number[][] = []
  for (const start of starts(n)) {
    const x = solveFrom(F, start)
    if (!x) {
      if (evals > MAX_EVALS) break
      continue
    }
    const t = snap(F, x)
    if (!found.some((s) => same(s, t))) found.push(t)
  }
  if (!found.length) return null
  // a guess whose own run didn't settle (a touching root creeps) takes the solution nearest it
  if (start) {
    const dist = (p: number[]) => norm(p.map((v, i) => v - start[i]!))
    return { solutions: [found.reduce((best, p) => (dist(p) < dist(best) ? p : best))], more: false }
  }
  found.sort((a, b) => norm(a) - norm(b) || a[0]! - b[0]!)
  // the nearest few, then in order of x
  const shown = found.slice(0, MAX_SHOWN).sort(cmp)
  return { solutions: shown, more: found.length > MAX_SHOWN }
}

function cmp(a: number[], b: number[]): number {
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return a[i]! - b[i]!
  return 0
}

// the same handful of starts every time, so an answer never flickers between keystrokes
function starts(n: number): number[][] {
  const values = [1, -1, 0.5, -0.5, 2, -2, 0.1, 3, -3, 5, -5, 10, 0.3, -0.7, 1.7, 30, -30, 90, 7, -10]
  const out: number[][] = [Array(n).fill(1), Array(n).fill(0.5), Array(n).fill(-1)]
  let seed = 12345
  const next = () => {
    seed = (seed * 1103515245 + 12345) % 2147483648
    return values[seed % values.length]!
  }
  while (out.length < 40) out.push(Array.from({ length: n }, next))
  return out
}

function norm(x: number[]): number {
  return Math.sqrt(x.reduce((s, v) => s + v * v, 0))
}

// two runs into a touching root stop at slightly different points, so they count as one a little more loosely
function same(a: number[], b: number[]): boolean {
  const size = Math.max(1, ...a.map(Math.abs), ...b.map(Math.abs))
  return a.every((v, i) => Math.abs(v - b[i]!) <= 1e-5 * size)
}

/**
 * A whole number nearby where the equations balance at least as well: a touching root (`x = -9e-7` where the
 * curves meet at x = 0) creeps in slowly and never quite lands, so 0 is tried, and kept if it balances.
 * Checked by balance, never by distance alone, so a true tiny value (a length of 3e-9 m) stays.
 */
function snap(F: (x: number[]) => number[] | null, x: number[]): number[] {
  const residual = (p: number[]) => {
    const f = F(p)
    return f ? Math.sqrt(dot(f, f)) : Infinity
  }
  let best = x.slice()
  let bestRes = residual(best)
  const near = (v: number) => {
    const r = Math.round(v)
    return Math.abs(v - r) <= 1e-3 * Math.max(1, Math.abs(r)) ? r : v
  }
  // coupled values move together: (0.0005, 0.9995) is (0, 1) only as a pair
  const whole = best.map(near)
  const wholeRes = residual(whole)
  if (wholeRes <= bestRes) {
    best = whole
    bestRes = wholeRes
  }
  for (let i = 0; i < x.length; i++) {
    const r = Math.round(best[i]!)
    if (r === best[i] || Math.abs(best[i]! - r) > 1e-3 * Math.max(1, Math.abs(r))) continue
    const trial = best.slice()
    trial[i] = r
    const res = residual(trial)
    if (res <= bestRes) {
      best = trial
      bestRes = res
    }
  }
  return best.map((v) => (v === 0 ? 0 : v))
}

/**
 * Newton's method from `x0` for a square system, each step judged by whether the next Newton step shrinks (a test
 * that doesn't care how big the equations' own numbers are, so `x^2 = 1e-18` and `xy = 1e12` both converge).
 * The step halves until it passes; a flat spot (a singular slope) takes a damped least-squares step instead.
 * The point where every equation balances, or null.
 */
function solveFrom(F: (x: number[]) => number[] | null, x0: number[]): number[] | null {
  let x = x0.slice()
  let fx = F(x)
  if (!fx) return null
  const start = Math.sqrt(dot(fx, fx))
  let last: number[] | null = null
  for (let iter = 0; iter < 200; iter++) {
    if (fx.every((v) => v === 0)) return x
    const J = jacobian(F, x, fx)
    if (!J) return null
    const dx = newtonStep(J, fx)
    if (!dx) return null
    last = dx
    // done once the step is lost in the last digits of x
    if (dx.every((d, i) => Math.abs(d) <= 1e-15 * Math.max(Math.abs(x[i]!), 1e-300))) break
    const size = norm(dx)
    let moved = false
    for (let alpha = 1; alpha > 1e-6; alpha /= 2) {
      const xt = x.map((v, i) => v + alpha * dx[i]!)
      const ft = F(xt)
      if (!ft) continue
      const next = newtonStep(J, ft)
      if (next && norm(next) <= (1 - alpha / 4) * size) {
        x = xt
        fx = ft
        moved = true
        break
      }
    }
    if (!moved) break
  }
  if (!last) return null
  // a real solution: the last step was down in the rounding, and the equations are far closer to balanced than at
  // the start, not a stall at a low point (where xy = 1e12 meets x + y = 2e6 instead of 3e6) that never reaches zero
  const settled = last.every((d, i) => Math.abs(d) <= 1e-8 * Math.max(Math.abs(x[i]!), norm(x) * 1e-8, 1e-300))
  const residual = Math.sqrt(dot(fx, fx))
  // a touching root (curves meeting at (0, -2)) creeps in and its steps never get rounding-small, but it balances
  // the equations a hundred trillion times better than the start, which a stall point never does
  const touching = residual <= 1e-14 * start
  return (settled && residual <= 1e-6 * start) || touching ? x : null
}

// J dx = -f, or where J is singular, the damped least-squares step (JᵀJ + μI) dx = -Jᵀf
function newtonStep(J: number[][], f: number[]): number[] | null {
  const direct = linearSolve(J, f.map((v) => -v))
  if (direct) return direct
  const n = J[0]!.length
  const A = Array.from({ length: n }, (_, i) => Array.from({ length: n }, (_, j) => J.reduce((s, row) => s + row[i]! * row[j]!, 0)))
  const mu = 1e-10 * Math.max(...A.map((row, i) => row[i]!), 1e-300)
  const g = Array.from({ length: n }, (_, i) => -J.reduce((s, row, k) => s + row[i]! * f[k]!, 0))
  return linearSolve(A.map((row, i) => row.map((v, j) => (i === j ? v + mu : v))), g)
}

function jacobian(F: (x: number[]) => number[] | null, x: number[], fx: number[]): number[][] | null {
  const m = fx.length
  const size = Math.max(...x.map(Math.abs))
  const J = Array.from({ length: m }, () => Array<number>(x.length).fill(0))
  for (let j = 0; j < x.length; j++) {
    // sized to the point as a whole: a value near 0 beside one near 1 still gets a step above rounding noise
    const h = 1e-7 * Math.max(Math.abs(x[j]!), size, 1e-300)
    const xp = x.slice()
    xp[j] = x[j]! + h
    const fp = F(xp)
    if (!fp) return null
    for (let i = 0; i < m; i++) J[i]![j] = (fp[i]! - fx[i]!) / h
  }
  return J
}

function dot(a: number[], b: number[]): number {
  return a.reduce((s, v, i) => s + v * b[i]!, 0)
}

// Gaussian elimination with partial pivoting; null when singular
function linearSolve(M: number[][], b: number[]): number[] | null {
  const n = b.length
  const A = M.map((row, i) => [...row, b[i]!])
  for (let c = 0; c < n; c++) {
    let p = c
    for (let r = c + 1; r < n; r++) if (Math.abs(A[r]![c]!) > Math.abs(A[p]![c]!)) p = r
    if (Math.abs(A[p]![c]!) < 1e-300) return null
    ;[A[c], A[p]] = [A[p]!, A[c]!]
    for (let r = c + 1; r < n; r++) {
      const f = A[r]![c]! / A[c]![c]!
      for (let k = c; k <= n; k++) A[r]![k]! -= f * A[c]![k]!
    }
  }
  const x = Array<number>(n).fill(0)
  for (let r = n - 1; r >= 0; r--) {
    let s = A[r]![n]!
    for (let k = r + 1; k < n; k++) s -= A[r]![k]! * x[k]!
    x[r] = s / A[r]![r]!
  }
  return x.every(Number.isFinite) ? x : null
}

/** A guess as typed (`1`, `-pi/4`), as a number. */
export function guessNumber(text: string): number | null {
  try {
    const v = math.evaluate(text.trim())
    return typeof v === 'number' && Number.isFinite(v) ? v : null
  } catch {
    return null
  }
}
