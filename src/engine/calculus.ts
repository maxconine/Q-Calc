import type { MathNode } from 'mathjs'
import { derivativeAt, integrate, justified, limit, type RealFn } from './calcNumeric'
import type { Ast, ClosedFormJob } from './closedForm'
import { num, textVal } from './format'
import { math } from './math'
import { compileScientific, evalScientific, preprocessChecked, SCIENTIFIC_NAMES, type ScientificContext } from './scientific'
import type { UserFunction, Value } from './types'

type Intent =
  /** One variable per differentiation, innermost first; `''` means pick one from the body. */
  | { op: 'derivative'; body: string; vars: string[]; at?: string; partial?: boolean }
  | { op: 'integral'; body: string; v?: string; lower: string; upper: string }
  | { op: 'limit'; body: string; v: string; to: string; side: -1 | 0 | 1 }

/** Which letters a time derivative took as changing with time, and which as constant, as shown (θ, not theta). */
export type TimeLetters = {
  varying: string[]
  constant: string[]
  /** Letters written with a dot somewhere, which have to move; a chip can't make them constant. */
  locked: string[]
}

export type CalculusResult = {
  value: Value | null
  time?: TimeLetters
  exact?: string
  /** High-precision work for the closed-form worker; absent once it has answered. */
  job?: ClosedFormJob
}

// ---- syntax

const SUPER_ORDER: Record<string, number> = { '²': 2, '³': 3 }

function orderOf(s: string | undefined): number | null {
  if (!s) return null
  return SUPER_ORDER[s] ?? Number(s)
}

/** Index of the last ` at ` outside parentheses. */
function splitAt(s: string): { body: string; at?: string } {
  let depth = 0
  let found = -1
  for (let i = 0; i < s.length; i++) {
    const ch = s[i]!
    if (ch === '(' || ch === '[' || ch === '{') depth++
    else if (ch === ')' || ch === ']' || ch === '}') depth--
    else if (depth === 0 && /\s/.test(ch) && /^\s+at\s+\S/i.test(s.slice(i))) found = i
  }
  if (found < 0) return { body: s.trim() }
  const at = s.slice(found).replace(/^\s+at\s+/i, '').trim()
  return { body: s.slice(0, found).trim(), at }
}

function stripOuterParens(s: string): string {
  let t = s.trim()
  while (t.startsWith('(') && t.endsWith(')') && closingParen(t, 0) === t.length - 1) t = t.slice(1, -1).trim()
  return t
}

function closingParen(s: string, open: number): number {
  const pairs: Record<string, string> = { '(': ')', '{': '}', '[': ']' }
  const close = pairs[s[open]!]
  if (!close) return -1
  let depth = 0
  for (let i = open; i < s.length; i++) {
    if (s[i] === s[open]) depth++
    else if (s[i] === close && --depth === 0) return i
  }
  return -1
}

/** Top-level comma-separated arguments of `(a, b, c)`. */
function splitArgs(inner: string): string[] {
  const out: string[] = []
  let depth = 0
  let start = 0
  for (let i = 0; i < inner.length; i++) {
    const ch = inner[i]
    if (ch === '(' || ch === '[' || ch === '{') depth++
    else if (ch === ')' || ch === ']' || ch === '}') depth--
    else if (ch === ',' && depth === 0) {
      out.push(inner.slice(start, i).trim())
      start = i + 1
    }
  }
  out.push(inner.slice(start).trim())
  return out
}

/** A bound written before the body: up to the first space, or a `(` that starts the body (`∫0..1(x+1)`). */
function scanBound(s: string): { bound: string; rest: string } | null {
  const t = s.trimStart()
  if (t.startsWith('{')) {
    const close = closingParen(t, 0)
    if (close < 0) return null
    return { bound: t.slice(1, close), rest: t.slice(close + 1).trim() }
  }
  let depth = 0
  let i = 0
  for (; i < t.length; i++) {
    const ch = t[i]!
    if (depth === 0 && /\s/.test(ch)) break
    if (ch === '(') {
      if (depth === 0 && i > 0 && !/[A-Za-z_]/.test(t[i - 1]!)) break
      depth++
    } else if (ch === ')') {
      if (depth === 0) return null
      depth--
    }
  }
  const bound = t.slice(0, i)
  if (!bound || depth !== 0) return null
  return { bound, rest: t.slice(i).trim() }
}

/** `x^2 dx` names its variable; the `dx` isn't part of the body. */
function stripDifferential(body: string): { body: string; v?: string } {
  const m = body.match(/^(.*?)(?:\s*\*)?\s*\bd([A-Za-z])\s*$/s)
  if (!m || !m[1]!.trim()) return { body: body.trim() }
  return { body: m[1]!.trim(), v: m[2] }
}

function integralFromArgs(args: string[]): Intent | null {
  if (args.length === 3 && args.every(Boolean)) {
    const { body, v } = stripDifferential(args[0]!)
    return { op: 'integral', body, v, lower: args[1]!, upper: args[2]! }
  }
  if (args.length === 4 && /^[A-Za-z]$/.test(args[1]!) && args[0] && args[2] && args[3]) {
    return { op: 'integral', body: args[0], v: args[1], lower: args[2], upper: args[3] }
  }
  return null
}

/** `f from a to b` */
function fromTo(s: string): { body: string; lower: string; upper: string } | null {
  const m = s.match(/^(.+?)\s+from\s+(.+?)\s+to\s+(.+)$/is)
  if (!m) return null
  return { body: m[1]!.trim(), lower: m[2]!.trim(), upper: m[3]!.trim() }
}

function parseIntegral(text: string, functions: Record<string, UserFunction>): Intent | null {
  const call = text.match(/^(?:int|integral|integrate|∫)\s*\((.*)\)$/is)
  if (call && closingParen(text, text.indexOf('(')) === text.length - 1) {
    const name = text.match(/^[A-Za-z]+/)?.[0]
    if (name && name in functions) return null
    return integralFromArgs(splitArgs(call[1]!))
  }
  const words = text.match(/^(?:the\s+)?(?:integral|integrate)\s+(?:of\s+)?(.+)$/is)
  if (words) {
    const from = words[1]!.match(/^from\s+(.+?)\s+to\s+(.+?)\s+of\s+(.+)$/is)
    const ft = from ? { body: from[3]!, lower: from[1]!, upper: from[2]! } : fromTo(words[1]!)
    if (!ft) return null
    const { body, v } = stripDifferential(ft.body)
    return { op: 'integral', body, v, lower: ft.lower, upper: ft.upper }
  }
  // the word works as the sign too, for anyone who keeps typed words as text
  const sign = text.match(/^(?:∫|int(?=_|\s|[^A-Za-z\s(]\S*\.\.))/i)
  if (!sign) return null
  const rest = text.slice(sign[0].length).trim()
  let lower: string | undefined
  let upper: string | undefined
  let body: string | undefined
  const under = rest.match(/^_\s*/)
  if (under) {
    let r = rest.slice(under[0].length)
    const lo = r.startsWith('{') ? scanBound(r) : (() => {
      const k = r.indexOf('^')
      return k > 0 ? { bound: r.slice(0, k), rest: r.slice(k) } : null
    })()
    if (!lo) return null
    r = lo.rest.trim()
    if (!r.startsWith('^')) return null
    const hi = scanBound(r.slice(1))
    if (!hi) return null
    lower = lo.bound
    upper = hi.bound
    body = hi.rest
  } else {
    const dots = rest.indexOf('..')
    if (dots > 0 && !/\s/.test(rest.slice(0, dots).trim())) {
      const hi = scanBound(rest.slice(dots + 2))
      if (!hi) return null
      lower = rest.slice(0, dots)
      upper = hi.bound
      body = hi.rest
    } else {
      const ft = fromTo(rest)
      if (!ft) return null
      ;({ lower, upper, body } = ft)
    }
  }
  if (!lower?.trim() || !upper?.trim() || !body?.trim()) return null
  const d = stripDifferential(body)
  return { op: 'integral', body: d.body, v: d.v, lower: lower.trim(), upper: upper.trim() }
}

const INTEGRAL_START_RE = /(?:the\s+)?(?:integrate|integral)|int|∫/gi

/** First `)` `]` or `}` that the integral itself did not open, else the end of the line. */
function groupEnd(s: string, from: number): number {
  let depth = 0
  for (let i = from; i < s.length; i++) {
    const ch = s[i]!
    if ('([{'.includes(ch)) depth++
    else if (')]}'.includes(ch)) {
      if (depth === 0) return i
      depth--
    }
  }
  return s.length
}

function isDifferentialAt(s: string, i: number): boolean {
  if (s[i] !== 'd' && s[i] !== 'D') return false
  if (i > 0 && /[A-Za-z0-9_]/.test(s[i - 1]!)) return false
  if (!/^[A-Za-z]$/.test(s[i + 1] ?? '')) return false
  return !/[A-Za-z0-9_]/.test(s[i + 2] ?? '')
}

/** Where a sign-form integrand ends so `∫ x^2 dx + 1` is the integral plus one, not a failed body. */
function differentialCut(s: string, from: number, limit: number): number | null {
  let depth = 0
  for (let i = from; i < limit; i++) {
    const ch = s[i]!
    if ('([{'.includes(ch)) depth++
    else if (')]}'.includes(ch)) {
      if (depth === 0) return null
      depth--
    } else if (depth === 0 && isDifferentialAt(s, i)) {
      let j = i + 2
      while (j < limit && /\s/.test(s[j]!)) j++
      if (j < limit && /[+\-*/^×·⋅]/.test(s[j]!)) return i + 2
    }
  }
  return null
}

function locateIntegral(
  text: string,
  at: number,
  functions: Record<string, UserFunction>,
): { start: number; end: number } | null {
  const slice = text.slice(at)
  const call = slice.match(/^(?:int|integral|integrate|∫)\s*\(/i)
  if (call) {
    const open = at + call[0].length - 1
    const close = closingParen(text, open)
    if (close < 0) return null
    return parseIntegral(text.slice(at, close + 1), functions) ? { start: at, end: close + 1 } : null
  }
  const limit = groupEnd(text, at)
  let end = limit
  while (end > at && /\s/.test(text[end - 1]!)) end--
  const sign = slice.match(/^(?:∫|int(?=_|\s|[^A-Za-z\s(]\S*\.\.))/i)
  if (sign && !/^(?:the\s+)?(?:integral|integrate)\b/i.test(slice)) {
    const cut = differentialCut(text, at, end)
    if (cut != null && parseIntegral(text.slice(at, cut), functions)) return { start: at, end: cut }
  }
  if (!/^(?:the\s+)?(?:integral|integrate)\b|^(?:∫|int(?=_|\s|[^A-Za-z\s(]\S*\.\.))/i.test(slice)) return null
  return parseIntegral(text.slice(at, end), functions) ? { start: at, end } : null
}

/**
 * Replaces integrals inside a larger line (`4*∫0..1 x^2`, `(∫0..1 x dx)+1`) with their values.
 * Null when the line has none. `'fail'` when one is there but has no trustworthy value.
 */
export function embedIntegrals(expr: string, ctx: ScientificContext): string | 'fail' | null {
  const functions = ctx.functions ?? {}
  const found: { start: number; end: number }[] = []
  INTEGRAL_START_RE.lastIndex = 0
  for (let m = INTEGRAL_START_RE.exec(expr); m; m = INTEGRAL_START_RE.exec(expr)) {
    const at = m.index
    if (at > 0 && /[A-Za-z0-9_]/.test(expr[at - 1]!)) continue
    const loc = locateIntegral(expr, at, functions)
    if (loc) found.push(loc)
  }
  if (!found.length) return null
  const innermost = found.filter(
    (a) => !found.some((b) => b !== a && b.start >= a.start && b.end <= a.end && (b.start > a.start || b.end < a.end)),
  )
  const ready: { start: number; end: number; n: number }[] = []
  for (const loc of innermost) {
    let result: CalculusResult | null
    try {
      result = evaluateCalculus(expr.slice(loc.start, loc.end), ctx)
    } catch {
      return 'fail'
    }
    if (!result?.value || result.value.kind !== 'number' || !Number.isFinite(result.value.n)) return 'fail'
    ready.push({ ...loc, n: result.value.n })
  }
  ready.sort((a, b) => b.start - a.start)
  let text = expr
  for (const p of ready) text = `${text.slice(0, p.start)}(${p.n})${text.slice(p.end)}`
  return text
}

function parseSide(to: string): { to: string; side: -1 | 0 | 1 } {
  const m = to.match(/^(.+?)(?:\^?\s*([+-])|([⁺⁻]))$/)
  if (!m || !m[1]!.trim()) return { to, side: 0 }
  const s = m[2] ?? (m[3] === '⁺' ? '+' : '-')
  return { to: m[1]!.trim(), side: s === '+' ? 1 : -1 }
}

const ARROW = String.raw`(?:->|→|\\to\b)`

function parseLimit(text: string): Intent | null {
  const words = text.match(new RegExp(String.raw`^(?:the\s+)?limit\s+of\s+(.+?)\s+as\s+([A-Za-z])\s*(?:${ARROW}|approaches\s)\s*(.+)$`, 'is'))
  if (words) {
    const { to, side } = parseSide(words[3]!.trim())
    return { op: 'limit', body: words[1]!.trim(), v: words[2]!, to, side }
  }
  const m = text.match(new RegExp(String.raw`^lim(?:it)?\s*(?:_\s*)?([({]?)\s*([A-Za-z])\s*${ARROW}\s*(.*)$`, 's'))
  if (!m) return null
  const v = m[2]!
  let rest = m[3]!
  let toText: string
  if (m[1]) {
    const close = m[1] === '(' ? ')' : '}'
    let depth = 1
    let i = 0
    for (; i < rest.length; i++) {
      if (rest[i] === m[1]) depth++
      else if (rest[i] === close && --depth === 0) break
    }
    if (i >= rest.length) return null
    toText = rest.slice(0, i)
    rest = rest.slice(i + 1)
  } else {
    const b = scanBound(rest)
    if (!b) return null
    toText = b.bound
    rest = b.rest
  }
  const body = rest.trim()
  if (!body || !toText.trim()) return null
  const { to, side } = parseSide(toText.trim())
  return { op: 'limit', body, v, to, side }
}

/** A variable a derivative can be taken in: a letter, a Greek letter, or `theta` (θ as the engine reads it). */
const VAR = String.raw`(?:theta|[A-Za-z]|(?![πτζθΠΣ])[α-ωΑ-Ω])`
const ORDER = String.raw`(?:\^?([2-9]|[²³]))?`
const ORDINARY_RE = new RegExp(String.raw`^d${ORDER}\s*\/\s*d(${VAR})${ORDER}(?![A-Za-z0-9_α-ωΑ-Ω])(.*)$`, 's')
const PARTIAL_RE = new RegExp(String.raw`^∂${ORDER}\s*\/\s*((?:∂\s*${VAR}(?:\^?(?:[2-9]|[²³]))?\s*)+?)(?![A-Za-z0-9_α-ωΑ-Ω])(?=[\s(]|$)(.*)$`, 's')
const PARTIAL_PART_RE = new RegExp(String.raw`∂\s*(${VAR})(?:\^?([2-9]|[²³]))?`, 'g')
const WRT_RE = new RegExp(String.raw`^(.+?)\s+(?:with\s+respect\s+to|wrt|w\.r\.t\.?)\s+(${VAR})$`, 'is')

/** `∂²/∂x∂y`: the denominator's variables, rightmost first since that one is applied first. */
function partialVars(denominator: string): string[] {
  const out: string[] = []
  for (const m of denominator.matchAll(PARTIAL_PART_RE)) {
    const n = orderOf(m[2]) ?? 1
    for (let i = 0; i < n; i++) out.push(m[1]!)
  }
  return out.reverse()
}

function parseDerivative(text: string, functions: Record<string, UserFunction>, variables: Record<string, number>): Intent | null {
  const m = text.match(ORDINARY_RE)
  if (m) {
    const v = m[2]!
    // `d/dx` with `d` and `dx` both defined is plain division
    if (`d${v}` in variables || `d${v}` in functions) return null
    const top = orderOf(m[1])
    const bottom = orderOf(m[3])
    if (top !== bottom && !(top == null && bottom == null)) return null
    const { body, at } = splitAt(m[4]!)
    if (!body) return null
    return { op: 'derivative', body: stripOuterParens(body), vars: Array(top ?? 1).fill(v), at }
  }
  const p = text.match(PARTIAL_RE)
  if (p) {
    const vars = partialVars(p[2]!)
    if (vars.length !== (orderOf(p[1]) ?? 1) || vars.length > 4) return null
    const { body, at } = splitAt(p[3]!)
    if (!body) return null
    return { op: 'derivative', body: stripOuterParens(body), vars, at, partial: true }
  }
  const words = text.match(/^(?:the\s+)?(?:(second|2nd)\s+)?(?:partial\s+)?derivative\s+of\s+(.+)$/is)
  if (words) {
    const split = splitAt(words[2]!)
    const wrt = split.body.match(WRT_RE)
    const body = wrt ? wrt[1]!.trim() : split.body
    if (!body) return null
    return { op: 'derivative', body, vars: Array(words[1] ? 2 : 1).fill(wrt?.[2] ?? ''), at: split.at }
  }
  const prime = text.match(/^([A-Za-z][A-Za-z0-9]*)\s*('+|′+|″)\s*\((.+)\)$/s)
  if (prime) {
    const name = prime[1]!
    const def = functions[name]
    if (!def || def.params.length !== 1) return null
    const order = prime[2] === '″' ? 2 : prime[2]!.length
    if (order > 3) return null
    const arg = prime[3]!.trim()
    const param = def.params[0]!
    if (/^[A-Za-z]$/.test(arg) && !(arg in variables)) return { op: 'derivative', body: `${name}(${arg})`, vars: Array(order).fill(arg) }
    return { op: 'derivative', body: `${name}(${param})`, vars: Array(order).fill(param), at: arg }
  }
  return null
}

function parseCalculus(
  raw: string,
  functions: Record<string, UserFunction> = {},
  variables: Record<string, number> = {},
): Intent | null {
  const text = raw.trim()
  if (!text) return null
  return parseDerivative(text, functions, variables) ?? parseIntegral(text, functions) ?? parseLimit(text)
}

const CALCULUS_START = /^(?:∫|int(?:_|[^A-Za-z\s(]\S*\.\.)|d[²³]?\s*\/\s*d(?:[A-Za-z]|[α-ωΑ-Ω])|∂[²³]?\s*\/\s*∂|lim(?:it)?\b.*(?:->|→)|(?:the\s+)?(?:integral|integrate|(?:partial\s+)?derivative|limit)\s+of\b|[A-Za-z][A-Za-z0-9]*\s*(?:'+|′+|″)\s*\()/i

/** Cheap syntactic check, true while a calculus line is still being typed, so soulvercore stays out of it. */
export function isCalculusInput(text: string): boolean {
  const t = text.trim()
  return CALCULUS_START.test(t) || parseCalculus(t) != null
}

// ---- expression trees

type N = {
  type: string
  value?: unknown
  name?: string
  op?: string
  fn?: string | { name: string }
  args?: N[]
  content?: N
}

const asN = (n: MathNode): N => n as unknown as N
const asMath = (n: N): MathNode => n as unknown as MathNode

const constant = (v: number): N => asN(new math.ConstantNode(v))
const symbol = (name: string): N => asN(new math.SymbolNode(name))
const op = (o: string, fn: string, args: N[]): N => asN(new math.OperatorNode(o as '+', fn as 'add', args.map(asMath)))
const call = (name: string, args: N[]): N => asN(new math.FunctionNode(name, args.map(asMath)))

function fnName(n: N): string {
  return typeof n.fn === 'string' ? n.fn : (n.fn?.name ?? '')
}

const DEG = 'deg__'
const TRIG = new Set(['sin', 'cos', 'tan', 'sec', 'csc', 'cot'])
const INVERSE_TRIG = new Set(['asin', 'acos', 'atan', 'asec', 'acsc', 'acot'])
/** Functions both mathjs `derivative` and the closed-form evaluator understand. */
const SMOOTH = new Set([
  ...TRIG,
  ...INVERSE_TRIG,
  'sinh', 'cosh', 'tanh', 'sech', 'csch', 'coth', 'asinh', 'acosh', 'atanh',
  'sqrt', 'cbrt', 'nthRoot', 'exp', 'log', 'log10', 'abs',
])

type TreeCtx = ScientificContext & { bound: string | null; depth: number; keep?: string[] }

/**
 * A preprocessed expression as a tree in radians: user functions inlined, known variables as numbers,
 * `ln` as mathjs's `log`, and in degree mode each trig call scaled by the `deg__` symbol.
 */
function toTree(text: string, ctx: TreeCtx): N | null {
  if (ctx.depth > 8) return null
  const variables = { ...ctx.variables }
  for (const name of [ctx.bound, ...(ctx.keep ?? [])]) if (name) variables[name] = 0
  const expr = preprocessChecked(text, { ...ctx, variables })
  if (expr == null) return null
  let root: N
  try {
    root = asN(math.parse(expr))
  } catch {
    return null
  }
  return convert(root, ctx)
}

function convert(n: N, ctx: TreeCtx): N | null {
  switch (n.type) {
    case 'ParenthesisNode':
      return n.content ? convert(n.content, ctx) : null
    case 'ConstantNode':
      return typeof n.value === 'number' && Number.isFinite(n.value) ? constant(n.value) : null
    case 'SymbolNode': {
      const name = n.name!
      if (name === ctx.bound || ctx.keep?.includes(name)) return symbol(name)
      if (name === 'pi' || name === 'e') return symbol(name)
      if (name === 'tau') return op('*', 'multiply', [constant(2), symbol('pi')])
      const v = ctx.variables?.[name]
      if (v != null && Number.isFinite(v)) return constant(v)
      return null
    }
    case 'OperatorNode': {
      const fn = fnName(n)
      if (!['add', 'subtract', 'multiply', 'divide', 'pow', 'unaryMinus', 'unaryPlus'].includes(fn)) return null
      const args = n.args!.map((a) => convert(a, ctx))
      if (args.some((a) => !a)) return null
      return fn === 'unaryPlus' ? args[0]! : op(n.op!, fn, args as N[])
    }
    case 'FunctionNode': {
      const args = n.args!.map((a) => convert(a, ctx))
      if (args.some((a) => !a)) return null
      return convertCall(fnName(n), args as N[], ctx)
    }
    default:
      return null
  }
}

function convertCall(fn: string, args: N[], ctx: TreeCtx): N | null {
  const user = ctx.functions?.[fn]
  if (user) return inline(user, args, ctx)
  const name = fn === 'ln' ? 'log' : fn
  if (name === 'log' && args.length === 2) return op('/', 'divide', [call('log', [args[0]!]), call('log', [args[1]!])])
  if (!SMOOTH.has(name)) return null
  if (ctx.angleMode !== 'rad') {
    if (TRIG.has(name)) return call(name, [op('*', 'multiply', [args[0]!, symbol(DEG)])])
    if (INVERSE_TRIG.has(name)) return op('/', 'divide', [call(name, args), symbol(DEG)])
  }
  return call(name, args)
}

/** A user function's body with its parameters replaced by the (already converted) arguments. */
function inline(def: UserFunction, args: N[], ctx: TreeCtx): N | null {
  if (args.length !== def.params.length) return null
  const placeholders = def.params.map((_, i) => `pp${ctx.depth}q${i}`)
  let body = def.body
  def.params.forEach((p, i) => {
    body = body.replace(new RegExp(`(?<![A-Za-z0-9_])${p}(?![A-Za-z0-9_])`, 'g'), placeholders[i]!)
  })
  const tree = toTree(body, { ...ctx, bound: null, keep: placeholders, depth: ctx.depth + 1 })
  if (!tree) return null
  return substitute(tree, Object.fromEntries(placeholders.map((p, i) => [p, args[i]!])))
}

function substitute(n: N, map: Record<string, N>): N {
  if (n.type === 'SymbolNode' && n.name! in map) return map[n.name!]!
  if (n.type === 'OperatorNode') return op(n.op!, fnName(n), n.args!.map((a) => substitute(a, map)))
  if (n.type === 'FunctionNode') return call(fnName(n), n.args!.map((a) => substitute(a, map)))
  return n
}

function hasSymbol(n: N): boolean {
  if (n.type === 'SymbolNode') return n.name !== 'pi' && n.name !== 'e'
  return (n.args ?? []).some(hasSymbol) || (n.content ? hasSymbol(n.content) : false)
}

function mentions(n: N, name: string): boolean {
  if (n.type === 'SymbolNode') return n.name === name
  return (n.args ?? []).some((a) => mentions(a, name))
}

// ---- symbolic derivatives

/** Radian trees back to what the user typed: `sin(x deg__)` is `sin(x)` again, and `deg__` is π/180. */
function toUserAngles(n: N, deg: boolean): N | null {
  if (n.type === 'FunctionNode') {
    const name = fnName(n)
    const args = n.args!
    if (TRIG.has(name) && mentions(args[0]!, DEG)) {
      const a = args[0]!
      if (a.type !== 'OperatorNode' || fnName(a) !== 'multiply' || a.args![1]?.type !== 'SymbolNode' || a.args![1]!.name !== DEG) {
        return null
      }
      const inner = toUserAngles(a.args![0]!, deg)
      return inner && call(name, [inner])
    }
    const inner = args.map((x) => toUserAngles(x, deg))
    if (inner.some((x) => !x)) return null
    const back = call(name, inner as N[])
    // a radian inverse trig value is the user's degrees value times deg__
    if (deg && INVERSE_TRIG.has(name)) return op('*', 'multiply', [back, symbol(DEG)])
    return back
  }
  if (n.type === 'OperatorNode') {
    const args = n.args!.map((x) => toUserAngles(x, deg))
    if (args.some((x) => !x)) return null
    return op(n.op!, fnName(n), args as N[])
  }
  if (n.type === 'ParenthesisNode') return n.content ? toUserAngles(n.content, deg) : null
  return n
}

/**
 * simplify folds `log(2)` into 0.693...; constant calls hide behind placeholder symbols while it runs.
 * A call is constant when it doesn't mention `v`, or with `v` null when it mentions no letter at all.
 */
function simplifyKeepingConstants(n: N, v: string | null): N {
  const saved: N[] = []
  const hide = (x: N): N => {
    // d/dx e^x comes back from mathjs as e^x log(e)
    if (x.type === 'FunctionNode' && fnName(x) === 'log' && x.args![0]!.type === 'SymbolNode' && x.args![0]!.name === 'e') return constant(1)
    if (x.type === 'FunctionNode' && (v == null ? !hasSymbol(x) : !mentions(x, v))) {
      saved.push(x)
      return symbol(`k${saved.length - 1}__`)
    }
    if (x.type === 'OperatorNode') return op(x.op!, fnName(x), x.args!.map(hide))
    if (x.type === 'FunctionNode') return call(fnName(x), x.args!.map(hide))
    return x
  }
  const simplified = asN(math.simplify(asMath(hide(n))))
  const show = (x: N): N => {
    if (x.type === 'SymbolNode') {
      const m = x.name!.match(/^k(\d+)__$/)
      return m ? saved[Number(m[1])]! : x
    }
    if (x.type === 'ParenthesisNode') return x.content ? show(x.content) : x
    if (x.type === 'OperatorNode') return op(x.op!, fnName(x), x.args!.map(show))
    if (x.type === 'FunctionNode') return call(fnName(x), x.args!.map(show))
    return x
  }
  return show(simplified)
}

/** Text the engine reads back: natural log as `ln`, π as `pi`. */
function engineText(n: N): string {
  const rename = (x: N): N => {
    if (x.type === 'FunctionNode') {
      const name = fnName(x)
      return call(name === 'log' ? 'ln' : name, x.args!.map(rename))
    }
    if (x.type === 'OperatorNode') return op(x.op!, fnName(x), x.args!.map(rename))
    return x
  }
  return asMath(rename(n)).toString({ parenthesis: 'auto' })
}

const SUPERSCRIPT: Record<string, string> = {
  '0': '⁰', '1': '¹', '2': '²', '3': '³', '4': '⁴', '5': '⁵', '6': '⁶', '7': '⁷', '8': '⁸', '9': '⁹', '-': '⁻',
}

type Printed = { s: string; prec: number; kind: 'num' | 'sym' | 'call' | 'group' | 'other' }

const SUM = 1
const PRODUCT = 2
const UNARY = 3
const POWER = 4
const ATOM = 5

function numberText(v: number): string {
  return String(Number(v.toPrecision(12)))
}

function isNegConst(n: N): boolean {
  return n.type === 'ConstantNode' && typeof n.value === 'number' && n.value < 0
}

function negate(n: N): N | null {
  if (n.type === 'OperatorNode' && fnName(n) === 'unaryMinus') return n.args![0]!
  if (isNegConst(n)) return constant(-(n.value as number))
  if (n.type === 'OperatorNode' && (fnName(n) === 'multiply' || fnName(n) === 'divide')) {
    const first = negate(n.args![0]!)
    if (first) return op(n.op!, fnName(n), [first, ...n.args!.slice(1)])
  }
  return null
}

function wrap(p: Printed, min: number): string {
  return p.prec < min ? `(${p.s})` : p.s
}

function factors(n: N): N[] {
  if (n.type === 'OperatorNode' && fnName(n) === 'multiply') return n.args!.flatMap(factors)
  return [n]
}

/** `x` before `y`: letters and their powers in a product go in alphabetical order. */
function letterOrder(a: N, b: N): number {
  const name = (f: N) => (f.type === 'SymbolNode' ? f.name! : f.args![0]!.name!)
  const x = name(a)
  const y = name(b)
  return x < y ? -1 : x > y ? 1 : 0
}

/** `2x sin(x) + x² cos(x)`: typeset, but still text the engine can read back. */
function pretty(n: N): Printed {
  switch (n.type) {
    case 'ConstantNode': {
      const v = n.value as number
      return { s: numberText(v), prec: v < 0 ? UNARY : ATOM, kind: 'num' }
    }
    case 'SymbolNode':
      return { s: n.name === 'pi' ? 'π' : n.name === 'theta' ? 'θ' : n.name!, prec: ATOM, kind: 'sym' }
    case 'ParenthesisNode':
      return pretty(n.content!)
    case 'FunctionNode': {
      const name = fnName(n)
      const args = n.args!.map(pretty)
      if (name === 'sqrt' || name === 'cbrt') {
        const root = name === 'sqrt' ? '√' : '∛'
        const a = args[0]!
        return { s: a.prec === ATOM && a.kind !== 'call' ? `${root}${a.s}` : `${root}(${a.s})`, prec: ATOM, kind: 'call' }
      }
      if (name === 'abs') return { s: `|${args[0]!.s}|`, prec: ATOM, kind: 'call' }
      if (name === 'exp') {
        const a = args[0]!
        return { s: a.prec === ATOM && a.kind !== 'call' ? `e^${a.s}` : `e^(${a.s})`, prec: POWER, kind: 'call' }
      }
      const shown = name === 'log' ? 'ln' : name === 'log10' ? 'log' : name
      return { s: `${shown}(${args.map((a) => a.s).join(', ')})`, prec: ATOM, kind: 'call' }
    }
    case 'OperatorNode':
      break
    default:
      return { s: asMath(n).toString(), prec: SUM, kind: 'other' }
  }
  const fn = fnName(n)
  const args = n.args!
  if (fn === 'unaryMinus') {
    const a = pretty(args[0]!)
    return { s: `-${wrap(a, PRODUCT)}`, prec: UNARY, kind: 'other' }
  }
  if (fn === 'add' || fn === 'subtract') {
    const terms: Array<{ neg: boolean; n: N }> = []
    const collect = (x: N, neg: boolean) => {
      if (x.type === 'OperatorNode' && fnName(x) === 'add') x.args!.forEach((a) => collect(a, neg))
      else if (x.type === 'OperatorNode' && fnName(x) === 'subtract') {
        collect(x.args![0]!, neg)
        collect(x.args![1]!, !neg)
      } else if (x.type === 'ParenthesisNode' && x.content) collect(x.content, neg)
      else {
        const flipped = negate(x)
        if (flipped) terms.push({ neg: !neg, n: flipped })
        else terms.push({ neg, n: x })
      }
    }
    collect(n, false)
    const s = terms
      .map((t, i) => {
        const body = wrap(pretty(t.n), PRODUCT)
        if (i === 0) return t.neg ? `-${body}` : body
        return t.neg ? ` - ${body}` : ` + ${body}`
      })
      .join('')
    return { s, prec: SUM, kind: 'other' }
  }
  if (fn === 'multiply') {
    const fs = factors(n)
    let coeff = 1
    const rest: N[] = []
    for (let f of fs) {
      while (f.type === 'OperatorNode' && fnName(f) === 'unaryMinus') {
        coeff = -coeff
        f = f.args![0]!
      }
      if (f.type === 'ConstantNode' && typeof f.value === 'number') coeff *= f.value
      else rest.push(f)
    }
    // π, then letters and their powers, then the rest, with constant calls like ln(2) last
    const rank = (f: N) => {
      if (f.type === 'SymbolNode') return f.name === 'pi' ? 0 : 1
      if (f.type === 'OperatorNode' && fnName(f) === 'pow' && f.args![0]!.type === 'SymbolNode' && f.args![0]!.name !== 'e') return 1
      if (f.type === 'FunctionNode' && !f.args!.some((a) => hasSymbol(a))) return 3
      return 2
    }
    const parts = rest
      .map((f, i) => ({ f, i }))
      .sort((a, b) => rank(a.f) - rank(b.f) || (rank(a.f) === 1 ? letterOrder(a.f, b.f) : 0) || a.i - b.i)
      .map(({ f }) => pretty(f))
    let s = ''
    let prevKind: Printed['kind'] | null = null
    const abs = Math.abs(coeff)
    if (abs !== 1 || !parts.length) {
      s = numberText(abs)
      prevKind = 'num'
    }
    for (const p of parts) {
      const text = p.prec < POWER && p.kind !== 'call' ? `(${p.s})` : p.s
      const kind = text.startsWith('(') ? 'group' : p.kind
      // `2x`, `2(x + 1)`, `2√x` and `πx` sit tight; everything else is spaced
      const tight =
        (prevKind === 'num' && (kind === 'sym' || kind === 'group' || /^[√∛]/.test(text))) ||
        (prevKind === 'sym' && /^[a-zπα-ωΑ-Ω]$/i.test(text))
      // `x (x + 1)` would read back as a call to x
      const sep = prevKind === 'sym' && kind === 'group' ? '·' : ' '
      s += prevKind == null || tight ? text : `${sep}${text}`
      prevKind = kind
    }
    return { s: coeff < 0 ? `-${s}` : s, prec: coeff < 0 ? UNARY : PRODUCT, kind: coeff < 0 ? 'other' : prevKind === 'num' ? 'num' : 'other' }
  }
  if (fn === 'divide') {
    let top = args[0]!
    const bottoms = [args[1]!]
    while (top.type === 'OperatorNode' && fnName(top) === 'divide') {
      bottoms.unshift(top.args![1]!)
      top = top.args![0]!
    }
    const flipped = negate(top)
    const numer = pretty(flipped ?? top)
    const den = pretty(bottoms.length > 1 ? op('*', 'multiply', bottoms) : bottoms[0]!)
    const s = `${wrap(numer, PRODUCT)}/${den.prec <= PRODUCT || den.kind === 'num' && den.s.startsWith('-') ? `(${den.s})` : den.s}`
    return flipped ? { s: `-${s}`, prec: UNARY, kind: 'other' } : { s, prec: PRODUCT, kind: 'other' }
  }
  if (fn === 'pow') {
    const base = pretty(args[0]!)
    const ex = args[1]!
    const baseText = base.prec <= POWER || base.kind === 'num' && base.s.startsWith('-') ? `(${base.s})` : base.s
    const intEx =
      ex.type === 'ConstantNode' && Number.isInteger(ex.value)
        ? (ex.value as number)
        : ex.type === 'OperatorNode' && fnName(ex) === 'unaryMinus' && ex.args![0]!.type === 'ConstantNode' && Number.isInteger(ex.args![0]!.value)
          ? -(ex.args![0]!.value as number)
          : null
    const kind = base.kind === 'sym' && base.s !== 'e' ? 'sym' : 'call'
    if (intEx != null && Math.abs(intEx) < 1000) {
      const sup = String(intEx).split('').map((c) => SUPERSCRIPT[c]).join('')
      return { s: `${baseText}${sup}`, prec: POWER, kind }
    }
    const e = pretty(ex)
    return { s: `${baseText}^${e.prec === ATOM && e.kind !== 'call' ? e.s : `(${e.s})`}`, prec: POWER, kind }
  }
  return { s: asMath(n).toString(), prec: SUM, kind: 'other' }
}

// ---- one denominator: `((γ²D - γLv)/√(γ² + v²)² - D)/(m √(γ² + v²))` is `-v·(Dv + Lγ)/(m (γ² + v²)^(3/2))`

/** A coefficient times atoms to powers; an atom is a letter, a call, or a sum that can't be multiplied out. */
type Term = { c: number; p: Map<string, number> }
type Atom = { node: N; sum?: Term[] }

const MAX_TERMS = 40
const MAX_EXPAND = 6

const isWhole = (x: number) => Math.abs(x - Math.round(x)) < 1e-9

class TooBig extends Error {}

function termKey(p: Map<string, number>): string {
  return [...p]
    .filter(([, e]) => !isWhole(e) || Math.round(e) !== 0)
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([k, e]) => `${k}^${Number(e.toFixed(9))}`)
    .join('*')
}

function sumKey(terms: Term[]): string {
  return terms.map((t) => `${Number(t.c.toPrecision(12))}:${termKey(t.p)}`).sort().join('+')
}

function combine(terms: Term[]): Term[] {
  const out = new Map<string, Term>()
  const scale = Math.max(0, ...terms.map((t) => Math.abs(t.c)))
  for (const t of terms) {
    const p = new Map([...t.p].filter(([, e]) => !isWhole(e) || Math.round(e) !== 0))
    const key = termKey(p)
    const prev = out.get(key)
    if (prev) prev.c += t.c
    else out.set(key, { c: t.c, p })
  }
  const kept = [...out.values()].filter((t) => Math.abs(t.c) > 1e-12 * scale)
  if (kept.length > MAX_TERMS) throw new TooBig()
  return kept
}

function mulTerm(a: Term, b: Term): Term {
  const p = new Map(a.p)
  for (const [k, e] of b.p) p.set(k, (p.get(k) ?? 0) + e)
  return { c: a.c * b.c, p }
}

function mulSum(a: Term[], b: Term[]): Term[] {
  return combine(a.flatMap((x) => b.map((y) => mulTerm(x, y))))
}

function wholePower(a: Term[], k: number): Term[] {
  let out: Term[] = [{ c: 1, p: new Map() }]
  for (let i = 0; i < k; i++) out = mulSum(out, a)
  return out
}

/** `p/q` with a small `q`, or null. */
function toFraction(x: number): [number, number] | null {
  if (!Number.isFinite(x)) return null
  for (let q = 1; q <= 100000; q++) {
    const p = Math.round(x * q)
    if (Math.abs(p / q - x) <= 1e-10 * Math.max(1, Math.abs(x))) return [p, q]
  }
  return null
}

function constantOf(n: N): number | null {
  if (n.type === 'ConstantNode' && typeof n.value === 'number') return n.value
  if (n.type === 'ParenthesisNode' && n.content) return constantOf(n.content)
  if (n.type === 'OperatorNode' && fnName(n) === 'unaryMinus') {
    const a = constantOf(n.args![0]!)
    return a == null ? null : -a
  }
  if (n.type === 'OperatorNode' && fnName(n) === 'divide') {
    const a = constantOf(n.args![0]!)
    const b = constantOf(n.args![1]!)
    return a == null || b == null || b === 0 ? null : a / b
  }
  return null
}

class Terms {
  atoms = new Map<string, Atom>()

  atom(key: string, node: N, e = 1, sum?: Term[]): Term[] {
    if (!this.atoms.has(key)) this.atoms.set(key, { node, sum })
    return [{ c: 1, p: new Map([[key, e]]) }]
  }

  of(n: N): Term[] {
    switch (n.type) {
      case 'ConstantNode':
        if (typeof n.value !== 'number' || !Number.isFinite(n.value)) throw new TooBig()
        return n.value === 0 ? [] : [{ c: n.value, p: new Map() }]
      case 'SymbolNode':
        return this.atom(n.name!, n)
      case 'ParenthesisNode':
        return this.of(n.content!)
      case 'FunctionNode': {
        const name = fnName(n)
        if (name === 'sqrt') return this.power(this.of(n.args![0]!), 0.5)
        return this.atom(asMath(n).toString(), n)
      }
      case 'OperatorNode':
        break
      default:
        throw new TooBig()
    }
    const args = n.args!
    switch (fnName(n)) {
      case 'add':
        return combine(args.flatMap((a) => this.of(a)))
      case 'subtract':
        return combine([...this.of(args[0]!), ...this.of(args[1]!).map((t) => ({ ...t, c: -t.c }))])
      case 'unaryMinus':
        return this.of(args[0]!).map((t) => ({ ...t, c: -t.c }))
      case 'unaryPlus':
        return this.of(args[0]!)
      case 'multiply':
        return args.map((a) => this.of(a)).reduce(mulSum)
      case 'divide':
        return mulSum(this.of(args[0]!), this.power(this.of(args[1]!), -1))
      case 'pow': {
        const k = constantOf(args[1]!)
        if (k == null) return this.atom(asMath(n).toString(), n)
        return this.power(this.of(args[0]!), k)
      }
      default:
        throw new TooBig()
    }
  }

  power(base: Term[], k: number): Term[] {
    if (k === 0) return [{ c: 1, p: new Map() }]
    if (!base.length) {
      if (k < 0) throw new TooBig()
      return []
    }
    if (isWhole(k) && k > 0 && k <= MAX_EXPAND) return wholePower(base, Math.round(k))
    if (base.length === 1) {
      const [t] = base as [Term]
      if (isWhole(k)) return [{ c: t.c ** k, p: new Map([...t.p].map(([a, e]) => [a, e * k])) }]
      // (x²)^(1/2) is |x|, not x
      if (t.c > 0 && [...t.p].every(([, e]) => !isWhole(e) || Math.round(e) % 2 !== 0 || isWhole(e * k))) {
        let out: Term[] = [{ c: t.c ** k, p: new Map() }]
        for (const [a, e] of t.p) {
          const even = isWhole(e) && Math.round(e) % 2 === 0
          const piece = even
            ? this.atom(`abs(${a})`, call('abs', [this.atoms.get(a)!.node]), e * k)
            : [{ c: 1, p: new Map([[a, e * k]]) }]
          out = mulSum(out, piece)
        }
        return out
      }
    }
    const key = `(${sumKey(base)})`
    return this.atom(key, this.node(base), k, base)
  }

  /** Atoms to powers, as a product; roots print as √. */
  factorNodes(p: Map<string, number>): N[] | null {
    const out: N[] = []
    for (const [k, e] of [...p].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))) {
      if (isWhole(e) && Math.round(e) === 0) continue
      const inner = k.match(/^abs\((.*)\)$/)?.[1]
      // |v|² prints as v²
      const base = inner && this.atoms.has(inner) && isWhole(e) && Math.round(e) % 2 === 0 ? this.atoms.get(inner)!.node : this.atoms.get(k)!.node
      if (isWhole(e)) out.push(Math.round(e) === 1 ? base : op('^', 'pow', [base, constant(Math.round(e))]))
      else if (Math.abs(e - 0.5) < 1e-9) out.push(call('sqrt', [base]))
      else {
        const f = toFraction(e)
        if (!f || f[1] > 12) return null
        out.push(op('^', 'pow', [base, op('/', 'divide', [constant(f[0]), constant(f[1])])]))
      }
    }
    return out
  }

  node(terms: Term[]): N {
    const sorted = [...terms].sort((a, b) => degree(b) - degree(a) || (termKey(a.p) < termKey(b.p) ? -1 : 1))
    const parts = sorted.map((t) => {
      const fs = this.factorNodes(t.p) ?? [constant(Number.NaN)]
      const c = Math.abs(t.c)
      const body = fs.length ? (c === 1 ? product(fs) : product([constant(c), ...fs])) : constant(c)
      return { neg: t.c < 0, body }
    })
    let out: N = parts[0]!.neg ? op('-', 'unaryMinus', [parts[0]!.body]) : parts[0]!.body
    for (const part of parts.slice(1)) out = op(part.neg ? '-' : '+', part.neg ? 'subtract' : 'add', [out, part.body])
    return out
  }
}

function degree(t: Term): number {
  let d = 0
  for (const [, e] of t.p) d += e
  return d
}

function product(fs: N[]): N {
  return fs.length === 1 ? fs[0]! : op('*', 'multiply', fs)
}

function gcd(a: number, b: number): number {
  return b ? gcd(b, a % b) : Math.abs(a)
}

/** The expression over a single denominator with the numerator multiplied out and its common factors pulled out front. */
function together(n: N): N | null {
  const terms = new Terms()
  let sum: Term[]
  try {
    sum = terms.of(n)
    if (!sum.length) return constant(0)
    // v² is |v|², so it can cancel with a |v| from a root
    sum = combine(sum.map((t) => {
      const p = new Map(t.p)
      for (const [k, e] of t.p) {
        const abs = `abs(${k})`
        if (terms.atoms.has(abs) && isWhole(e) && Math.round(e) % 2 === 0) {
          p.delete(k)
          p.set(abs, (p.get(abs) ?? 0) + e)
        }
      }
      return { c: t.c, p }
    }))
    const den = new Map<string, number>()
    for (const t of sum) for (const [k, e] of t.p) if (e < 0) den.set(k, Math.min(den.get(k) ?? 0, e))
    let top: Term[] = []
    for (const t of sum) {
      let expanded: Term[] = [{ c: t.c, p: new Map() }]
      const p = new Map(t.p)
      for (const [k, e] of den) p.set(k, (p.get(k) ?? 0) - e)
      for (const [k, e] of p) {
        // a sum back to a whole power multiplies out, so its terms can cancel
        const inner = terms.atoms.get(k)?.sum
        const whole = Math.floor(e + 1e-9)
        const rest = e - whole
        if (inner && whole >= 1 && whole <= MAX_EXPAND) {
          expanded = mulSum(expanded, wholePower(inner, whole))
          if (Math.abs(rest) > 1e-9) expanded = expanded.map((x) => mulTerm(x, { c: 1, p: new Map([[k, rest]]) }))
        } else expanded = expanded.map((x) => mulTerm(x, { c: 1, p: new Map([[k, e]]) }))
      }
      top.push(...expanded)
    }
    top = combine(top)
    if (!top.length) return constant(0)

    // what every numerator term shares comes out front, and cancels against the denominator
    const outer = new Map<string, number>()
    for (const [k] of top[0]!.p) {
      const least = Math.min(...top.map((t) => t.p.get(k) ?? 0))
      if (least > 1e-9) outer.set(k, least)
    }
    top = top.map((t) => {
      const p = new Map(t.p)
      for (const [k, e] of outer) p.set(k, p.get(k)! - e)
      return { c: t.c, p }
    })
    const bottom = new Map([...den].map(([k, e]) => [k, -e]))
    for (const [k, e] of outer) {
      const m = Math.min(e, bottom.get(k) ?? 0)
      if (m > 0) {
        outer.set(k, e - m)
        bottom.set(k, bottom.get(k)! - m)
      }
    }

    // whole-number coefficients, with the sign of the leading term out front
    const fractions = top.map((t) => toFraction(t.c))
    if (fractions.some((f) => !f)) return null
    const g = fractions.reduce((a, f) => gcd(a, f![0]), 0)
    const l = fractions.reduce((a, f) => (a * f![1]) / gcd(a, f![1]), 1)
    const ordered = [...top].sort((a, b) => degree(b) - degree(a) || (termKey(a.p) < termKey(b.p) ? -1 : 1))
    const sign = ordered[0]!.c < 0 ? -1 : 1
    const scale = (sign * g) / l
    top = top.map((t) => ({ c: Math.round(t.c / scale), p: t.p }))
    let [pOut, qOut] = toFraction(Math.abs(scale)) ?? [0, 0]
    if (!qOut) return null

    // a numerator that is itself one of the denominator's sums cancels with it
    if (top.length > 1) {
      const key = `(${sumKey(top)})`
      if (bottom.has(key) && (bottom.get(key) ?? 0) >= 1) {
        bottom.set(key, bottom.get(key)! - 1)
        top = [{ c: 1, p: new Map() }]
      }
    }
    if (top.length === 1) {
      const [t] = top as [Term]
      for (const [k, e] of t.p) outer.set(k, (outer.get(k) ?? 0) + e)
      pOut *= Math.abs(t.c)
      top = []
    }

    const upper = terms.factorNodes(outer)
    const lower = terms.factorNodes(bottom)
    if (!upper || !lower) return null
    const numFactors: N[] = [...(pOut !== 1 || (!upper.length && !top.length) ? [constant(pOut)] : []), ...upper, ...(top.length ? [terms.node(top)] : [])]
    const denFactors: N[] = [...(qOut !== 1 ? [constant(qOut)] : []), ...lower]
    let out = product(numFactors)
    if (denFactors.length) out = op('/', 'divide', [out, product(denFactors)])
    return sign < 0 ? op('-', 'unaryMinus', [out]) : out
  } catch {
    return null
  }
}

const TIDY_SAMPLES = [
  [0.83, 1.37, 0.61, 2.21, 0.47, 1.93, 1.28, 0.72],
  [1.61, 0.74, 2.58, 1.12, 0.45, 0.91, 2.67, 1.33],
  [-1.19, 2.31, -0.57, 1.46, -2.03, 0.66, -0.88, 1.74],
]

function symbolNames(n: N, out = new Set<string>()): Set<string> {
  if (n.type === 'SymbolNode' && n.name !== 'pi' && n.name !== 'e') out.add(n.name!)
  for (const a of n.args ?? []) symbolNames(a, out)
  if (n.content) symbolNames(n.content, out)
  return out
}

/** Same value wherever both are real; at least one sample has to be. */
function sameValue(a: N, b: N): boolean {
  const names = [...new Set([...symbolNames(a), ...symbolNames(b)])]
  let compared = 0
  try {
    const fa = asMath(a).compile()
    const fb = asMath(b).compile()
    for (const sample of TIDY_SAMPLES) {
      const scope = Object.fromEntries(names.map((n, i) => [n, sample[i % sample.length]!]))
      const x = fa.evaluate(scope)
      const y = fb.evaluate(scope)
      if (typeof x !== 'number' || typeof y !== 'number' || !Number.isFinite(x) || !Number.isFinite(y)) continue
      if (Math.abs(x - y) > 1e-9 * Math.max(1, Math.abs(x))) return false
      compared++
    }
  } catch {
    return false
  }
  return compared > 0
}

/** The one-denominator form when it reads shorter than what simplify gave. */
function tidy(n: N): N {
  const t = together(n)
  if (!t || !sameValue(n, t)) return n
  return pretty(t).s.length < pretty(n).s.length ? t : n
}

type Derived = { tree: N; user: N }

/** d/dv of a radian tree; `user` is the same derivative written back in the user's angle mode. */
function differentiate(tree: N, v: string, deg: boolean): Derived | null {
  let d: N
  try {
    d = asN(math.derivative(asMath(tree), v, { simplify: false }))
  } catch {
    return null
  }
  const back = toUserAngles(d, deg)
  if (!back) return null
  const withDeg = substitute(back, { [DEG]: op('/', 'divide', [symbol('pi'), constant(180)]) })
  let user: N
  try {
    user = tidy(simplifyKeepingConstants(withDeg, v))
  } catch {
    return null
  }
  return { tree: d, user }
}

// ---- evaluation

function realFn(text: string, ctx: ScientificContext, v: string): RealFn | null {
  const compiled = compileScientific(text, ctx, v)
  if (!compiled) return null
  return (x) => {
    const r = compiled(x)
    return r && r.kind === 'number' ? r.n : Number.NaN
  }
}

/** The body as one expression with user functions inlined, which evaluates far faster than nested calls. */
function bodyFn(body: string, ctx: ScientificContext, v: string): RealFn | null {
  const names = Object.keys(ctx.functions ?? {})
  if (names.some((n) => new RegExp(`(?<![A-Za-z0-9_])${n}\\s*\\(`).test(body))) {
    const tree = toTree(body, { ...ctx, bound: v, depth: 0 })
    const user = tree && toUserAngles(tree, ctx.angleMode !== 'rad')
    if (user) {
      const flat = substitute(user, { [DEG]: op('/', 'divide', [symbol('pi'), constant(180)]) })
      const fast = realFn(engineText(flat), ctx, v)
      if (fast) return fast
    }
  }
  return realFn(body, ctx, v)
}

const INFINITY_RE = /^([+-]?)\s*(?:∞|inf|infty|infinity)$/i

function boundValue(text: string, ctx: ScientificContext): number | null {
  const inf = text.trim().match(INFINITY_RE)
  if (inf) return inf[1] === '-' ? -Infinity : Infinity
  const r = evalScientific(text, ctx)
  return r && r.kind === 'number' && Number.isFinite(r.n) ? r.n : null
}

function jobAst(n: N, v: string | null): Ast | null {
  switch (n.type) {
    case 'ConstantNode':
      return typeof n.value === 'number' ? { t: 'num', v: String(n.value) } : null
    case 'SymbolNode':
      if (n.name === v) return { t: 'var' }
      if (n.name === 'pi' || n.name === 'e') return { t: 'const', name: n.name }
      if (n.name === DEG) return { t: 'op', op: '/', a: [{ t: 'const', name: 'pi' }, { t: 'num', v: '180' }] }
      return null
    case 'ParenthesisNode':
      return n.content ? jobAst(n.content, v) : null
    case 'OperatorNode': {
      const args = n.args!.map((a) => jobAst(a, v))
      if (args.some((a) => !a)) return null
      const fn = fnName(n)
      const o = fn === 'unaryMinus' ? 'neg' : n.op
      if (!o || !['+', '-', '*', '/', '^', 'neg'].includes(o)) return null
      return { t: 'op', op: o as '+', a: args as Ast[] }
    }
    case 'FunctionNode': {
      const args = n.args!.map((a) => jobAst(a, v))
      if (args.some((a) => !a)) return null
      return { t: 'fn', name: fnName(n), a: args as Ast[] }
    }
    default:
      return null
  }
}

function boundAst(text: string, ctx: TreeCtx): Ast | 'inf' | '-inf' | null {
  const inf = text.trim().match(INFINITY_RE)
  if (inf) return inf[1] === '-' ? '-inf' : 'inf'
  const tree = toTree(text, { ...ctx, bound: null })
  return tree && jobAst(tree, null)
}

const closedForms = new Map<string, string | null>()

export function jobKey(job: ClosedFormJob): string {
  return JSON.stringify({ ...job, approx: undefined, err: undefined })
}

/** undefined: not asked yet; null: the worker looked and found nothing. */
export function knownClosedForm(key: string): string | null | undefined {
  return closedForms.get(key)
}

/** The worker's verified answer for a job; null means it looked and found nothing. */
export function rememberClosedForm(job: ClosedFormJob, exact: string | null): void {
  closedForms.set(jobKey(job), exact)
}

type JobDraft = ClosedFormJob extends infer J ? (J extends ClosedFormJob ? Omit<J, 'approx' | 'err' | 'rad'> : never) : never

function withJob(value: number, err: number, draft: JobDraft | null, ctx: ScientificContext): CalculusResult {
  const result: CalculusResult = { value: num(value) }
  // integers already are their own closed form
  if (!draft || !Number.isFinite(value) || Number.isInteger(value) || Math.abs(value) > 1e12) return result
  const job = { ...draft, approx: value, err, rad: ctx.angleMode === 'rad' } as ClosedFormJob
  const key = jobKey(job)
  if (closedForms.has(key)) {
    const exact = closedForms.get(key)
    if (exact) result.exact = exact
    return result
  }
  result.job = job
  return result
}

function freeLetters(body: string, ctx: ScientificContext): string[] {
  const found = new Set<string>()
  for (const m of body.matchAll(/(?<![A-Za-z0-9_])([A-Za-z])(?![A-Za-z0-9_(])/g)) {
    const c = m[1]!
    // a bare function name isn't a variable: after f(x) = x², ∫0..3 f isn't ∫0..3 f df
    if (c === 'e' || c === 'i' || (ctx.variables && c in ctx.variables) || (ctx.functions && c in ctx.functions)) continue
    found.add(c)
  }
  return [...found]
}

function pickVariable(body: string, ctx: ScientificContext, given?: string): string | null {
  if (given) return given
  const free = freeLetters(body, ctx)
  if (free.includes('x') || !free.length) return 'x'
  return free.length === 1 ? free[0]! : null
}

/** A derivative must agree with finite differences wherever both exist, or it isn't shown. */
function agreesNumerically(f: RealFn, df: RealFn, points: number[]): boolean {
  for (const p of points) {
    const want = derivativeAt(f, p)
    const got = df(p)
    if (!want || !Number.isFinite(got) || !Number.isFinite(want.value)) continue
    if (want.err > 1e-6 * Math.max(1, Math.abs(want.value))) continue
    if (Math.abs(got - want.value) > 1e-6 * Math.max(1, Math.abs(got)) + 10 * want.err) return false
  }
  return true
}

const CHECK_POINTS = [0.7, 1.3, 2.9, -0.6, 4.2, 11.5, 0.2, -3.1]

const LETTER = String.raw`A-Za-z0-9_α-ωΑ-Ω`
const SYMBOL_RE = new RegExp(String.raw`(?<![${LETTER}])(${VAR})(?![${LETTER}(])`, 'g')
const WORD_RE = new RegExp(String.raw`(?<![${LETTER}\\.])[A-Za-zα-ωΑ-Ω]{2,}(?![${LETTER}.]|\s*\()`, 'g')
const WHOLE_WORDS = new Set(`${SCIENTIFIC_NAMES}|e|i|theta|mod|deg|rad|and|or|not|xor`.toLowerCase().split('|'))

/** `Lv` in a derivative is L times v: an unknown word of letters is a product of one-letter constants. */
function splitSymbols(body: string, ctx: ScientificContext): string {
  return body.replace(WORD_RE, (word) => {
    if (WHOLE_WORDS.has(word.toLowerCase()) || (ctx.variables && word in ctx.variables) || (ctx.functions && word in ctx.functions)) return word
    return [...word].join(' ')
  })
}

/** Letters, Greek letters included, that aren't a known variable, function or constant, in order of appearance. */
function freeSymbols(body: string, ctx: ScientificContext): string[] {
  const found = new Set<string>()
  for (const m of body.matchAll(SYMBOL_RE)) {
    const c = m[1]!
    if (c === 'e' || c === 'i' || (ctx.variables && c in ctx.variables) || (ctx.functions && c in ctx.functions)) continue
    found.add(c)
  }
  return [...found]
}

/** `at x = 1, y = 2`, `at (x, y) = (1, 2)`, or a bare `at 2` when there is one variable. Null when it doesn't read. */
function readPoint(text: string, vars: string[]): Record<string, string> | null {
  const t = text.trim()
  const named = new RegExp(String.raw`^(${VAR})\s*=(?!=)\s*(.+)$`, 's')
  const tuple = t.match(/^\((.+)\)\s*=\s*\((.+)\)$/s)
  if (tuple) {
    const names = splitArgs(tuple[1]!)
    const values = splitArgs(tuple[2]!)
    if (names.length !== values.length || !names.every((n) => new RegExp(`^${VAR}$`).test(n)) || !values.every(Boolean)) return null
    return Object.fromEntries(names.map((n, i) => [n, values[i]!]))
  }
  const parts = splitArgs(t).flatMap((p) => p.split(/\s+and\s+/i)).map((p) => p.trim())
  const pairs = parts.map((p) => p.match(named))
  if (pairs.every(Boolean)) {
    const out: Record<string, string> = {}
    for (const m of pairs) {
      if (m![1]! in out) return null
      out[m![1]!] = m![2]!.trim()
    }
    return out
  }
  if (new Set(vars).size === 1 && parts.length === 1 && !pairs[0]) return { [vars[0]!]: t }
  return null
}

function runDerivative(intent: Extract<Intent, { op: 'derivative' }>, ctx: ScientificContext): CalculusResult | null {
  // d/dt of letters that aren't t (`x sin θ`) is a time derivative: they move with time
  // ∂/∂t holds the other letters still, by definition
  if (!intent.partial && intent.at == null && intent.vars.length <= 2 && intent.vars.every((v) => v === 't')) {
    const timed = runTimeDerivative(intent.body, intent.vars.length, ctx)
    if (timed) return timed
  }
  const body = splitSymbols(intent.body, ctx)
  const free = freeSymbols(body, ctx)
  const first = intent.vars[0] || (free.includes('x') || !free.length ? 'x' : free.length === 1 ? free[0]! : null)
  if (!first) return null
  const vars = intent.vars.map((v) => v || first)
  const point = intent.at == null ? {} : readPoint(intent.at, vars)
  if (!point) return null
  const symbols = [...new Set([...vars, ...free])]
  if (symbols.length === 1) return runOrdinary(body, first, vars.length, point[first], ctx)
  return runPartial(body, vars, symbols, point, ctx)
}

/** Sample values for the letters held constant while a partial derivative is checked. */
const SAMPLE_SETS = [
  [0.83, 1.37, -0.61, 2.21, 0.47, 1.93, -1.28],
  [1.61, -0.74, 0.58, 1.12, -1.45, 0.91, 2.67],
]

/** A derivative in one of several letters, the others held constant; symbolic unless `at` gives every letter a value. */
function runPartial(body: string, vars: string[], symbols: string[], point: Record<string, string>, ctx: ScientificContext): CalculusResult | null {
  const deg = ctx.angleMode !== 'rad'
  const tree = toTree(body, { ...ctx, bound: null, keep: symbols, depth: 0 })
  if (!tree) return null
  let current = tree
  let user: N | null = null
  const texts = [body]
  for (const v of vars) {
    const d = differentiate(current, v, deg)
    if (!d) return null
    const dText = engineText(d.user)
    for (const sample of SAMPLE_SETS) {
      const held = Object.fromEntries(symbols.filter((s) => s !== v).map((s, i) => [s, sample[i % sample.length]!]))
      const sctx = { ...ctx, variables: { ...ctx.variables, ...held } }
      const f = realFn(texts.at(-1)!, sctx, v)
      const df = realFn(dText, sctx, v)
      if (!f || !df || !agreesNumerically(f, df, CHECK_POINTS)) return null
    }
    current = d.tree
    user = d.user
    texts.push(dText)
  }
  if (!user) return null

  const names = Object.keys(point)
  if (!names.length) return { value: textVal(pretty(user).s) }
  if (names.some((n) => !symbols.includes(n))) return null
  const values: Record<string, number> = {}
  for (const n of names) {
    const x = boundValue(point[n]!, ctx)
    if (x == null) return null
    values[n] = x
  }
  const vctx = { ...ctx, variables: { ...ctx.variables, ...values } }
  if (symbols.every((s) => s in values)) {
    // the function and each lower derivative have to exist there too
    let r: Value | null = null
    for (const t of texts) {
      r = evalScientific(t, vctx)
      if (!r || r.kind !== 'number' || !Number.isFinite(r.n)) return null
    }
    return { value: r }
  }
  const at = substitute(user, Object.fromEntries(names.map((n) => [n, constant(values[n]!)])))
  let simplified: N
  try {
    // simplify would make (v²)^(3/2) into v³
    const whole = together(at)
    simplified = whole && sameValue(at, whole) ? whole : simplifyKeepingConstants(at, null)
  } catch {
    return null
  }
  return { value: textVal(pretty(simplified).s) }
}

function runOrdinary(body: string, v: string, order: number, atText: string | undefined, ctx: ScientificContext): CalculusResult | null {
  const tctx: TreeCtx = { ...ctx, bound: v, depth: 0 }
  const tree = toTree(body, tctx)
  const f = realFn(body, ctx, v)
  if (!f) return null
  const at = atText != null ? boundValue(atText, ctx) : null
  if (atText != null && (at == null || !Number.isFinite(at))) return null
  // no derivative where the function itself is undefined (ln at -1)
  if (at != null && !Number.isFinite(f(at))) return null
  const checkAt = at != null ? [at] : CHECK_POINTS

  let steps: Derived | null = null
  if (tree) {
    let current = tree
    let prevFn = f
    for (let k = 0; k < order; k++) {
      // each lower derivative has to exist there too: abs'' is 0 either side of 0 but abs' has no value at 0
      if (at != null && !Number.isFinite(prevFn(at))) return null
      const d = differentiate(current, v, ctx.angleMode !== 'rad')
      if (!d) {
        steps = null
        break
      }
      const df = realFn(engineText(d.user), ctx, v)
      if (!df || !agreesNumerically(prevFn, df, checkAt)) return null
      steps = d
      current = d.tree
      prevFn = df
    }
    if (steps && at != null) {
      const value = prevFn(at)
      if (!Number.isFinite(value)) return null
      const atTree = toTree(atText!, { ...tctx, bound: null })
      const dAst = jobAst(steps.tree, v)
      const atAst = atTree && jobAst(atTree, null)
      const draft: JobDraft | null = dAst && atAst ? { kind: 'value', f: dAst, at: atAst } : null
      return withJob(value, 1e-13 * Math.abs(value), draft, ctx)
    }
    if (steps) return { value: textVal(pretty(steps.user).s) }
  }
  // no symbolic form (floor, mod, ...): a first derivative at a point can still be numeric
  if (at == null || order !== 1) return null
  const est = derivativeAt(f, at)
  if (!est || est.err > 1e-7 * Math.max(1, Math.abs(est.value))) return null
  const value = justified(est.value, est.err, Math.max(1, Math.abs(est.value)))
  return value == null ? null : { value: num(value) }
}

function runIntegral(intent: Extract<Intent, { op: 'integral' }>, ctx: ScientificContext): CalculusResult | null {
  const v = pickVariable(intent.body, ctx, intent.v)
  if (!v) return null
  const f = bodyFn(intent.body, ctx, v)
  const a = boundValue(intent.lower, ctx)
  const b = boundValue(intent.upper, ctx)
  if (!f || a == null || b == null) return null
  const est = integrate(f, a, b)
  if (!est) return null
  const tctx: TreeCtx = { ...ctx, bound: v, depth: 0 }
  const tree = toTree(intent.body, tctx)
  const fAst = tree && jobAst(tree, v)
  const lo = boundAst(intent.lower, tctx)
  const hi = boundAst(intent.upper, tctx)
  const draft: JobDraft | null = fAst && lo && hi ? { kind: 'integral', f: fAst, a: lo, b: hi } : null
  return withJob(est.value, est.err, draft, ctx)
}

function runLimit(intent: Extract<Intent, { op: 'limit' }>, ctx: ScientificContext): CalculusResult | null {
  const f = bodyFn(intent.body, ctx, intent.v)
  const to = boundValue(intent.to, ctx)
  if (!f || to == null) return null
  const est = limit(f, to, intent.side)
  if (!est) return null
  if (!Number.isFinite(est.value)) return { value: num(est.value) }
  const tctx: TreeCtx = { ...ctx, bound: intent.v, depth: 0 }
  const tree = toTree(intent.body, tctx)
  const fAst = tree && jobAst(tree, intent.v)
  const toAst = boundAst(intent.to, tctx)
  const draft: JobDraft | null = fAst && toAst ? { kind: 'limit', f: fAst, to: toAst, side: intent.side } : null
  return withJob(est.value, est.err, draft, ctx)
}

/** Null when `text` isn't calculus; a null value when it is but has no trustworthy answer. */
export function evaluateCalculus(text: string, ctx: ScientificContext = {}): CalculusResult | null {
  const intent = parseCalculus(text, ctx.functions, ctx.variables)
  if (!intent) return null
  try {
    const out =
      intent.op === 'derivative' ? runDerivative(intent, ctx) : intent.op === 'integral' ? runIntegral(intent, ctx) : runLimit(intent, ctx)
    return out ?? { value: null }
  } catch {
    return { value: null }
  }
}

// ---- time derivatives: d/dt (x sin θ) = ẋ sin θ + x cos θ θ̇

/** The physics convention: positions and angles move, other letters (L, h, k, m, g, R…) are constants. */
const VARYING_BY_DEFAULT = new Set(['x', 'y', 'z', 'r', 's', 'q', 'u', 'v', 'w', 'theta', 'φ', 'ψ'])
const GREEK_WORDS: Record<string, string> = { phi: 'φ', psi: 'ψ', omega: 'ω', alpha: 'α', beta: 'β', gamma: 'γ' }
const GREEK_SHOWN: Record<string, string> = { theta: 'θ' }
// stand-ins for ẋ, θ̈ and the rest while mathjs works: Greek capitals nobody types (not Σ Π Δ Ω Θ Φ, which mean things)
const STAND_INS = ['Ξ', 'Ψ', 'Γ', 'Λ', 'Υ', 'Ζ', 'Η', 'Ι', 'Κ', 'Μ', 'Ν']
const MARKS = ['', '̇', '̈', '⃛']
/** `thetadot`, `xddot`, `θdot`: a letter or Greek name, then d once per derivative, then ot. */
const DOTTED_WORD = /(?<![A-Za-z])(theta|phi|psi|omega|alpha|beta|gamma|[A-Za-zθφψωαβγ])(d{1,3})ot(?![A-Za-z])/g

/** A letter as the answer shows it: θ for theta. */
function shownLetter(name: string): string {
  return GREEK_SHOWN[name] ?? name
}

/**
 * d/dt or d²/dt² of an expression whose letters move with time. By the physics convention x, y, z, r, s, θ, φ, ψ
 * (and anything written dotted) vary and the rest are constants; `ctx.timeVarying` overrides a letter either way.
 * Each varying q contributes ∂f/∂q · q̇, and q̇'s own derivative is q̈. Worked in radians, as dotted physics is.
 * Null when there's nothing to move (only t, or no letters at all), so the plain derivative takes over.
 */
function runTimeDerivative(text: string, order: number, ctx: ScientificContext): CalculusResult | null {
  let body = ` ${text} `.replace(/\b(phi|psi|omega|alpha|beta|gamma)\b/g, (w) => GREEK_WORDS[w]!)
  const free = STAND_INS.filter((c) => !body.includes(c))
  // each dotted name, and each derivative of a moving letter, gets a stand-in
  const stands = new Map<string, { base: string; order: number }>()
  const standFor = (base: string, k: number): string | null => {
    for (const [c, d] of stands) if (d.base === base && d.order === k) return c
    const c = free.shift()
    if (!c) return null
    stands.set(c, { base, order: k })
    return c
  }
  let missing = false
  body = body.replace(DOTTED_WORD, (_, base: string, ds: string) => {
    const name = GREEK_WORDS[base] ?? (base === 'θ' ? 'theta' : base)
    const c = standFor(name, ds.length)
    if (!c) missing = true
    return ` ${c} `
  })
  if (missing) return null
  // `ktheta` is k times θ, not six letters
  const split = splitSymbols(body.replace(/theta/g, ' theta '), ctx)
  const letters = freeSymbols(split, ctx).filter((n) => !STAND_INS.includes(n) && n !== 't')
  const dottedBases = new Set([...stands.values()].map((d) => d.base))
  if (!letters.length && !stands.size) return null
  const varies = (name: string) =>
    dottedBases.has(name) || (ctx.timeVarying?.[shownLetter(name)] ?? VARYING_BY_DEFAULT.has(name))
  const moving = letters.filter(varies)
  const still = letters.filter((n) => !varies(n))
  const explicitT = /(?<![A-Za-z])t(?![A-Za-z(])/.test(split)
  // nothing moves and there's no t: d/dt is 0, but then the letters probably weren't meant as constants, so
  // say what was assumed rather than quietly answer 0
  const rctx = { ...ctx, angleMode: 'rad' as const }
  let tree = toTree(split, { ...rctx, bound: null, keep: [...letters, ...stands.keys(), 't'], depth: 0 })
  if (!tree) return null
  // a letter only ever written dotted (`1/2 m xdot^2`) moves too, and is listed so
  const onlyDotted = [...dottedBases].filter((b) => !moving.includes(b))
  const time: TimeLetters = {
    varying: [...moving, ...onlyDotted].map(shownLetter),
    constant: still.map(shownLetter),
    locked: [...dottedBases].map(shownLetter),
  }
  for (let k = 0; k < order; k++) {
    const terms: N[] = []
    const part = (q: string): N | null => {
      try {
        return asN(math.derivative(asMath(tree!), q, { simplify: false }))
      } catch {
        return null
      }
    }
    const symbolsNow = symbolNames(tree)
    if (explicitT && symbolsNow.has('t')) {
      const d = part('t')
      if (!d) return null
      terms.push(d)
    }
    for (const q of [...moving, ...stands.keys()]) {
      if (!symbolsNow.has(q)) continue
      const known = stands.get(q)
      const base = known ? known.base : q
      const next = standFor(base, (known?.order ?? 0) + 1)
      if (!next || (known?.order ?? 0) + 1 >= MARKS.length) return null
      const d = part(q)
      if (!d) return null
      terms.push(op('*', 'multiply', [d, symbol(next)]))
    }
    const sum = terms.length ? terms.reduce((a, b) => op('+', 'add', [a, b])) : constant(0)
    try {
      tree = tidy(simplifyKeepingConstants(sum, null))
    } catch {
      return null
    }
  }
  let shown = pretty(tree).s
  for (const [c, d] of stands) shown = shown.split(c).join(shownLetter(d.base) + MARKS[d.order]!)
  // one character where there is one (ẋ, not x and a combining dot); θ̇ has none and keeps its mark
  return { value: textVal(shown.normalize('NFC')), time }
}
