import type { MathNode } from 'mathjs'
import { math } from './math'
import { normalizeMathText } from './plainMath'
import { preprocessAscii, SCIENTIFIC_NAMES } from './scientific'
import { solveCall } from './solve'

/** `isolate x in a*x + b = c`, `isolate x: …`, `… isolate x`; a line with no `=` is set to 0. */
export interface IsolateCmd {
  variable: string
  lhs: string
  rhs: string
}

// a subscript is part of the name: R_eq, R_1
const VAR = '([A-Za-z][A-Za-z0-9]*(?:_[A-Za-z0-9]+)?|θ)'

/** `R_(eq)`, `R_("eq")` and `R_{eq}` are all the name `R_eq`. */
function subscripts(text: string): string {
  return text.replace(/([A-Za-z][A-Za-z0-9]*)_\s*[({]\s*"?([A-Za-z0-9]+)"?\s*[)}]/g, '$1_$2')
}
const LEAD = new RegExp(`^isolate\\s+${VAR}(?:\\s+in\\s+|\\s*[:,;]\\s*|\\s+)(.+)$`, 'is')
const TRAIL = new RegExp(`^(.+?)\\s*[,;]?\\s+isolate\\s+${VAR}$`, 'is')
/** `isolate x` alone, which reads the equation on the line before. */
export const ISOLATE_ALONE = new RegExp(`^\\s*isolate\\s+${VAR}\\s*$`, 'i')

const NAMES = new Set(`${SCIENTIFIC_NAMES}|e|i|theta`.toLowerCase().split('|'))

export function parseIsolate(text: string): IsolateCmd | null {
  const s = subscripts(text.trim())
  const lead = s.match(LEAD)
  const trail = lead ? null : s.match(TRAIL)
  if (!lead && !trail) return null
  const variable = (lead ? lead[1] : trail![2])!
  const body = (lead ? lead[2] : trail![1])!.trim()
  if (/==|[<>!]=|[≤≥≠<>]/.test(body)) return null
  const parts = body.split('=')
  if (parts.length > 2) return null
  const lhs = parts[0]!.trim()
  const rhs = (parts[1] ?? '0').trim()
  if (!lhs || !rhs) return null
  return { variable, lhs, rhs }
}

const SOLVE_LEAD = new RegExp(`^solve\\s+(?:for\\s+)?${VAR}(?:\\s+in\\s+|\\s*[:,;]\\s*)(.+)$`, 'is')
const SOLVE_FOR_LEAD = new RegExp(`^solve\\s+for\\s+${VAR}\\s+(.+)$`, 'is')
const SOLVE_TRAIL = new RegExp(`^(.+?)\\s*[,;]?\\s+solve\\s+(?:for\\s+)?${VAR}$`, 'is')
const FOR_TRAIL = new RegExp(`^(solve\\s+)?(.+?)\\s*[,;]?\\s+for\\s+${VAR}$`, 'is')
const SOLVE_ALONE = new RegExp(`^\\s*(isolate|solve)\\s+(?:for\\s+)?${VAR}\\s*$`, 'i')

/** A solve that names its letter: `solve x in …`, `solve for x: …`, `… for x`, `…, solve x`. */
export interface NamedSolve {
  variable: string
  /** The equation, or an expression set to 0. */
  eq: string
  /** The same line as the numeric solver reads it. */
  numeric: string
  /** The same line as isolate reads it. */
  isolate: string
}

export function parseNamedSolve(text: string): NamedSolve | null {
  const s = subscripts(solveCall(text).trim())
  let variable: string | undefined
  let eq: string | undefined
  let m: RegExpMatchArray | null
  if ((m = s.match(SOLVE_LEAD)) || (m = s.match(SOLVE_FOR_LEAD))) [variable, eq] = [m[1], m[2]]
  else if ((m = s.match(SOLVE_TRAIL))) [eq, variable] = [m[1], m[2]]
  else if ((m = s.match(FOR_TRAIL))) {
    // without the word solve, `… for x` needs an = to be an equation
    if (!m[1] && !m[2]!.includes('=')) return null
    ;[eq, variable] = [m[2], m[3]]
  }
  if (!variable || !eq) return null
  eq = eq.trim().replace(/^solve\s+/i, '')
  if (/^(?:graph|sys)\b/i.test(eq)) return null
  const isolate = `isolate ${variable} in ${eq}`
  if (!parseIsolate(isolate)) return null
  // the numeric solver reads `mx` the way isolate does, as m times x
  return { variable, eq, numeric: `solve ${splitShortWords(eq, variable)} for ${variable}`, isolate }
}

/** Lines isolate or a named solve owns, which natural-language math shouldn't answer instead. */
export function isIsolateCommand(text: string): boolean {
  return /^\s*isolate\b|\bisolate\s+[A-Za-zθ]/i.test(text) || SOLVE_ALONE.test(text) || parseNamedSolve(text) != null
}

/** The equation a history row holds, from `…`, `isolate x in …` or `solve … for x`. */
function rowEquation(expr: string): string | null {
  const named = parseNamedSolve(expr)
  if (named) return named.eq
  const iso = parseIsolate(expr)
  if (iso) return `${iso.lhs} = ${iso.rhs}`
  const eq = expr.trim().replace(/^solve\s+/i, '')
  if (eq.split('=').length !== 2 || /^(?:graph|sys)\b/i.test(eq) || /^[A-Za-z][A-Za-z0-9]*\s*\(.*\)\s*=/.test(eq)) return null
  return eq
}

/** `isolate x` or `solve for x` alone, rewritten to work on the previous line's equation. */
export function isolatePrevious(text: string, previous: string | undefined): string | null {
  const m = subscripts(text).match(SOLVE_ALONE)
  if (!m || !previous) return null
  const eq = rowEquation(previous)
  if (!eq) return null
  return m[1]!.toLowerCase() === 'solve' ? `solve ${eq} for ${m[2]}` : `isolate ${m[2]} in ${eq}`
}

/**
 * Letters written together are a product in a formula (`PV = nRT`), so short runs split.
 * Longer words, names the engine knows, the variable itself and calls stay whole.
 */
function splitShortWords(side: string, keep: string): string {
  return side.replace(/(?<![A-Za-z_.])[A-Za-z]{2,3}(?![A-Za-z0-9_]|\s*\()/g, (w) =>
    w === keep || NAMES.has(w.toLowerCase()) ? w : [...w].join(' '),
  )
}

function parseSide(side: string, v: string, known: Record<string, number>): MathNode | null {
  try {
    const text = preprocessAscii(splitShortWords(normalizeMathText(side), v))
    const node = math.parse(text)
    // an assignment or a matrix means this isn't a formula
    let ok = true
    node.traverse((n) => {
      if (n.type === 'AssignmentNode' || n.type === 'FunctionAssignmentNode' || n.type === 'ArrayNode') ok = false
    })
    if (!ok) return null
    return node.transform((n, path, parent) => {
      if (n.type !== 'SymbolNode' || (parent?.type === 'FunctionNode' && path === 'fn')) {
        // log is natural from here on; show writes it back as ln
        if (n.type === 'FunctionNode' && (n as unknown as { fn: { name: string } }).fn.name === 'ln') return call('log', ...(n as unknown as { args: N[] }).args)
        return n
      }
      const name = (n as unknown as { name: string }).name
      return name !== v && name in known && Number.isFinite(known[name]) ? C(known[name]!) : n
    })
  } catch {
    return null
  }
}

// ---------- node helpers ----------

type N = MathNode
const C = (n: number): N => new math.ConstantNode(n)
const S = (name: string): N => new math.SymbolNode(name)
const op = (o: string, a: N, b: N): N => {
  const fn = { '+': 'add', '-': 'subtract', '*': 'multiply', '/': 'divide', '^': 'pow' }[o]!
  return new math.OperatorNode(o as '+', fn as 'add', [a, b])
}
const neg = (a: N): N => new math.OperatorNode('-', 'unaryMinus', [a])
const call = (name: string, ...args: N[]): N => new math.FunctionNode(S(name), args)

function strip(n: N): N {
  while (n.type === 'ParenthesisNode') n = (n as unknown as { content: N }).content
  return n
}

function has(n: N, v: string): boolean {
  let found = false
  n.traverse((m, path, parent) => {
    if (m.type === 'SymbolNode' && (m as unknown as { name: string }).name === v && !(parent?.type === 'FunctionNode' && path === 'fn')) found = true
  })
  return found
}

function count(n: N, v: string): number {
  let k = 0
  n.traverse((m, path, parent) => {
    if (m.type === 'SymbolNode' && (m as unknown as { name: string }).name === v && !(parent?.type === 'FunctionNode' && path === 'fn')) k++
  })
  return k
}

/** simplify, except a function of plain numbers (`ln(2)`, `sqrt(3)`) stays exact instead of turning decimal. */
function simp(n: N): N {
  const held: N[] = []
  const hold = n.transform((m) => {
    if (m.type !== 'FunctionNode' || (m as unknown as { args: N[] }).args.some((a) => a.filter((x) => x.type === 'SymbolNode').length > 0)) return m
    held.push(m)
    return S(`__held${held.length - 1}`)
  })
  let out: N
  try {
    out = math.simplify(hold)
  } catch {
    out = hold
  }
  return out.transform((m) => {
    const k = m.type === 'SymbolNode' ? (m as unknown as { name: string }).name.match(/^__held(\d+)$/) : null
    return k ? held[Number(k[1])]! : m
  })
}

function constValue(n: N): number | null {
  const s = strip(n)
  if (s.type === 'ConstantNode') {
    const v = (s as unknown as { value: unknown }).value
    return typeof v === 'number' ? v : null
  }
  if (s.type === 'OperatorNode') {
    const o = s as unknown as { fn: string; args: N[] }
    if (o.fn === 'unaryMinus') {
      const a = constValue(o.args[0]!)
      return a == null ? null : -a
    }
    if (o.fn === 'divide') {
      const a = constValue(o.args[0]!)
      const b = constValue(o.args[1]!)
      return a == null || b == null || b === 0 ? null : a / b
    }
  }
  return null
}

const isZero = (n: N) => constValue(simp(n)) === 0

// ---------- path 1: the variable appears once; undo each operation around it ----------

const INVERSE: Record<string, (y: N) => N[]> = {
  sqrt: (y) => [op('^', y, C(2))],
  cbrt: (y) => [op('^', y, C(3))],
  // log is natural here: preprocessAscii has already made a base-10 log into log10
  exp: (y) => [call('log', y)],
  log: (y) => [op('^', S('e'), y)],
  log10: (y) => [op('^', C(10), y)],
  log2: (y) => [op('^', C(2), y)],
  sin: (y) => [call('asin', y)],
  cos: (y) => [call('acos', y)],
  tan: (y) => [call('atan', y)],
  asin: (y) => [call('sin', y)],
  acos: (y) => [call('cos', y)],
  atan: (y) => [call('tan', y)],
  sinh: (y) => [call('asinh', y)],
  cosh: (y) => [call('acosh', y)],
  tanh: (y) => [call('atanh', y)],
  asinh: (y) => [call('sinh', y)],
  acosh: (y) => [call('cosh', y)],
  atanh: (y) => [call('tanh', y)],
  abs: (y) => [y, neg(y)],
}

/** Every `x = …` from `node = other`, where `node` holds x once; null when an operation can't be undone. */
function unwind(node: N, others: N[], v: string): N[] | null {
  for (let guard = 0; guard < 64; guard++) {
    node = strip(node)
    if (node.type === 'SymbolNode' && (node as unknown as { name: string }).name === v) return others
    if (node.type === 'OperatorNode') {
      const { fn, args } = node as unknown as { fn: string; args: N[] }
      if (fn === 'unaryPlus') {
        node = args[0]!
        continue
      }
      if (fn === 'unaryMinus') {
        others = others.map(neg)
        node = args[0]!
        continue
      }
      const [a, b] = args as [N, N]
      const inA = has(a, v)
      if (fn === 'add') {
        const [inner, rest] = inA ? [a, b] : [b, a]
        others = others.map((y) => op('-', y, rest))
        node = inner
      } else if (fn === 'subtract') {
        others = inA ? others.map((y) => op('+', y, b)) : others.map((y) => op('-', a, y))
        node = inA ? a : b
      } else if (fn === 'multiply') {
        const [inner, rest] = inA ? [a, b] : [b, a]
        others = others.map((y) => op('/', y, rest))
        node = inner
      } else if (fn === 'divide') {
        others = inA ? others.map((y) => op('*', y, b)) : others.map((y) => op('/', a, y))
        node = inA ? a : b
      } else if (fn === 'pow') {
        if (inA) {
          const k = constValue(b)
          // an even power has a root on each side of 0
          if (k != null && Number.isInteger(k) && k % 2 === 0) {
            const root = (y: N) => (k === 2 ? call('sqrt', y) : op('^', y, op('/', C(1), C(k))))
            others = others.flatMap((y) => [root(y), neg(root(y))])
          } else if (k === 3) {
            others = others.map((y) => call('cbrt', y))
          } else if (k === 0.5) {
            others = others.map((y) => op('^', y, C(2)))
          } else {
            others = others.map((y) => op('^', y, op('/', C(1), b)))
          }
          node = a
        } else {
          const base = strip(a)
          const natural = base.type === 'SymbolNode' && (base as unknown as { name: string }).name === 'e'
          others = others.map((y) => (natural ? call('log', y) : op('/', call('log', y), call('log', a))))
          node = b
        }
      } else return null
      continue
    }
    if (node.type === 'FunctionNode') {
      const f = node as unknown as { fn: { name: string }; args: N[] }
      const name = f.fn.name
      if (name === 'nthRoot' && f.args.length === 2 && !has(f.args[1]!, v)) {
        others = others.map((y) => op('^', y, f.args[1]!))
        node = f.args[0]!
        continue
      }
      if (name === 'log' && f.args.length === 2 && !has(f.args[1]!, v)) {
        others = others.map((y) => op('^', f.args[1]!, y))
        node = f.args[0]!
        continue
      }
      const inv = INVERSE[name]
      if (!inv || f.args.length !== 1) return null
      others = others.flatMap(inv)
      node = f.args[0]!
      continue
    }
    return null
  }
  return null
}

// ---------- path 2: the variable appears more than once; collect a polynomial ----------

/** Coefficients by power of x, over a denominator polynomial. */
interface Rational {
  num: N[]
  den: N[]
}

const MAX_DEGREE = 6

function padd(p: N[], q: N[]): N[] {
  const out: N[] = []
  for (let i = 0; i < Math.max(p.length, q.length); i++) {
    const a = p[i]
    const b = q[i]
    out.push(a && b ? op('+', a, b) : (a ?? b)!)
  }
  return out
}

function pmul(p: N[], q: N[]): N[] {
  if (p.length + q.length - 1 > MAX_DEGREE + 1) throw new Error('degree')
  const out: (N | undefined)[] = []
  p.forEach((a, i) =>
    q.forEach((b, j) => {
      const t = op('*', a, b)
      out[i + j] = out[i + j] ? op('+', out[i + j]!, t) : t
    }),
  )
  return out.map((t) => t ?? C(0))
}

const pneg = (p: N[]) => p.map(neg)
const pone = (): N[] => [C(1)]

function rational(node: N, v: string): Rational | null {
  node = strip(node)
  // constant sums and fractions are taken apart too, so 1/a + 1/b clears to one fraction
  const arith = node.type === 'OperatorNode' && ['add', 'subtract', 'multiply', 'divide', 'unaryMinus', 'unaryPlus'].includes((node as unknown as { fn: string }).fn)
  if (!has(node, v) && (!arith || constValue(node) != null)) return { num: [node], den: pone() }
  if (node.type === 'SymbolNode') return { num: [C(0), C(1)], den: pone() }
  if (node.type !== 'OperatorNode') return null
  const { fn, args } = node as unknown as { fn: string; args: N[] }
  if (fn === 'unaryPlus') return rational(args[0]!, v)
  if (fn === 'unaryMinus') {
    const r = rational(args[0]!, v)
    return r && { num: pneg(r.num), den: r.den }
  }
  if (fn === 'pow') {
    const k = constValue(args[1]!)
    if (k == null || !Number.isInteger(k) || Math.abs(k) > MAX_DEGREE) return null
    const r = rational(args[0]!, v)
    if (!r) return null
    let out: Rational = { num: pone(), den: pone() }
    for (let i = 0; i < Math.abs(k); i++) out = { num: pmul(out.num, r.num), den: pmul(out.den, r.den) }
    return k < 0 ? { num: out.den, den: out.num } : out
  }
  const a = rational(args[0]!, v)
  const b = rational(args[1]!, v)
  if (!a || !b) return null
  if (fn === 'add' || fn === 'subtract') {
    const bn = fn === 'add' ? b.num : pneg(b.num)
    return { num: padd(pmul(a.num, b.den), pmul(bn, a.den)), den: pmul(a.den, b.den) }
  }
  if (fn === 'multiply') return { num: pmul(a.num, b.num), den: pmul(a.den, b.den) }
  if (fn === 'divide') return { num: pmul(a.num, b.den), den: pmul(a.den, b.num) }
  return null
}

interface Found {
  roots: N[]
  /** A quadratic's `(-b ± sqrt(d))/(2a)`, written as one line. */
  text?: string
}

function polySolve(f: N, v: string): Found | null {
  let r: Rational | null
  try {
    r = rational(f, v)
  } catch {
    return null
  }
  if (!r) return null
  const c = r.num.map(simp)
  while (c.length && isZero(c[c.length - 1]!)) c.pop()
  if (c.length === 2) return { roots: [simp(op('/', neg(c[0]!), c[1]!))] }
  if (c.length === 3) {
    const [c0, c1, c2] = c as [N, N, N]
    if (isZero(c1)) {
      const sq = simp(op('/', neg(c0), c2))
      const root = call('sqrt', sq)
      return { roots: [root, neg(root)], text: `±${show(root)}` }
    }
    const disc = simp(op('-', op('^', c1, C(2)), op('*', op('*', C(4), c2), c0)))
    let top = simp(neg(c1))
    let bottom = simp(op('*', C(2), c2))
    // (v ± s)/(-a) is (-v ± s)/a
    if (negative(bottom)) {
      top = simp(neg(top))
      bottom = simp(neg(bottom))
    }
    const root = (sign: string) => op('/', op(sign, top, call('sqrt', disc)), bottom)
    const pm = `(${show(top)} ± sqrt(${show(disc)}))`
    const text = constValue(bottom) === 1 ? pm.slice(1, -1) : `${pm}/${wrap(bottom)}`
    return { roots: [root('+'), root('-')], text }
  }
  return null
}

// ---------- checking and showing ----------

/** Plugs each root back in at a few points; one that disagrees anywhere is dropped. */
type Check = 'everywhere' | 'somewhere' | false

/**
 * Plugs a root back in. `everywhere` agrees at every point where both sides are real;
 * `somewhere` only on part of the domain (`sqrt(x) = -y` gives x = y^2, true for y ≤ 0).
 */
function checks(lhs: N, rhs: N, root: N, v: string): Check {
  const plus = agreement(lhs, rhs, root, v, false)
  if (plus.agreed && !plus.disagreed) return 'everywhere'
  return plus.agreed || agreement(lhs, rhs, root, v, true).agreed ? 'somewhere' : false
}

function agreement(lhs: N, rhs: N, root: N, v: string, signed: boolean): { agreed: number; disagreed: number } {
  const names = new Set<string>()
  for (const n of [lhs, rhs, root])
    n.traverse((m, path, parent) => {
      if (m.type === 'SymbolNode' && !(parent?.type === 'FunctionNode' && path === 'fn')) names.add((m as unknown as { name: string }).name)
    })
  const free = [...names].filter((n) => n !== v && !['e', 'pi', 'tau', 'i', 'Infinity'].includes(n))
  let agreed = 0
  let disagreed = 0
  const f = math.compile(`(${lhs.toString()}) - (${rhs.toString()})`)
  const g = root.compile()
  for (let t = 0; t < 8; t++) {
    const scope: Record<string, number> = {}
    free.forEach((n, i) => {
      const u = 0.37 + (((i + 1) * 0.618 + t * 0.414) % 1) * 1.7
      scope[n] = signed && (t + i) % 2 ? -u : u
    })
    let x: unknown
    let y: unknown
    try {
      x = g.evaluate(scope)
      if (typeof x !== 'number' || !Number.isFinite(x)) continue
      y = f.evaluate({ ...scope, [v]: x })
    } catch {
      continue
    }
    if (typeof y !== 'number' || !Number.isFinite(y)) continue
    const scale = Math.max(1, Math.abs(x as number))
    if (Math.abs(y) > 1e-7 * scale) disagreed++
    else agreed++
  }
  return { agreed, disagreed }
}

function negative(n: N): boolean {
  const s = strip(n)
  if (s.type === 'OperatorNode' && (s as unknown as { fn: string }).fn === 'unaryMinus') return true
  const k = constValue(s)
  return k != null && k < 0
}

/** Flattens `a/b/c` into `a/(b*c)` so a formula reads with one fraction bar. */
function oneBar(n: N): N {
  return n.transform((m) => {
    const s = strip(m)
    if (s.type !== 'OperatorNode' || (s as unknown as { fn: string }).fn !== 'divide') return m
    let [top, bottom] = (s as unknown as { args: [N, N] }).args.map(oneBar) as [N, N]
    for (let t = strip(top); t.type === 'OperatorNode' && (t as unknown as { fn: string }).fn === 'divide'; t = strip(top)) {
      const [a, b] = (t as unknown as { args: [N, N] }).args
      top = a
      bottom = op('*', b, bottom)
    }
    return op('/', top, bottom)
  })
}

/** A number leads its product, so `y*-3` reads `-3y`. */
function numberFirst(n: N): N {
  return n.transform((m) => {
    if (m.type !== 'OperatorNode' || (m as unknown as { fn: string }).fn !== 'multiply') return m
    const [a, b] = (m as unknown as { args: N[] }).args
    return b && a && constValue(b) != null && constValue(a) == null ? op('*', numberFirst(b), numberFirst(a)) : m
  })
}

function show(n: N): string {
  const text = oneBar(numberFirst(n)).toString({ parenthesis: 'auto', implicit: 'hide' })
  return text
    .replace(/\blog\(/g, 'ln(')
    .replace(/(?<![A-Za-z_\d.])(\d+(?:\.\d+)?)(?: \* | )(?=[A-Za-z(])/g, '$1')
    .replace(/ ([*/^]) /g, '$1')
}

function wrap(n: N): string {
  const s = show(n)
  return /^(?:[A-Za-z][A-Za-z0-9]*|\d+(?:\.\d+)?)$/.test(s) ? s : `(${s})`
}

/** The shortest of simplify's and rationalize's forms (`1/(1/a + 1/b)` becomes `a*b/(a + b)`). */
function tidy(n: N): N[] {
  const out = [simp(n)]
  try {
    out.push(math.rationalize(n))
  } catch {
    // rationalize only takes rational expressions
  }
  // a decimal rationalize invented reads worse than a longer exact form
  const cost = (t: N) => show(t).length + (/\d\.\d/.test(show(t)) ? 1000 : 0)
  return out.sort((a, b) => cost(a) - cost(b))
}

/** Two roots as one `m ± h` when that reads shorter: `sqrt(y) - 1, -(sqrt(y) + 1)` is `-1 ± sqrt(y)`. */
function plusMinus(a: N, b: N, [sa, sb]: [string, string]): string | null {
  if (sb === `-${sa}` || sb === `-(${sa})`) return `±${sa}`
  const mid = simp(op('/', op('+', a, b), C(2)))
  if (isZero(mid)) return `±${show(simp(op('/', op('-', a, b), C(2))))}`
  let half = simp(op('/', op('-', a, b), C(2)))
  if (negative(half)) half = simp(neg(half))
  const text = `${show(mid)} ± ${show(half)}`
  return text.length <= `${sa}, ${sb}`.length ? text : null
}

export interface Isolated {
  variable: string
  /** `x = …`, with `±` or `,` between two forms. */
  display: string
  forms: string[]
}

/** `isolate x in …` solved symbolically; null when the line isn't one or x can't be moved out alone. */
export function isolateVariable(text: string, opts: { variables?: Record<string, number> } = {}): Isolated | null {
  const cmd = parseIsolate(text)
  if (!cmd) return null
  const v = cmd.variable === 'θ' ? 'theta' : cmd.variable
  const known = opts.variables ?? {}
  const lhs = parseSide(cmd.lhs, v, known)
  const rhs = parseSide(cmd.rhs, v, known)
  if (!lhs || !rhs) return null
  const inL = count(lhs, v)
  const inR = count(rhs, v)
  if (!inL && !inR) return null

  let found: Found | null = null
  if (inL + inR === 1) {
    const roots = inL ? unwind(lhs, [rhs], v) : unwind(rhs, [lhs], v)
    if (roots) found = { roots: roots.map(simp) }
  }
  const poly = polySolve(op('-', lhs, rhs), v)
  // the polynomial reading can clear fractions unwinding leaves (1/(1/a + 1/b) is a*b/(a + b))
  const len = (f: Found) => {
    const t = f.text ?? f.roots.map(show).join()
    return t.length + 4 * (t.match(/\//g)?.length ?? 0)
  }
  if (poly && (!found || (poly.roots.length === found.roots.length && len(poly) < len(found)))) found = poly
  if (!found) return null

  const good: N[] = []
  let all = true
  for (const r of found.roots) {
    const tries = has(r, v) ? [] : found.text ? [r] : tidy(r)
    const verdicts = tries.map((t) => checks(lhs, rhs, t, v))
    const at = verdicts.indexOf('everywhere') >= 0 ? verdicts.indexOf('everywhere') : verdicts.indexOf('somewhere')
    if (at >= 0) good.push(tries[at]!)
    else all = false
  }
  if (!good.length) return null
  const forms = [...new Set(good.map(show))]
  let body = forms.join(', ')
  if (all && found.text) body = found.text
  else if (forms.length === 2) body = plusMinus(good[0]!, good[1]!, forms as [string, string]) ?? body
  const name = cmd.variable
  return { variable: name, display: `${name} = ${body}`, forms }
}
