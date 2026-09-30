import type { ArrayNode, ConstantNode, FunctionNode, MathNode, OperatorNode, ParenthesisNode, SymbolNode } from 'mathjs'
import { formatNumber } from './format'
import { math } from './math'
import { formatAsFraction } from './scientific'

type Grid = number[][]
type M = Grid | number

export interface MatrixAnswer {
  /** A matrix as `[1 2; 3 4]`, a number, or a message such as `not square`. */
  display: string
  /** Fraction form, when some entry isn't whole and every entry is a simple fraction. */
  exact?: string
  n?: number
  /** Literal a stored matrix or `ans` is read back from; exact wherever an entry is a simple fraction. */
  literal?: string
  message?: boolean
}

export interface MatrixOptions {
  variables?: Record<string, number>
  sigFigs: number
  fractionMode?: boolean
  /** The user's own function names, which win over `det(…)` and friends. */
  functions?: string[]
  /** The calculator's reading of a plain-number entry (`sin(30)` in degrees, `sqrt(2)`); null when it can't. */
  scalar?: (expr: string) => number | null
}

class MatrixError extends Error {}
/** A name nobody defined: blank, so the input can point at it, rather than a message. */
class Blank extends Error {}

/** Denominators up to this read as fractions (`2/3`); past it an entry stays decimal. */
const MAX_DEN = 1000
/** Past this (after scaling rows alike) an inverse has too few trustworthy digits to show. */
const MAX_CONDITION = 1e8
const MAX_IDENTITY = 20
/** Matrix answers are wide; a few digits per entry keep a row on screen. */
const MATRIX_SIG_FIGS = 6

const WORDS = 'inv|inverse|det|determinant|transpose|trace|tr|rank|rref|norm'
const FNS = `${WORDS}|identity|eye|dot|cross`
const TWO_D = /\[\s*\[|\[[^\][]*;[^\][]*\]/
/** `[1 2 3]`, a row the way answers write it; a list takes commas. */
const SPACED_ROW = /\[[^\][,;]*[\d.)A-Za-z]\s+[-+]?[\d.(A-Za-z][^\][,;]*\]/
const MARKS = /\^\s*\(?\s*T\s*\)?(?![A-Za-z0-9_])|ᵀ|⁻¹|[\])]\s*'|[\])]\s+(?:transpose|inverse)\b/i
const WORD_FORM = new RegExp(`(?<![A-Za-z_])(?:the\\s+)?(?:${WORDS})\\s+(?:of\\s+)?[\\[(A-Za-z0-9]`, 'i')

function fnCall(names: string[]): RegExp {
  const own = new Set(names.map((n) => n.toLowerCase()))
  const list = FNS.split('|').filter((n) => !own.has(n))
  return new RegExp(`(?<![A-Za-z_])(?:${list.join('|')})\\s*\\(`, 'i')
}

/** A line for the matrix engine: a 2D literal, a matrix function, a word form, or a transpose mark. */
export function looksLikeMatrix(expr: string, functions: string[] = []): boolean {
  if (fnCall(functions).test(expr)) return true
  if (!expr.includes('[')) return false
  return TWO_D.test(expr) || SPACED_ROW.test(expr) || MARKS.test(expr) || WORD_FORM.test(expr)
}

function size(a: Grid): string {
  return `${a.length}×${a[0]?.length ?? 0}`
}

function grid(x: unknown): Grid {
  const arr = math.isMatrix(x) ? (x.toArray() as unknown[]) : x
  if (!Array.isArray(arr) || !arr.length) throw new MatrixError('empty matrix')
  // a flat list is one row
  const rows = Array.isArray(arr[0]) ? (arr as unknown[][]) : [arr]
  const width = rows[0]!.length
  return rows.map((r) => {
    if (!Array.isArray(r) || r.length !== width || !width) throw new MatrixError('rows differ in length')
    return r.map((v) => {
      if (typeof v !== 'number' || !Number.isFinite(v)) throw new MatrixError('an entry is undefined')
      return v
    })
  })
}

/** A number is a 1×1 matrix wherever an operation needs one (`det(5)` is 5). */
function asGrid(a: M): Grid {
  return typeof a === 'number' ? [[a]] : a
}

function square(a: M, what: string): Grid {
  const g = asGrid(a)
  if (g.length !== g[0]!.length) throw new MatrixError(`${what} needs a square matrix (this is ${size(g)})`)
  return g
}

function add(a: M, b: M, sign: 1 | -1): M {
  if (typeof a === 'number' && typeof b === 'number') return a + sign * b
  const verb = sign > 0 ? 'add' : 'subtract'
  if (typeof a === 'number' || typeof b === 'number') throw new MatrixError(`can't ${verb} a number and a matrix`)
  if (a.length !== b.length || a[0]!.length !== b[0]!.length) {
    throw new MatrixError(`can't ${verb} ${size(a)} and ${size(b)}: sizes must match`)
  }
  return a.map((r, i) => r.map((v, j) => v + sign * b[i]![j]!))
}

function scale(a: Grid, k: number): Grid {
  return a.map((r) => r.map((v) => v * k))
}

function mul(a: M, b: M): M {
  if (typeof a === 'number') return typeof b === 'number' ? a * b : scale(b, a)
  if (typeof b === 'number') return scale(a, b)
  const inner = a[0]!.length
  if (inner !== b.length) {
    const hint = a.length === b.length && inner === b[0]!.length ? '; for entry by entry, use dot for vectors' : ''
    throw new MatrixError(`can't multiply ${size(a)} by ${size(b)}: columns of the first must equal rows of the second${hint}`)
  }
  return a.map((r) => b[0]!.map((_, j) => r.reduce((s, v, k) => s + v * b[k]![j]!, 0)))
}

function transpose(a: M): M {
  if (typeof a === 'number') return a
  return a[0]!.map((_, j) => a.map((r) => r[j]!))
}

function scaleOf(a: Grid): number {
  return Math.max(...a.flat().map(Math.abs), Number.MIN_VALUE)
}

/** Row echelon by partial pivoting: the reduced grid, the pivot columns, and the determinant's sign and product. */
function eliminate(src: Grid, reduce: boolean, scaleFrom: Grid = src): { m: Grid; pivots: number[]; det: number } {
  const m = src.map((r) => [...r])
  const rows = m.length
  const cols = m[0]!.length
  const tiny = 1e-12 * scaleOf(scaleFrom)
  const pivots: number[] = []
  let det = 1
  let r = 0
  for (let c = 0; c < cols && r < rows; c++) {
    let p = r
    for (let i = r + 1; i < rows; i++) if (Math.abs(m[i]![c]!) > Math.abs(m[p]![c]!)) p = i
    if (Math.abs(m[p]![c]!) <= tiny) {
      det = 0
      for (let i = r; i < rows; i++) m[i]![c] = 0
      continue
    }
    if (p !== r) {
      ;[m[p], m[r]] = [m[r]!, m[p]!]
      det = -det
    }
    const piv = m[r]![c]!
    det *= piv
    if (reduce) m[r] = m[r]!.map((v) => v / piv)
    for (let i = reduce ? 0 : r + 1; i < rows; i++) {
      if (i === r) continue
      const f = m[i]![c]! / m[r]![c]!
      if (f) m[i] = m[i]!.map((v, k) => (k === c ? 0 : v - f * m[r]![k]!))
    }
    pivots.push(c)
    r++
  }
  return { m, pivots, det }
}

function det(a: M): number {
  const sq = square(a, 'determinant')
  return eliminate(sq, false).det
}

/** ∞-norm condition number of `a` with its rows scaled to the same size, so `[1e5 0; 0 1e-5]` counts as tame. */
function condition(a: Grid, inverse: Grid): number {
  const rowMax = a.map((r) => Math.max(...r.map(Math.abs)))
  const s = a.map((r, i) => r.map((v) => v / rowMax[i]!))
  const si = inverse.map((r) => r.map((v, j) => v * rowMax[j]!))
  const norm = (g: Grid) => Math.max(...g.map((r) => r.reduce((t, v) => t + Math.abs(v), 0)))
  return norm(s) * norm(si)
}

function inv(a: M): M {
  if (typeof a === 'number') {
    if (a === 0) throw new MatrixError('no inverse (it is 0)')
    return 1 / a
  }
  const sq = square(a, 'inverse')
  const n = sq.length
  const { m, pivots } = eliminate(
    sq.map((r, i) => [...r, ...r.map((_, j) => (i === j ? 1 : 0))]),
    true,
    sq,
  )
  if (pivots.length < n || pivots[n - 1] !== n - 1) throw new MatrixError('no inverse (determinant is 0)')
  const out = m.map((r) => r.slice(n))
  if (!(condition(sq, out) <= MAX_CONDITION)) throw new MatrixError('no inverse (nearly singular: too close to determinant 0 to trust)')
  return out
}

function trace(a: M): number {
  return square(a, 'trace').reduce((s, r, i) => s + r[i]!, 0)
}

function rref(a: M): Grid {
  const g = asGrid(a)
  return eliminate(g, true).m
}

function rank(a: M): number {
  return eliminate(asGrid(a), false).pivots.length
}

function identity(n: M): Grid {
  if (typeof n !== 'number' || !Number.isInteger(n) || n < 1 || n > MAX_IDENTITY) {
    throw new MatrixError(`identity needs a whole size from 1 to ${MAX_IDENTITY}`)
  }
  return Array.from({ length: n }, (_, i) => Array.from({ length: n }, (_, j) => (i === j ? 1 : 0)))
}

/** A row or column as a flat list; null for anything else. */
function vector(a: M): number[] | null {
  if (typeof a === 'number') return null
  if (a.length === 1) return a[0]!
  if (a[0]!.length === 1) return a.map((r) => r[0]!)
  return null
}

function dot(a: M, b: M): number {
  const u = vector(a)
  const v = vector(b)
  if (!u || !v) throw new MatrixError('dot needs two vectors')
  if (u.length !== v.length) throw new MatrixError(`dot needs vectors of the same length (${u.length} and ${v.length})`)
  return u.reduce((s, x, i) => s + x * v[i]!, 0)
}

function cross(a: M, b: M): Grid {
  const u = vector(a)
  const v = vector(b)
  if (!u || !v || u.length !== 3 || v.length !== 3) throw new MatrixError('cross needs two vectors of length 3')
  const w = [u[1]! * v[2]! - u[2]! * v[1]!, u[2]! * v[0]! - u[0]! * v[2]!, u[0]! * v[1]! - u[1]! * v[0]!]
  // the answer is laid out like the first vector
  return (a as Grid).length === 1 ? [w] : w.map((x) => [x])
}

function norm(a: M): number {
  return Math.sqrt(asGrid(a).flat().reduce((s, v) => s + v * v, 0))
}

function power(a: M, e: M): M {
  if (typeof a === 'number' && typeof e === 'number') return a ** e
  if (typeof e !== 'number') throw new MatrixError("can't raise to a matrix power")
  if (!Number.isInteger(e)) throw new MatrixError('a matrix power must be a whole number')
  const sq = square(a, 'a power')
  let base = e < 0 ? (inv(sq) as Grid) : sq
  let out: Grid = identity(sq.length)
  for (let k = Math.abs(e); k > 0; k = Math.floor(k / 2)) {
    if (k % 2) out = mul(out, base) as Grid
    base = mul(base, base) as Grid
  }
  return out
}

const CALLS: Record<string, (...a: M[]) => M> = {
  inv,
  inverse: inv,
  det,
  determinant: det,
  transpose,
  trace,
  tr: trace,
  rank,
  rref,
  norm,
  identity,
  eye: identity,
  dot,
  cross,
}

function hasMatrixPart(node: MathNode): boolean {
  let found = false
  node.traverse((n) => {
    if (n.type === 'ArrayNode') found = true
    if (n.type === 'FunctionNode' && Object.hasOwn(CALLS, (n as FunctionNode).fn.name.toLowerCase())) found = true
    if (n.type === 'OperatorNode' && (n as OperatorNode).fn === 'pow' && (n as OperatorNode).args[1]?.type === 'SymbolNode' && ((n as OperatorNode).args[1] as SymbolNode).name === 'T') found = true
  })
  return found
}

const CONSTANTS: Record<string, number> = { pi: Math.PI, e: Math.E, tau: 2 * Math.PI }

function evalNode(node: MathNode, opts: MatrixOptions): M {
  const vars = opts.variables ?? {}
  // an entry like `sqrt(2)` or `sin(30)` is plain calculator math
  if (node.type !== 'ConstantNode' && node.type !== 'SymbolNode' && opts.scalar && !hasMatrixPart(node)) {
    const v = opts.scalar(node.toString())
    if (v == null || !Number.isFinite(v)) throw new Blank()
    return v
  }
  switch (node.type) {
    case 'ParenthesisNode':
      return evalNode((node as ParenthesisNode).content, opts)
    case 'ConstantNode': {
      const v = (node as ConstantNode).value
      if (typeof v !== 'number') throw new Blank()
      return v
    }
    case 'SymbolNode': {
      const name = (node as SymbolNode).name
      if (Object.hasOwn(vars, name)) return vars[name]!
      if (Object.hasOwn(CONSTANTS, name)) return CONSTANTS[name]!
      throw new Blank()
    }
    case 'ArrayNode': {
      const items = (node as ArrayNode).items.map((item) => evalNode(item, opts))
      if (items.every((v) => typeof v === 'number')) return grid(items)
      // `[[1, 2], [3, 4]]`: each item is one row
      if (items.every((v) => typeof v !== 'number' && v.length === 1)) return grid(items.map((v) => (v as Grid)[0]!))
      throw new MatrixError('rows differ in length')
    }
    case 'FunctionNode': {
      const f = node as FunctionNode
      const name = f.fn.name
      // `k (…)` with a stored k multiplies
      if (Object.hasOwn(vars, name) && f.args.length === 1) return mul(vars[name]!, evalNode(f.args[0]!, opts))
      const key = name.toLowerCase()
      const fn = Object.hasOwn(CALLS, key) ? CALLS[key]! : undefined
      if (!fn) throw new MatrixError(`${name} doesn't take a matrix`)
      const want = key === 'dot' || key === 'cross' ? 2 : 1
      if (f.args.length !== want) throw new MatrixError(`${key} takes ${want === 1 ? 'one matrix' : 'two vectors'}`)
      return fn(...f.args.map((a) => evalNode(a, opts)))
    }
    case 'OperatorNode': {
      const o = node as OperatorNode
      const [l, r] = o.args
      if (o.fn === 'unaryMinus') return mul(-1, evalNode(l!, opts))
      if (o.fn === 'unaryPlus') return evalNode(l!, opts)
      if (!l || !r) throw new Blank()
      // `A^T` is the transpose
      if (o.fn === 'pow' && r.type === 'SymbolNode' && (r as SymbolNode).name === 'T') return transpose(evalNode(l, opts))
      const a = evalNode(l, opts)
      const b = evalNode(r, opts)
      switch (o.fn) {
        case 'add':
          return add(a, b, 1)
        case 'subtract':
          return add(a, b, -1)
        case 'multiply':
          return mul(a, b)
        case 'divide':
          if (typeof b !== 'number') throw new MatrixError("can't divide by a matrix; multiply by its inverse instead")
          if (b === 0) throw new MatrixError("can't divide by 0")
          return mul(a, 1 / b)
        case 'pow':
          return power(a, b)
      }
      throw new MatrixError(`${o.op} doesn't take a matrix`)
    }
  }
  throw new Blank()
}

/** `[1 2; 3 4]` is `[1, 2; 3, 4]`: spaces separate entries inside a bracket with no commas. */
function spaced(expr: string): string {
  return expr.replace(/\[([^\][]*)\]/g, (whole, body: string) => {
    if (body.includes(',')) return whole
    const cells = body.replace(/(?<=[\d.)A-Za-z])\s+(?=[\d.(A-Za-z]|[-+][\d.(])/g, ', ')
    return `[${cells}]`
  })
}

/** The end of the operand starting at `i`: a bracket or paren group, or a name or number. */
function operandEnd(s: string, i: number): number {
  const open = s[i]
  if (open === '[' || open === '(') {
    const close = open === '[' ? ']' : ')'
    let depth = 0
    for (let k = i; k < s.length; k++) {
      if (s[k] === open) depth++
      else if (s[k] === close && --depth === 0) return k + 1
    }
    return -1
  }
  const m = s.slice(i).match(/^[A-Za-z_][A-Za-z0-9_]*|^\d+(?:\.\d+)?/)
  return m ? i + m[0].length : -1
}

/** `det A`, `the inverse of A`, `transpose [1 2; 3 4]` are calls; `A transpose` and `A inverse` are marks. */
function wordForms(expr: string): string {
  const word = new RegExp(`(?<![A-Za-z_])(?:the\\s+)?(${WORDS})\\s+(?:of\\s+)?(?=[\\[(A-Za-z0-9])`, 'gi')
  let out = expr
  for (let guard = 0; guard < 8; guard++) {
    word.lastIndex = 0
    const m = word.exec(out)
    if (!m) break
    const start = m.index + m[0].length
    const end = operandEnd(out, start)
    if (end < 0) break
    out = `${out.slice(0, m.index)}${m[1]!.toLowerCase()}(${out.slice(start, end)})${out.slice(end)}`
  }
  return out.replace(/([\])A-Za-z0-9])\s+transpose\b/gi, '$1^T').replace(/([\])A-Za-z0-9])\s+inverse\b/gi, '$1^(-1)')
}

function prep(expr: string): string {
  // `3[[1, 2]]` and `k [1 2]` scale; mathjs would read a bracket after a value as an index
  return wordForms(spaced(expr))
    .replace(/(?<=[\w.)\]])\s*\[/g, '*[')
    .replace(/([\])])\s*'/g, '$1^T')
    .replace(/ᵀ/g, '^T')
    .replace(/⁻¹/g, '^(-1)')
    .replace(/[−–]/g, '-')
    .replace(/[×·]/g, '*')
    .replace(/÷/g, '/')
    .replace(/π/g, 'pi')
}

function tidy(n: number, scale: number): number {
  const v = Math.abs(n) <= 1e-12 * scale ? 0 : n
  // float noise off a whole number: -1.9999999999999996 is -2
  const whole = Math.round(v)
  return Math.abs(v - whole) <= 1e-12 * Math.max(1, Math.abs(v)) ? whole : v
}

function fraction(v: number): string | null {
  return Number.isInteger(v) ? String(v) : formatAsFraction(v, MAX_DEN)
}

/** `[1 2; 3 4]`, a row as `[1 2 3]`, a column as `[1; 2; 3]`. */
function formatGrid(a: Grid, cell: (n: number) => string): string {
  return `[${a.map((r) => r.map(cell).join(' ')).join('; ')}]`
}

function literalOf(a: Grid): string {
  return `[${a.map((r) => `[${r.map((v) => fraction(v) ?? String(v)).join(', ')}]`).join(', ')}]`
}

export function matrixAnswer(expr: string, opts: MatrixOptions): MatrixAnswer | null {
  if (!looksLikeMatrix(expr, opts.functions)) return null
  let node: MathNode
  try {
    node = math.parse(prep(expr))
  } catch {
    // `[1, 2; 3]` fails in the parser; a bracket that doesn't close is still being typed
    return TWO_D.test(expr) && !/\[[^\]]*$/.test(expr) ? { display: 'rows differ in length', message: true } : null
  }
  let value: M
  try {
    value = evalNode(node, opts)
  } catch (err) {
    if (err instanceof MatrixError) return { display: err.message, message: true }
    if (err instanceof Blank) return { display: '' }
    return null
  }
  // a row times a column is a number (`[1 2 3] * [1; 2; 3]` is 14)
  if (typeof value !== 'number' && value.length === 1 && value[0]!.length === 1) value = value[0]![0]!
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) return { display: 'undefined', message: true }
    const n = tidy(value, 1)
    const exact = Number.isInteger(n) ? undefined : (fraction(n) ?? undefined)
    const display = opts.fractionMode && exact ? exact : formatNumber(n, opts.sigFigs)
    return { display, exact: exact !== display ? exact : undefined, n }
  }
  const s = scaleOf(value)
  const clean = value.map((r) => r.map((v) => tidy(v, s)))
  const flat = clean.flat()
  const fractions = flat.some((v) => !Number.isInteger(v)) && flat.every((v) => fraction(v) != null)
  const exact = fractions ? formatGrid(clean, (v) => fraction(v)!) : undefined
  const figs = Math.min(opts.sigFigs, MATRIX_SIG_FIGS)
  const display = opts.fractionMode && exact ? exact : formatGrid(clean, (v) => formatNumber(v, figs))
  return { display, exact: exact !== display ? exact : undefined, literal: literalOf(clean) }
}
