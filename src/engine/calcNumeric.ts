/** Double-precision quadrature, limits and derivatives. Every result carries an error bound, or is null. */

import { workBudget } from './work'

export type RealFn = (x: number) => number

export type Estimate = { value: number; err: number }

const HALF_PI = Math.PI / 2
const MAX_LEVEL = 7
const MAX_EVALS = 40_000
// the answer is recomputed on every keystroke; a slow user function gets a blank rather than lag
const MAX_WORK = 4_000_000

type Budget = { evals: number; work: { over: () => boolean } }

function spent(budget: Budget): boolean {
  return --budget.evals < 0 || (budget.evals % 64 === 0 && budget.work.over())
}

/** Keeps only the digits `err` vouches for, less one and at most 13; null when fewer than 3 survive. */
export function justified(value: number, err: number, scale = Math.abs(value)): number | null {
  if (!Number.isFinite(value)) return null
  if (!(err >= 0)) return null
  // indistinguishable from zero at the scale of the problem (∫-1..1 x³, √x at 0)
  if (Math.abs(value) <= 10 * err) return 10 * err <= 1e-9 * Math.max(scale, 1e-300) ? 0 : null
  const digits = err > 0 ? Math.floor(Math.log10(Math.abs(value))) - Math.floor(Math.log10(err)) - 1 : 13
  if (digits < 3) return null
  return Number(value.toPrecision(Math.min(13, digits)))
}

type Cut = { f: number; dist: number }
type Side = { sum: number; l1: number; cut: Cut | null; ok: boolean }

/**
 * One side of a double exponential rule at step h, over t = h, 2h, ... (odd multiples only when `odd`).
 * `node(t)` gives the abscissa, its distance from the nearest end, and the weight. `cut` is the last node
 * before the float range ran out near an end.
 */
function sweep(
  f: RealFn,
  node: (t: number) => { x: number; w: number; dist: number } | null,
  h: number,
  odd: boolean,
  sign: 1 | -1,
  budget: Budget,
): Side {
  let sum = 0
  let l1 = 0
  let small = 0
  // a float blowup far out on an infinite range is harmless once the terms there no longer matter
  let negligible = false
  let last: Cut = { f: 0, dist: 0 }
  for (let k = 1; ; k += odd ? 2 : 1) {
    const t = sign * k * h
    if (Math.abs(t) > 7) return { sum, l1, cut: null, ok: false }
    const n = node(t)
    if (!n) {
      // far out on an infinite range the terms must already have died away
      if (last.dist === Infinity) return { sum, l1, cut: null, ok: negligible }
      return { sum, l1, cut: last, ok: true }
    }
    if (spent(budget)) return { sum, l1, cut: null, ok: false }
    const fx = f(n.x)
    if (!Number.isFinite(fx)) {
      if (negligible && n.dist === Infinity) return { sum, l1, cut: null, ok: true }
      // 0/0 or x/0 right at an end (x²/(eˣ - 1) at 1e-19): the cut's tail estimate keeps it honest
      if (n.dist < 1e-12 && last.dist > 0) return { sum, l1, cut: last, ok: true }
      return { sum, l1, cut: null, ok: false }
    }
    const term = n.w * fx
    sum += term
    l1 += Math.abs(term)
    last = { f: fx, dist: n.dist }
    negligible = Math.abs(term) <= 1e-15 * l1
    if (Math.abs(term) <= 1e-18 * l1 || n.w === 0) {
      if (++small >= 3 && Math.abs(t) > 2) return { sum, l1, cut: null, ok: true }
    } else small = 0
  }
}

type Rule = {
  node: (t: number) => { x: number; w: number; dist: number } | null
  center: { x: number; w: number } | null
}

function tanhSinhRule(a: number, b: number): Rule {
  const r = (b - a) / 2
  return {
    center: { x: (a + b) / 2, w: r * HALF_PI },
    node: (t) => {
      const u = HALF_PI * Math.sinh(Math.abs(t))
      const e2u = Math.exp(2 * u)
      // distance from the nearest end, computed directly so it keeps its digits
      const delta = 1 / (e2u + 1)
      const dist = 2 * r * delta
      const ch = Math.cosh(u)
      const w = (r * HALF_PI * Math.cosh(t)) / (ch * ch)
      const x = t > 0 ? b - dist : a + dist
      if (!Number.isFinite(w) || dist === 0 || x === a || x === b) return null
      return { x, w, dist }
    },
  }
}

/** [a, ∞): x = a + exp(π/2 sinh t). */
function expSinhRule(a: number): Rule {
  return {
    center: { x: a + 1, w: HALF_PI },
    node: (t) => {
      const e = Math.exp(HALF_PI * Math.sinh(t))
      const x = a + e
      const w = HALF_PI * Math.cosh(t) * e
      if (!Number.isFinite(x) || !Number.isFinite(w) || x === a) return null
      return { x, w, dist: t < 0 ? e : Infinity }
    },
  }
}

/** (-∞, ∞): x = sinh(π/2 sinh t). */
function sinhSinhRule(): Rule {
  return {
    center: { x: 0, w: HALF_PI },
    node: (t) => {
      const u = HALF_PI * Math.sinh(t)
      const x = Math.sinh(u)
      const w = HALF_PI * Math.cosh(t) * Math.cosh(u)
      if (!Number.isFinite(x) || !Number.isFinite(w)) return null
      return { x, w, dist: Infinity }
    },
  }
}

/** Double exponential quadrature, refined until two levels agree. */
function doubleExponential(f: RealFn, rule: Rule, budget: Budget): (Estimate & { l1: number }) | null {
  if (!rule.center) return null
  const c = f(rule.center.x)
  budget.evals--
  if (!Number.isFinite(c)) return null
  let h = 1
  let raw = rule.center.w * c
  let l1 = Math.abs(raw)
  // the sliver past the closest node any level reached, on each side, is left out of the sum
  const cuts: Record<number, Cut | null> = { 1: null, [-1]: null }
  const run = (odd: boolean): boolean => {
    for (const sign of [1, -1] as const) {
      const s = sweep(f, rule.node, h, odd, sign, budget)
      if (!s.ok) return false
      raw += s.sum
      l1 += s.l1
      const prev = cuts[sign]
      if (s.cut && (!prev || s.cut.dist < prev.dist)) cuts[sign] = s.cut
    }
    return true
  }
  if (!run(false)) return null
  const tail = () => [cuts[1], cuts[-1]].reduce((t, c) => t + (c ? Math.abs(c.f) * c.dist * 10 : 0), 0)
  let prev = raw * h
  let diff = Infinity
  let scale = 0
  for (let level = 1; level <= MAX_LEVEL; level++) {
    h /= 2
    if (!run(true)) return null
    const value = raw * h
    diff = Math.abs(value - prev)
    scale = l1 * h
    prev = value
    // convergence is quadratic: once two levels agree this closely the newer one is at roundoff
    if (level >= 3 && diff <= 1e-14 * scale) return { value, err: Math.max(diff, 4e-16 * scale) + tail(), l1: scale }
  }
  if (diff <= 1e-9 * scale) return { value: prev, err: diff + tail(), l1: scale }
  return null
}

const GK_X = [
  0.9914553711208126, 0.9491079123427585, 0.8648644233597691,
  0.7415311855993945, 0.5860872354676911, 0.4058451513773972,
  0.20778495500789848, 0,
]
const GK_WK = [
  0.022935322010529224, 0.06309209262997856, 0.10479001032225019,
  0.14065325971552592, 0.1690047266392679, 0.19035057806478542,
  0.20443294007529889, 0.20948214108472782,
]
const GK_WG = [0.1294849661688697, 0.27970539148927664, 0.3818300505051189, 0.4179591836734694]

type Panel = { a: number; b: number; value: number; err: number; l1: number }

function gk15(f: RealFn, a: number, b: number): Panel | null {
  const c = (a + b) / 2
  const r = (b - a) / 2
  let k = 0
  let g = 0
  let l1 = 0
  for (let i = 0; i < 8; i++) {
    const xs = i === 7 ? [c] : [c - r * GK_X[i]!, c + r * GK_X[i]!]
    for (const x of xs) {
      const fx = f(x)
      if (!Number.isFinite(fx)) return null
      k += GK_WK[i]! * fx
      l1 += GK_WK[i]! * Math.abs(fx)
      if (i % 2 === 1) g += GK_WG[(i - 1) / 2]! * fx
    }
  }
  const value = k * r
  const gauss = g * r
  return { a, b, value, err: Math.abs(value - gauss), l1: l1 * Math.abs(r) }
}

/** Adaptive Gauss–Kronrod 7/15, splitting the worst panel until the total error is small enough. */
export function gaussKronrod(
  f: RealFn,
  a: number,
  b: number,
  maxPanels = 2000,
  over = () => false,
): (Estimate & { l1: number }) | null {
  const first = gk15(f, a, b)
  if (!first) return null
  const panels: Panel[] = [first]
  for (let n = 1; n < maxPanels; n++) {
    let total = 0
    let err = 0
    let l1 = 0
    let worst = 0
    for (let i = 0; i < panels.length; i++) {
      const p = panels[i]!
      total += p.value
      err += p.err
      l1 += p.l1
      if (p.err > panels[worst]!.err) worst = i
    }
    if (err <= Math.max(1e-11 * l1, 1e-300)) return { value: total, err: Math.max(err, 4e-16 * l1), l1 }
    if (n % 16 === 0 && over()) return null
    const p = panels[worst]!
    const m = (p.a + p.b) / 2
    if (m <= p.a || m >= p.b) return null
    const left = gk15(f, p.a, m)
    const right = gk15(f, m, p.b)
    if (!left || !right) return null
    panels.splice(worst, 1, left, right)
  }
  return null
}

function finite(f: RealFn, a: number, b: number, budget: Budget): (Estimate & { l1: number }) | null {
  const est = doubleExponential(f, tanhSinhRule(a, b), budget)
  // a spike or kink the double exponential rule stepped over shows up as disagreement here
  const gk = gaussKronrod(f, a, b, est ? 200 : Math.floor(MAX_EVALS / 30), budget.work.over)
  // a check that only ever saw zeros has checked nothing
  const check = gk && gk.l1 > 0 ? gk : null
  if (est && check && Math.abs(est.value - check.value) > 10 * (est.err + check.err) + 1e-9 * est.l1) return null
  const out = est ?? check
  // every sample exactly 0 could still have stepped over a bump (e^-(x-737)² on 0..1000, e^-x on 0..1e6)
  return out && out.l1 === 0 && hiddenBump(f, a, b) ? null : out
}

/**
 * The sharpest peaks or dips an even scan sees. Splitting there crowds the rules' nodes in on them,
 * since both can step clean over a narrow bump (e^(-1000(x-3.7)²) + 1 on 0..10).
 */
function peaks(f: RealFn, a: number, b: number, budget: Budget): number[] {
  const n = 512
  const xs: number[] = []
  const ys: number[] = []
  for (let i = 1; i < n; i++) {
    const x = a + ((b - a) * i) / n
    const y = f(x)
    // a pole inside is for the rules to find
    if (!Number.isFinite(y) || spent(budget)) return []
    xs.push(x)
    ys.push(y)
  }
  const top = Math.max(...ys.map(Math.abs))
  const out: Array<{ i: number; k: number }> = []
  for (let i = 1; i + 1 < ys.length; i++) {
    const [l, c, r] = [ys[i - 1]!, ys[i]!, ys[i + 1]!]
    const k = Math.abs(2 * c - l - r)
    if (((c > l && c > r) || (c < l && c < r)) && k > 1e-6 * top) out.push({ i, k })
  }
  return out
    .sort((p, q) => q.k - p.k)
    .slice(0, 4)
    .map(({ i }) => extremum(f, xs[i - 1]!, xs[i + 1]!, ys[i]! > ys[i - 1]! ? -1 : 1))
    .sort((p, q) => p - q)
    .filter((x, i, cuts) => x > a && x < b && x !== cuts[i - 1])
}

/** Golden section down to adjacent floats, so the split lands on the peak or kink itself. */
function extremum(f: RealFn, lo: number, hi: number, sign: 1 | -1): number {
  const g = (x: number) => sign * f(x)
  const R = (Math.sqrt(5) - 1) / 2
  let a = lo
  let b = hi
  let c = b - R * (b - a)
  let d = a + R * (b - a)
  let gc = g(c)
  let gd = g(d)
  for (let k = 0; k < 80 && c < d; k++) {
    if (gc <= gd) {
      b = d
      d = c
      gd = gc
      c = b - R * (b - a)
      gc = g(c)
    } else {
      a = c
      c = d
      gc = gd
      d = a + R * (b - a)
      gd = g(d)
    }
  }
  return (a + b) / 2
}

/** Evenly across the range, and closing in on each end by halves. */
function hiddenBump(f: RealFn, a: number, b: number): boolean {
  const n = 4000
  const xs = Array.from({ length: n - 1 }, (_, i) => a + ((b - a) * (i + 1)) / n)
  for (let k = 12; k <= 40; k++) xs.push(a + (b - a) * 2 ** -k, b - (b - a) * 2 ** -k)
  return xs.some((x) => {
    const y = f(x)
    return Number.isFinite(y) && y !== 0
  })
}

/** ∫ f from a to b, with ±Infinity bounds allowed. */
export function integrate(f: RealFn, a: number, b: number): Estimate | null {
  if (Number.isNaN(a) || Number.isNaN(b)) return null
  if (a === b) return { value: 0, err: 0 }
  if (a > b) {
    const r = integrate(f, b, a)
    return r && { value: -r.value, err: r.err }
  }
  const budget = { evals: MAX_EVALS, work: workBudget(MAX_WORK) }
  let est: (Estimate & { l1: number }) | null
  if (a === -Infinity && b === Infinity) est = doubleExponential(f, sinhSinhRule(), budget)
  else if (b === Infinity) est = doubleExponential(f, expSinhRule(a), budget)
  else if (a === -Infinity) est = doubleExponential((y) => f(b - y), expSinhRule(0), budget)
  else {
    const ends = [a, ...peaks(f, a, b, budget), b]
    est = { value: 0, err: 0, l1: 0 }
    for (let i = 1; i < ends.length && est; i++) {
      const piece = finite(f, ends[i - 1]!, ends[i]!, budget)
      est = piece && { value: est.value + piece.value, err: est.err + piece.err, l1: est.l1 + piece.l1 }
    }
  }
  if (!est || !Number.isFinite(est.value)) return null
  // an infinite range that only ever sampled zeros has no scan to check it
  if (est.l1 === 0 && !(Number.isFinite(a) && Number.isFinite(b))) return null
  const value = justified(est.value, est.err, est.l1)
  // err covers the rounding too, so the shown value is within err of the truth
  return value == null ? null : { value, err: est.err + Math.abs(value - est.value) }
}

/** Neville extrapolation of g(h) to h = 0 over h, h/2, h/4, ..., stopping once roundoff wins. */
function extrapolate(vals: number[]): Estimate | null {
  const rows: number[][] = []
  let best: Estimate | null = null
  for (let i = 0; i < vals.length; i++) {
    const row = [vals[i]!]
    const prev = rows[rows.length - 1]
    if (prev) {
      let fac = 2
      for (let j = 1; j <= Math.min(prev.length, 8); j++, fac *= 2) {
        row[j] = row[j - 1]! + (row[j - 1]! - prev[j - 1]!) / (fac - 1)
        const err = Math.max(Math.abs(row[j]! - row[j - 1]!), Math.abs(row[j]! - prev[j - 1]!))
        if (!best || err < best.err) best = { value: row[j]!, err }
      }
      if (best && Math.abs(row[row.length - 1]! - prev[prev.length - 1]!) > 4 * best.err && i > 8) break
    }
    rows.push(row)
  }
  return best
}

/** The raw sequence settling on its own, for limits too ragged to extrapolate (x sin(1/x) at 0). */
function settles(vals: number[]): Estimate | null {
  if (vals.length < 24) return null
  const tail = vals.slice(-12)
  const last = tail[tail.length - 1]!
  const err = Math.max(...tail.map((v) => Math.abs(v - last)))
  return { value: last, err: err * 2 }
}

/** Aitken's Δ² for errors like C·h^p with any p (√x at 0), trusted where six in a row agree. */
function aitken(v: number[]): Estimate | null {
  const acc: number[] = []
  for (let k = 0; k + 2 < v.length; k++) {
    const den = v[k + 2]! - 2 * v[k + 1]! + v[k]!
    acc.push(den === 0 ? v[k + 2]! : v[k]! - (v[k + 1]! - v[k]!) ** 2 / den)
  }
  let best: Estimate | null = null
  for (let k = 0; k + 6 <= acc.length; k++) {
    const w = acc.slice(k, k + 6)
    const spread = Math.max(...w) - Math.min(...w)
    if (!best || spread * 2 < best.err) best = { value: w[5]!, err: spread * 2 }
  }
  return best
}

/**
 * The samples before roundoff takes over. (1 - cos(h))/h² slides toward 1/2 and sits there, then collapses
 * to 0 once cos(h) rounds to 1; a step far bigger than the smallest step so far marks that point.
 */
function cleanPrefix(vals: number[]): number[] {
  let start = 0
  while (start < vals.length && !Number.isFinite(vals[start]!)) start++
  let minStep = Infinity
  for (let k = start + 1; k < vals.length; k++) {
    if (!Number.isFinite(vals[k]!)) return vals.slice(start, k)
    const step = Math.abs(vals[k]! - vals[k - 1]!)
    if (minStep < Infinity && step > 100 * minStep) return vals.slice(start, k)
    minStep = Math.min(minStep, step)
  }
  return vals.slice(start)
}

/**
 * ±Infinity when |g| grows steadily from the start, like 1/x at 0. Growth that only shows up late is
 * roundoff: (cos(h) - 1 + h²/2)/h⁴ turns into 1/(2h²) once cos(h) rounds to 1.
 */
function diverges(vals: number[]): number | null {
  const finite = vals.slice(1)
  const cut = finite.findIndex((v) => !Number.isFinite(v))
  const head = cut < 0 ? finite : finite.slice(0, cut)
  const rest = cut < 0 ? [] : finite.slice(cut)
  if (head.length < 5) return null
  const sign = Math.sign(head[0]!)
  if (!sign || head.some((v) => Math.sign(v) !== sign)) return null
  for (let i = 1; i < head.length; i++) {
    const grew = Math.abs(head[i]!) - Math.abs(head[i - 1]!)
    if (!(grew > 0)) return null
    if (i > 1 && grew < 0.9 * (Math.abs(head[i - 1]!) - Math.abs(head[i - 2]!))) return null
  }
  const last = Math.abs(head[head.length - 1]!)
  // the engine reports an overflow (e^1000) or tan at its pole as undefined, so a blowup can end in NaN
  if (rest.some((v) => v !== sign * Infinity && !Number.isNaN(v))) return null
  if (rest.some(Number.isNaN) && last < 1e6) return null
  return rest.length || last > 1e6 ? sign * Infinity : null
}

/** Every sample further from `value` than the one before: 1 + 1e-10/h looks settled until the prefix is cut. */
function driftsAway(vals: number[], value: number): boolean {
  const dev = vals.filter(Number.isFinite).map((v) => Math.abs(v - value))
  return dev.length >= 16 && dev.every((d, i) => i === 0 || d > dev[i - 1]!)
}

const LIMIT_STEPS = 44

function oneSided(f: RealFn, to: number, side: 1 | -1): Estimate | null {
  const g: RealFn = Number.isFinite(to) ? (h) => f(to + side * h) : (h) => f(Math.sign(to) / h)
  let h = Number.isFinite(to) ? 0.125 * Math.max(1, Math.abs(to)) : 0.125
  const vals: number[] = []
  for (let i = 0; i < LIMIT_STEPS; i++, h /= 2) vals.push(g(h))
  const clean = cleanPrefix(vals)
  // richardson fits whole powers of h, aitken any single power; the tighter bound wins
  let best: Estimate | null = null
  if (clean.length >= 8) {
    for (const t of [extrapolate(clean), settles(clean), aitken(clean)]) {
      if (t && Number.isFinite(t.value) && (!best || t.err < best.err)) best = t
    }
  }
  // extrapolating 1/h or 1/√h gives a confident 0 (an anti-limit); the samples must already be close
  const near = best && Math.abs(clean[clean.length - 1]! - best.value) <= 1e-2 * Math.max(1, Math.abs(best.value))
  if (best && near && best.err <= 1e-7 * Math.max(1, Math.abs(best.value)) && !driftsAway(vals, best.value)) return best
  const inf = diverges(vals)
  return inf == null ? null : { value: inf, err: 0 }
}

/** side 0 is two-sided; both sides must agree or there's no limit. */
export function limit(f: RealFn, to: number, side: -1 | 0 | 1): Estimate | null {
  if (Number.isNaN(to)) return null
  if (!Number.isFinite(to)) side = to > 0 ? -1 : 1
  let est: Estimate | null
  if (side !== 0) est = oneSided(f, to, side)
  else {
    const right = oneSided(f, to, 1)
    const left = right && oneSided(f, to, -1)
    if (!right || !left) return null
    if (!Number.isFinite(right.value) || !Number.isFinite(left.value)) {
      return right.value === left.value ? right : null
    }
    const err = Math.max(right.err, left.err)
    if (Math.abs(right.value - left.value) > 2 * err + 1e-9 * Math.max(1, Math.abs(right.value))) return null
    est = { value: (right.value + left.value) / 2, err: err + Math.abs(right.value - left.value) }
  }
  if (!est) return null
  if (!Number.isFinite(est.value)) return est
  const value = justified(est.value, est.err, Math.max(1, Math.abs(est.value)))
  return value == null ? null : { value, err: est.err + Math.abs(value - est.value) }
}

/** Ridders' method: central differences extrapolated as the step shrinks. */
export function derivativeAt(f: RealFn, x: number): Estimate | null {
  const CON = 1.4
  const CON2 = CON * CON
  const table: number[][] = []
  let h = 0.1 * Math.max(1, Math.abs(x))
  let best: Estimate | null = null
  for (let i = 0; i < 14; i++, h /= CON) {
    const d = (f(x + h) - f(x - h)) / (2 * h)
    if (!Number.isFinite(d)) return null
    table[i] = [d]
    let fac = CON2
    for (let j = 1; j <= i; j++, fac *= CON2) {
      const prev = table[i]![j - 1]!
      const up = table[i - 1]![j - 1]!
      table[i]![j] = (prev * fac - up) / (fac - 1)
      const err = Math.max(Math.abs(table[i]![j]! - prev), Math.abs(table[i]![j]! - up))
      if (!best || err <= best.err) best = { value: table[i]![j]!, err }
    }
    if (i > 0 && best && Math.abs(table[i]![i]! - table[i - 1]![i - 1]!) >= 2 * best.err) break
  }
  return best
}
