import type { MathNode } from 'mathjs'
import { math } from './math'
import { fillParens } from './parens'
import { evalScientific, preprocessAscii, rewriteTypesetMul, type AngleMode } from './scientific'
import type { Meas, UserFunction } from './types'

/** A measured quantity while walking: exact parts carry infinite sig figs and decimal places. */
type M = { v: number; sig: number; dp: number; unc: number }

export type MeasureContext = {
  ans?: number
  angleMode?: AngleMode
  variables?: Record<string, number>
  functions?: Record<string, UserFunction>
  measures?: Record<string, Meas>
}

const LITERAL_RE = /(?<![A-Za-z_0-9.])(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?/gi
const PERCENT_UNC_RE = /±\s*(\d+\.?\d*|\.\d+)\s*%/g

/** Result keeps the sig figs of its argument. */
const SIG_FNS = new Set(
  'sqrt cbrt sin cos tan csc sec cot asin acos atan acsc asec acot sinh cosh tanh csch sech coth asinh acosh atanh exp abs'.split(' '),
)
/** Result keeps as many decimal places as its argument has sig figs. */
const LOG_FNS = new Set(['ln', 'log', 'log10', 'log2'])
const MIN_SIG_FNS = new Set(['min', 'max', 'mean', 'median', 'hypot'])
const SUM_FNS = new Set(['sum', 'total'])
/** Roots propagate uncertainty like powers (`sqrt` = `^0.5`). */
const ROOT_POWER: Record<string, number> = { sqrt: 0.5, cbrt: 1 / 3 }

function exact(v: number): M {
  return { v, sig: Infinity, dp: Infinity, unc: 0 }
}

function mag(v: number): number {
  return Math.floor(Math.log10(Math.abs(v)))
}

function bySig(v: number, sig: number, unc: number): M {
  if (!Number.isFinite(sig)) return { v, sig, dp: Infinity, unc }
  return { v, sig, dp: v === 0 ? sig - 1 : sig - 1 - mag(v), unc }
}

function byDp(v: number, dp: number, unc: number): M {
  if (!Number.isFinite(dp)) return { v, sig: Infinity, dp, unc }
  return { v, sig: v === 0 ? 0 : mag(v) + 1 + dp, dp, unc }
}

/** `2.50` → 3 s.f.; `6.02e23` → 3 s.f.; `300.` → 3 s.f.; a bare integer is exact. */
export function literalMeas(text: string): M {
  const v = Number(text)
  const [mant = '', exp] = text.toLowerCase().split('e')
  const dot = mant.indexOf('.')
  if (dot < 0 && exp == null) return exact(v)
  const decimals = dot < 0 ? 0 : mant.length - dot - 1
  const digits = mant.replace('.', '').replace(/^0+/, '')
  const dp = decimals - Number(exp ?? 0)
  return v === 0 ? { v, sig: 0, dp, unc: 0 } : { v, sig: Math.max(1, digits.length), dp, unc: 0 }
}

function fromStored(v: number, meas: Meas | undefined): M {
  if (!meas) return exact(v)
  const unc = meas.unc ?? 0
  if (meas.dp != null) return byDp(v, meas.dp, unc)
  if (meas.sig != null) return bySig(v, meas.sig, unc)
  return { ...exact(v), unc }
}

function toStored(m: M): Meas | undefined {
  const out: Meas = {}
  if (Number.isFinite(m.sig) && Number.isFinite(m.dp)) {
    out.sig = m.sig
    out.dp = m.dp
  }
  if (m.unc > 0) out.unc = m.unc
  return out.sig == null && out.unc == null ? undefined : out
}

/** Stored metadata from history/JSON: finite numbers only, or nothing. */
export function sanitizeMeas(raw: unknown): Meas | undefined {
  if (!raw || typeof raw !== 'object') return undefined
  const r = raw as Record<string, unknown>
  const fin = (x: unknown) => (typeof x === 'number' && Number.isFinite(x) ? x : undefined)
  const sig = fin(r.sig)
  const dp = fin(r.dp)
  const unc = fin(r.unc)
  const out: Meas = {}
  if (sig != null && dp != null) {
    out.sig = sig
    out.dp = dp
  }
  if (unc != null && unc > 0) out.unc = unc
  return out.sig == null && out.unc == null ? undefined : out
}

export function hasPlusMinus(text: string): boolean {
  return text.includes('±')
}

function unwrap(node: MathNode): MathNode {
  let n = node
  while (n.type === 'ParenthesisNode') n = (n as unknown as { content: MathNode }).content
  return n
}

type Walk = (node: MathNode) => M | null

function callValue(name: string, args: number[], ctx: MeasureContext): number | null {
  const out = evalScientific(`${name}(${args.map(String).join(',')})`, { angleMode: ctx.angleMode })
  if (!out || out.kind !== 'number' || !Number.isFinite(out.n)) return null
  return out.n
}

function walkFunction(name: string, args: M[], ctx: MeasureContext): M | null {
  const v = callValue(name, args.map((a) => a.v), ctx)
  if (v == null) return null
  if (MIN_SIG_FNS.has(name) || SUM_FNS.has(name)) {
    if (args.some((a) => a.unc > 0)) return null
    if (SUM_FNS.has(name)) return byDp(v, Math.min(...args.map((a) => a.dp)), 0)
    return bySig(v, Math.min(...args.map((a) => a.sig)), 0)
  }
  const log = LOG_FNS.has(name)
  if (!log && !SIG_FNS.has(name)) return null
  const [a, base] = args
  if (!a || args.length > (name === 'log' ? 2 : 1)) return null
  if (base && (Number.isFinite(base.sig) || base.unc > 0)) return null
  let unc = 0
  if (a.unc > 0) {
    const power = ROOT_POWER[name]
    if (power != null) {
      if (a.v === 0) return null
      unc = (Math.abs(v * power) * a.unc) / Math.abs(a.v)
    } else {
      const extra = base ? [base.v] : []
      const hi = callValue(name, [a.v + a.unc, ...extra], ctx)
      const lo = callValue(name, [a.v - a.unc, ...extra], ctx)
      if (hi == null || lo == null) return null
      unc = Math.max(Math.abs(hi - v), Math.abs(lo - v))
    }
  }
  return log ? byDp(v, a.sig, unc) : bySig(v, a.sig, unc)
}

/** `a ± b` / `a ± b%` with literal sides (a may be negated). */
function walkPlusMinus(node: MathNode, literals: M[]): M | null {
  const [lhs, rhs] = (node as unknown as { args: MathNode[] }).args.map(unwrap)
  const lit = (n: MathNode | undefined): M | null => {
    if (n?.type !== 'SymbolNode') return null
    const m = /^__L(\d+)$/.exec((n as unknown as { name: string }).name)
    return m ? (literals[Number(m[1])] ?? null) : null
  }
  let a = lit(lhs)
  if (!a && lhs?.type === 'OperatorNode' && (lhs as unknown as { fn: string }).fn === 'unaryMinus') {
    const inner = lit(unwrap((lhs as unknown as { args: MathNode[] }).args[0]!))
    if (inner) a = { ...inner, v: -inner.v }
  }
  if (!a) return null
  let u: number | null = null
  const direct = lit(rhs)
  if (direct) u = Math.abs(direct.v)
  else if (rhs?.type === 'FunctionNode' && (rhs as unknown as { fn: { name: string } }).fn.name === '__pct') {
    const args = (rhs as unknown as { args: MathNode[] }).args
    const pct = args.length === 1 ? lit(unwrap(args[0]!)) : null
    if (pct) u = (Math.abs(a.v) * Math.abs(pct.v)) / 100
  }
  if (u == null) return null
  return { ...a, unc: u }
}

/**
 * Walk the expression as measured quantities: sig figs from the literals, ± uncertainties
 * propagated worst-case. Returns null for anything it does not model (the caller falls back).
 */
export function measure(text: string, ctx: MeasureContext = {}): { v: number; meas?: Meas } | null {
  let src = fillParens(rewriteTypesetMul(text.trim()))
  if (!src || src.includes('|')) return null
  src = src.replace(PERCENT_UNC_RE, '± __pct($1)')
  const fnNames = Object.keys(ctx.functions ?? {})
  const literals: M[] = []
  const expr = preprocessAscii(src, fnNames)
    .replace(/±/g, '|')
    .replace(LITERAL_RE, (lit) => {
      literals.push(literalMeas(lit))
      return `(__L${literals.length - 1})`
    })
  let root: MathNode
  try {
    root = math.parse(expr)
  } catch {
    return null
  }
  const vars = ctx.variables ?? {}
  const measures = ctx.measures ?? {}

  const walk: Walk = (node) => {
    switch (node.type) {
      case 'ParenthesisNode':
        return walk((node as unknown as { content: MathNode }).content)
      case 'ConstantNode': {
        const v = (node as unknown as { value: unknown }).value
        return typeof v === 'number' ? exact(v) : null
      }
      case 'SymbolNode': {
        const name = (node as unknown as { name: string }).name
        const lit = /^__L(\d+)$/.exec(name)
        if (lit) return literals[Number(lit[1])] ?? null
        if (name === 'pi') return exact(Math.PI)
        if (name === 'tau') return exact(Math.PI * 2)
        if (name === 'e') return exact(Math.E)
        if (name === 'ans') return ctx.ans == null ? null : fromStored(ctx.ans, measures.ans)
        const v = vars[name]
        return v == null || !Number.isFinite(v) ? null : fromStored(v, measures[name])
      }
      case 'OperatorNode': {
        const op = node as unknown as { fn: string; args: MathNode[] }
        if (op.fn === 'bitOr') return walkPlusMinus(node, literals)
        const args = op.args.map(walk)
        if (args.some((a) => a == null)) return null
        const [a, b] = args as M[]
        if (!a) return null
        switch (op.fn) {
          case 'unaryMinus':
            return { ...a, v: -a.v }
          case 'unaryPlus':
            return a
        }
        if (!b || args.length !== 2) return null
        switch (op.fn) {
          case 'add':
            return byDp(a.v + b.v, Math.min(a.dp, b.dp), a.unc + b.unc)
          case 'subtract':
            return byDp(a.v - b.v, Math.min(a.dp, b.dp), a.unc + b.unc)
          case 'multiply':
            return bySig(a.v * b.v, Math.min(a.sig, b.sig), Math.abs(a.v) * b.unc + Math.abs(b.v) * a.unc)
          case 'divide': {
            if (b.v === 0) return null
            const v = a.v / b.v
            return bySig(v, Math.min(a.sig, b.sig), (a.unc + Math.abs(v) * b.unc) / Math.abs(b.v))
          }
          case 'pow': {
            if (b.unc > 0) return null
            const v = a.v ** b.v
            if (!Number.isFinite(v)) return null
            if (a.unc > 0 && a.v === 0) return null
            const unc = a.unc > 0 ? (Math.abs(v * b.v) * a.unc) / Math.abs(a.v) : 0
            return bySig(v, Math.min(a.sig, b.sig), unc)
          }
        }
        return null
      }
      case 'FunctionNode': {
        const fn = node as unknown as { fn: { name?: string }; args: MathNode[] }
        const name = fn.fn.name
        if (!name || name in (ctx.functions ?? {})) return null
        const args = fn.args.map(walk)
        if (!args.length || args.some((a) => a == null)) return null
        return walkFunction(name, args as M[], ctx)
      }
    }
    return null
  }

  let m: M | null
  try {
    m = walk(root)
  } catch {
    return null
  }
  if (!m || !Number.isFinite(m.v) || !Number.isFinite(m.unc)) return null
  return { v: m.v, meas: toStored(m) }
}

/** `-0.0` → `0.0`. */
function unsigned(s: string): string {
  return Number(s) === 0 ? s.replace(/^-/, '') : s
}

function stripExpPlus(s: string): string {
  return s.replace('e+', 'e')
}

/** `v` at `sig` significant figures, keeping trailing zeros; ambiguous integer zeros go scientific (`1.00e2`). */
export function formatSig(v: number, sig: number, dp: number): string | null {
  if (!Number.isFinite(v)) return null
  if (v === 0 || sig < 1) return unsigned(v.toFixed(Math.max(0, Math.min(20, dp))))
  if (sig > 16) return null
  const s = v.toPrecision(sig)
  if (s.includes('e')) return stripExpPlus(s)
  if (!s.includes('.') && s.endsWith('0')) return stripExpPlus(v.toExponential(sig - 1))
  return s
}

function roundSig(v: number, sig: number): number {
  return v === 0 || !Number.isFinite(sig) || sig < 1 || sig > 16 ? v : Number(v.toPrecision(sig))
}

/** `10.0 ± 0.7`: the uncertainty at one significant figure, the value to the same place. */
export function formatUncertain(v: number, unc: number): string | null {
  if (!Number.isFinite(v) || !(unc > 0) || !Number.isFinite(unc)) return null
  const u = Number(unc.toPrecision(1))
  const place = mag(u)
  const decimals = Math.max(0, Math.min(20, -place))
  const value = place > 0 ? (Math.round(v / 10 ** place) * 10 ** place).toFixed(0) : v.toFixed(decimals)
  return `${unsigned(value)} ± ${u.toFixed(decimals)}`
}

/**
 * Display for a measured answer, or null when the normal display applies:
 * sig-fig rounding (when the mode is on) first, then the ± rounding.
 */
export function formatMeasured(v: number, meas: Meas | undefined, sigFigMode: boolean): string | null {
  if (!meas) return null
  const sig = sigFigMode && meas.sig != null ? meas.sig : undefined
  if (meas.unc) return formatUncertain(sig == null ? v : roundSig(v, sig), meas.unc)
  if (sig == null) return null
  return formatSig(v, sig, meas.dp ?? Infinity)
}
