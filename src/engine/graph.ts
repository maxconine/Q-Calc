import { parseFunctionDef } from './evaluate'
import { formatNumber } from './format'
import { namesPattern } from './math'
import { compileScientific, compileScientific2, evalScientific, splitGluedFunctions, type AngleMode } from './scientific'
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
  /** The plot's height over its width, so a contour plot's x and y share a scale. */
  aspect?: number
}

export interface GraphIntent {
  /** In the free variable `x`. */
  expression: string
  /** e.g. `x^3`, `f(x)`, `y = x^3`. */
  label: string
  /** Present for `graph f(x) = …`; the caller registers it. */
  functionDef?: { name: string; params: string[]; body: string }
  /** From `{0 < x < 3}` after the curve. */
  bounds?: GraphBounds
  /** Braces that don't read as bounds. */
  boundsError?: string
  /** In `x` and `y`, drawn as level curves; `zero` draws only where it is 0 (`x^2 + y^2 = 25`). */
  contour?: 'levels' | 'zero'
}

export interface GraphRange {
  lo: number
  hi: number
}

/** The pieces of the plot to keep: a union of ranges per axis, intersected across x and y. */
export interface GraphBounds {
  x: GraphRange[]
  y: GraphRange[]
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
  /** Present for a function of x and y; yScale is then its y range, at the plot's aspect. */
  contours?: ContourLevel[]
  /** f(x, y) inside the bounds, for a contour plot's readout. */
  z?: (x: number, y: number) => number | null
  error?: string
}

export interface ContourLevel {
  level: number
  label: string
  /** x1, y1, x2, y2 for each piece of the line. */
  segments: number[]
  /** Where its label sits, clear of the edges and the other labels. */
  at: { x: number; y: number } | null
}

const GRAPH_CMD = /^\s*graph\s+(.+?)\s*$/is
const Y_EQ = /^y\s*=\s*(.+)$/is
const Z_EQ = /^z\s*=\s*(.+)$/is
// one `=`, not part of <=, >= or ==
const IMPLICIT_EQ = /^([^=<>!]+)=([^=<>]+)$/s
const BARE_NAME = /^([A-Za-z][A-Za-z0-9]*)$/
const FREE_Y = /(?<![A-Za-z0-9_])y(?![A-Za-z0-9_])/

/** `graph f` looks `f` up in `options.functions`; trailing braces like `{0 < x < 3}` are bounds. */
export function parseGraphIntent(
  input: string,
  options: Pick<GraphOptions, 'functions' | 'variables'> = {},
): GraphIntent | null {
  const m = input.match(GRAPH_CMD)
  if (!m) return null
  const { rest: raw, groups, open } = peelBounds(m[1]!.trim())
  const rest = splitGluedFunctions(raw)
  if (!rest) return null
  const intent = curveIntent(rest, options)
  if (!intent || !intent.expression || (!groups.length && open == null)) return intent

  // a function's own parameters name its axes: f(t) bounds t, g(u, v) bounds u and v
  const params = intent.functionDef?.params ?? options.functions?.[rest]?.params
  const names: Record<string, 'x' | 'y'> =
    params?.length === 2 ? { [params[0]!]: 'x', [params[1]!]: 'y' } : { y: 'y', [params?.[0] ?? 'x']: 'x' }
  const ctx = { variables: options.variables, functions: options.functions }
  const bounds = parseBounds(groups, names, ctx)
  if (!bounds) return { ...intent, boundsError: 'Write bounds like {0 < x < 3} or {2 < y < 5}' }
  // a group still being typed counts once it reads as bounds
  const typing = open != null ? parseBounds([open], names, ctx) : null
  if (typing) {
    bounds.x.push(...typing.x)
    bounds.y.push(...typing.y)
  }
  if (!bounds.x.length && !bounds.y.length) return intent
  return { ...intent, bounds, label: `${intent.label} ${groups.map((g) => `{${g.trim()}}`).join(' ')}`.trim() }
}

function curveIntent(rest: string, options: Pick<GraphOptions, 'functions' | 'variables'>): GraphIntent | null {
  const fnDef = parseFunctionDef(rest)
  if (fnDef) {
    const label = `${fnDef.name}(${fnDef.params.join(', ')})`
    const [p, q] = fnDef.params
    if (fnDef.params.length === 2) {
      return { expression: rewriteParams(fnDef.body, { [p!]: 'x', [q!]: 'y' }), label, functionDef: fnDef, contour: 'levels' }
    }
    if (fnDef.params.length !== 1) return { expression: '', label: rest, functionDef: fnDef }
    return { expression: rewriteParams(fnDef.body, { [p!]: 'x' }), label, functionDef: fnDef }
  }

  const yEq = rest.match(Y_EQ)
  if (yEq) {
    const expression = yEq[1]!.trim()
    if (!expression) return null
    return { expression, label: `y = ${expression}` }
  }
  const zEq = rest.match(Z_EQ)
  if (zEq) return { expression: zEq[1]!.trim(), label: `z = ${zEq[1]!.trim()}`, contour: 'levels' }

  const bare = rest.match(BARE_NAME)
  if (bare) {
    const name = bare[1]!
    const def = options.functions?.[name]
    if (def) {
      const [p, q] = def.params
      // inlined so sampling compiles one expression instead of making a call per sample
      if (def.params.length === 2) {
        return { expression: rewriteParams(def.body, { [p!]: 'x', [q!]: 'y' }), label: `${name}(${p}, ${q})`, contour: 'levels' }
      }
      if (def.params.length !== 1) return { expression: '', label: name }
      return { expression: rewriteParams(def.body, { [p!]: 'x' }), label: `${name}(${p})` }
    }
  }

  // `x^2 + y^2 = 25` is the curve where the sides meet
  const eq = rest.match(IMPLICIT_EQ)
  if (eq && eq[1]!.trim() && eq[2]!.trim()) {
    const [l, r] = [eq[1]!.trim(), eq[2]!.trim()]
    return { expression: `(${l}) - (${r})`, label: `${l} = ${r}`, contour: 'zero' }
  }
  // a free y makes it a surface, unless y is a variable
  if (FREE_Y.test(rest) && !(options.variables && 'y' in options.variables)) {
    return { expression: rest, label: rest, contour: 'levels' }
  }
  return { expression: rest, label: rest }
}

// trailing `{…}` groups holding a comparison, and one still open at the end
function peelBounds(text: string): { rest: string; groups: string[]; open: string | null } {
  const groups: string[] = []
  let rest = text
  let open: string | null = null
  const typing = rest.match(/\{([^{}]*)$/)
  if (typing && /[<>≤≥]/.test(typing[1]!)) {
    open = typing[1]!
    rest = rest.slice(0, typing.index).replace(/[\s,]+$/, '')
  }
  for (let g = rest.match(/\{([^{}]*)\}\s*$/); g && /[<>≤≥]/.test(g[1]!); g = rest.match(/\{([^{}]*)\}\s*$/)) {
    groups.unshift(g[1]!)
    rest = rest.slice(0, g.index).replace(/[\s,]+$/, '')
  }
  return { rest, groups, open }
}

const CMP = /(<=|>=|=<|=>|≤|≥|<|>)/
const isLess = (op: string) => op.includes('<') || op === '≤'

/** `0 < x < 3`, `y >= 2`, `5 > y > 2`, comma-separated; null when a piece isn't one. */
function parseBounds(
  groups: string[],
  names: Record<string, 'x' | 'y'>,
  ctx: Pick<GraphOptions, 'variables' | 'functions'>,
): GraphBounds | null {
  const out: GraphBounds = { x: [], y: [] }
  const num = (text: string): number | null => {
    if (!text) return null
    try {
      const v = evalScientific(text, ctx)
      return v?.kind === 'number' && !Number.isNaN(v.n) ? v.n : null
    } catch {
      return null
    }
  }
  const axisOf = (name: string) => (Object.hasOwn(names, name) ? names[name] : undefined)
  for (const piece of groups.flatMap(splitTopCommas)) {
    if (!piece.trim()) continue
    const t = piece.split(CMP).map((s) => s.trim())
    let axis: 'x' | 'y' | undefined
    let lo = -Infinity
    let hi = Infinity
    if (t.length === 3) {
      const [a, op, b] = t as [string, string, string]
      const left = axisOf(a)
      axis = left ?? axisOf(b)
      const bound = axis ? num(left ? b : a) : null
      if (!axis || bound == null) return null
      // x < 3 and 3 > x both cap x
      if (isLess(op) === Boolean(left)) hi = bound
      else lo = bound
    } else if (t.length === 5) {
      const [a, op1, v, op2, b] = t as [string, string, string, string, string]
      axis = axisOf(v)
      const na = num(a)
      const nb = num(b)
      if (!axis || isLess(op1) !== isLess(op2) || na == null || nb == null) return null
      ;[lo, hi] = isLess(op1) ? [na, nb] : [nb, na]
    } else return null
    out[axis].push({ lo, hi })
  }
  return out
}

function splitTopCommas(text: string): string[] {
  const out: string[] = []
  let depth = 0
  let from = 0
  for (let i = 0; i < text.length; i++) {
    const ch = text[i]
    if (ch === '(' || ch === '[') depth++
    else if (ch === ')' || ch === ']') depth--
    else if (ch === ',' && depth === 0) {
      out.push(text.slice(from, i))
      from = i + 1
    }
  }
  out.push(text.slice(from))
  return out
}

const inRanges = (v: number, rs: GraphRange[]) => !rs.length || rs.some((r) => v >= r.lo && v <= r.hi)

// the span the ranges cover, when it is finite
function hull(rs: GraphRange[] | undefined): [number, number] | null {
  if (!rs?.length) return null
  const lo = Math.min(...rs.map((r) => r.lo))
  const hi = Math.max(...rs.map((r) => r.hi))
  return Number.isFinite(lo) && Number.isFinite(hi) && hi > lo ? [lo, hi] : null
}

/** null outside the bounds, so the path breaks there and bisection finds the edges. */
function restrict(f: GraphY, bounds: GraphBounds | undefined): GraphY {
  if (!bounds) return f
  return (x) => {
    if (!inRanges(x, bounds.x)) return null
    const y = f(x)
    return y != null && inRanges(y, bounds.y) ? y : null
  }
}

// samples at the x bounds' own ends, so a piece starts and stops exactly there
function withEnds(points: GraphPoint[], f: GraphY, bounds: GraphBounds | undefined): GraphPoint[] {
  if (!bounds?.x.length || points.length < 2) return points
  const lo = points[0]!.x
  const hi = points[points.length - 1]!.x
  const ends = bounds.x.flatMap((r) => [r.lo, r.hi]).filter((x) => x > lo && x < hi && !points.some((p) => p.x === x))
  if (!ends.length) return points
  return [...points, ...ends.map((x) => ({ x, y: f(x) }))].sort((a, b) => a.x - b.x)
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

  if (!intent.expression || intent.boundsError) {
    const msg = intent.boundsError
      ?? (intent.functionDef && intent.functionDef.params.length !== 1
        ? 'Graphing takes a function of x, or of x and y'
        : 'Unknown function — define it first (e.g. f(x) = …)')
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
  if (intent.contour) return buildContour(intent, base, mode)
  const curve = compileGraphY(intent.expression, { ...base, angleMode: mode })
  const angleUnit = dependsOnAngle(curve, compileGraphY(intent.expression, { ...base, angleMode: mode === 'deg' ? 'rad' : 'deg' }))
    ? mode
    : null
  const f = restrict(curve, intent.bounds)
  const domain = normalizeDomain(options.domain, angleUnit === 'deg' ? DEGREE_GRAPH_DOMAIN : DEFAULT_GRAPH_DOMAIN)
  const uniform = withEnds(sampleWith(f, { ...base, domain }), f, intent.bounds)
  const yScale = autoYScale(uniform)
  if (!uniform.some((p) => p.y != null)) {
    const somewhere = intent.bounds && sampleWith(curve, { ...base, domain }).some((p) => p.y != null)
    return {
      intent,
      points: uniform,
      domain,
      yScale,
      criticalPoints: [],
      roots: [],
      angleUnit,
      y: f,
      error: somewhere ? 'Nothing to plot within the bounds here' : 'Could not evaluate expression over the domain',
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
  // finite bounds frame the view, with a little room either side
  const intent = parseGraphIntent(input, options)
  const bx = intent?.boundsError ? null : hull(intent?.bounds?.x)
  const by = intent?.contour && !intent.boundsError ? hull(intent.bounds?.y) : null
  if (bx || by) {
    const span = Math.max(bx ? bx[1] - bx[0] : 0, by ? (by[1] - by[0]) / (options.aspect ?? CONTOUR_ASPECT) : 0)
    return niceWindow(bx ? (bx[0] + bx[1]) / 2 : 0, span * 1.2)
  }
  const g = buildGraph(input, { ...options, domain: undefined })
  if (!g || g.error) return g?.domain ?? [DEFAULT_GRAPH_DOMAIN[0], DEFAULT_GRAPH_DOMAIN[1]]
  const [lo, hi] = g.domain
  // y bounds frame the pieces they keep
  const drawn = intent?.bounds && !g.contours ? g.points.filter((p) => p.y != null).map((p) => p.x) : []
  if (drawn.length > 1) {
    const a = Math.min(...drawn)
    const b = Math.max(...drawn)
    if (b > a && b - a < 0.6 * (hi - lo)) return niceWindow((a + b) / 2, (b - a) * 1.2)
  }
  const xs = [...g.roots.map((r) => r.x), ...g.criticalPoints.map((c) => c.x)]
  if (xs.length < 2) return g.domain
  const a = Math.min(...xs)
  const b = Math.max(...xs)
  const span = Math.max((b - a) * 1.5, (hi - lo) / 10)
  if (!(b > a) || span > 0.6 * (hi - lo)) return g.domain
  return niceWindow((a + b) / 2, span)
}

// at least `span` wide about `mid`, with ends on a round step
function niceWindow(mid: number, span: number): [number, number] {
  const step = niceStep(span / 10, 'floor')
  return [Math.floor((mid - span / 2) / step + 1e-9) * step, Math.ceil((mid + span / 2) / step - 1e-9) * step].map(
    (v) => Number(v.toPrecision(12)),
  ) as [number, number]
}

/** Height over width of the graph panel's plot. */
export const CONTOUR_ASPECT = 160 / 600
// grid columns sampled across the view; rows follow the aspect, so cells are square
const CONTOUR_COLUMNS = 120
const CONTOUR_LINES = 6

export type GraphZ = (x: number, y: number) => number | null

export function compileGraphZ(expression: string, options: GraphOptions = {}): GraphZ {
  const f = expression.trim()
    ? compileScientific2(
        expression,
        { ans: options.ans, angleMode: options.angleMode, variables: options.variables, functions: options.functions },
        'x',
        'y',
      )
    : null
  if (!f) return () => null
  return (x, y) => {
    const v = f(x, y)
    return v && v.kind === 'number' && Number.isFinite(v.n) ? v.n : null
  }
}

/** Level curves of f(x, y) by marching squares, clipped to the bounds. */
function buildContour(intent: GraphIntent, options: GraphOptions, mode: AngleMode): GraphResult {
  const raw = compileGraphZ(intent.expression, { ...options, angleMode: mode })
  const other = compileGraphZ(intent.expression, { ...options, angleMode: mode === 'deg' ? 'rad' : 'deg' })
  const angleUnit = dependsOnAngle((x) => raw(x, 0.61 - x / 3), (x) => other(x, 0.61 - x / 3)) ? mode : null
  const domain = normalizeDomain(options.domain, angleUnit === 'deg' ? DEGREE_GRAPH_DOMAIN : DEFAULT_GRAPH_DOMAIN)
  const [x0, x1] = domain
  const aspect = options.aspect ?? CONTOUR_ASPECT
  const by = hull(intent.bounds?.y)
  const yMid = by ? (by[0] + by[1]) / 2 : 0
  const yHalf = ((x1 - x0) * aspect) / 2
  const yScale = { min: yMid - yHalf, max: yMid + yHalf }
  const b = intent.bounds
  const inside = (x: number, y: number) => !b || (inRanges(x, b.x) && inRanges(y, b.y))
  const z: GraphZ = (x, y) => (inside(x, y) ? raw(x, y) : null)

  const nx = CONTOUR_COLUMNS
  const ny = Math.max(4, Math.round(nx * aspect))
  const dx = (x1 - x0) / nx
  const dy = (yScale.max - yScale.min) / ny
  const grid = new Float64Array((nx + 1) * (ny + 1))
  const seen: number[] = []
  for (let j = 0; j <= ny; j++) {
    const y = yScale.min + j * dy
    for (let i = 0; i <= nx; i++) {
      const x = x0 + i * dx
      const v = raw(x, y) ?? Number.NaN
      grid[j * (nx + 1) + i] = v
      if (!Number.isNaN(v) && inside(x, y)) seen.push(v)
    }
  }
  const result: GraphResult = { intent, points: [], domain, yScale, criticalPoints: [], roots: [], angleUnit, y: () => null, z }
  if (!seen.length) {
    const somewhere = b && grid.some((v) => !Number.isNaN(v))
    return { ...result, error: somewhere ? 'Nothing to plot within the bounds here' : 'Could not evaluate expression over the domain' }
  }

  const levels = intent.contour === 'zero' ? [0] : contourLevels(seen)
  // the bounds as rectangles, each segment kept only where it's inside one
  const rects = b ? (b.x.length ? b.x : [FULL]).flatMap((rx) => (b.y.length ? b.y : [FULL]).map((ry) => [rx, ry] as const)) : null
  const contours: ContourLevel[] = levels.map((level) => {
    const found = marchingSquares(grid, nx, ny, level, x0, yScale.min, dx, dy)
    let segments = found
    if (rects) {
      segments = []
      for (let k = 0; k < found.length; k += 4) {
        for (const [rx, ry] of rects) {
          const c = clipSegment(found[k]!, found[k + 1]!, found[k + 2]!, found[k + 3]!, rx, ry)
          if (c) segments.push(...c)
        }
      }
    }
    return { level, label: withMinus(formatNumber(level, 4)), segments, at: null }
  })
  // a lone f = 0 curve needs no label
  if (intent.contour === 'levels') placeLabels(contours, domain, yScale)
  return { ...result, contours }
}

const FULL: GraphRange = { lo: -Infinity, hi: Infinity }

// a handful of round values across the middle of what f takes, so a pole doesn't crowd them out
function contourLevels(values: number[]): number[] {
  const sorted = values.sort((a, b) => a - b)
  const lo = quantile(sorted, 0.02)
  const hi = quantile(sorted, 0.98)
  if (!(hi > lo)) return []
  const step = niceStep((hi - lo) / CONTOUR_LINES, 'ceil')
  const out: number[] = []
  for (let k = Math.floor(lo / step) + 1; k * step < hi && out.length < 12; k++) out.push(Number((k * step).toPrecision(12)))
  return out
}

/** x1, y1, x2, y2 per crossing, with saddles split by the cell's centre. */
export function marchingSquares(
  grid: ArrayLike<number>,
  nx: number,
  ny: number,
  level: number,
  x0: number,
  y0: number,
  dx: number,
  dy: number,
): number[] {
  const out: number[] = []
  const w = nx + 1
  const ex = new Float64Array(4)
  const ey = new Float64Array(4)
  for (let j = 0; j < ny; j++) {
    for (let i = 0; i < nx; i++) {
      const a = grid[j * w + i]! // (i, j)
      const b = grid[j * w + i + 1]! // (i + 1, j)
      const c = grid[(j + 1) * w + i + 1]! // (i + 1, j + 1)
      const d = grid[(j + 1) * w + i]! // (i, j + 1)
      if (Number.isNaN(a + b + c + d)) continue
      const idx = (a > level ? 1 : 0) | (b > level ? 2 : 0) | (c > level ? 4 : 0) | (d > level ? 8 : 0)
      if (idx === 0 || idx === 15) continue
      const x = x0 + i * dx
      const y = y0 + j * dy
      const t = (p: number, q: number) => (level - p) / (q - p)
      // edges 0 bottom, 1 right, 2 top, 3 left, where they cross
      const cross: number[] = []
      if ((idx & 1) !== (idx & 2) >> 1) {
        ex[0] = x + t(a, b) * dx
        ey[0] = y
        cross.push(0)
      }
      if ((idx & 2) >> 1 !== (idx & 4) >> 2) {
        ex[1] = x + dx
        ey[1] = y + t(b, c) * dy
        cross.push(1)
      }
      if ((idx & 4) >> 2 !== (idx & 8) >> 3) {
        ex[2] = x + t(d, c) * dx
        ey[2] = y + dy
        cross.push(2)
      }
      if ((idx & 8) >> 3 !== (idx & 1)) {
        ex[3] = x
        ey[3] = y + t(a, d) * dy
        cross.push(3)
      }
      let pairs: number[]
      if (cross.length === 4) {
        const high = (a + b + c + d) / 4 > level
        // a high centre joins the high corners, so the lines cut off the low ones
        pairs = (idx === 5) === high ? [0, 1, 2, 3] : [0, 3, 1, 2]
      } else pairs = cross
      for (let k = 0; k + 1 < pairs.length; k += 2) out.push(ex[pairs[k]!]!, ey[pairs[k]!]!, ex[pairs[k + 1]!]!, ey[pairs[k + 1]!]!)
    }
  }
  return out
}

// liang–barsky against a box whose sides may be infinite
function clipSegment(
  x1: number,
  y1: number,
  x2: number,
  y2: number,
  rx: GraphRange,
  ry: GraphRange,
): [number, number, number, number] | null {
  const dx = x2 - x1
  const dy = y2 - y1
  let t0 = 0
  let t1 = 1
  for (const [p, q] of [[-dx, x1 - rx.lo], [dx, rx.hi - x1], [-dy, y1 - ry.lo], [dy, ry.hi - y1]] as const) {
    if (p === 0) {
      if (q < 0) return null
      continue
    }
    const r = q / p
    if (p < 0) {
      if (r > t1) return null
      if (r > t0) t0 = r
    } else {
      if (r < t0) return null
      if (r < t1) t1 = r
    }
  }
  return [x1 + t0 * dx, y1 + t0 * dy, x1 + t1 * dx, y1 + t1 * dy]
}

// each level's label at the point of its line farthest from the edges and the labels already placed
function placeLabels(contours: ContourLevel[], domain: [number, number], yScale: YScale): void {
  const span = domain[1] - domain[0]
  // a label is about 4% of the plot wide and 2% tall
  const w = 0.04 * span
  const h = 0.02 * span
  const placed: { x: number; y: number }[] = []
  for (const c of contours) {
    let best = 0
    const s = c.segments
    for (let k = 0; k < s.length; k += 4) {
      const x = (s[k]! + s[k + 2]!) / 2
      const y = (s[k + 1]! + s[k + 3]!) / 2
      let score = Math.min(x - domain[0] - w, domain[1] - x - w, y - yScale.min - h, yScale.max - y - h)
      for (const p of placed) score = Math.min(score, Math.hypot((x - p.x) / 2, y - p.y) - 2 * h)
      if (score > best) {
        best = score
        c.at = { x, y }
      }
    }
    if (c.at) placed.push(c.at)
  }
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

// all at once, so f(y, x) swaps rather than merging
function rewriteParams(body: string, map: Record<string, string>): string {
  const from = Object.keys(map).filter((k) => map[k] !== k)
  if (!from.length) return body
  return body.replace(new RegExp(`(?<![A-Za-z_])(?:${namesPattern(from)})(?![A-Za-z0-9_])`, 'g'), (n) => map[n] ?? n)
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
