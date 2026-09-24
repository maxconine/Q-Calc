import type { Decimal } from 'decimal.js'
import {
  BigNumberDependencies,
  create,
  parseDependencies,
  type ConstantNode,
  type FunctionNode,
  type MathNode,
  type OperatorNode,
  type ParenthesisNode,
  type SymbolNode,
} from 'mathjs'
import { math } from './math'
import { builtinScope, preprocessChecked, type AngleMode, type ScientificContext } from './scientific'
import type { Value } from './types'

// Truthful digits: the float answer is re-run in decimals at two precisions at once and replaced
// by the higher one only when both agree with each other and every function agrees with the
// engine's own at the same inputs. Anything unsupported, slow or doubtful keeps the float answer.

export const BUDGET_MS = 5
/** Precision pairs, tried in turn while the answer hasn't settled and there's time left. */
const LADDER: [number, number][] = [
  [32, 48],
  [48, 80],
]
/** Enough to tell a real mismatch from float rounding in the per-function check. */
const CHECK = 20

const big = create({ parseDependencies, BigNumberDependencies }, { number: 'BigNumber', precision: 84 })
type Dec = Decimal
type DecCtor = typeof Decimal
const BaseDec = (big as unknown as { BigNumber: DecCtor }).BigNumber
const Decs = new Map([...new Set([...LADDER.flat(), CHECK])].map((p) => [p, BaseDec.clone({ precision: p })]))
// mathjs builds its parser lazily; pay that once at load, not on the first keystroke
big.parse('1')

const ABORT = new Error('no precise twin')
const MEANING = new Error('precise twin disagrees')
function abort(): never {
  throw ABORT
}

type Fn = (...args: number[]) => unknown
const engineScopes: Partial<Record<AngleMode, Record<string, Fn>>> = {}
function engineScope(mode: AngleMode): Record<string, Fn> {
  return (engineScopes[mode] ??= builtinScope(mode) as Record<string, Fn>)
}

interface Backend<T> {
  lit(value: Dec): T
  fromNumber(n: number): T
  constant(name: string): T
  op(fn: string, args: T[]): T
  call(name: string, args: T[]): T
  tick(v: T): T
}

interface Env<T> {
  vars: Record<string, T>
  ctx: ScientificContext
  depth: number
}

const CONSTANTS = new Set(['pi', 'tau', 'e'])
const OPS: Record<string, number> = {
  add: 2,
  subtract: 2,
  multiply: 2,
  divide: 2,
  pow: 2,
  mod: 2,
  unaryMinus: 1,
  unaryPlus: 1,
  factorial: 1,
}

function walk<T>(node: MathNode, b: Backend<T>, env: Env<T>): T {
  switch (node.type) {
    case 'ParenthesisNode':
      return walk((node as ParenthesisNode).content, b, env)
    case 'ConstantNode': {
      const v = (node as ConstantNode).value as unknown
      if (!big.isBigNumber(v)) abort()
      return b.tick(b.lit(v as unknown as Dec))
    }
    case 'SymbolNode': {
      const name = (node as SymbolNode).name
      if (CONSTANTS.has(name)) return b.constant(name)
      if (Object.hasOwn(env.vars, name)) return env.vars[name]!
      return abort()
    }
    case 'OperatorNode': {
      const { fn, args } = node as OperatorNode
      if (OPS[fn] !== args.length) abort()
      return b.tick(b.op(fn, args.map((a) => walk(a, b, env))))
    }
    case 'FunctionNode': {
      const { fn, args } = node as FunctionNode
      if (fn.type !== 'SymbolNode') abort()
      const name = (fn as SymbolNode).name
      const values = args.map((a) => walk(a, b, env))
      const user = env.ctx.functions?.[name]
      if (user && Object.hasOwn(env.ctx.functions!, name)) return b.tick(callUser(user.params, user.body, values, b, env))
      if (!Object.hasOwn(TWINS, name)) abort()
      return b.tick(b.call(name, values))
    }
    default:
      return abort()
  }
}

function callUser<T>(params: string[], body: string, args: T[], b: Backend<T>, env: Env<T>): T {
  if (env.depth > 8 || args.length !== params.length) abort()
  const names: Record<string, number> = { ...env.ctx.variables }
  for (const p of params) names[p] = 0
  const expr = preprocessChecked(body, { ...env.ctx, variables: names })
  if (expr == null) abort()
  const vars = { ...env.vars }
  params.forEach((p, i) => (vars[p] = args[i]!))
  return walk(parse(expr), b, { vars, ctx: env.ctx, depth: env.depth + 1 })
}

function parse(expr: string): MathNode {
  try {
    return big.parse(expr)
  } catch {
    return abort()
  }
}

/** The engine's own functions over plain numbers: the reference the big path must mean the same as. */
function numberBackend(mode: AngleMode): Backend<number> {
  const scope = engineScope(mode)
  const ops = math as unknown as Record<string, Fn>
  const real = (v: unknown): number => (typeof v === 'number' && !Number.isNaN(v) ? v : abort())
  return {
    lit: (v) => Number(v.toString()),
    fromNumber: (n) => n,
    constant: (name) => real(scope[name] as unknown),
    op: (fn, args) => real(ops[fn]!(...args)),
    call: (name, args) => real(scope[name]!(...args)),
    tick: (v) => v,
  }
}

type Twin = (D: DecCtor, args: Dec[], k: TwinKit) => Dec
interface TwinKit {
  toRad(x: Dec): Dec
  fromRad(x: Dec): Dec
  pi: Dec
  e: Dec
}

function one(f: (x: Dec, D: DecCtor, k: TwinKit) => Dec): Twin {
  return (D, args, k) => (args.length === 1 ? f(args[0]!, D, k) : abort())
}

function intArg(x: Dec): Dec {
  return x.isInteger() ? x : abort()
}

function product(D: DecCtor, from: number, to: number): Dec {
  let p = new D(1)
  for (let i = from; i <= to; i++) p = p.times(i)
  return p
}

function factorial(D: DecCtor, x: Dec): Dec {
  const n = intArg(x).toNumber()
  if (n < 0 || n > 170) abort()
  return product(D, 2, n)
}

function choose(perm: boolean): Twin {
  return (D, args) => {
    if (args.length !== 2) abort()
    const n = intArg(args[0]!).toNumber()
    const k = intArg(args[1]!).toNumber()
    if (n < 0 || k < 0 || n > 10_000) abort()
    if (k > n) return new D(0)
    const falling = product(D, n - k + 1, n)
    return perm ? falling : falling.div(product(D, 2, k))
  }
}

function modulo(a: Dec, b: Dec): Dec {
  if (b.isZero()) abort()
  if (a.abs().gt(Number.MAX_SAFE_INTEGER) || b.abs().gt(Number.MAX_SAFE_INTEGER)) abort()
  const m = b.abs()
  return a.mod(m).plus(m).mod(m)
}

function inverse(x: Dec): Dec {
  return x.isZero() ? abort() : new (x.constructor as DecCtor)(1).div(x)
}

// decimal.js has no fast path for huge arguments (tanh(1e68) runs for minutes), and past
// these bounds a double can't hold the answer anyway
function bounded(x: Dec, limit: number): Dec {
  return x.abs().gt(limit) ? abort() : x
}

function nonzeroTrig(x: Dec): Dec {
  return x.abs().lt(1e-12) ? abort() : x
}

const TRIG: Record<string, Twin> = {
  sin: one((x, D, k) => D.sin(k.toRad(bounded(x, 1e100)))),
  cos: one((x, D, k) => D.cos(k.toRad(bounded(x, 1e100)))),
  tan: one((x, D, k) => {
    const r = k.toRad(bounded(x, 1e100))
    return D.sin(r).div(nonzeroTrig(D.cos(r)))
  }),
  csc: one((x, D, k) => inverse(nonzeroTrig(D.sin(k.toRad(bounded(x, 1e100)))))),
  sec: one((x, D, k) => inverse(nonzeroTrig(D.cos(k.toRad(bounded(x, 1e100)))))),
  cot: one((x, D, k) => {
    const r = k.toRad(bounded(x, 1e100))
    return D.cos(r).div(nonzeroTrig(D.sin(r)))
  }),
  asin: one((x, D, k) => (x.abs().gt(1) ? abort() : k.fromRad(D.asin(x)))),
  acos: one((x, D, k) => (x.abs().gt(1) ? abort() : k.fromRad(D.acos(x)))),
  atan: one((x, D, k) => k.fromRad(D.atan(x))),
  atan2: (D, args, k) => (args.length === 2 ? k.fromRad(D.atan2(args[0]!, args[1]!)) : abort()),
  acsc: one((x, D, k) => (x.abs().lt(1) ? abort() : k.fromRad(D.asin(inverse(x))))),
  asec: one((x, D, k) => (x.abs().lt(1) ? abort() : k.fromRad(D.acos(inverse(x))))),
  acot: one((x, D, k) => k.fromRad(x.isZero() ? k.pi.div(2) : D.atan(inverse(x)))),
}

const TWINS: Record<string, Twin> = {
  sqrt: one((x) => (x.isNeg() ? abort() : x.sqrt())),
  cbrt: one((x) => x.cbrt()),
  nthRoot: (D, args) => {
    const [x, nArg] = args
    if (!x || args.length > 2) abort()
    const n = nArg ? intArg(nArg).toNumber() : 2
    if (n < 1) abort()
    if (!x.isNeg()) return x.pow(new D(1).div(n))
    return n % 2 ? x.neg().pow(new D(1).div(n)).neg() : abort()
  },
  exp: one((x) => bounded(x, 1000).exp()),
  ln: one((x) => (x.lte(0) ? abort() : x.ln())),
  log10: one((x) => (x.lte(0) ? abort() : x.log(10))),
  log2: one((x) => (x.lte(0) ? abort() : x.log(2))),
  log: (_D, [x, base, ...rest]) => {
    if (!x || rest.length || x.lte(0)) abort()
    if (!base) return x.ln()
    if (base.lte(0) || base.eq(1)) abort()
    return x.ln().div(base.ln())
  },
  abs: one((x) => x.abs()),
  sign: one((x, D) => new D(x.isZero() ? 0 : x.isNeg() ? -1 : 1)),
  floor: one((x) => x.floor()),
  ceil: one((x) => x.ceil()),
  round: (D, [x, digits, ...rest]) => {
    if (!x || rest.length) abort()
    if (!digits) return x.toDecimalPlaces(0, D.ROUND_HALF_UP)
    const p = new D(10).pow(intArg(digits))
    return x.times(p).toDecimalPlaces(0, D.ROUND_HALF_UP).div(p)
  },
  min: (D, args) => (args.length ? D.min(...args) : abort()),
  max: (D, args) => (args.length ? D.max(...args) : abort()),
  mean: (D, args) => (args.length ? D.sum(...args).div(args.length) : abort()),
  total: (D, args) => (args.length ? D.sum(...args) : abort()),
  mod: (_D, args) => (args.length === 2 ? modulo(args[0]!, args[1]!) : abort()),
  factorial: one((x, D) => factorial(D, x)),
  combinations: choose(false),
  permutations: choose(true),
  nCr: choose(false),
  nPr: choose(true),
  ...TRIG,
  arcsin: TRIG.asin!,
  arccos: TRIG.acos!,
  arctan: TRIG.atan!,
  arctan2: TRIG.atan2!,
  arccsc: TRIG.acsc!,
  arcsec: TRIG.asec!,
  arccot: TRIG.acot!,
  sinh: one((x) => bounded(x, 1000).sinh()),
  cosh: one((x) => bounded(x, 1000).cosh()),
  tanh: one((x) => bounded(x, 1000).tanh()),
  csch: one((x) => inverse(bounded(x, 1000).sinh())),
  sech: one((x) => inverse(bounded(x, 1000).cosh())),
  coth: one((x) => inverse(bounded(x, 1000).tanh())),
  asinh: one((x) => x.asinh()),
  acosh: one((x) => (x.lt(1) ? abort() : x.acosh())),
  atanh: one((x) => (x.abs().gte(1) ? abort() : x.atanh())),
  acsch: one((x) => inverse(x).asinh()),
  asech: one((x) => (x.lte(0) || x.gt(1) ? abort() : inverse(x).acosh())),
  acoth: one((x) => (x.abs().lte(1) ? abort() : inverse(x).atanh())),
}

/** Jumps and integer tests; everything else is continuous and must match the engine locally. */
const DISCONTINUOUS = new Set(['floor', 'ceil', 'round', 'sign', 'mod', 'factorial', 'combinations', 'permutations', 'nCr', 'nPr'])

const TRIG_NAMES = new Set(['sin', 'cos', 'tan', 'csc', 'sec', 'cot'])

/** The digits of a double exactly, so the twin sees the very input the engine saw. */
function exactly(D: DecCtor, x: number): Dec {
  if (!Number.isFinite(x)) abort()
  let m = x
  let k = 0
  while (!Number.isInteger(m)) {
    m *= 2
    k++
  }
  // m / 2^k is m * 5^k / 10^k, written out without rounding
  return new D(`${BigInt(m) * 5n ** BigInt(k)}e-${k}`)
}

/**
 * Same meaning, same input: only the engine's own float rounding may separate them. A real
 * mismatch (a factor, a sign, a branch) is far past this. Degree trig also carries the float
 * error of converting to radians, which grows with the angle; past a point it can't be checked.
 */
function agrees(engine: number, twin: number, name: string, x: number, mode: AngleMode): boolean {
  if (engine === twin) return true
  let slack = 1e-12
  // decimal.js reduces huge angles at working precision, so it can't see the double's low digits
  if (TRIG_NAMES.has(name) && Math.abs(x) > 1e15) abort()
  if (mode === 'deg' && TRIG_NAMES.has(name)) {
    const convert = 4e-16 * Math.max(1, Math.abs((x * Math.PI) / 180)) * (1 + twin * twin)
    if (convert > 1e-6) abort()
    slack += convert
  }
  const diff = Math.abs(engine - twin)
  return diff <= 1e-9 * Math.max(Math.abs(engine), Math.abs(twin)) + slack
}

/** Every value carried at two precisions: where they differ is how far the low one can be off. */
interface Pair {
  lo: Dec
  hi: Dec
}

function noise(p: Pair): Dec {
  return p.lo.minus(p.hi).abs().times(1000)
}

/**
 * An inexact value going into a jump (floor, round, mod, an integer test) that is a short decimal
 * to within its noise is taken as that decimal, so `sin(30)*2` meets floor as 1, not 0.99999…
 */
function settle(p: Pair, lo: DecCtor, hi: DecCtor): Pair {
  if (p.lo.eq(p.hi)) return p
  const room = noise(p)
  const at = (c: Dec): Pair => ({ lo: new lo(c), hi: new hi(c) })
  if (p.hi.abs().lte(room)) return at(new hi(0))
  if (room.gt(p.hi.abs().times(1e-20))) return p
  for (let dp = 0; dp <= hi.precision; dp++) {
    const c = p.hi.toDecimalPlaces(dp)
    if (c.minus(p.hi).abs().lte(room)) return c.isInteger() || c.sd() <= 15 ? at(c) : p
  }
  return p
}

function kitFor(D: DecCtor, mode: AngleMode): TwinKit {
  const pi = D.acos(-1)
  const deg = pi.div(180)
  return {
    pi,
    e: D.exp(1),
    toRad: (x) => (mode === 'rad' ? x : x.times(deg)),
    fromRad: (x) => (mode === 'rad' ? x : x.div(deg)),
  }
}

const kits = new Map<string, TwinKit>()
function kit(D: DecCtor, mode: AngleMode): TwinKit {
  const key = `${D.precision}${mode}`
  let k = kits.get(key)
  if (!k) kits.set(key, (k = kitFor(D, mode)))
  return k
}

const OPS_BIG: Record<string, (args: Dec[]) => Dec> = {
  add: ([a, b]) => a!.plus(b!),
  subtract: ([a, b]) => a!.minus(b!),
  multiply: ([a, b]) => a!.times(b!),
  divide: ([a, b]) => (b!.isZero() ? abort() : a!.div(b!)),
  pow: ([a, b]) => {
    if (a!.isNeg() && !b!.isInteger()) abort()
    if (a!.isZero()) return b!.isNeg() ? abort() : b!.isZero() ? new (a!.constructor as DecCtor)(1) : a!
    if (Math.abs(b!.toNumber() * Math.log10(Math.abs(a!.toNumber()))) > 400) abort()
    return a!.pow(b!)
  },
  unaryMinus: ([a]) => a!.neg(),
  unaryPlus: ([a]) => a!,
  factorial: ([a]) => factorial(a!.constructor as DecCtor, a!),
}

function bigBackend(low: number, high: number, mode: AngleMode, deadline: number, scale: { max: number }): Backend<Pair> {
  const L = Decs.get(low)!
  const H = Decs.get(high)!
  const C = Decs.get(CHECK)!
  const kl = kit(L, mode)
  const kh = kit(H, mode)
  const kc = kit(C, mode)
  const engine = engineScope(mode)
  const both = (f: (D: DecCtor, k: TwinKit) => Dec): Pair => ({ lo: f(L, kl), hi: f(H, kh) })
  const jumpArgs = (fn: string, args: Pair[]) => (DISCONTINUOUS.has(fn) ? args.map((a) => settle(a, L, H)) : args)
  const pairMod = ([a, b]: Pair[]): Pair => {
    if (!a || !b) return abort()
    const r = { lo: modulo(a.lo, b.lo), hi: modulo(a.hi, b.hi) }
    // the remainder inherits the divisor's noise times the quotient
    const m = b.hi.abs()
    const slack = a.hi.div(m).abs().times(noise(b)).plus(noise(a))
    if (slack.gt(m.times(1e-6))) abort()
    return r.hi.lte(slack) || m.minus(r.hi).lte(slack) ? both((D) => new D(0)) : r
  }
  return {
    lit: (v) => both((D) => new D(v)),
    fromNumber: (n) => (Number.isFinite(n) ? both((D) => new D(String(n))) : abort()),
    constant: (name) => both((_D, k) => (name === 'pi' ? k.pi : name === 'tau' ? k.pi.times(2) : k.e)),
    op: (fn, args) => {
      const xs = jumpArgs(fn, args)
      if (fn === 'mod') return pairMod(xs)
      return { lo: OPS_BIG[fn]!(xs.map((a) => a.lo)), hi: OPS_BIG[fn]!(xs.map((a) => a.hi)) }
    },
    call: (name, args) => {
      const xs = jumpArgs(name, args)
      if (name === 'mod') return args.length === 2 ? pairMod(xs) : abort()
      const twin = TWINS[name]!
      const out = both((D, k) => twin(D, xs.map((a) => (D === L ? a.lo : a.hi)), k))
      if (!DISCONTINUOUS.has(name)) {
        const nums = xs.map((a) => a.lo.toNumber())
        if (!nums.every(Number.isFinite)) abort()
        const ref = engine[name]!(...nums)
        const same = twin(C, nums.map((x) => exactly(C, x)), kc).toNumber()
        if (typeof ref !== 'number' || !agrees(ref, same, name, nums[0]!, mode)) throw MEANING
      }
      return out
    },
    tick: (v) => {
      if (!v.lo.isFinite() || !v.hi.isFinite() || performance.now() > deadline) abort()
      const mag = Math.abs(v.hi.toNumber())
      if (mag > scale.max) scale.max = mag
      return v
    },
  }
}

export type Verdict =
  | { kind: 'kept'; reason: 'not-a-number' | 'unsupported' | 'meaning' | 'budget' | 'unsettled' | 'range' | 'inexact input' }
  | { kind: 'same' }
  | { kind: 'corrected'; n: number }

type Kept = Extract<Verdict, { kind: 'kept' }>

/** The precise value of `tree`, climbing the ladder until the two precisions agree. */
function precise(tree: MathNode, vars: Record<string, number>, f: number, ctx: ScientificContext, mode: AngleMode, deadline: number): number | Kept {
  const scale = { max: 0 }
  let out: Pair | undefined
  for (const [low, high] of LADDER) {
    try {
      const b = bigBackend(low, high, mode, deadline, scale)
      const env: Record<string, Pair> = {}
      for (const [k, v] of Object.entries(vars)) env[k] = b.fromNumber(v)
      out = walk(tree, b, { vars: env, ctx, depth: 0 })
    } catch (e) {
      if (performance.now() > deadline) return { kind: 'kept', reason: 'budget' }
      return { kind: 'kept', reason: e === MEANING ? 'meaning' : 'unsupported' }
    }
    const { lo, hi } = out
    // rounding error shrinks with precision, so when the low precision agrees with the high one
    // to 6 places, the high one is good to far more places than a double holds
    if (lo.eq(hi) || hi.minus(lo).abs().lte(hi.abs().times(1e-6))) {
      const n = hi.toNumber()
      if (!Number.isFinite(n) || (n === 0 && !hi.isZero())) return { kind: 'kept', reason: 'range' }
      return n + 0
    }
  }
  // zero within its own noise, and the float answer is rounding noise too
  if (out && out.hi.abs().lte(noise(out)) && Math.abs(f) <= 1e-10 * Math.max(1, scale.max)) return 0
  return { kind: 'kept', reason: 'unsettled' }
}

function sigDigits(n: number): number {
  return String(Math.abs(n)).replace(/e.*$/, '').replace('.', '').replace(/^0+/, '').replace(/0+$/, '').length
}

/** Why the float answer stays, or what replaces it. */
export function verdict(text: string, value: Value, ctx: ScientificContext, budgetMs = BUDGET_MS): Verdict {
  if (value.kind !== 'number' || value.unit || !Number.isFinite(value.n)) return { kind: 'kept', reason: 'not-a-number' }
  const deadline = performance.now() + budgetMs
  const mode: AngleMode = ctx.angleMode === 'rad' ? 'rad' : 'deg'
  const f = value.n
  // `ans` stays a name, not pasted digits, so it can be told apart from a typed number
  const bare: ScientificContext = { ...ctx, ans: undefined }
  const vars: Record<string, number> = { ...ctx.variables, ...(ctx.ans !== undefined && { ans: ctx.ans }) }
  let tree: MathNode
  try {
    const expr = preprocessChecked(text, bare)
    if (expr == null) return { kind: 'kept', reason: 'unsupported' }
    tree = parse(expr)
    const t = walk(tree, numberBackend(mode), { vars: { ...vars }, ctx: bare, depth: 0 })
    if (!(t === f || Math.abs(t - f) <= 1e-12 * Math.max(Math.abs(t), Math.abs(f)))) return { kind: 'kept', reason: 'meaning' }
  } catch {
    return { kind: 'kept', reason: 'unsupported' }
  }

  const n = precise(tree, vars, f, bare, mode, deadline)
  if (typeof n !== 'number') return n
  if (n === f) return { kind: 'same' }
  // a computed input like 1/3 is only known to the last bit of its double; the correction must
  // survive that bit moving either way, or `a*3 - 1` would turn a right 0 into -1e-16
  const shaky = Object.keys(vars).filter((k) => sigDigits(vars[k]!) > 15)
  for (const dir of shaky.length ? [1, -1] : []) {
    const nudged = { ...vars }
    for (const k of shaky) nudged[k] = vars[k]! * (1 + dir * Number.EPSILON)
    const m = precise(tree, nudged, f, bare, mode, deadline)
    if (typeof m !== 'number') return m
    if (m !== n && !(Math.abs(m - n) <= 1e-13 * Math.abs(n))) return { kind: 'kept', reason: 'inexact input' }
  }
  return { kind: 'corrected', n }
}

/** The float answer with any float artefact replaced by the true digits. */
export function truthful(text: string, value: Value, ctx: ScientificContext): Value {
  const v = verdict(text, value, ctx)
  return v.kind === 'corrected' ? { ...value, n: v.n } : value
}
