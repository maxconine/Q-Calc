import Decimal from 'decimal.js'

/** An expression in radians, as calculus.ts hands it to the worker. */
export type Ast =
  | { t: 'num'; v: string }
  | { t: 'var' }
  | { t: 'const'; name: 'pi' | 'e' }
  | { t: 'op'; op: '+' | '-' | '*' | '/' | '^' | 'neg'; a: Ast[] }
  | { t: 'fn'; name: string; a: Ast[] }

export type Bound = Ast | 'inf' | '-inf'

export type ClosedFormJob =
  | ({ kind: 'integral'; f: Ast; a: Bound; b: Bound } & JobCheck)
  | ({ kind: 'limit'; f: Ast; to: Bound; side: -1 | 0 | 1 } & JobCheck)
  | ({ kind: 'value'; f: Ast; at: Ast } & JobCheck)

/** The main thread's double answer and its error bound; `rad` says whether sin(1) reads back as radians. */
type JobCheck = { approx: number; err: number; rad: boolean }

type D = Decimal.Constructor
type Dec = Decimal
type Fn = (x: Dec) => Dec

/** Digits the relation search uses, and the stricter digits a found relation must still hold to. */
const SEARCH_DIGITS = 24
const VERIFY_DIGITS = 40

class OutOfTime extends Error {}

type Clock = { deadline: number }

function tick(clock: Clock): void {
  if (Date.now() > clock.deadline) throw new OutOfTime()
}

const clones = new Map<number, D>()

function decimalAt(precision: number): D {
  let c = clones.get(precision)
  if (!c) {
    c = Decimal.clone({ precision, rounding: Decimal.ROUND_HALF_EVEN, toExpNeg: -9e15, toExpPos: 9e15, maxE: 9e15, minE: -9e15 })
    clones.set(precision, c)
  }
  return c
}

const piCache = new Map<D, Dec>()

function piOf(Dc: D): Dec {
  let p = piCache.get(Dc)
  if (!p) {
    p = Dc.acos(-1)
    piCache.set(Dc, p)
  }
  return p
}

function nan(Dc: D): Dec {
  return new Dc(Number.NaN)
}

function compile(ast: Ast, Dc: D): Fn {
  switch (ast.t) {
    case 'num': {
      const v = new Dc(ast.v)
      return () => v
    }
    case 'var':
      return (x) => x
    case 'const': {
      const v = ast.name === 'pi' ? piOf(Dc) : Dc.exp(1)
      return () => v
    }
    case 'op': {
      const [a, b] = ast.a.map((x) => compile(x, Dc))
      switch (ast.op) {
        case '+':
          return (x) => a!(x).plus(b!(x))
        case '-':
          return (x) => a!(x).minus(b!(x))
        case '*':
          return (x) => {
            // e^(-x) sin(x) far out: skip the costly sin of a huge argument once e^(-x) is 0
            const l = a!(x)
            return l.isZero() ? l : l.times(b!(x))
          }
        case '/':
          return (x) => {
            const d = b!(x)
            return d.isZero() ? nan(Dc) : a!(x).div(d)
          }
        case '^':
          return (x) => power(a!(x), b!(x), Dc)
        case 'neg':
          return (x) => a!(x).neg()
      }
      break
    }
    case 'fn':
      return fnOf(ast.name, ast.a.map((x) => compile(x, Dc)), Dc)
  }
  throw new Error('unsupported')
}

function power(base: Dec, ex: Dec, Dc: D): Dec {
  if (base.isZero()) return ex.isPositive() && !ex.isZero() ? new Dc(0) : nan(Dc)
  if (base.isNegative() && !ex.isInteger()) return nan(Dc)
  return base.pow(ex)
}

function fnOf(name: string, args: Fn[], Dc: D): Fn {
  const [a, b] = args
  const one = new Dc(1)
  const guard = (f: (v: Dec) => Dec, ok: (v: Dec) => boolean) => (x: Dec) => {
    const v = a!(x)
    return ok(v) ? f(v) : nan(Dc)
  }
  const inv = (v: Dec) => one.div(v)
  switch (name) {
    case 'sin':
      return (x) => Dc.sin(a!(x))
    case 'cos':
      return (x) => Dc.cos(a!(x))
    case 'tan':
      return (x) => Dc.tan(a!(x))
    case 'sec':
      return (x) => inv(Dc.cos(a!(x)))
    case 'csc':
      return (x) => inv(Dc.sin(a!(x)))
    case 'cot':
      return (x) => inv(Dc.tan(a!(x)))
    case 'asin':
      return guard((v) => Dc.asin(v), (v) => v.abs().lte(1))
    case 'acos':
      return guard((v) => Dc.acos(v), (v) => v.abs().lte(1))
    case 'atan':
      return (x) => Dc.atan(a!(x))
    case 'asec':
      return guard((v) => Dc.acos(inv(v)), (v) => v.abs().gte(1))
    case 'acsc':
      return guard((v) => Dc.asin(inv(v)), (v) => v.abs().gte(1))
    case 'acot':
      return (x) => {
        const v = a!(x)
        return v.isZero() ? piOf(Dc).div(2) : Dc.atan(inv(v))
      }
    case 'sinh':
      return (x) => Dc.sinh(a!(x))
    case 'cosh':
      return (x) => Dc.cosh(a!(x))
    case 'tanh':
      return (x) => Dc.tanh(a!(x))
    case 'sech':
      return (x) => inv(Dc.cosh(a!(x)))
    case 'csch':
      return guard((v) => inv(Dc.sinh(v)), (v) => !v.isZero())
    case 'coth':
      return guard((v) => inv(Dc.tanh(v)), (v) => !v.isZero())
    case 'asinh':
      return (x) => Dc.asinh(a!(x))
    case 'acosh':
      return guard((v) => Dc.acosh(v), (v) => v.gte(1))
    case 'atanh':
      return guard((v) => Dc.atanh(v), (v) => v.abs().lt(1))
    case 'sqrt':
      return guard((v) => v.sqrt(), (v) => !v.isNegative())
    case 'cbrt':
      return (x) => Dc.cbrt(a!(x))
    case 'nthRoot':
      return (x) => {
        const v = a!(x)
        const n = b ? b(x) : new Dc(2)
        if (!n.isInteger() || n.isZero()) return nan(Dc)
        if (v.isNegative()) return n.mod(2).isZero() ? nan(Dc) : v.neg().pow(one.div(n)).neg()
        return v.pow(one.div(n))
      }
    case 'exp':
      return (x) => Dc.exp(a!(x))
    case 'log':
      return guard((v) => Dc.ln(v), (v) => v.isPositive() && !v.isZero())
    case 'log10':
      return guard((v) => Dc.log10(v), (v) => v.isPositive() && !v.isZero())
    case 'abs':
      return (x) => a!(x).abs()
  }
  throw new Error('unsupported')
}

function finite(v: Dec): boolean {
  return v.isFinite()
}

// ---- quadrature

type RuleKind = 'tanh' | 'exp' | 'sinh'
/** A node of a rule on its standard range, before the affine map onto the job's bounds. */
type Std = { p: Dec; q: Dec; w: Dec }

const tables = new Map<string, Std[]>()

/**
 * Standard nodes at t = k·2^-level for k ≥ 1 (odd k only past level 0), built from exp alone since
 * decimal.js's sinh and cosh are slow. tanh: p = distance fraction from the end; exp: p = y, q = 1/y; sinh: p = x.
 */
function stdNode(kind: RuleKind, W: number, level: number, i: number): Std {
  const key = `${kind}:${W}:${level}`
  let list = tables.get(key)
  if (!list) {
    list = []
    tables.set(key, list)
  }
  while (list.length <= i) {
    const Dc = decimalAt(W)
    const halfPi = piOf(Dc).div(2)
    const k = level === 0 ? list.length + 1 : 2 * list.length + 1
    const t = new Dc(k).div(2 ** level)
    const et = Dc.exp(t)
    const sh = et.minus(new Dc(1).div(et)).div(2)
    const ch = et.plus(new Dc(1).div(et)).div(2)
    const eu = Dc.exp(halfPi.times(sh))
    const ieu = new Dc(1).div(eu)
    if (kind === 'tanh') {
      const coshU = eu.plus(ieu).div(2)
      list.push({ p: new Dc(1).div(eu.times(eu).plus(1)), q: new Dc(0), w: halfPi.times(ch).div(coshU.times(coshU)) })
    } else if (kind === 'exp') {
      list.push({ p: eu, q: ieu, w: halfPi.times(ch) })
    } else {
      list.push({ p: eu.minus(ieu).div(2), q: new Dc(0), w: halfPi.times(ch).times(eu.plus(ieu).div(2)) })
    }
  }
  return list[i]!
}

type Node = { x: Dec; w: Dec }

/** Both mirrored nodes of standard node i, mapped onto the job's range; null once past the working precision. */
type Rule = { kind: RuleKind; center: Node; map: (n: Std) => [Node | null, Node | null] }

function tanhSinh(a: Dec, b: Dec, Dc: D, W: number): Rule {
  const halfPi = piOf(Dc).div(2)
  const r = b.minus(a).div(2)
  const floor = new Dc(10).pow(-W).times(Dc.max(a.abs(), b.abs(), 1))
  return {
    kind: 'tanh',
    center: { x: a.plus(b).div(2), w: r.times(halfPi) },
    map: (n) => {
      const dist = r.times(2).times(n.p)
      if (dist.lt(floor)) return [null, null]
      const w = r.times(n.w)
      return [{ x: b.minus(dist), w }, { x: a.plus(dist), w }]
    },
  }
}

/** [a, ∞) as x = a + y, or (-∞, a] as x = a - y, with y = exp(π/2 sinh t). */
function expSinh(a: Dec, flip: boolean, Dc: D, W: number): Rule {
  const halfPi = piOf(Dc).div(2)
  const floor = new Dc(10).pow(-W).times(Dc.max(a.abs(), 1))
  const at = (y: Dec) => (flip ? a.minus(y) : a.plus(y))
  return {
    kind: 'exp',
    center: { x: at(new Dc(1)), w: halfPi },
    map: (n) => [{ x: at(n.p), w: n.w.times(n.p) }, n.q.lt(floor) ? null : { x: at(n.q), w: n.w.times(n.q) }],
  }
}

function sinhSinh(Dc: D): Rule {
  const halfPi = piOf(Dc).div(2)
  return {
    kind: 'sinh',
    center: { x: new Dc(0), w: halfPi },
    map: (n) => [{ x: n.p, w: n.w }, { x: n.p.neg(), w: n.w }],
  }
}

/**
 * Refines until two levels agree. The error roughly squares each level, so a step of 10^-(d/2) between
 * levels already means d good digits once the quadratic regime shows.
 */
function quadrature(f: Fn, rule: Rule, Dc: D, digits: number, W: number, clock: Clock): Dec | null {
  const tiny = new Dc(10).pow(-W)
  const c = f(rule.center.x)
  if (!finite(c)) return null
  let raw = rule.center.w.times(c)
  let l1 = raw.abs()
  const level = (lv: number): boolean => {
    const done = [false, false]
    const small = [0, 0]
    for (let i = 0; !(done[0] && done[1]); i++) {
      tick(clock)
      if ((lv === 0 ? i + 1 : 2 * i + 1) / 2 ** lv > 9) return false
      const pair = rule.map(stdNode(rule.kind, W, lv, i))
      for (let s = 0; s < 2; s++) {
        if (done[s]) continue
        const n = pair[s]
        if (!n) {
          done[s] = true
          continue
        }
        const fx = f(n.x)
        if (!finite(fx)) return false
        const term = n.w.times(fx)
        raw = raw.plus(term)
        l1 = l1.plus(term.abs())
        if (term.abs().lte(tiny.times(l1))) {
          if (++small[s]! >= 2 && (lv === 0 ? i + 1 : 2 * i + 1) > 2 ** lv) done[s] = true
        } else small[s] = 0
      }
    }
    return true
  }
  if (!level(0)) return null
  let h = new Dc(1)
  let prev = raw
  let prevDigits = 0
  for (let lv = 1; lv <= 9; lv++) {
    h = h.div(2)
    if (!level(lv)) return null
    const value = raw.times(h)
    const diff = value.minus(prev).abs()
    const scale = l1.times(h)
    prev = value
    if (diff.isZero()) return value
    const agreed = -diff.div(scale).log(10).toNumber()
    if (lv >= 3 && (agreed >= digits || (agreed >= digits / 2 + 2 && agreed >= 1.6 * prevDigits))) return value
    prevDigits = agreed
  }
  return null
}

function boundOf(b: Bound, Dc: D): Dec | 'inf' | '-inf' | null {
  if (b === 'inf' || b === '-inf') return b
  const v = compile(b, Dc)(new Dc(0))
  return finite(v) ? v : null
}

function integralAt(job: Extract<ClosedFormJob, { kind: 'integral' }>, digits: number, clock: Clock): Dec | null {
  // a 1/√ singularity at a nonzero end loses half the working digits to the sliver past the last node
  const W = 2 * digits + 8
  const Dc = decimalAt(W)
  const f = compile(job.f, Dc)
  const a = boundOf(job.a, Dc)
  const b = boundOf(job.b, Dc)
  if (a == null || b == null) return null
  const rank = (v: Dec | 'inf' | '-inf') => (v === 'inf' ? 1 : v === '-inf' ? -1 : 0)
  if (rank(a) > rank(b) || (rank(a) === 0 && rank(b) === 0 && (a as Dec).gt(b as Dec))) {
    const v = integralAt({ ...job, a: job.b, b: job.a }, digits, clock)
    return v && v.neg()
  }
  let rule: Rule
  if (a === '-inf' && b === 'inf') rule = sinhSinh(Dc)
  else if (b === 'inf' && a !== '-inf' && a !== 'inf') rule = expSinh(a, false, Dc, W)
  else if (a === '-inf' && b !== 'inf' && b !== '-inf') rule = expSinh(b, true, Dc, W)
  else if (a !== 'inf' && a !== '-inf' && b !== 'inf' && b !== '-inf') {
    if (a.eq(b)) return new Dc(0)
    rule = tanhSinh(a, b, Dc, W)
  } else return null
  return quadrature(f, rule, Dc, digits, W, clock)
}

function limitAt(job: Extract<ClosedFormJob, { kind: 'limit' }>, digits: number): Dec | null {
  const W = 2 * digits + 16
  const Dc = decimalAt(W)
  const f = compile(job.f, Dc)
  const to = boundOf(job.to, Dc)
  if (to == null) return null
  const h0 = new Dc(10).pow(-Math.ceil(digits / 3) - 2)
  // richardson over h, h/2, h/4, h/8 removes the h, h², h³ terms
  const extrapolated = (g: (h: Dec) => Dec): Dec | null => {
    let row: Dec[] = []
    for (let i = 0; i < 4; i++) {
      const v = g(h0.div(2 ** i))
      if (!finite(v)) return null
      row.push(v)
    }
    for (let j = 1; j < 4; j++) {
      const fac = 2 ** j
      row = row.slice(1).map((v, i) => v.times(fac).minus(row[i]!).div(fac - 1))
    }
    return row[0]!
  }
  if (to === 'inf' || to === '-inf') {
    const s = to === 'inf' ? 1 : -1
    return extrapolated((h) => f(new Dc(s).div(h)))
  }
  const sides = job.side === 0 ? [1, -1] : [job.side]
  const vals = sides.map((s) => extrapolated((h) => f(to.plus(h.times(s)))))
  if (vals.some((v) => !v)) return null
  if (vals.length === 2 && vals[0]!.minus(vals[1]!).abs().gt(new Dc(10).pow(-(digits - 3)).times(Dc.max(1, vals[0]!.abs())))) return null
  return vals[0]!
}

function valueAt(job: Extract<ClosedFormJob, { kind: 'value' }>, digits: number): Dec | null {
  const Dc = decimalAt(digits + 12)
  const at = compile(job.at, Dc)(new Dc(0))
  if (!finite(at)) return null
  const v = compile(job.f, Dc)(at)
  return finite(v) ? v : null
}

function valueOf(job: ClosedFormJob, digits: number, clock: Clock): Dec | null {
  if (job.kind === 'integral') return integralAt(job, digits, clock)
  if (job.kind === 'limit') return limitAt(job, digits)
  return valueAt(job, digits)
}

// ---- constants

function zeta3(Dc: D, W: number): Dec {
  // ζ(3) = 5/2 Σ (-1)^(k+1) / (k³ C(2k, k))
  let sum = new Dc(0)
  let binom = new Dc(2)
  const tiny = new Dc(10).pow(-W - 2)
  for (let k = 1; k < 10_000; k++) {
    const term = new Dc(1).div(binom.times(k * k * k))
    sum = k % 2 ? sum.plus(term) : sum.minus(term)
    if (term.lt(tiny)) break
    binom = binom.times(2 * (k + 1) * (2 * k + 1)).div((k + 1) * (k + 1))
  }
  return sum.times(5).div(2)
}

function catalan(Dc: D, W: number): Dec {
  // G = π/8 ln(2 + √3) + 3/8 Σ (n!)² / ((2n)! (2n+1)²)
  let sum = new Dc(0)
  let ratio = new Dc(1)
  const tiny = new Dc(10).pow(-W - 2)
  for (let n = 0; n < 10_000; n++) {
    const term = ratio.div((2 * n + 1) * (2 * n + 1))
    sum = sum.plus(term)
    if (term.lt(tiny)) break
    ratio = ratio.times((n + 1) * (n + 1)).div((2 * n + 1) * (2 * n + 2))
  }
  return piOf(Dc).div(8).times(Dc.ln(new Dc(3).sqrt().plus(2))).plus(sum.times(3).div(8))
}

type Atom = {
  id: string
  /** Typeset, and text the engine reads back. */
  text: string
  /** `1/π` is written `p/(qπ)` rather than `(p/q)(1/π)`. */
  inverse?: string
  value: (Dc: D, W: number) => Dec
}

const ln = (n: number) => (Dc: D) => Dc.ln(n)
const root = (n: number) => (Dc: D) => new Dc(n).sqrt()

const ATOMS: Record<string, Atom> = {
  pi: { id: 'pi', text: 'π', value: (Dc) => piOf(Dc) },
  pi2: { id: 'pi2', text: 'π²', value: (Dc) => piOf(Dc).pow(2) },
  pi3: { id: 'pi3', text: 'π³', value: (Dc) => piOf(Dc).pow(3) },
  pi4: { id: 'pi4', text: 'π⁴', value: (Dc) => piOf(Dc).pow(4) },
  invpi: { id: 'invpi', text: '', inverse: 'π', value: (Dc) => new Dc(1).div(piOf(Dc)) },
  invpi2: { id: 'invpi2', text: '', inverse: 'π²', value: (Dc) => new Dc(1).div(piOf(Dc).pow(2)) },
  sqrt2: { id: 'sqrt2', text: '√2', value: root(2) },
  sqrt3: { id: 'sqrt3', text: '√3', value: root(3) },
  sqrt5: { id: 'sqrt5', text: '√5', value: root(5) },
  sqrt6: { id: 'sqrt6', text: '√6', value: root(6) },
  pisqrt2: { id: 'pisqrt2', text: 'π√2', value: (Dc) => piOf(Dc).times(new Dc(2).sqrt()) },
  pisqrt3: { id: 'pisqrt3', text: 'π√3', value: (Dc) => piOf(Dc).times(new Dc(3).sqrt()) },
  sqrtpi: { id: 'sqrtpi', text: '√π', value: (Dc) => piOf(Dc).sqrt() },
  sqrt2pi: { id: 'sqrt2pi', text: '√(2π)', value: (Dc) => piOf(Dc).times(2).sqrt() },
  e: { id: 'e', text: 'e', value: (Dc) => Dc.exp(1) },
  e2: { id: 'e2', text: 'e²', value: (Dc) => Dc.exp(2) },
  inve: { id: 'inve', text: '', inverse: 'e', value: (Dc) => Dc.exp(-1) },
  ln2: { id: 'ln2', text: 'ln(2)', value: ln(2) },
  ln3: { id: 'ln3', text: 'ln(3)', value: ln(3) },
  ln5: { id: 'ln5', text: 'ln(5)', value: ln(5) },
  ln2sq: { id: 'ln2sq', text: 'ln(2)²', value: (Dc) => Dc.ln(2).pow(2) },
  piln2: { id: 'piln2', text: 'π ln(2)', value: (Dc) => piOf(Dc).times(Dc.ln(2)) },
  lnsilver: { id: 'lnsilver', text: 'ln(1 + √2)', value: (Dc) => Dc.ln(new Dc(2).sqrt().plus(1)) },
  zeta3: { id: 'zeta3', text: 'ζ(3)', value: zeta3 },
  catalan: { id: 'catalan', text: 'catalan', value: catalan },
}

/** Tried in order; the first relation that verifies wins, so simpler bases come first. */
const FAMILIES: Array<{ atoms: Array<string | Atom>; bound: number }> = [
  { atoms: [], bound: 1e6 },
  ...['pi', 'pi2', 'invpi', 'sqrt2', 'sqrt3', 'ln2', 'e', 'inve', 'sqrtpi', 'pi3', 'pi4', 'invpi2', 'sqrt5', 'sqrt6', 'pisqrt2', 'pisqrt3', 'sqrt2pi', 'e2', 'ln3', 'ln5', 'ln2sq', 'piln2', 'lnsilver', 'zeta3', 'catalan'].map(
    (a) => ({ atoms: [a], bound: 1e4 }),
  ),
  ...[
    ['pi', 'pi2'],
    ['pi', 'ln2'],
    ['pi', 'sqrt3'],
    ['pi', 'sqrt2'],
    ['pi', 'pisqrt3'],
    ['ln2', 'ln3'],
    ['pi2', 'ln2'],
    ['pi2', 'ln2sq'],
    ['ln2', 'ln2sq'],
    ['pi2', 'zeta3'],
    ['pi', 'catalan'],
    ['e', 'inve'],
    ['e', 'e2'],
    ['sqrt2', 'lnsilver'],
  ].map((atoms) => ({ atoms, bound: 1e3 })),
  { atoms: ['pi', 'pi2', 'ln2', 'ln2sq', 'piln2'], bound: 200 },
]

// ---- PSLQ

/** Integer relation among `xs` with every coefficient at most `bound`, or null. */
export function pslq(xs: Dec[], Dc: D, digits: number, bound: number, clock: Clock): number[] | null {
  const n = xs.length
  if (n < 2) return null
  const zero = new Dc(0)
  const gamma = new Dc(4).div(3).sqrt()
  const s: Dec[] = []
  for (let k = 0; k < n; k++) {
    let acc = zero
    for (let j = k; j < n; j++) acc = acc.plus(xs[j]!.pow(2))
    s[k] = acc.sqrt()
  }
  if (s[0]!.isZero()) return null
  const y = xs.map((v) => v.div(s[0]!))
  const ss = s.map((v) => v.div(s[0]!))
  if (ss.some((v, i) => i < n && v.isZero())) {
    // an exact zero among the tail means a trivial relation; the basis is degenerate
    return null
  }
  const H: Dec[][] = Array.from({ length: n }, () => Array.from({ length: n - 1 }, () => zero))
  for (let i = 0; i < n; i++) {
    for (let j = 0; j < Math.min(i, n - 1); j++) H[i]![j] = y[i]!.neg().times(y[j]!).div(ss[j]!.times(ss[j + 1]!))
    if (i < n - 1) H[i]![i] = ss[i + 1]!.div(ss[i]!)
  }
  const A: Dec[][] = Array.from({ length: n }, (_, i) => Array.from({ length: n }, (_, j) => new Dc(i === j ? 1 : 0)))
  const B: Dec[][] = A.map((r) => r.slice())
  const reduce = (i: number, j: number) => {
    if (H[j]![j]!.isZero()) return
    const q = H[i]![j]!.div(H[j]![j]!).round()
    if (q.isZero()) return
    y[j] = y[j]!.plus(q.times(y[i]!))
    for (let k = 0; k <= j; k++) H[i]![k] = H[i]![k]!.minus(q.times(H[j]![k]!))
    for (let k = 0; k < n; k++) {
      A[i]![k] = A[i]![k]!.minus(q.times(A[j]![k]!))
      B[k]![j] = B[k]![j]!.plus(q.times(B[k]![i]!))
    }
  }
  for (let i = 1; i < n; i++) for (let j = i - 1; j >= 0; j--) reduce(i, j)
  const eps = new Dc(10).pow(-(digits - 5))
  for (let iter = 0; iter < 400 * n; iter++) {
    tick(clock)
    let m = 0
    let best = new Dc(-1)
    let g = gamma
    for (let i = 0; i < n - 1; i++, g = g.times(gamma)) {
      const v = g.times(H[i]![i]!.abs())
      if (v.gt(best)) {
        best = v
        m = i
      }
    }
    ;[y[m], y[m + 1]] = [y[m + 1]!, y[m]!]
    ;[A[m], A[m + 1]] = [A[m + 1]!, A[m]!]
    ;[H[m], H[m + 1]] = [H[m + 1]!, H[m]!]
    for (const r of B) [r[m], r[m + 1]] = [r[m + 1]!, r[m]!]
    if (m < n - 2) {
      const t0 = H[m]![m]!.pow(2).plus(H[m]![m + 1]!.pow(2)).sqrt()
      if (t0.isZero()) return null
      const c = H[m]![m]!.div(t0)
      const sn = H[m]![m + 1]!.div(t0)
      for (let i = m; i < n; i++) {
        const t3 = H[i]![m]!
        const t4 = H[i]![m + 1]!
        H[i]![m] = c.times(t3).plus(sn.times(t4))
        H[i]![m + 1] = sn.neg().times(t3).plus(c.times(t4))
      }
    }
    for (let i = m + 1; i < n; i++) for (let j = Math.min(i - 1, m + 1); j >= 0; j--) reduce(i, j)
    for (let j = 0; j < n; j++) {
      if (y[j]!.abs().lt(eps)) {
        const rel = B.map((r) => r[j]!.toNumber())
        return rel.every((c) => Math.abs(c) <= bound) ? rel : null
      }
    }
    // no relation this small exists once 1/max|H_jj| passes the bound
    let maxDiag = zero
    for (let j = 0; j < n - 1; j++) if (H[j]![j]!.abs().gt(maxDiag)) maxDiag = H[j]![j]!.abs()
    if (maxDiag.isZero() || new Dc(1).div(maxDiag).gt(bound * Math.sqrt(n))) return null
  }
  return null
}

// ---- formatting

function gcd(a: number, b: number): number {
  a = Math.abs(a)
  b = Math.abs(b)
  while (b) [a, b] = [b, a % b]
  return a || 1
}

type Term = { p: number; q: number; atom: Atom | null }

function termText(t: Term): string {
  const p = Math.abs(t.p)
  if (!t.atom) return t.q === 1 ? `${p}` : `${p}/${t.q}`
  if (t.atom.inverse) {
    const inv = t.atom.inverse
    const den = t.q === 1 ? inv : `(${t.q}${/^[a-z]/i.test(inv) ? ' ' : ''}${inv})`
    return `${p}/${den}`
  }
  const lead = /^[a-z]/i.test(t.atom.text) && p !== 1 ? `${p} ` : p === 1 ? '' : `${p}`
  const body = `${lead}${t.atom.text}`
  return t.q === 1 ? body : `${body}/${t.q}`
}

/** x = -(c1·1 + c2·a2 + ...)/c0, as `2 - π²/6`. */
function formatRelation(rel: number[], atoms: Atom[]): string | null {
  const c0 = rel[0]!
  if (!c0) return null
  const terms: Term[] = []
  rel.slice(1).forEach((c, i) => {
    if (!c) return
    let p = -c
    let q = c0
    if (q < 0) {
      p = -p
      q = -q
    }
    const g = gcd(p, q)
    terms.push({ p: p / g, q: q / g, atom: i === 0 ? null : atoms[i - 1]! })
  })
  if (!terms.length) return '0'
  const firstPos = terms.findIndex((t) => t.p > 0)
  if (terms[0]!.p < 0 && firstPos > 0) terms.unshift(...terms.splice(firstPos, 1))
  return terms.map((t, i) => (i === 0 ? (t.p < 0 ? '-' : '') : t.p < 0 ? ' - ' : ' + ') + termText(t)).join('')
}

// ---- identify

function holds(rel: number[], values: Dec[], Dc: D, digits: number): boolean {
  let sum = new Dc(0)
  let scale = new Dc(0)
  rel.forEach((c, i) => {
    const t = values[i]!.times(c)
    sum = sum.plus(t)
    if (t.abs().gt(scale)) scale = t.abs()
  })
  return sum.abs().lte(scale.times(new Dc(10).pow(-(digits - 5))))
}

/** The closed form of the job's exact value, verified at higher precision; null when none is found. */
export function identifyJob(job: ClosedFormJob, budgetMs = 8000): string | null {
  const clock = { deadline: Date.now() + budgetMs }
  try {
    const x = valueOf(job, VERIFY_DIGITS, clock)
    if (!x || x.isZero()) return null
    // the double answer is computed independently; disagreeing means this isn't the same number
    if (Math.abs(x.toNumber() - job.approx) > 10 * job.err + 1e-12 * Math.abs(job.approx)) return null
    const W1 = SEARCH_DIGITS + 10
    const D1 = decimalAt(W1)
    const W2 = VERIFY_DIGITS + 10
    const D2 = decimalAt(W2)
    const x1 = new D1(x.toSignificantDigits(W1))
    const x2 = new D2(x.toSignificantDigits(W2))
    for (const fam of [...FAMILIES, ...contextFamilies(job)]) {
      const atoms = fam.atoms.map((a) => (typeof a === 'string' ? ATOMS[a]! : a))
      const basis = [x1, new D1(1), ...atoms.map((a) => a.value(D1, W1))]
      const rel = pslq(basis, D1, SEARCH_DIGITS, fam.bound, clock)
      if (!rel || !rel[0]) continue
      const values = [x2, new D2(1), ...atoms.map((a) => a.value(D2, W2))]
      if (!holds(rel, values, D2, VERIFY_DIGITS)) continue
      return formatRelation(rel, atoms)
    }
    return null
  } catch (e) {
    if (e instanceof OutOfTime) return null
    return null
  }
}

function mentionsFn(ast: Ast, names: string[]): boolean {
  if (ast.t === 'fn' && names.includes(ast.name)) return true
  if (ast.t === 'op' && ast.op === '^' && ast.a[0]!.t === 'const' && ast.a[0]!.name === 'e' && names.includes('exp')) return true
  return (ast.t === 'op' || ast.t === 'fn') && ast.a.some((a) => mentionsFn(a, names))
}

/** Constants the integrand itself suggests: sin(1) and cos(1) for ∫0..1 sin(x), ln(10) for ∫1..10 1/x. */
function contextFamilies(job: ClosedFormJob): Array<{ atoms: Atom[]; bound: number }> {
  const ends = job.kind === 'integral' ? [job.a, job.b] : job.kind === 'limit' ? [job.to] : [job.at]
  const out: Array<{ atoms: Atom[]; bound: number }> = []
  for (const end of ends) {
    if (typeof end === 'string' || end.t !== 'num') continue
    const c = Number(end.v)
    if (!Number.isInteger(c) || c === 0 || Math.abs(c) > 100) continue
    if (job.rad && mentionsFn(job.f, ['sin', 'cos', 'tan', 'sec', 'csc', 'cot'])) {
      out.push({
        atoms: [
          { id: `sin${c}`, text: `sin(${c})`, value: (Dc) => Dc.sin(c) },
          { id: `cos${c}`, text: `cos(${c})`, value: (Dc) => Dc.cos(c) },
        ],
        bound: 1e3,
      })
    }
    if (mentionsFn(job.f, ['exp']) && Math.abs(c) !== 1 && Math.abs(c) !== 2) {
      out.push({ atoms: [{ id: `exp${c}`, text: `e^${c < 0 ? `(${c})` : c}`, value: (Dc) => Dc.exp(c) }], bound: 1e3 })
    }
    if ((job.kind === 'integral' || mentionsFn(job.f, ['log', 'log10'])) && c > 1 && ![2, 3, 5].includes(c)) {
      out.push({ atoms: [{ id: `ln${c}`, text: `ln(${c})`, value: (Dc) => Dc.ln(c) }], bound: 1e3 })
    }
  }
  return out
}
