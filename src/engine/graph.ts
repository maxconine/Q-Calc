import { parseFunctionDef } from './evaluate'
import { compileScientific, evalScientific, type AngleMode } from './scientific'
import type { UserFunction } from './types'

/** Default plot domain for y = f(x). */
export const DEFAULT_GRAPH_DOMAIN: readonly [number, number] = [-10, 10]

/** Sample count across the domain (inclusive endpoints). */
export const DEFAULT_GRAPH_SAMPLES = 401

export interface GraphOptions {
  domain?: [number, number]
  sampleCount?: number
  variables?: Record<string, number>
  functions?: Record<string, UserFunction>
  angleMode?: AngleMode
  ans?: number
  /** When true (default), include approximate roots from sign changes. */
  findRoots?: boolean
}

/** Parsed graph command — expression to plot plus optional inline function registration. */
export interface GraphIntent {
  /** Expression in free variable `x` evaluated as y. */
  expression: string
  /** Human label (e.g. `x^3`, `f(x)`, `y = x^3`). */
  label: string
  /**
   * Present for `graph f(x) = …` — caller should register this definition
   * (same shape as evaluateSheet function results).
   */
  functionDef?: { name: string; params: string[]; body: string }
}

export interface GraphPoint {
  x: number
  /** `null` when the sample is non-finite / discontinuous. */
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
  error?: string
}

const GRAPH_CMD = /^\s*graph\s+(.+?)\s*$/is
const Y_EQ = /^y\s*=\s*(.+)$/is
const BARE_NAME = /^([A-Za-z][A-Za-z0-9]*)$/

/**
 * Parse a `graph …` command. Returns `null` if the input is not a graph command.
 * Name lookup (`graph f`) resolves against `options.functions`.
 */
export function parseGraphIntent(
  input: string,
  options: Pick<GraphOptions, 'functions'> = {},
): GraphIntent | null {
  const m = input.match(GRAPH_CMD)
  if (!m) return null
  const rest = m[1]!.trim()
  if (!rest) return null

  const fnDef = parseFunctionDef(rest)
  if (fnDef) {
    if (fnDef.params.length !== 1) {
      return {
        expression: '',
        label: rest,
        functionDef: fnDef,
      }
    }
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
      if (def.params.length !== 1) {
        return { expression: '', label: name }
      }
      const param = def.params[0]!
      // Inline the body so sampling compiles one expression instead of re-evaluating a call per sample.
      const expression = rewriteParam(def.body, param, 'x')
      return { expression, label: `${name}(${param})` }
    }
  }

  return { expression: rest, label: rest }
}

/** True when trimmed input starts a graph command (even if incomplete). */
export function isGraphCommand(input: string): boolean {
  return /^\s*graph(?:\s|$)/i.test(input)
}

/** Evaluate expression at a single x; returns null for non-finite / failed eval. */
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

type GraphY = (x: number) => number | null

/** Compile the expression once into y(x); same results as evaluateGraphY. */
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

/** Sample y = f(x) over the domain. */
export function sampleGraph(expression: string, options: GraphOptions = {}): GraphPoint[] {
  return sampleWith(compileGraphY(expression, options), options)
}

function sampleWith(y: GraphY, options: GraphOptions): GraphPoint[] {
  const [xMin, xMax] = normalizeDomain(options.domain)
  const n = Math.max(2, Math.floor(options.sampleCount ?? DEFAULT_GRAPH_SAMPLES))
  const points: GraphPoint[] = []
  const span = xMax - xMin
  for (let i = 0; i < n; i++) {
    const t = i / (n - 1)
    const x = xMin + span * t
    points.push({ x, y: y(x) })
  }
  return points
}

/**
 * Auto y-scale from finite samples. Uses a central percentile band so vertical
 * asymptotes do not dominate the view, then pads slightly.
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

  if (min === max) {
    const bump = Math.max(1, Math.abs(min) * 0.1)
    min -= bump
    max += bump
  }

  const spread = max - min
  const margin = spread * pad
  return { min: min - margin, max: max + margin }
}

/**
 * Critical points where the finite-difference slope changes sign. With `f`, each is refined by
 * golden-section search on its neighbouring interval, and spikes at poles are dropped.
 */
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
    if (d1 === 0 && d2 === 0) continue
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
    // A smooth turn moves y by less than the local variation; a pole runs away.
    const variation = Math.abs(cur.y - prev.y) + Math.abs(next.y - cur.y)
    if (Math.abs(best.y - cur.y) > 4 * variation) continue
    out.push({ ...best, kind })
  }
  return dedupeByX(out)
}

/**
 * Roots where consecutive samples change sign, refined by bisection with `f` (poles and jumps,
 * where |f| stays large, are dropped). A run of zero samples is one root; an all-zero curve has none.
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
      // Report the end of the run that meets the curve (`floor(x)` → 0, `max(0, x)` → 0).
      if (i > 0 || j < last) out.push({ x: points[i === 0 ? j : i]!.x })
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
  return dedupeRoots(out)
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
    if (ym === 0) return mid
    if (Math.sign(ym) === Math.sign(ya)) {
      a = mid
      ya = ym
    } else b = mid
  }
  const fa = f(a)
  const fb = f(b)
  if (fa == null || fb == null) return null
  const [x, y] = Math.abs(fa) <= Math.abs(fb) ? [a, fa] : [b, fb]
  return Math.abs(y) <= 1e-6 * scale ? x : null
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

/**
 * Full pipeline: parse graph command → sample → y-scale → critical points → roots.
 * Returns `null` when input is not a graph command.
 */
export function buildGraph(input: string, options: GraphOptions = {}): GraphResult | null {
  const intent = parseGraphIntent(input, options)
  if (!intent) return null

  const domain = normalizeDomain(options.domain)
  const findRootsFlag = options.findRoots !== false

  if (!intent.expression) {
    const msg =
      intent.functionDef && intent.functionDef.params.length !== 1
        ? 'Graphing requires a single-variable function'
        : 'Unknown function — define it first (e.g. f(x) = …)'
    return {
      intent,
      points: [],
      domain,
      yScale: { min: -10, max: 10 },
      criticalPoints: [],
      roots: [],
      error: msg,
    }
  }

  // Inline defs are available for sampling even before history registers them.
  const functions: Record<string, UserFunction> = { ...options.functions }
  if (intent.functionDef) {
    functions[intent.functionDef.name] = {
      params: intent.functionDef.params,
      body: intent.functionDef.body,
    }
  }

  // Always radians: in degrees sin(x) over [-10, 10] is a flat line.
  const sampleOpts: GraphOptions = { ...options, angleMode: 'rad', functions, domain }
  const f = compileGraphY(intent.expression, sampleOpts)
  const points = sampleWith(f, sampleOpts)
  const finite = points.some((p) => p.y != null)
  if (!finite) {
    return {
      intent,
      points,
      domain,
      yScale: autoYScale(points),
      criticalPoints: [],
      roots: [],
      error: 'Could not evaluate expression over the domain',
    }
  }

  return {
    intent,
    points,
    domain,
    yScale: autoYScale(points),
    criticalPoints: findCriticalPoints(points, f),
    roots: findRootsFlag ? findRoots(points, f) : [],
  }
}

function normalizeDomain(domain?: [number, number]): [number, number] {
  const lo = domain?.[0] ?? DEFAULT_GRAPH_DOMAIN[0]
  const hi = domain?.[1] ?? DEFAULT_GRAPH_DOMAIN[1]
  if (!Number.isFinite(lo) || !Number.isFinite(hi) || lo === hi) {
    return [DEFAULT_GRAPH_DOMAIN[0], DEFAULT_GRAPH_DOMAIN[1]]
  }
  return lo < hi ? [lo, hi] : [hi, lo]
}

/** Replace standalone identifier `from` with `to` in an expression body. */
function rewriteParam(body: string, from: string, to: string): string {
  if (from === to) return body
  const re = new RegExp(`(?<![A-Za-z_])${escapeRegExp(from)}(?![A-Za-z0-9_])`, 'g')
  return body.replace(re, to)
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
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

function dedupeByX(pts: CriticalPoint[], tol = 1e-6): CriticalPoint[] {
  const out: CriticalPoint[] = []
  for (const p of pts) {
    const last = out[out.length - 1]
    if (last && Math.abs(last.x - p.x) <= tol) continue
    out.push(p)
  }
  return out
}

function dedupeRoots(roots: GraphRoot[], tol = 1e-6): GraphRoot[] {
  const out: GraphRoot[] = []
  for (const r of roots) {
    const last = out[out.length - 1]
    if (last && Math.abs(last.x - r.x) <= tol) continue
    out.push(r)
  }
  return out
}
