import { parseFunctionDef } from './evaluate'
import { formatNumber } from './format'
import { namesPattern } from './math'
import { compileScientific, evalScientific, splitGluedFunctions, type AngleMode } from './scientific'
import type { UserFunction } from './types'

export const DEFAULT_GRAPH_DOMAIN: readonly [number, number] = [-10, 10]

/** Two turns either side, for x in degrees. */
const DEGREE_GRAPH_DOMAIN: readonly [number, number] = [-360, 360]

/** Includes both endpoints. */
export const DEFAULT_GRAPH_SAMPLES = 401

export interface GraphOptions {
  domain?: [number, number]
  sampleCount?: number
  variables?: Record<string, number>
  functions?: Record<string, UserFunction>
  angleMode?: AngleMode
  ans?: number
  /** Default true. */
  findRoots?: boolean
}

export interface GraphIntent {
  /** In the free variable `x`. */
  expression: string
  /** e.g. `x^3`, `f(x)`, `y = x^3`. */
  label: string
  /** Present for `graph f(x) = …`; the caller registers it. */
  functionDef?: { name: string; params: string[]; body: string }
}

export interface GraphPoint {
  x: number
  /** null where the curve is undefined or breaks. */
  y: number | null
}

export type CriticalKind = 'min' | 'max' | 'critical'

export interface CriticalPoint {
  x: number
  y: number
  kind: CriticalKind
}

export interface GraphRoot {
  x: number
}

export interface YScale {
  min: number
  max: number
}

export interface GraphResult {
  intent: GraphIntent
  points: GraphPoint[]
  domain: [number, number]
  yScale: YScale
  criticalPoints: CriticalPoint[]
  roots: GraphRoot[]
  /** null when the curve doesn't depend on the angle mode. */
  angleUnit: AngleMode | null
  /** For readouts between samples. */
  y: GraphY
  error?: string
}

const GRAPH_CMD = /^\s*graph\s+(.+?)\s*$/is
const Y_EQ = /^y\s*=\s*(.+)$/is
const BARE_NAME = /^([A-Za-z][A-Za-z0-9]*)$/

/** `graph f` looks `f` up in `options.functions`. */
export function parseGraphIntent(
  input: string,
  options: Pick<GraphOptions, 'functions'> = {},
): GraphIntent | null {
  const m = input.match(GRAPH_CMD)
  if (!m) return null
  const rest = splitGluedFunctions(m[1]!.trim())
  if (!rest) return null

  const fnDef = parseFunctionDef(rest)
  if (fnDef) {
    if (fnDef.params.length !== 1) return { expression: '', label: rest, functionDef: fnDef }
    const param = fnDef.params[0]!
    const expression = param === 'x' ? fnDef.body : rewriteParam(fnDef.body, param, 'x')
    return {
      expression,
      label: `${fnDef.name}(${fnDef.params.join(', ')})`,
      functionDef: fnDef,
    }
  }

  const yEq = rest.match(Y_EQ)
  if (yEq) {
    const expression = yEq[1]!.trim()
    if (!expression) return null
    return { expression, label: `y = ${expression}` }
  }

  const bare = rest.match(BARE_NAME)
  if (bare) {
    const name = bare[1]!
    const def = options.functions?.[name]
    if (def) {
      if (def.params.length !== 1) return { expression: '', label: name }
      const param = def.params[0]!
      // inlined so sampling compiles one expression instead of making a call per sample
      return { expression: rewriteParam(def.body, param, 'x'), label: `${name}(${param})` }
    }
  }

  return { expression: rest, label: rest }
}

/** True even for an incomplete command. */
export function isGraphCommand(input: string): boolean {
  return /^\s*graph(?:\s|$)/i.test(input)
}

export function evaluateGraphY(
  expression: string,
  x: number,
  options: GraphOptions = {},
): number | null {
  if (!expression.trim() || !Number.isFinite(x)) return null
  const variables = { ...options.variables, x }
  try {
    const v = evalScientific(expression, {
      ans: options.ans,
      angleMode: options.angleMode,
      variables,
      functions: options.functions,
    })
    if (!v || v.kind !== 'number' || !Number.isFinite(v.n)) return null
    return v.n
  } catch {
    return null
  }
}

export type GraphY = (x: number) => number | null

/** Same results as evaluateGraphY, compiled once. */
export function compileGraphY(expression: string, options: GraphOptions = {}): GraphY {
  const f = expression.trim()
    ? compileScientific(
        expression,
        {
          ans: options.ans,
          angleMode: options.angleMode,
          variables: options.variables,
          functions: options.functions,
        },
        'x',
      )
    : null
  if (!f) return () => null
  return (x) => {
    if (!Number.isFinite(x)) return null
    const v = f(x)
    if (!v || v.kind !== 'number' || !Number.isFinite(v.n)) return null
    return v.n
  }
}

export function sampleGraph(expression: string, options: GraphOptions = {}): GraphPoint[] {
  return sampleWith(compileGraphY(expression, options), options)
}

function sampleWith(y: GraphY, options: GraphOptions): GraphPoint[] {
  const [xMin, xMax] = normalizeDomain(options.domain)
  const n = Math.max(2, Math.floor(options.sampleCount ?? DEFAULT_GRAPH_SAMPLES))
  const points: GraphPoint[] = []
  for (let i = 0; i < n; i++) {
    const x = xMin + (xMax - xMin) * (i / (n - 1))
    points.push({ x, y: y(x) })
  }
  return points
}

/**
 * Add samples where the uniform grid is too coarse for the view: bisect to where the curve's
 * domain ends (so √(9 − x²) reaches the axis), subdivide steep visible stretches, and break the
 * path (a `null` sample) where a jump survives down to a vanishing interval (floor, tan's poles).
 */
function refineSamples(points: GraphPoint[], f: GraphY, view: YScale): GraphPoint[] {
  if (points.length < 2) return points
  const span = points[points.length - 1]!.x - points[0]!.x
  const minWidth = span * 1e-9
  const steep = (view.max - view.min) * 0.02
  let budget = points.length * 8
  const out: GraphPoint[] = [points[0]!]
  const sample = (x: number): GraphPoint => {
    budget--
    return { x, y: f(x) }
  }
  const offView = (a: number, b: number) =>
    (a > view.max && b > view.max) || (a < view.min && b < view.min)

  // pushes the samples strictly between a and b; the caller pushes b
  const between = (a: GraphPoint, b: GraphPoint): void => {
    if (a.y == null && b.y == null) return
    if (a.y == null || b.y == null) {
      // bisect for the last defined x
      let def = a.y == null ? b : a
      let undef = a.y == null ? a : b
      for (let k = 0; k < 40 && budget > 0; k++) {
        const m = sample((def.x + undef.x) / 2)
        if (m.x === def.x || m.x === undef.x) break
        if (m.y == null) undef = m
        else def = m
      }
      if (def === a || def === b) return
      if (a.y == null) {
        out.push(def)
        between(def, b)
      } else {
        between(a, def)
        out.push(def)
      }
      return
    }
    if (Math.abs(b.y - a.y) <= steep || offView(a.y, b.y) || budget <= 0) return
    if (b.x - a.x <= minWidth) {
      out.push({ x: (a.x + b.x) / 2, y: null })
      return
    }
    const m = sample((a.x + b.x) / 2)
    between(a, m)
    out.push(m)
    between(m, b)
  }

  for (let i = 1; i < points.length; i++) {
    between(points[i - 1]!, points[i]!)
    out.push(points[i]!)
  }
  return out
}

/**
 * A central percentile band keeps poles from dominating the view, but only when there are real
 * outliers. A range that nearly reaches zero is widened to include the x-axis.
 */
export function autoYScale(
  points: GraphPoint[],
  opts: { pad?: number; percentile?: number; fallback?: number } = {},
): YScale {
  const pad = opts.pad ?? 0.08
  const percentile = opts.percentile ?? 0.02
  const fallback = opts.fallback ?? 10
  const ys = points.map((p) => p.y).filter((y): y is number => y != null && Number.isFinite(y))
  if (ys.length === 0) return { min: -fallback, max: fallback }

  const sorted = [...ys].sort((a, b) => a - b)
  const lo = quantile(sorted, percentile)
  const hi = quantile(sorted, 1 - percentile)
  let min = Number.isFinite(lo) ? lo : sorted[0]!
  let max = Number.isFinite(hi) ? hi : sorted[sorted.length - 1]!
  // trimming is for poles; a curve without them (eˣ, a semicircle) is shown whole
  const fullMin = sorted[0]!
  const fullMax = sorted[sorted.length - 1]!
  if (max > min && fullMax - fullMin <= 2 * (max - min)) {
    min = fullMin
    max = fullMax
  }
  if (min > 0 && min <= 0.25 * (max - min)) min = 0
  else if (max < 0 && -max <= 0.25 * (max - min)) max = 0

  if (min === max) {
    const bump = Math.max(1, Math.abs(min) * 0.1)
    min -= bump
    max += bump
  }

  const margin = (max - min) * pad
  return { min: min - margin, max: max + margin }
}

/** Where the sampled slope changes sign. With `f`, each is refined by golden-section search and poles are dropped. */
export function findCriticalPoints(points: GraphPoint[], f?: GraphY): CriticalPoint[] {
  const out: CriticalPoint[] = []
  for (let i = 1; i < points.length - 1; i++) {
    const prev = points[i - 1]!
    const cur = points[i]!
    const next = points[i + 1]!
    if (prev.y == null || cur.y == null || next.y == null) continue
    const dx1 = cur.x - prev.x
    const dx2 = next.x - cur.x
    if (dx1 === 0 || dx2 === 0) continue
    const d1 = (cur.y - prev.y) / dx1
    const d2 = (next.y - cur.y) / dx2
    if (!Number.isFinite(d1) || !Number.isFinite(d2)) continue
    const s1 = Math.sign(d1)
    const s2 = Math.sign(d2)
    if (s1 === 0 || s2 === 0 || s1 === s2) continue
    const kind: CriticalKind = s1 > 0 ? 'max' : 'min'
    if (!f) {
      out.push({ x: cur.x, y: cur.y, kind })
      continue
    }
    const best = goldenSection(f, prev.x, next.x, kind)
    if (!best) continue
    // a smooth turn moves y by less than the local variation; a pole runs away
    const variation = Math.abs(cur.y - prev.y) + Math.abs(next.y - cur.y)
    if (Math.abs(best.y - cur.y) > 4 * variation) continue
    // golden section only pins x to about √ε, so report the simplest x f can't tell apart (1, not 0.99999999)
    const sign = kind === 'max' ? -1 : 1
    const tol = 8 * Number.EPSILON * Math.max(Math.abs(best.y), Math.abs(cur.y))
    const x = tidy(best.x, (c) => {
      if (c < prev.x || c > next.x) return false
      const y = f(c)
      return y != null && sign * y <= sign * best.y + tol
    })
    out.push({ x, y: x === best.x ? best.y : f(x)!, kind })
  }
  return dedupeByX(out)
}

/**
 * With `f`, sign changes are refined by bisection and poles and jumps are dropped.
 * A run of zero samples is one root; an all-zero curve has none.
 */
export function findRoots(points: GraphPoint[], f?: GraphY): GraphRoot[] {
  const out: GraphRoot[] = []
  const isZero = (y: number | null) => y != null && Math.abs(y) <= 1e-12
  const last = points.length - 1
  for (let i = 0; i <= last; i++) {
    const p = points[i]!
    if (isZero(p.y)) {
      let j = i
      while (j < last && isZero(points[j + 1]!.y)) j++
      // report the end of the run that meets the curve (`floor(x)`, `max(0, x)` give 0)
      if (i > 0 || j < last) {
        const k = i === 0 ? j : i
        const x = points[k]!.x
        const y = Math.abs(points[k]!.y!)
        const lo = points[Math.max(0, k - 1)]!.x
        const hi = points[Math.min(last, k + 1)]!.x
        // a grid x like 0.3000000000000007 is reported as 0.3 when f(0.3) is as small
        out.push({ x: f ? tidy(x, (c) => c > lo && c < hi && Math.abs(f(c) ?? Infinity) <= y) : x })
      }
      i = j
      continue
    }
    if (i === 0) continue
    const prev = points[i - 1]!
    if (prev.y == null || p.y == null || isZero(prev.y)) continue
    if (Math.sign(prev.y) === Math.sign(p.y)) continue
    const x = f ? bisectRoot(f, prev.x, prev.y, p.x, p.y) : prev.x + (prev.y / (prev.y - p.y)) * (p.x - prev.x)
    if (x != null) out.push({ x })
  }
  return dedupeByX(out)
}

function bisectRoot(f: GraphY, lo: number, yLo: number, hi: number, yHi: number): number | null {
  const scale = Math.max(Math.abs(yLo), Math.abs(yHi))
  let a = lo
  let b = hi
  let ya = yLo
  for (let k = 0; k < 200; k++) {
    const mid = (a + b) / 2
    if (mid <= a || mid >= b) break
    const ym = f(mid)
    if (ym == null) return null
    if (ym === 0) {
      a = b = mid
      break
    }
    if (Math.sign(ym) === Math.sign(ya)) {
      a = mid
      ya = ym
    } else b = mid
  }
  const fa = f(a)
  const fb = f(b)
  if (fa == null || fb == null) return null
  const [x, y] = Math.abs(fa) <= Math.abs(fb) ? [a, fa] : [b, fb]
  if (Math.abs(y) > 1e-6 * scale) return null
  // prefer 0.3 over 0.30000000000000004 and 0 over 5e-324 when f can't tell them apart
  const tol = Math.abs(y) + 8 * Number.EPSILON * scale
  return tidy(x, (c) => {
    if (c < lo || c > hi) return false
    const yc = f(c)
    return yc != null && Math.abs(yc) <= tol
  })
}

/** The shortest decimal near `x` that `accept` still takes: 0, then 1, 2, ... significant digits. */
export function tidy(x: number, accept: (c: number) => boolean): number {
  if (x !== 0 && accept(0)) return 0
  for (let digits = 1; digits < 17; digits++) {
    const c = Number(x.toPrecision(digits))
    if (c === x) return x
    if (accept(c)) return c
  }
  return x
}

const INV_PHI = (Math.sqrt(5) - 1) / 2

function goldenSection(f: GraphY, lo: number, hi: number, kind: CriticalKind): { x: number; y: number } | null {
  const sign = kind === 'max' ? -1 : 1
  const g = (x: number) => {
    const y = f(x)
    return y == null ? null : sign * y
  }
  let a = lo
  let b = hi
  let c = b - INV_PHI * (b - a)
  let d = a + INV_PHI * (b - a)
  let gc = g(c)
  let gd = g(d)
  for (let k = 0; k < 200 && c < d; k++) {
    if (gc == null || gd == null) return null
    if (gc < gd) {
      b = d
      d = c
      gd = gc
      c = b - INV_PHI * (b - a)
      gc = g(c)
    } else {
      a = c
      c = d
      gc = gd
      d = a + INV_PHI * (b - a)
      gd = g(d)
    }
  }
  const x = (a + b) / 2
  const y = f(x)
  return y == null ? null : { x, y }
}

/** Null when the input is not a graph command. */
export function buildGraph(input: string, options: GraphOptions = {}): GraphResult | null {
  const intent = parseGraphIntent(input, options)
  if (!intent) return null

  if (!intent.expression) {
    const msg =
      intent.functionDef && intent.functionDef.params.length !== 1
        ? 'Graphing requires a single-variable function'
        : 'Unknown function — define it first (e.g. f(x) = …)'
    return {
      intent,
      points: [],
      domain: normalizeDomain(options.domain),
      yScale: { min: -10, max: 10 },
      criticalPoints: [],
      roots: [],
      angleUnit: null,
      y: () => null,
      error: msg,
    }
  }

  // an inline def is usable before history registers it
  const functions: Record<string, UserFunction> = { ...options.functions }
  if (intent.functionDef) {
    functions[intent.functionDef.name] = {
      params: intent.functionDef.params,
      body: intent.functionDef.body,
    }
  }

  const mode: AngleMode = options.angleMode ?? 'rad'
  const base: GraphOptions = { ...options, functions }
  const f = compileGraphY(intent.expression, { ...base, angleMode: mode })
  const angleUnit = dependsOnAngle(f, compileGraphY(intent.expression, { ...base, angleMode: mode === 'deg' ? 'rad' : 'deg' }))
    ? mode
    : null
  const domain = normalizeDomain(options.domain, angleUnit === 'deg' ? DEGREE_GRAPH_DOMAIN : DEFAULT_GRAPH_DOMAIN)
  const uniform = sampleWith(f, { ...base, domain })
  const yScale = autoYScale(uniform)
  if (!uniform.some((p) => p.y != null)) {
    return {
      intent,
      points: uniform,
      domain,
      yScale,
      criticalPoints: [],
      roots: [],
      angleUnit,
      y: f,
      error: 'Could not evaluate expression over the domain',
    }
  }

  const points = refineSamples(uniform, f, yScale)
  return {
    intent,
    points,
    domain,
    yScale,
    criticalPoints: findCriticalPoints(points, f),
    roots: options.findRoots !== false ? findRoots(points, f) : [],
    angleUnit,
    y: f,
  }
}

/** Compares a degree and a radian compilation at a few probe points. */
export function dependsOnAngle(a: GraphY, b: GraphY): boolean {
  return [0.37, 1.1, 2.9, -4.3, 7.7].some((x) => {
    const ya = a(x)
    const yb = b(x)
    if (ya == null || yb == null) return ya !== yb
    return Math.abs(ya - yb) > 1e-9 * Math.max(1, Math.abs(ya))
  })
}

/**
 * The window a graph opens on: ±10 (±360 for trig in degrees), narrowed around the roots and
 * extrema when they all sit in a small middle part of it, so x³ − 3x shows its hump and dip.
 */
export function graphHome(input: string, options: GraphOptions = {}): [number, number] {
  const g = buildGraph(input, { ...options, domain: undefined })
  if (!g || g.error) return g?.domain ?? [DEFAULT_GRAPH_DOMAIN[0], DEFAULT_GRAPH_DOMAIN[1]]
  const [lo, hi] = g.domain
  const xs = [...g.roots.map((r) => r.x), ...g.criticalPoints.map((c) => c.x)]
  if (xs.length < 2) return g.domain
  const a = Math.min(...xs)
  const b = Math.max(...xs)
  const span = Math.max((b - a) * 1.5, (hi - lo) / 10)
  if (!(b > a) || span > 0.6 * (hi - lo)) return g.domain
  const mid = (a + b) / 2
  const step = niceStep(span / 10, 'floor')
  return [Math.floor((mid - span / 2) / step + 1e-9) * step, Math.ceil((mid + span / 2) / step - 1e-9) * step].map(
    (v) => Number(v.toPrecision(12)),
  ) as [number, number]
}

function normalizeDomain(
  domain: [number, number] | undefined,
  fallback: readonly [number, number] = DEFAULT_GRAPH_DOMAIN,
): [number, number] {
  const lo = domain?.[0] ?? fallback[0]
  const hi = domain?.[1] ?? fallback[1]
  if (!Number.isFinite(lo) || !Number.isFinite(hi) || lo === hi) {
    return [fallback[0], fallback[1]]
  }
  return lo < hi ? [lo, hi] : [hi, lo]
}

/** A 1, 2 or 5 × 10ⁿ step at or above (`ceil`) or at or below (`floor`) `raw`. */
function niceStep(raw: number, round: 'ceil' | 'floor'): number {
  const mag = 10 ** Math.floor(Math.log10(raw))
  const steps = [1, 2, 5, 10].map((m) => m * mag)
  return round === 'ceil' ? steps.find((s) => s >= raw * (1 - 1e-9))! : [...steps].reverse().find((s) => s <= raw * (1 + 1e-9))!
}

export interface GraphTick {
  value: number
  label: string
}

const MINUS = '−'

/** Display only; copied text keeps `-`. */
export function withMinus(s: string): string {
  return s.replace(/^-/, MINUS)
}

/**
 * Plain axes step 1, 2 or 5 × 10ⁿ. An angle axis steps in multiples of π (rad) or 15° to 720°
 * (deg) while those stay readable, labelled `π/2`, `−2π`, `90°`.
 */
export function graphTicks(lo: number, hi: number, unit: AngleMode | null = null, target = 5): GraphTick[] {
  const span = hi - lo
  if (!(span > 0) || !Number.isFinite(span)) return []
  const angle = angleTicks(lo, hi, unit)
  if (angle) return angle
  const step = niceStep(span / target, 'ceil')
  const out: GraphTick[] = []
  for (let k = Math.ceil(lo / step - 1e-9); k * step <= hi + step * 1e-9 && out.length < 12; k++) {
    const value = Number((k * step).toPrecision(12))
    out.push({ value, label: withMinus(formatNumber(value, 6)) })
  }
  return out
}

function angleTicks(lo: number, hi: number, unit: AngleMode | null): GraphTick[] | null {
  if (!unit) return null
  const span = hi - lo
  // at most 7 steps keeps labels about 90px apart in the panel
  const fits = (step: number) => span / step <= 7
  if (unit === 'deg') {
    const step = [15, 30, 45, 90, 180, 360, 720].find(fits)
    if (!step || span / step < 2) return null
    return stepTicks(lo, hi, step, (k) => `${k * step < 0 ? MINUS : ''}${Math.abs(k * step)}°`)
  }
  // steps of π/4 up to 8π
  const quarter = [1, 2, 4, 8, 16, 32].find((q) => fits((q * Math.PI) / 4))
  if (!quarter || span / ((quarter * Math.PI) / 4) < 2) return null
  return stepTicks(lo, hi, (quarter * Math.PI) / 4, (k) => piLabel(k * quarter))
}

function stepTicks(lo: number, hi: number, step: number, label: (k: number) => string): GraphTick[] {
  const out: GraphTick[] = []
  for (let k = Math.ceil(lo / step - 1e-9); k * step <= hi + step * 1e-9; k++) {
    out.push({ value: k * step, label: label(k) })
  }
  return out
}

/** `quarters`·π/4 as `0`, `π/4`, `−3π/2`, `2π`. */
function piLabel(quarters: number): string {
  if (quarters === 0) return '0'
  const sign = quarters < 0 ? MINUS : ''
  let num = Math.abs(quarters)
  let den = 4
  while (den > 1 && num % 2 === 0) {
    num /= 2
    den /= 2
  }
  return `${sign}${num === 1 ? '' : num}π${den === 1 ? '' : `/${den}`}`
}

function rewriteParam(body: string, from: string, to: string): string {
  if (from === to) return body
  return body.replace(new RegExp(`(?<![A-Za-z_])${namesPattern([from])}(?![A-Za-z0-9_])`, 'g'), to)
}

function quantile(sorted: number[], q: number): number {
  if (sorted.length === 0) return Number.NaN
  if (sorted.length === 1) return sorted[0]!
  const pos = (sorted.length - 1) * q
  const base = Math.floor(pos)
  const rest = pos - base
  const a = sorted[base]!
  const b = sorted[Math.min(base + 1, sorted.length - 1)]!
  return a + rest * (b - a)
}

function dedupeByX<T extends { x: number }>(pts: T[], tol = 1e-6): T[] {
  const out: T[] = []
  for (const p of pts) {
    const last = out[out.length - 1]
    if (last && Math.abs(last.x - p.x) <= tol) continue
    out.push(p)
  }
  return out
}
