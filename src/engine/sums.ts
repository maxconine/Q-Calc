import type { MathNode } from 'mathjs'
import { num, textVal } from './format'
import { latexToAscii, tryPlainMath } from './plainMath'
import { compileScientific, parseScientific, SCIENTIFIC_NAMES, type AngleMode } from './scientific'
import { exactForm, wantsExactForm } from './simplify'
import type { UserFunction, Value } from './types'
import type { DefaultUnits } from './units'
import { spend, workBudget } from './work'

export type SumContext = {
  ans?: number
  angleMode?: AngleMode
  variables?: Record<string, number>
  functions?: Record<string, UserFunction>
  defaultUnits?: DefaultUnits
  rationalize?: boolean
}

export type SumAnswer = { value: Value | null; exact?: string }

type Op = 'sum' | 'prod'
type Spec = { op: Op; body: string; index?: string; from: string; to: string }

// work caps per keystroke; past them the answer is blank, never a partial sum
const BUDGET = 2_500_000
/** Work units per node for one exact term; float terms are counted as they're evaluated. */
const EXACT_NODE_COST = 10
const EXACT_TERMS = 5_000
const FLOAT_TERMS = 200_000
const PROD_FLOAT_TERMS = 1_000
const MAX_BOUND = 1e15
const MAX_EXACT_TEXT = 24
const EPS = Number.EPSILON

/** Typed Σ and Π read exactly like the words `sum` and `prod`. */
export function normalizeSums(text: string): string {
  if (!/[Σ∑Π∏]/.test(text)) return text
  return text.replace(/[Σ∑Π∏]/g, (ch, at: number, whole: string) => {
    const word = ch === 'Σ' || ch === '∑' ? 'sum' : 'prod'
    return /[A-Za-z0-9]/.test(whole[at + 1] ?? '') ? `${word} ` : word
  })
}

const HINT_RE = /(?<![A-Za-z_])\\?(?:sum|prod|product)(?![A-Za-z0-9])/i
const LEAD_RE = /^\\?(sum|prod|product)(?![A-Za-z0-9])\s*/i
const CALL_RE = /(?<![A-Za-z_\\])(sum|prod|product)\s*\(/gi

function closeOf(s: string, open: number): number {
  const pairs: Record<string, string> = { '(': ')', '[': ']', '{': '}' }
  const stack: string[] = []
  for (let i = open; i < s.length; i++) {
    const ch = s[i]!
    if (pairs[ch]) stack.push(pairs[ch]!)
    else if (ch === ')' || ch === ']' || ch === '}') {
      if (stack.pop() !== ch) return -1
      if (!stack.length) return i
    }
  }
  return -1
}

/** Top-level pieces between separators, outside any brackets. */
function splitTop(s: string, sep: RegExp): string[] {
  const out: string[] = []
  let depth = 0
  let start = 0
  for (let i = 0; i < s.length; i++) {
    const ch = s[i]!
    if ('([{'.includes(ch)) depth++
    else if (')]}'.includes(ch)) depth--
    else if (depth === 0 && sep.test(ch)) {
      out.push(s.slice(start, i))
      start = i + 1
    }
  }
  out.push(s.slice(start))
  return out
}

type Cut = { at: number; len: number }

/** Top-level matches of `re` (a comma, or a word like ` for `). */
function topMatches(s: string, re: RegExp): Cut[] {
  const out: Cut[] = []
  const g = new RegExp(re.source, 'gi')
  for (let m = g.exec(s); m; m = g.exec(s)) {
    let depth = 0
    for (let i = 0; i < m.index; i++) {
      if ('([{'.includes(s[i]!)) depth++
      else if (')]}'.includes(s[i]!)) depth--
    }
    if (depth === 0) out.push({ at: m.index, len: m[0].length })
  }
  return out
}

const lastTopWord = (s: string, re: RegExp): Cut | undefined => topMatches(s, re).at(-1)
const firstTopWord = (s: string, re: RegExp): Cut | undefined => topMatches(s, re)[0]

const RANGE_RE = /^\s*(?:([A-Za-z]|theta)\s*(?:=|\bfrom\b|\bin\b)\s*)?(.+?)\s*(?:\.\.\.?|…|\bto\b)\s*(.+?)\s*$/i

function parseRange(text: string): { index?: string; from: string; to: string } | null {
  let s = text.trim()
  const bracket = s.match(/^([A-Za-z]|theta)\s*(?:=|\bin\b)\s*\[(.*)\]$/i)
  if (bracket) s = `${bracket[1]}=${bracket[2]}`
  const m = s.match(RANGE_RE)
  if (!m) return null
  const from = m[2]!.trim()
  const to = m[3]!.trim()
  // `n = 1..10 + 5` is ambiguous about where the range ends
  if (!from || !to || /\s/.test(to) || /\s/.test(from)) return null
  return { index: m[1], from, to }
}

function specFromCall(op: Op, args: string[], ctx: SumContext): Spec | null {
  const body = args[0]?.trim()
  if (!body) return null
  if (args.length === 2) {
    const r = parseRange(args[1]!)
    return r ? { op, body, ...r } : null
  }
  if (args.length === 3) {
    const at = args[1]!.match(/^\s*([A-Za-z]|theta)\s*=\s*(\S+)\s*$/)
    if (at) return { op, body, index: at[1], from: at[2]!, to: args[2]!.trim() }
    // `sum(k^2, 1, 10)` with k unknown; with no free letter it's a list
    return pickIndex(body, ctx) ? { op, body, from: args[1]!.trim(), to: args[2]!.trim() } : null
  }
  if (args.length === 4) {
    const index = args[1]!.trim()
    if (!/^(?:[A-Za-z]|theta)$/.test(index)) return null
    // `sum(x, y, 1, 10)` with y a variable not in the body is a list of four numbers
    const mentions = new RegExp(`(?<![A-Za-z0-9_])${index}(?![A-Za-z0-9_])`).test(body)
    if (index in (ctx.variables ?? {}) && !mentions) return null
    return { op, body, index, from: args[2]!.trim(), to: args[3]!.trim() }
  }
  return null
}

function grabGroup(s: string, at: number, stop: RegExp): { inner: string; end: number } | null {
  if (s[at] === '{' || s[at] === '(') {
    const close = closeOf(s, at)
    return close < 0 ? null : { inner: s.slice(at + 1, close), end: close + 1 }
  }
  let end = at
  while (end < s.length && !stop.test(s[end]!)) end++
  return end > at ? { inner: s.slice(at, end), end } : null
}

/** `_{n=1}^{10} n^2`, also `_(n=1)^10 n^2`, the way sums are typeset. */
function specFromScripts(op: Op, rest: string): Spec | null {
  const s = rest.replace(/^\\limits/, '')
  if (s[0] !== '_') return null
  const lower = grabGroup(s, 1, /[\^\s]/)
  if (!lower || s[lower.end] !== '^') return null
  const upper = grabGroup(s, lower.end + 1, /\s/)
  if (!upper) return null
  const body = s.slice(upper.end).trim()
  if (!body) return null
  const at = lower.inner.match(/^\s*([A-Za-z]|theta)\s*=\s*(.+?)\s*$/)
  return { op, body, index: at?.[1], from: at ? at[2]! : lower.inner.trim(), to: upper.inner.trim() }
}

/** `sum n^2, n = 1..10`, `sum n^2 for n = 1 to 10`, `sum from n = 1 to 10 of n^2`. */
function specFromWords(op: Op, rest: string): Spec | null {
  const s = rest.replace(/^of\s+/i, '').trim()
  if (!s) return null
  // the last comma, but the first `for`: `n^2 for n from 1 to 10`
  const cut = [lastTopWord(s, /[,;]/), firstTopWord(s, /\s(?:for|from|over)\s/)]
  for (const c of cut) {
    if (!c) continue
    const body = s.slice(0, c.at).trim()
    const r = parseRange(s.slice(c.at + c.len))
    if (body && r) return { op, body, ...r }
  }
  const lead = s.match(/^(?:(?:from|for|over)\s+)?(.+?)(?:\s+of\s+|\s*[,;:]\s*)(.+)$/i)
  if (lead) {
    const r = parseRange(lead[1]!)
    const body = lead[2]!.trim()
    if (r && body) return { op, body, ...r }
  }
  return null
}

function opOf(word: string): Op {
  return word.toLowerCase() === 'sum' ? 'sum' : 'prod'
}

function wholeLineSpec(text: string, ctx: SumContext): Spec | null {
  const m = text.match(LEAD_RE)
  if (!m) return null
  const op = opOf(m[1]!)
  const rest = text.slice(m[0].length)
  if (rest[0] === '(') {
    const close = closeOf(rest, 0)
    if (close === rest.length - 1) {
      const spec = specFromCall(op, splitTop(rest.slice(1, close), /,/), ctx)
      if (spec) return spec
    }
  }
  return specFromScripts(op, rest) ?? specFromWords(op, rest)
}

const KNOWN = new Set(`${SCIENTIFIC_NAMES}|e`.toLowerCase().split('|'))

/** The one free single letter in the body, when there is exactly one. */
function pickIndex(body: string, ctx: SumContext): string | undefined {
  const letters = new Set<string>()
  for (const t of body.match(/[A-Za-z][A-Za-z0-9_]*/g) ?? []) {
    if (t.length !== 1 || KNOWN.has(t.toLowerCase())) continue
    if (t in (ctx.variables ?? {}) || t in (ctx.functions ?? {})) continue
    letters.add(t)
  }
  return letters.size === 1 ? [...letters][0] : undefined
}

type Q = { n: bigint; d: bigint }
const UNDEF = 'undef' as const
type Ex = Q | null | typeof UNDEF
const LIMIT = 1n << 320n
const ONE: Q = { n: 1n, d: 1n }

const babs = (x: bigint) => (x < 0n ? -x : x)

function bgcd(a: bigint, b: bigint): bigint {
  a = babs(a)
  b = babs(b)
  while (b) [a, b] = [b, a % b]
  return a || 1n
}

function mkQ(n: bigint, d: bigint): Q | null {
  if (d < 0n) {
    n = -n
    d = -d
  }
  const g = bgcd(n, d)
  n /= g
  d /= g
  return babs(n) > LIMIT || d > LIMIT ? null : { n, d }
}

const qAdd = (a: Q, b: Q) => mkQ(a.n * b.d + b.n * a.d, a.d * b.d)
const qSub = (a: Q, b: Q) => mkQ(a.n * b.d - b.n * a.d, a.d * b.d)
const qMul = (a: Q, b: Q) => mkQ(a.n * b.n, a.d * b.d)
const qDiv = (a: Q, b: Q): Ex => (b.n === 0n ? UNDEF : mkQ(a.n * b.d, a.d * b.n))
const qInt = (a: Q) => a.d === 1n

function qPow(a: Q, e: bigint): Ex {
  if (e === 0n) return ONE
  if (a.n === 0n) return e < 0n ? UNDEF : { n: 0n, d: 1n }
  const bits = BigInt(Math.max(babs(a.n).toString(2).length, a.d.toString(2).length))
  if (babs(e) * (bits - 1n) > 320n) return null
  const k = babs(e)
  const r = mkQ(a.n ** k, a.d ** k)
  if (!r) return null
  return e < 0n ? qDiv(ONE, r) : r
}

function qFromNumber(x: number): Q | null {
  if (!Number.isFinite(x)) return null
  const m = String(x).match(/^(-?)(\d+)(?:\.(\d+))?(?:e([+-]?\d+))?$/)
  if (!m) return null
  const frac = m[3] ?? ''
  const exp = Number(m[4] ?? 0) - frac.length
  if (Math.abs(exp) > 400) return null
  let n = BigInt(m[2]! + frac)
  if (m[1]) n = -n
  return exp >= 0 ? mkQ(n * 10n ** BigInt(exp), 1n) : mkQ(n, 10n ** BigInt(-exp))
}

function qToNumber(q: Q): number {
  return Number(q.n) / Number(q.d)
}

function qText(q: Q): string {
  return q.d === 1n ? String(q.n) : `${q.n}/${q.d}`
}

type Node = MathNode & {
  type: string
  fn?: string | { name: string }
  name?: string
  args?: Node[]
  content?: Node
  value?: unknown
}

function unwrap(node: Node): Node {
  let n = node
  while (n.type === 'ParenthesisNode' && n.content) n = n.content
  return n
}

function fnName(node: Node): string {
  return typeof node.fn === 'string' ? node.fn : (node.fn?.name ?? '')
}

function hasIndex(node: Node, index: string): boolean {
  return node.filter((n) => (n as Node).type === 'SymbolNode' && (n as Node).name === index).length > 0
}

/** Null when the node isn't a rational of +, −, ×, ÷, integer powers and factorials. */
function exactEval(node: Node, index: string, k: bigint | null, vars: Record<string, number>): Ex {
  switch (node.type) {
    case 'ParenthesisNode':
      return exactEval(node.content!, index, k, vars)
    case 'ConstantNode':
      return typeof node.value === 'number' ? qFromNumber(node.value) : null
    case 'SymbolNode': {
      if (node.name === index) return k == null ? null : { n: k, d: 1n }
      const v = vars[node.name!]
      // a float variable like 1/3 isn't known exactly
      return v != null && Number.isSafeInteger(v) ? { n: BigInt(v), d: 1n } : null
    }
    case 'OperatorNode': {
      const args = node.args ?? []
      const vals: Q[] = []
      for (const a of args) {
        const v = exactEval(a, index, k, vars)
        if (v == null || v === UNDEF) return v
        vals.push(v)
      }
      const [a, b] = vals
      switch (fnName(node)) {
        case 'unaryMinus':
          return mkQ(-a!.n, a!.d)
        case 'unaryPlus':
          return a!
        case 'add':
          return qAdd(a!, b!)
        case 'subtract':
          return qSub(a!, b!)
        case 'multiply':
          return qMul(a!, b!)
        case 'divide':
          return qDiv(a!, b!)
        case 'pow':
          return qInt(b!) ? qPow(a!, b!.n) : null
        default:
          return null
      }
    }
    case 'FunctionNode': {
      const name = fnName(node)
      if (!['factorial', 'abs', 'floor', 'ceil'].includes(name) || node.args?.length !== 1) return null
      const a = exactEval(node.args[0]!, index, k, vars)
      if (a == null || a === UNDEF) return a
      if (name === 'abs') return { n: babs(a.n), d: a.d }
      if (name === 'floor' || name === 'ceil') {
        let q = a.n / a.d
        if (name === 'floor' && a.n < 0n && q * a.d !== a.n) q -= 1n
        if (name === 'ceil' && a.n > 0n && q * a.d !== a.n) q += 1n
        return { n: q, d: 1n }
      }
      if (!qInt(a)) return null
      if (a.n < 0n) return UNDEF
      if (a.n > 200n) return null
      let f = 1n
      for (let i = 2n; i <= a.n; i++) f *= i
      return mkQ(f, 1n)
    }
    default:
      return null
  }
}

/** A bound on the polynomial degree in the index, or null when the body isn't a polynomial. */
function degree(node: Node, index: string, vars: Record<string, number>): number | null {
  switch (node.type) {
    case 'ParenthesisNode':
      return degree(node.content!, index, vars)
    case 'ConstantNode':
      return 0
    case 'SymbolNode':
      return node.name === index ? 1 : 0
    case 'OperatorNode': {
      const args = node.args ?? []
      const ds = args.map((a) => degree(a, index, vars))
      if (ds.some((d) => d == null)) return null
      const [a, b] = ds as number[]
      switch (fnName(node)) {
        case 'unaryMinus':
        case 'unaryPlus':
          return a!
        case 'add':
        case 'subtract':
          return Math.max(a!, b!)
        case 'multiply':
          return a! + b!
        case 'divide':
          return b === 0 ? a! : null
        case 'pow': {
          if (b !== 0) return null
          if (a === 0) return 0
          const e = exactEval(args[1]!, index, null, vars)
          if (e == null || e === UNDEF || !qInt(e) || e.n < 0n || e.n > 64n) return null
          return a! * Number(e.n)
        }
        default:
          return null
      }
    }
    case 'FunctionNode':
      return (node.args ?? []).every((a) => !hasIndex(a, index)) ? 0 : null
    default:
      return null
  }
}

function binom(n: bigint, k: number): bigint {
  let out = 1n
  for (let i = 0n; i < BigInt(k); i++) out = (out * (n - i)) / (i + 1n)
  return out
}

/** Σ p(a+k) for k < count, from the forward differences of p at a (exact for any count). */
function polySum(node: Node, index: string, vars: Record<string, number>, a: bigint, count: bigint, deg: number): Ex {
  const m = Number(count < BigInt(deg + 1) ? count : BigInt(deg + 1))
  let row: Q[] = []
  for (let j = 0; j < m; j++) {
    const v = exactEval(node, index, a + BigInt(j), vars)
    if (v == null || v === UNDEF) return v
    row.push(v)
  }
  let total: Q = { n: 0n, d: 1n }
  for (let j = 0; j < m; j++) {
    const term = mkQ(row[0]!.n * binom(count, j + 1), row[0]!.d)
    const next = term && qAdd(total, term)
    if (!next) return null
    total = next
    const diffs: Q[] = []
    for (let i = 0; i + 1 < row.length; i++) {
      const d = qSub(row[i + 1]!, row[i]!)
      if (!d) return null
      diffs.push(d)
    }
    row = diffs
  }
  return total
}

function exactAnswer(q: Q): SumAnswer {
  const n = qToNumber(q)
  if (!Number.isFinite(n)) return { value: null }
  const text = qText(q)
  return { value: num(n === 0 ? 0 : n), exact: text.length <= MAX_EXACT_TEXT ? text : undefined }
}

const undefinedAnswer = (): SumAnswer => ({ value: textVal('undefined') })

function finite(spec: Spec, index: string, a: number, b: number, ctx: SumContext): SumAnswer {
  const vars = { ...ctx.variables }
  delete vars[index]
  const sctx = { ...ctx, variables: vars }
  const parsed = parseScientific(spec.body, sctx, index)
  const term = compileScientific(spec.body, sctx, index)
  if (!parsed || !term) return { value: null }
  const node = parsed.node as Node
  const count = b - a + 1
  const budget = workBudget(BUDGET)
  const exactCost = EXACT_NODE_COST * parsed.node.filter(() => true).length
  const isSum = spec.op === 'sum'

  if (isSum) {
    const deg = degree(node, index, vars)
    if (deg != null && deg <= 64) {
      const q = polySum(node, index, vars, BigInt(a), BigInt(count), deg)
      if (q === UNDEF) return undefinedAnswer()
      if (q) return exactAnswer(q)
    }
  }

  if (count <= EXACT_TERMS) {
    let acc: Q | null = isSum ? { n: 0n, d: 1n } : ONE
    for (let k = a; k <= b && acc; k++) {
      const v = exactEval(node, index, BigInt(k), vars)
      if (v === UNDEF) return undefinedAnswer()
      acc = v && (isSum ? qAdd(acc, v) : qMul(acc, v))
      spend(exactCost)
      if ((k & 63) === 0 && budget.over()) acc = null
    }
    if (acc) return exactAnswer(acc)
  }

  if (count > (isSum ? FLOAT_TERMS : PROD_FLOAT_TERMS)) return { value: null }
  let sum = isSum ? 0 : 1
  let comp = 0
  let absSum = 0
  for (let k = a; k <= b; k++) {
    const v = term(k)
    if (!v) return { value: null }
    if (v.kind !== 'number' || !Number.isFinite(v.n)) return undefinedAnswer()
    if (isSum) {
      // Neumaier summation
      const t = sum + v.n
      comp += Math.abs(sum) >= Math.abs(v.n) ? sum - t + v.n : v.n - t + sum
      sum = t
      absSum += Math.abs(v.n)
    } else sum *= v.n
    if ((k & 255) === 0 && budget.over()) return { value: null }
  }
  if (!isSum) return Number.isFinite(sum) ? { value: num(sum) } : { value: null }
  sum += comp
  // each term carries a few ulps of its own; what cancels below that is noise, and too much of it is no answer
  const err = 16 * EPS * absSum
  if (Math.abs(sum) <= err) return { value: num(0) }
  if (err > 1e-13 * Math.abs(sum)) return { value: null }
  return { value: num(sum) }
}

/** One term c·rⁿ·n^(−p)/(n!)^f, with c and r exact when they are rationals. */
type Term = { c: number; cq: Q | null; r: number; rq: Q | null; p: number; f: number }

type Walk = { index: string; vars: Record<string, number>; scope: Record<string, unknown> }

function constOf(node: Node, w: Walk): { v: number; q: Q | null } | null {
  let v: unknown
  try {
    v = node.compile().evaluate({ ...w.scope })
  } catch {
    return null
  }
  if (typeof v !== 'number' || !Number.isFinite(v)) return null
  const q = exactEval(node, w.index, null, w.vars)
  return { v, q: q && q !== UNDEF ? q : null }
}

function affine(node: Node, w: Walk): { al: number; be: number } | null {
  if (!hasIndex(node, w.index)) {
    const c = constOf(node, w)
    return c ? { al: 0, be: c.v } : null
  }
  const n = unwrap(node)
  if (n.type === 'SymbolNode') return { al: 1, be: 0 }
  if (n.type !== 'OperatorNode') return null
  const [x, y] = (n.args ?? []).map((a) => affine(a, w))
  switch (fnName(n)) {
    case 'unaryMinus':
      return x ? { al: -x.al, be: -x.be } : null
    case 'unaryPlus':
      return x ?? null
    case 'add':
      return x && y ? { al: x.al + y.al, be: x.be + y.be } : null
    case 'subtract':
      return x && y ? { al: x.al - y.al, be: x.be - y.be } : null
    case 'multiply':
      if (!x || !y) return null
      if (x.al === 0) return { al: x.be * y.al, be: x.be * y.be }
      if (y.al === 0) return { al: y.be * x.al, be: y.be * x.be }
      return null
    case 'divide':
      return x && y && y.al === 0 && y.be !== 0 ? { al: x.al / y.be, be: x.be / y.be } : null
    default:
      return null
  }
}

function qPowNum(q: Q | null, e: number): Q | null {
  if (!q || !Number.isInteger(e)) return null
  const r = qPow(q, BigInt(e))
  return r && r !== UNDEF ? r : null
}

function powTerm(t: Term, e: number): Term | null {
  if (!Number.isInteger(e) && (t.c < 0 || t.r < 0)) return null
  return { c: t.c ** e, cq: qPowNum(t.cq, e), r: t.r ** e, rq: qPowNum(t.rq, e), p: t.p * e, f: t.f * e }
}

function exponential(base: { v: number; q: Q | null }, ex: { al: number; be: number }): Term | null {
  const ints = Number.isInteger(ex.al) && Number.isInteger(ex.be)
  if (base.v === 0 || (base.v < 0 && !ints)) return null
  return {
    c: base.v ** ex.be,
    cq: ints ? qPowNum(base.q, ex.be) : null,
    r: base.v ** ex.al,
    rq: ints ? qPowNum(base.q, ex.al) : null,
    p: 0,
    f: 0,
  }
}

function classify(node: Node, w: Walk): Term | null {
  if (!hasIndex(node, w.index)) {
    const c = constOf(node, w)
    return c ? { c: c.v, cq: c.q, r: 1, rq: ONE, p: 0, f: 0 } : null
  }
  const n = unwrap(node)
  if (n.type === 'SymbolNode') return { c: 1, cq: ONE, r: 1, rq: ONE, p: -1, f: 0 }
  const args = n.args ?? []
  if (n.type === 'FunctionNode') {
    const name = fnName(n)
    if (args.length !== 1) return null
    const arg = unwrap(args[0]!)
    if (name === 'factorial' && arg.type === 'SymbolNode') return { c: 1, cq: ONE, r: 1, rq: ONE, p: 0, f: -1 }
    if (name === 'exp') {
      const ex = affine(arg, w)
      return ex ? exponential({ v: Math.E, q: null }, ex) : null
    }
    if (name === 'sqrt') {
      const t = classify(arg, w)
      return t && powTerm(t, 0.5)
    }
    return null
  }
  if (n.type !== 'OperatorNode') return null
  const name = fnName(n)
  if (name === 'pow') {
    const [base, ex] = args as [Node, Node]
    if (!hasIndex(base, w.index)) {
      const b = constOf(base, w)
      const e = affine(ex, w)
      return b && e ? exponential(b, e) : null
    }
    if (hasIndex(ex, w.index)) return null
    const e = constOf(ex, w)
    const t = classify(base, w)
    return e && t ? powTerm(t, e.v) : null
  }
  const [x, y] = args.map((a) => classify(a, w))
  switch (name) {
    case 'unaryMinus':
      return x ? { ...x, c: -x.c, cq: x.cq && { n: -x.cq.n, d: x.cq.d } } : null
    case 'unaryPlus':
      return x ?? null
    case 'multiply':
      if (!x || !y) return null
      return {
        c: x.c * y.c,
        cq: x.cq && y.cq && qMul(x.cq, y.cq),
        r: x.r * y.r,
        rq: x.rq && y.rq && qMul(x.rq, y.rq),
        p: x.p + y.p,
        f: x.f + y.f,
      }
    case 'divide': {
      if (!x || !y || y.c === 0 || y.r === 0) return null
      const cq = x.cq && y.cq ? qDiv(x.cq, y.cq) : null
      const rq = x.rq && y.rq ? qDiv(x.rq, y.rq) : null
      return {
        c: x.c / y.c,
        cq: cq === UNDEF ? null : cq,
        r: x.r / y.r,
        rq: rq === UNDEF ? null : rq,
        p: x.p - y.p,
        f: x.f - y.f,
      }
    }
    default:
      return null
  }
}

function additiveParts(node: Node, sign: number, out: { node: Node; sign: number }[]): void {
  const n = unwrap(node)
  const name = n.type === 'OperatorNode' ? fnName(n) : ''
  if (name === 'add' || name === 'subtract') {
    additiveParts(n.args![0]!, sign, out)
    additiveParts(n.args![1]!, name === 'add' ? sign : -sign, out)
  } else if (name === 'unaryMinus') additiveParts(n.args![0]!, -sign, out)
  else out.push({ node: n, sign })
}

const BERNOULLI = [1 / 6, -1 / 30, 1 / 42, -1 / 30, 5 / 66, -691 / 2730, 7 / 6, -3617 / 510, 43867 / 798, -174611 / 330, 854513 / 138, -236364091 / 2730]

/** Hurwitz ζ(s, x) = Σ (x+k)^−s for k ≥ 0, by Euler–Maclaurin (its continuation below s = 1). */
export function hurwitz(s: number, x: number): number | null {
  if (!(x > 0) || s === 1 || !(s > 0) || s > 400) return null
  if (s > 20) {
    // the tail past y is under y^(1−s)/(s−1)
    let sum = 0
    for (let y = x; ; y++) {
      sum += y ** -s
      if (y ** (1 - s) / (s - 1) < 1e-18 * sum) return sum
    }
  }
  const shift = Math.max(0, Math.ceil(25 - x))
  let sum = 0
  for (let k = 0; k < shift; k++) sum += (x + k) ** -s
  const y = x + shift
  sum += y ** (1 - s) / (s - 1) + y ** -s / 2
  let rising = s
  let fact = 2
  for (let j = 1; j <= BERNOULLI.length; j++) {
    const t = (BERNOULLI[j - 1]! / fact) * rising * y ** (-s - 2 * j + 1)
    sum += t
    if (Math.abs(t) < 1e-18 * Math.abs(sum)) return sum
    rising *= (s + 2 * j - 1) * (s + 2 * j)
    fact *= (2 * j + 1) * (2 * j + 2)
  }
  return null
}

// ζ(2k) = q·π^(2k)
const ZETA_EVEN: Record<number, Q> = {
  2: { n: 1n, d: 6n },
  4: { n: 1n, d: 90n },
  6: { n: 1n, d: 945n },
  8: { n: 1n, d: 9450n },
  10: { n: 1n, d: 93555n },
  12: { n: 691n, d: 638512875n },
}

function piPower(q: Q, p: number): string {
  const sign = q.n < 0n ? '-' : ''
  const n = babs(q.n)
  return `${sign}${n === 1n ? '' : n}pi^${p}${q.d === 1n ? '' : `/${q.d}`}`
}

type Part = { v: number; q?: Q; exact?: string }

function isOne(q: Q | null): boolean {
  return q != null && q.n === 1n && q.d === 1n
}

/** Σ over n ≥ a of a term whose convergence is settled by its shape; null when it isn't certain. */
function termSum(t: Term, a: number): Part | null {
  if (t.c === 0) return { v: 0, q: { n: 0n, d: 1n } }
  if (a < 0 || (t.p > 0 && a < 1)) return null
  const ar = Math.abs(t.r)
  if (t.f === 1) {
    if (t.p !== 0) return null
    if (a === 0) {
      const k = t.rq && qInt(t.rq) ? Number(t.rq.n) : null
      const exact = isOne(t.cq) && k != null && k >= 1 && k <= 20 ? (k === 1 ? 'e' : `e^${k}`) : undefined
      return { v: t.c * Math.exp(t.r), exact }
    }
    if (a === 1 && isOne(t.cq) && isOne(t.rq)) return { v: Math.E - 1, exact: 'e - 1' }
    return series(t, a)
  }
  if (t.f !== 0) return null
  if (ar > 1) return null
  if (ar < 1) {
    if (t.p === 0) {
      const v = (t.c * t.r ** a) / (1 - t.r)
      const rp = t.rq && a <= 4096 ? qPow(t.rq, BigInt(a)) : null
      const den = t.rq && qSub(ONE, t.rq)
      const top = t.cq && rp && rp !== UNDEF ? qMul(t.cq, rp) : null
      const q = top && den ? qDiv(top, den) : null
      return q && q !== UNDEF ? { v: qToNumber(q), q } : { v }
    }
    if (t.p === 1 && a === 1) {
      const v = -t.c * Math.log1p(-t.r)
      const inv = t.rq && qSub(ONE, t.rq)
      const x = inv && qDiv(ONE, inv)
      const sign = isOne(t.cq) ? '' : t.cq && t.cq.n === -1n && t.cq.d === 1n ? '-' : null
      return { v, exact: x && x !== UNDEF && sign != null ? `${sign}ln(${qText(x)})` : undefined }
    }
    return series(t, a)
  }
  const evenP = Number.isInteger(t.p) && t.p % 2 === 0 && ZETA_EVEN[t.p] ? ZETA_EVEN[t.p]! : null
  if (t.r === 1) {
    if (!(t.p > 1)) return null
    const z = hurwitz(t.p, a)
    if (z == null) return null
    let exact: string | undefined
    if (evenP && t.cq && a === 1) exact = piPower(qMul(t.cq, evenP)!, t.p)
    else if (evenP && isOne(t.cq) && a <= 30) {
      let head: Q | null = { n: 0n, d: 1n }
      for (let n = 1; n < a && head; n++) head = qAdd(head, { n: 1n, d: BigInt(n) ** BigInt(t.p) })
      const h = head && qText(head)
      if (h && h.length <= MAX_EXACT_TEXT) exact = `${piPower(evenP, t.p)} - ${h}`
    }
    return { v: t.c * z, exact }
  }
  // r = −1: alternating, which converges for any p > 0
  if (!(t.p > 0) || a > 100) return null
  const sign = a % 2 === 0 ? 1 : -1
  let v: number
  if (t.p === 1) {
    let head = 0
    for (let n = 1; n < a; n++) head += (n % 2 === 0 ? 1 : -1) / n
    v = -Math.LN2 - head
  } else {
    const z1 = hurwitz(t.p, a / 2)
    const z2 = hurwitz(t.p, (a + 1) / 2)
    if (z1 == null || z2 == null) return null
    v = sign * 2 ** -t.p * (z1 - z2)
  }
  let exact: string | undefined
  if (a === 1 && t.cq) {
    if (t.p === 1 && babs(t.cq.n) === 1n && t.cq.d === 1n) exact = t.cq.n > 0n ? '-ln(2)' : 'ln(2)'
    else if (evenP) {
      // Σ (−1)ⁿ n^−p from 1 is −(1 − 2^(1−p))·ζ(p)
      const eta = qMul(qSub(ONE, { n: 1n, d: 2n ** BigInt(t.p - 1) })!, evenP)!
      exact = piPower(qMul(t.cq, { n: -eta.n, d: eta.d })!, t.p)
    }
  }
  return { v: t.c * v, exact }
}

/** Direct summation with a proven bound on what's left; |r| < 1, or a factorial below. FLOAT_TERMS caps the work. */
function series(t: Term, a: number): Part | null {
  const lnr = Math.log(Math.abs(t.r))
  const neg = t.r < 0
  let lnFact = 0
  for (let k = 2; k <= a; k++) lnFact += Math.log(k)
  let sum = 0
  let absSum = 0
  // exp(x) carries about |x| ulps of error
  let spread = 8
  for (let n = a, i = 0; i < FLOAT_TERMS; n++, i++) {
    if (n > a) lnFact += t.f ? Math.log(n) : 0
    const x = n === 0 ? 0 : n * lnr - t.p * Math.log(n) - t.f * lnFact
    spread = Math.max(spread, 8 + Math.abs(x))
    const mag = n === 0 ? (t.p < 0 ? 0 : 1) : Math.exp(x)
    const term = t.c * mag * (neg && n % 2 === 1 ? -1 : 1)
    sum += term
    absSum += Math.abs(term)
    // every later ratio |t(m+1)/t(m)| is at most rho, so the rest is under |t|·rho/(1−rho)
    const rho = n === 0 ? Infinity : (Math.abs(t.r) * Math.max(1, ((n + 1) / n) ** -t.p)) / (t.f ? n + 1 : 1)
    if (rho < 1 && (Math.abs(term) * rho) / (1 - rho) <= 1e-17 * Math.abs(sum)) {
      if (spread * EPS * absSum > 1e-13 * Math.abs(sum)) return null
      return { v: sum }
    }
  }
  return null
}

function infinite(spec: Spec, index: string, a: number, ctx: SumContext): SumAnswer {
  if (spec.op !== 'sum') return { value: null }
  const vars = { ...ctx.variables }
  delete vars[index]
  const sctx = { ...ctx, variables: vars }
  const parsed = parseScientific(spec.body, sctx, index)
  const term = compileScientific(spec.body, sctx, index)
  if (!parsed || !term) return { value: null }
  const w: Walk = { index, vars, scope: parsed.scope }
  const pieces: { node: Node; sign: number }[] = []
  additiveParts(parsed.node as Node, 1, pieces)
  const terms: Term[] = []
  for (const piece of pieces) {
    const t = classify(piece.node, w)
    if (!t || !Number.isFinite(t.c) || !Number.isFinite(t.r) || !Number.isFinite(t.p)) return { value: null }
    terms.push(piece.sign > 0 ? t : { ...t, c: -t.c, cq: t.cq && { n: -t.cq.n, d: t.cq.d } })
  }
  if (!agrees(terms, term, a)) return { value: null }
  const parts: Part[] = []
  for (const t of terms) {
    const part = termSum(t, a)
    if (!part || !Number.isFinite(part.v)) return { value: null }
    parts.push(part)
  }
  const v = parts.reduce((s, p) => s + p.v, 0)
  let exact: string | undefined
  if (parts.every((p) => p.q)) {
    let q: Q | null = { n: 0n, d: 1n }
    for (const p of parts) q = q && qAdd(q, p.q!)
    if (q) return exactAnswer(q)
  } else if (parts.length === 1) exact = parts[0]!.exact
  return { value: num(Math.abs(v) < 1e-300 ? 0 : v), exact }
}

function factorialOf(n: number): number {
  let f = 1
  for (let k = 2; k <= n; k++) f *= k
  return f
}

/** The shape read off the tree must give the same numbers as evaluating the body. */
function agrees(terms: Term[], term: (k: number) => Value | null, a: number): boolean {
  let checked = 0
  for (const n of [a, a + 1, a + 2, a + 5, a + 11]) {
    const actual = term(n)
    if (!actual || actual.kind !== 'number' || !Number.isFinite(actual.n)) return false
    let model = 0
    for (const t of terms) model += (t.c * t.r ** n * (n === 0 && t.p === 0 ? 1 : n ** -t.p)) / factorialOf(n) ** t.f
    if (!Number.isFinite(model) || Math.abs(model) < 1e-250) continue
    if (Math.abs(model - actual.n) > 1e-9 * Math.max(Math.abs(model), Math.abs(actual.n))) return false
    checked++
  }
  return checked >= 2
}

const INFINITY_RE = /^\+?(?:∞|inf|infty|infinity|\\infty|oo)$/i

function bound(text: string, ctx: SumContext): number | null {
  const v = tryPlainMath(text, { ...ctx, defaultUnits: undefined })
  if (!v || v.kind !== 'number' || v.unit || !Number.isSafeInteger(v.n) || Math.abs(v.n) > MAX_BOUND) return null
  return v.n
}

function evaluateSpec(spec: Spec, ctx: SumContext): SumAnswer {
  const body = /\\[a-zA-Z]|[\^_]\{/.test(spec.body) ? latexToAscii(spec.body) : spec.body
  const s = { ...spec, body }
  // a typed θ reaches here as theta, the one multi-letter index
  const index = spec.index ?? pickIndex(body, ctx)
  if (!index || !/^(?:[A-Za-z]|theta)$/.test(index) || index === 'e' || index in (ctx.functions ?? {})) return { value: null }
  const a = bound(spec.from, ctx)
  if (a == null) return { value: null }
  if (INFINITY_RE.test(spec.to)) return infinite(s, index, a, ctx)
  const b = bound(spec.to, ctx)
  if (b == null || b < a) return { value: null }
  return finite(s, index, a, b, ctx)
}

/** A `Π(2, 3, 4)` list multiplies its entries; a list inside (`[1, 2]`) is left alone. */
function productOfList(args: string[]): string | null {
  if (args.some((a) => !a.trim() || /[[\]]/.test(a))) return null
  return `(${args.map((a) => `(${a.trim()})`).join('*')})`
}

/**
 * Σ and Π lines (already `sum` and `prod` via normalizeSums), or null when the line holds none
 * and the usual evaluation should run.
 */
export function sumAnswer(expr: string, ctx: SumContext): SumAnswer | null {
  if (!HINT_RE.test(expr)) return null
  const whole = wholeLineSpec(expr.trim(), ctx)
  if (whole) {
    try {
      return evaluateSpec(whole, ctx)
    } catch {
      return { value: null }
    }
  }

  let out = ''
  let last = 0
  let replaced = false
  let allExact = true
  CALL_RE.lastIndex = 0
  for (let m = CALL_RE.exec(expr); m; m = CALL_RE.exec(expr)) {
    const open = m.index + m[0].length - 1
    const close = closeOf(expr, open)
    if (close < 0) break
    const op = opOf(m[1]!)
    const args = splitTop(expr.slice(open + 1, close), /,/)
    const spec = specFromCall(op, args, ctx)
    let put: string | null = null
    if (spec) {
      let r: SumAnswer
      try {
        r = evaluateSpec(spec, ctx)
      } catch {
        r = { value: null }
      }
      if (!r.value || r.value.kind !== 'number') return r
      if (r.exact == null && !Number.isInteger(r.value.n)) allExact = false
      put = `(${r.value.n})`
    } else if (op === 'prod') put = productOfList(args)
    if (put == null) continue
    out += expr.slice(last, m.index) + put
    last = close + 1
    CALL_RE.lastIndex = close + 1
    replaced = true
  }
  if (!replaced) return null
  const text = out + expr.slice(last)
  const value = tryPlainMath(text, ctx)
  const form =
    allExact && value?.kind === 'number' && Number.isFinite(value.n) && wantsExactForm(text)
      ? exactForm(value.n, { rationalize: ctx.rationalize })
      : null
  return { value, exact: form ?? undefined }
}
