import type { MathNode } from 'mathjs'
import { formatValue } from './format'
import { math } from './math'
import { readsAsUnit, siQuantity, siValue, type DefaultUnits, type SiQuantity } from './units'

/** A solved linear system, up to five equations. */
export interface SystemResult {
  status: 'unique' | 'finite' | 'infinite' | 'inconsistent'
  variables: string[]
  /** Exact values, one list per solution, parallel to `variables`. */
  solutions?: string[][]
  /** Infinite: each variable as an expression in the free names. */
  general?: string[]
  free?: string[]
  /** Infinite: just the variables the equations already fix, e.g. `z = 1`. */
  pinned?: string
  /** Linear solves in sin/cos (or similar) before inverting. */
  atoms?: { expr: string; value: string }[]
  /** Angles in [0, 2π) for a trig unknown. */
  angles?: Record<string, string[]>
  display: string
}

type Rat = { n: bigint; d: bigint }

const RESERVED = new Set(['pi', 'i', 'tau', 'infinity', 'inf'])

function bgcd(a: bigint, b: bigint): bigint {
  a = a < 0n ? -a : a
  b = b < 0n ? -b : b
  while (b) [a, b] = [b, a % b]
  return a || 1n
}

function rat(n: bigint, d: bigint = 1n): Rat {
  if (d < 0n) {
    n = -n
    d = -d
  }
  if (d === 0n) throw new Error('zero denominator')
  const g = bgcd(n, d)
  return { n: n / g, d: d / g }
}

const R0 = rat(0n)
const R1 = rat(1n)

function radd(a: Rat, b: Rat): Rat {
  return rat(a.n * b.d + b.n * a.d, a.d * b.d)
}
function rsub(a: Rat, b: Rat): Rat {
  return rat(a.n * b.d - b.n * a.d, a.d * b.d)
}
function rmul(a: Rat, b: Rat): Rat {
  return rat(a.n * b.n, a.d * b.d)
}
function rdiv(a: Rat, b: Rat): Rat {
  return rat(a.n * b.d, a.d * b.n)
}
function rneg(a: Rat): Rat {
  return rat(-a.n, a.d)
}
function rzero(a: Rat): boolean {
  return a.n === 0n
}
function req(a: Rat, b: Rat): boolean {
  return a.n === b.n && a.d === b.d
}
function rpow(a: Rat, k: number): Rat {
  if (k < 0) return rpow(rat(a.d, a.n), -k)
  let n = 1n
  let d = 1n
  for (let i = 0; i < k; i++) {
    n *= a.n
    d *= a.d
  }
  return rat(n, d)
}
function rstr(a: Rat): string {
  if (a.d === 1n) return a.n.toString()
  return `${a.n}/${a.d}`
}

function intOf(node: MathNode): bigint | null {
  if (node.type !== 'ConstantNode') return null
  const v = (node as unknown as { value: number }).value
  if (typeof v !== 'number' || !Number.isSafeInteger(v)) return null
  return BigInt(v)
}

type Aff<C> = { k: C; v: Map<string, C> }

interface Field<C> {
  zero: C
  one: C
  add: (a: C, b: C) => C
  sub: (a: C, b: C) => C
  mul: (a: C, b: C) => C
  div: (a: C, b: C) => C
  neg: (a: C) => C
  isZero: (a: C) => boolean
  str: (a: C) => string
}

function aff<C>(F: Field<C>, k: C = F.zero): Aff<C> {
  return { k, v: new Map() }
}
function isConst<C>(a: Aff<C>, F: Field<C>): boolean {
  for (const c of a.v.values()) if (!F.isZero(c)) return false
  return true
}
function addA<C>(a: Aff<C>, b: Aff<C>, F: Field<C>): Aff<C> {
  const v = new Map(a.v)
  for (const [name, c] of b.v) v.set(name, F.add(v.get(name) ?? F.zero, c))
  return { k: F.add(a.k, b.k), v }
}
function scaleA<C>(a: Aff<C>, c: C, F: Field<C>): Aff<C> {
  const v = new Map<string, C>()
  for (const [name, k] of a.v) v.set(name, F.mul(k, c))
  return { k: F.mul(a.k, c), v }
}
function subA<C>(a: Aff<C>, b: Aff<C>, F: Field<C>): Aff<C> {
  return addA(a, scaleA(b, F.neg(F.one), F), F)
}

const RAT: Field<Rat> = {
  zero: R0,
  one: R1,
  add: radd,
  sub: rsub,
  mul: rmul,
  div: rdiv,
  neg: rneg,
  isZero: rzero,
  str: rstr,
}

function unwrap(node: MathNode): MathNode {
  return node.type === 'ParenthesisNode' ? unwrap((node as unknown as { content: MathNode }).content) : node
}

function fnName(node: MathNode): string | null {
  if (node.type !== 'FunctionNode') return null
  const fn = (node as unknown as { fn: { name?: string } | string }).fn
  return typeof fn === 'string' ? fn : (fn.name ?? null)
}

function argsOf(node: MathNode): MathNode[] {
  return ((node as unknown as { args?: MathNode[] }).args ?? []).map(unwrap)
}

/** Constant rational, or null when the node isn't one. */
function constRat(node: MathNode): Rat | null {
  node = unwrap(node)
  const n = intOf(node)
  if (n != null) return rat(n)
  if (node.type !== 'OperatorNode') return null
  const op = (node as unknown as { op: string }).op
  const args = argsOf(node)
  if (op === '-' && args.length === 1) {
    const a = constRat(args[0]!)
    return a && rneg(a)
  }
  if (op === '+' && args.length === 1) return constRat(args[0]!)
  if (args.length !== 2) return null
  const a = constRat(args[0]!)
  const b = constRat(args[1]!)
  if (!a || !b) return null
  if (op === '+') return radd(a, b)
  if (op === '-') return rsub(a, b)
  if (op === '*') return rmul(a, b)
  if (op === '/') return rzero(b) ? null : rdiv(a, b)
  if (op === '^') {
    if (b.d !== 1n || b.n < 0n || b.n > 12n) return null
    return rpow(a, Number(b.n))
  }
  return null
}

function factorize(n: bigint): Array<[bigint, number]> {
  if (n < 0n) n = -n
  const out: Array<[bigint, number]> = []
  let p = 2n
  while (p * p <= n) {
    let e = 0
    while (n % p === 0n) {
      n /= p
      e++
    }
    if (e) out.push([p, e])
    p = p === 2n ? 3n : p + 2n
  }
  if (n > 1n) out.push([n, 1])
  return out
}

/** Square-free radical: rational coefficient times sqrt(rad). rad = 1 means rational. */
type Rad = { c: Rat; rad: bigint }

function radNorm(n: bigint, d: bigint): Rad {
  // sqrt(n/d) = sqrt(n*d)/d, then pull squares out of n*d
  if (n < 0n) throw new Error('negative radical')
  let m = n * d
  let coeffN = 1n
  let rad = 1n
  for (const [p, e] of factorize(m)) {
    const pairs = Math.floor(e / 2)
    const odd = e % 2
    for (let i = 0; i < pairs; i++) coeffN *= p
    if (odd) rad *= p
  }
  return { c: rat(coeffN, d), rad }
}

function sqrtRat(a: Rat): Rad | null {
  if (a.n < 0n) return null
  return radNorm(a.n, a.d)
}

class RadField implements Field<Rat[]> {
  primes: bigint[] = []
  zero: Rat[] = [R0]
  one: Rat[] = [R1]

  private idx(rad: bigint): number {
    if (rad === 1n) return 0
    const bits = factorize(rad)
    let mask = 0
    for (const [p] of bits) {
      let i = this.primes.indexOf(p)
      if (i < 0) {
        i = this.primes.length
        this.primes.push(p)
        // old vectors stay at the same masks; the new bit is the high one
        this.zero = this.embed(this.zero)
        this.one = this.embed(this.one)
      }
      mask |= 1 << i
    }
    return mask
  }

  private embed(v: Rat[]): Rat[] {
    const n = 1 << this.primes.length
    const out = Array.from({ length: n }, () => R0)
    v.forEach((c, i) => {
      out[i] = c
    })
    return out
  }

  fromSqrt(a: Rat): Rat[] | null {
    const s = sqrtRat(a)
    if (!s) return null
    const i = this.idx(s.rad)
    const out = Array.from({ length: 1 << this.primes.length }, () => R0)
    out[i] = s.c
    return this.embed(out)
  }

  fromRat(a: Rat): Rat[] {
    this.idx(1n)
    const out = Array.from({ length: 1 << this.primes.length }, () => R0)
    out[0] = a
    return out
  }

  add = (a: Rat[], b: Rat[]): Rat[] => {
    const n = 1 << this.primes.length
    const A = this.fit(a)
    const B = this.fit(b)
    return Array.from({ length: n }, (_, i) => radd(A[i] ?? R0, B[i] ?? R0))
  }
  sub = (a: Rat[], b: Rat[]): Rat[] => this.add(a, this.neg(b))
  neg = (a: Rat[]): Rat[] => this.fit(a).map(rneg)
  isZero = (a: Rat[]): boolean => this.fit(a).every(rzero)

  private fit(a: Rat[]): Rat[] {
    const n = 1 << this.primes.length
    if (a.length === n) return a
    return this.embed(a)
  }

  mul = (a: Rat[], b: Rat[]): Rat[] => {
    const n = 1 << this.primes.length
    const A = this.fit(a)
    const B = this.fit(b)
    const out = Array.from({ length: n }, () => R0)
    for (let i = 0; i < n; i++) {
      if (rzero(A[i]!)) continue
      for (let j = 0; j < n; j++) {
        if (rzero(B[j]!)) continue
        let mask = 0
        let coeff = rmul(A[i]!, B[j]!)
        for (let bit = 0; bit < this.primes.length; bit++) {
          const ba = (i >> bit) & 1
          const bb = (j >> bit) & 1
          if (ba && bb) coeff = rmul(coeff, rat(this.primes[bit]!))
          else if (ba !== bb) mask |= 1 << bit
        }
        out[mask] = radd(out[mask]!, coeff)
      }
    }
    return out
  }

  div = (a: Rat[], b: Rat[]): Rat[] => {
    const n = 1 << this.primes.length
    const B = this.fit(b)
    const A = this.fit(a)
    // solve b * x = a over the basis
    const M: Rat[][] = Array.from({ length: n }, () => Array.from({ length: n + 1 }, () => R0))
    const basis = Array.from({ length: n }, (_, i) => {
      const e = Array.from({ length: n }, () => R0)
      e[i] = R1
      return e
    })
    for (let j = 0; j < n; j++) {
      const col = this.mul(B, basis[j]!)
      for (let i = 0; i < n; i++) M[i]![j] = col[i]!
    }
    for (let i = 0; i < n; i++) M[i]![n] = A[i]!
    const solved = solveRat(M)
    if (solved.status !== 'unique' || !solved.solutions) throw new Error('singular radical')
    return solved.solutions[0]!.map((s) => {
      const [n0, d0] = s.split('/')
      return d0 ? rat(BigInt(n0!), BigInt(d0)) : rat(BigInt(s))
    })
  }

  str = (a: Rat[]): string => radStr(this.fit(a), this.primes)
}

function radStr(coeffs: Rat[], primes: bigint[]): string {
  const parts: { sign: number; body: string }[] = []
  coeffs.forEach((c, mask) => {
    if (rzero(c)) return
    const rad = primes.reduce((p, prime, bit) => ((mask >> bit) & 1 ? p * prime : p), 1n)
    const unit = rad === 1n ? '' : `sqrt(${rad})`
    const neg = c.n < 0n
    const mag = rat(neg ? -c.n : c.n, c.d)
    let body: string
    if (unit === '') body = rstr(mag)
    else if (req(mag, R1)) body = unit
    else body = `${rstr(mag)}*${unit}`
    parts.push({ sign: neg ? -1 : 1, body })
  })
  if (!parts.length) return '0'
  return parts
    .map((p, i) => (i === 0 ? (p.sign < 0 ? `-${p.body}` : p.body) : `${p.sign < 0 ? ' - ' : ' + '}${p.body}`))
    .join('')
}

type Mono = Map<string, bigint>

function monoKey(exps: number[]): string {
  const e = exps.slice()
  while (e.length && e[e.length - 1] === 0) e.pop()
  return e.length ? e.join(',') : ''
}
function keyExps(key: string): number[] {
  return key === '' ? [] : key.split(',').map(Number)
}

function pzero(): Mono {
  return new Map()
}
function pconst(n: bigint): Mono {
  return n === 0n ? pzero() : new Map([['', n]])
}
function padd(a: Mono, b: Mono): Mono {
  const o = new Map(a)
  for (const [k, c] of b) {
    const s = (o.get(k) ?? 0n) + c
    if (s === 0n) o.delete(k)
    else o.set(k, s)
  }
  return o
}
function pscale(a: Mono, c: bigint): Mono {
  if (c === 0n) return pzero()
  const o = new Map<string, bigint>()
  for (const [k, v] of a) o.set(k, v * c)
  return o
}
/** Divide every coefficient by `c`. `c` is a content gcd, so it divides each one. */
function pdivc(a: Mono, c: bigint): Mono {
  if (c === 1n || c === 0n) return a
  const o = new Map<string, bigint>()
  for (const [k, v] of a) o.set(k, v / c)
  return o
}
function pmul(a: Mono, b: Mono): Mono {
  const o: Mono = new Map()
  for (const [ka, ca] of a) {
    for (const [kb, cb] of b) {
      const ea = keyExps(ka)
      const eb = keyExps(kb)
      const n = Math.max(ea.length, eb.length)
      const exps = Array.from({ length: n }, (_, i) => (ea[i] ?? 0) + (eb[i] ?? 0))
      while (exps.length && exps[exps.length - 1] === 0) exps.pop()
      const key = exps.length ? monoKey(exps) : ''
      const s = (o.get(key) ?? 0n) + ca * cb
      if (s === 0n) o.delete(key)
      else o.set(key, s)
    }
  }
  return o
}
function pneg(a: Mono): Mono {
  return pscale(a, -1n)
}
function pempty(a: Mono): boolean {
  return a.size === 0
}
function pcontent(a: Mono): bigint {
  let g = 0n
  for (const c of a.values()) g = bgcd(g, c)
  return g || 1n
}

type RF = { n: Mono; d: Mono }

function rfNorm(n: Mono, d: Mono): RF {
  if (pempty(n)) return { n: pzero(), d: pconst(1n) }
  if (pempty(d)) throw new Error('zero denominator')
  const cn = pcontent(n)
  const cd = pcontent(d)
  let nn = pdivc(n, cn)
  let dd = pdivc(d, cd)
  const lead = [...dd.values()][0]!
  if (lead < 0n) {
    nn = pneg(nn)
    dd = pneg(dd)
  }
  const g = bgcd(cn, cd)
  // content already removed from nn/dd; scale back the uncancelled content into num
  const numC = cn / g
  const denC = cd / g
  nn = pscale(nn, numC)
  dd = pscale(dd, denC)
  const lead2 = [...dd.values()][0]!
  if (lead2 < 0n) {
    nn = pneg(nn)
    dd = pneg(dd)
  }
  if (dd.size === 1 && dd.get('') === 1n) return { n: nn, d: pconst(1n) }
  return { n: nn, d: dd }
}

function polyGcd1(a: Mono, b: Mono): Mono {
  // univariate Euclid. keys are single exponents "e" or ""
  const deg = (p: Mono) => {
    let m = -1
    for (const k of p.keys()) m = Math.max(m, k === '' ? 0 : Number(k))
    return m
  }
  const coef = (p: Mono, e: number) => p.get(e === 0 ? '' : String(e)) ?? 0n
  const trim = (p: Mono): Mono => {
    const o = new Map(p)
    for (const [k, c] of o) if (c === 0n) o.delete(k)
    return o
  }
  let A = trim(pdivc(a, pcontent(a)))
  let B = trim(pdivc(b, pcontent(b)))
  if (deg(A) < deg(B)) [A, B] = [B, A]
  while (!pempty(B)) {
    const db = deg(B)
    const lb = coef(B, db)
    let R = new Map(A)
    while (!pempty(R) && deg(R) >= db) {
      const dr = deg(R)
      const lr = coef(R, dr)
      const shift = dr - db
      // R -= (lr/lb) * x^shift * B, clearing denominators by multiplying through later
      // work in integers: R = lb*R - lr * x^shift * B, then content-reduce
      R = pscale(R, lb)
      for (let e = 0; e <= db; e++) {
        const c = coef(B, e)
        if (c === 0n) continue
        const key = e + shift === 0 ? '' : String(e + shift)
        const s = (R.get(key) ?? 0n) - lr * c
        if (s === 0n) R.delete(key)
        else R.set(key, s)
      }
      R = trim(pdivc(R, pcontent(R)))
    }
    A = B
    B = R
  }
  return pdivc(A, pcontent(A))
}

function rfCancel(r: RF, params: string[]): RF {
  let out = rfNorm(r.n, r.d)
  if (params.length === 1 && !pempty(out.n) && !(out.d.size === 1 && out.d.get('') != null)) {
    const g = polyGcd1(out.n, out.d)
    if (degOf(g) > 0) {
      out = rfNorm(pdiv1(out.n, g), pdiv1(out.d, g))
    }
  }
  return out
}

function degOf(p: Mono): number {
  let m = -1
  for (const k of p.keys()) m = Math.max(m, k === '' ? 0 : Number(k))
  return pempty(p) ? -1 : m
}

function pdiv1(a: Mono, b: Mono): Mono {
  // exact quotient, univariate
  const coef = (p: Mono, e: number) => p.get(e === 0 ? '' : String(e)) ?? 0n
  let R = new Map(a)
  const db = degOf(b)
  const lb = coef(b, db)
  const q: Mono = new Map()
  while (!pempty(R) && degOf(R) >= db) {
    const dr = degOf(R)
    const lr = coef(R, dr)
    if (lr % lb !== 0n) return a
    const qc = lr / lb
    const shift = dr - db
    q.set(shift === 0 ? '' : String(shift), qc)
    for (let e = 0; e <= db; e++) {
      const c = coef(b, e)
      if (c === 0n) continue
      const key = e + shift === 0 ? '' : String(e + shift)
      const s = (R.get(key) ?? 0n) - qc * c
      if (s === 0n) R.delete(key)
      else R.set(key, s)
    }
  }
  return pempty(R) ? q : a
}

function rfAdd(a: RF, b: RF): RF {
  return rfNorm(padd(pmul(a.n, b.d), pmul(b.n, a.d)), pmul(a.d, b.d))
}
function rfSub(a: RF, b: RF): RF {
  return rfAdd(a, { n: pneg(b.n), d: b.d })
}
function rfMul(a: RF, b: RF): RF {
  return rfNorm(pmul(a.n, b.n), pmul(a.d, b.d))
}
function rfDiv(a: RF, b: RF): RF {
  return rfNorm(pmul(a.n, b.d), pmul(a.d, b.n))
}
function rfNeg(a: RF): RF {
  return { n: pneg(a.n), d: a.d }
}
function rfZero(a: RF): boolean {
  return pempty(a.n)
}

function polyStr(p: Mono, params: string[]): string {
  if (pempty(p)) return '0'
  const parts: { sign: number; body: string }[] = []
  const keys = [...p.keys()].sort((a, b) => keyExps(a).reduce((s, x) => s + x, 0) - keyExps(b).reduce((s, x) => s + x, 0) || a.localeCompare(b))
  for (const key of keys) {
    const c = p.get(key)!
    const exps = key === '' ? [] : keyExps(key)
    const factors: string[] = []
    exps.forEach((e, i) => {
      if (!e) return
      factors.push(e === 1 ? params[i]! : `${params[i]}^${e}`)
    })
    const mon = factors.join('*')
    const neg = c < 0n
    const mag = neg ? -c : c
    let body: string
    if (!mon) body = mag.toString()
    else if (mag === 1n) body = mon
    else body = `${mag}*${mon}`
    parts.push({ sign: neg ? -1 : 1, body })
  }
  return parts
    .map((part, i) => (i === 0 ? (part.sign < 0 ? `-${part.body}` : part.body) : `${part.sign < 0 ? ' - ' : ' + '}${part.body}`))
    .join('')
}

function rfStr(a: RF, params: string[]): string {
  const n = polyStr(a.n, params)
  const d = polyStr(a.d, params)
  if (d === '1') return n
  const num = /[+-]/.test(n.slice(1)) ? `(${n})` : n
  const den = /[+-]/.test(d.slice(1)) || d.includes('*') ? `(${d})` : d
  const raw = `${num}/${den}`
  try {
    return math.simplify(raw).toString().replace(/\s+/g, ' ')
  } catch {
    return raw
  }
}

function makeRF(params: string[]): Field<RF> {
  return {
    zero: { n: pzero(), d: pconst(1n) },
    one: { n: pconst(1n), d: pconst(1n) },
    add: (a, b) => rfCancel(rfAdd(a, b), params),
    sub: (a, b) => rfCancel(rfSub(a, b), params),
    mul: (a, b) => rfCancel(rfMul(a, b), params),
    div: (a, b) => rfCancel(rfDiv(a, b), params),
    neg: rfNeg,
    isZero: rfZero,
    str: (a) => rfStr(rfCancel(a, params), params),
  }
}

function exprStr<C>(k: C, terms: Array<[string, C]>, F: Field<C>): string {
  const parts: string[] = []
  if (!F.isZero(k)) parts.push(F.str(k))
  for (const [name, c] of terms) {
    if (F.isZero(c)) continue
    const s = F.str(c)
    if (s === '1') parts.push(name)
    else if (s === '-1') parts.push(`-${name}`)
    else if (s.startsWith('-')) parts.push(`-${wrap(s.slice(1))}*${name}`)
    else parts.push(`${wrap(s)}*${name}`)
  }
  if (!parts.length) return '0'
  let out = parts[0]!
  for (const p of parts.slice(1)) out += p.startsWith('-') ? ` - ${p.slice(1)}` : ` + ${p}`
  return out
}

function wrap(s: string): string {
  if (/^[-\d/]+$/.test(s) || /^sqrt\(\d+\)$/.test(s)) return s
  return `(${s})`
}

type Parsed = { lhs: MathNode; rhs: MathNode; diff: MathNode }

function prepEq(text: string): Parsed | null {
  let s = text.trim()
  if (!s) return null
  s = s.replace(/θ/g, 'theta').replace(/φ/g, 'phi')
  s = s.replace(/(?<![\d)\]!])\|([^|]+)\|/g, 'abs($1)')
  s = s.replace(/(\d+)\.(\d+)/g, (_, w: string, f: string) => `(${w}${f}/1${'0'.repeat(f.length)})`)
  // `2x`, `2sin(x)`, `sqrt(2)x` — mathjs does not read juxtaposition
  s = s.replace(/(\d)(?![eE][+-]?\d)(?=[A-Za-z(])/g, '$1*')
  s = s.replace(/\)(?=[A-Za-z(])/g, ')*')
  const parts = s.split('=')
  if (parts.length !== 2) return null
  const lhs = parts[0]!.trim()
  const rhs = parts[1]!.trim()
  if (!lhs || !rhs) return null
  try {
    return { lhs: math.parse(lhs), rhs: math.parse(rhs), diff: math.parse(`(${lhs})-(${rhs})`) }
  } catch {
    return null
  }
}

function symbolsOf(node: MathNode, into: string[] = []): string[] {
  node.traverse((n: MathNode, path: string, parent: MathNode | null) => {
    if (n.type !== 'SymbolNode' || (parent?.type === 'FunctionNode' && path === 'fn')) return
    const name = (n as unknown as { name: string }).name
    if (!RESERVED.has(name) && !into.includes(name)) into.push(name)
  })
  return into
}

function preferred(name: string): boolean {
  return /^(x|y|z|w|theta|phi)$/.test(name) || /^[A-Z]$/.test(name) || /\d/.test(name)
}

function classify(names: string[]): { unknowns: string[]; params: string[] } {
  const pref = names.filter(preferred)
  if (pref.length && pref.length < names.length) return { unknowns: pref, params: names.filter((n) => !pref.includes(n)) }
  return { unknowns: names, params: [] }
}

function walkAff<C>(
  node: MathNode,
  F: Field<C>,
  unknowns: Set<string>,
  constCoeff: (node: MathNode) => C | null,
): Aff<C> | null {
  node = unwrap(node)
  const c0 = constCoeff(node)
  if (c0) return aff(F, c0)
  if (node.type === 'SymbolNode') {
    const name = (node as unknown as { name: string }).name
    if (!unknowns.has(name)) return null
    const a = aff(F)
    a.v.set(name, F.one)
    return a
  }
  if (node.type !== 'OperatorNode') return null
  const op = (node as unknown as { op: string }).op
  const args = argsOf(node)
  if (op === '-' && args.length === 1) {
    const a = walkAff(args[0]!, F, unknowns, constCoeff)
    return a && scaleA(a, F.neg(F.one), F)
  }
  if (op === '+' && args.length === 1) return walkAff(args[0]!, F, unknowns, constCoeff)
  if (args.length !== 2) return null
  if (op === '+' || op === '-') {
    const a = walkAff(args[0]!, F, unknowns, constCoeff)
    const b = walkAff(args[1]!, F, unknowns, constCoeff)
    if (!a || !b) return null
    return op === '+' ? addA(a, b, F) : subA(a, b, F)
  }
  if (op === '*') {
    const a = walkAff(args[0]!, F, unknowns, constCoeff)
    const b = walkAff(args[1]!, F, unknowns, constCoeff)
    if (!a || !b) return null
    if (isConst(a, F)) return scaleA(b, a.k, F)
    if (isConst(b, F)) return scaleA(a, b.k, F)
    return null
  }
  if (op === '/') {
    const a = walkAff(args[0]!, F, unknowns, constCoeff)
    const b = walkAff(args[1]!, F, unknowns, constCoeff)
    if (!a || !b || !isConst(b, F) || F.isZero(b.k)) return null
    return scaleA(a, F.div(F.one, b.k), F)
  }
  return null
}

type LinOut<C> = { status: 'inconsistent' } | { status: 'ok'; pivots: number[]; rows: C[][] }

function eliminate<C>(rows: C[][], F: Field<C>): LinOut<C> {
  const A = rows.map((r) => r.slice())
  const m = A.length
  if (!m) return { status: 'ok', pivots: [], rows: A }
  const n = A[0]!.length - 1
  const pivots: number[] = []
  let row = 0
  for (let col = 0; col < n && row < m; col++) {
    let sel = -1
    for (let i = row; i < m; i++) {
      if (!F.isZero(A[i]![col]!)) {
        sel = i
        break
      }
    }
    if (sel < 0) continue
    ;[A[row], A[sel]] = [A[sel]!, A[row]!]
    const piv = A[row]![col]!
    for (let j = col; j < n + 1; j++) A[row]![j] = F.div(A[row]![j]!, piv)
    for (let i = 0; i < m; i++) {
      if (i === row || F.isZero(A[i]![col]!)) continue
      const f = A[i]![col]!
      for (let j = col; j < n + 1; j++) A[i]![j] = F.sub(A[i]![j]!, F.mul(f, A[row]![j]!))
    }
    pivots.push(col)
    row++
  }
  for (let i = row; i < m; i++) {
    if (!F.isZero(A[i]![n]!)) return { status: 'inconsistent' }
  }
  return { status: 'ok', pivots, rows: A }
}

function solveRat(rows: Rat[][]): SystemResult {
  return finish(rows, RAT, rows[0] ? Array.from({ length: rows[0].length - 1 }, (_, i) => `x${i}`) : [])
}

function finish<C>(rows: C[][], F: Field<C>, variables: string[]): SystemResult {
  const out = eliminate(rows, F)
  if (out.status === 'inconsistent') {
    return { status: 'inconsistent', variables, display: 'no solution' }
  }
  const n = variables.length
  const freeIdx = variables.map((_, i) => i).filter((i) => !out.pivots.includes(i))
  const free = freeIdx.map((i) => variables[i]!)
  const exprs: string[] = Array.from({ length: n }, () => '0')
  if (!free.length) {
    const sol = Array.from({ length: n }, () => '0')
    out.pivots.forEach((col, k) => {
      sol[col] = F.str(out.rows[k]![n]!)
    })
    return {
      status: 'unique',
      variables,
      solutions: [sol],
      display: variables.map((name, i) => `${name} = ${sol[i]}`).join(', '),
    }
  }
  out.pivots.forEach((col, k) => {
    const row = out.rows[k]!
    const terms: Array<[string, C]> = freeIdx.map((j) => [variables[j]!, F.neg(row[j]!)])
    exprs[col] = exprStr(row[n]!, terms, F)
  })
  freeIdx.forEach((j) => {
    exprs[j] = variables[j]!
  })
  const fixed = out.pivots.filter((_, k) => freeIdx.every((j) => F.isZero(out.rows[k]![j]!)))
  const pinned = variables
    .map((name, i) => (fixed.includes(i) ? `${name} = ${exprs[i]}` : null))
    .filter(Boolean)
    .join(', ')
  const display = variables
    .map((name, i) => (free.includes(name) ? null : `${name} = ${exprs[i]}`))
    .filter(Boolean)
    .join(', ')
  return {
    status: 'infinite',
    variables,
    general: exprs,
    free,
    ...(pinned && { pinned }),
    display: display || `free: ${free.join(', ')}`,
  }
}

function matrixFrom<C>(diffs: MathNode[], variables: string[], F: Field<C>, constCoeff: (node: MathNode) => C | null): C[][] | null {
  const unk = new Set(variables)
  const rows: C[][] = []
  for (const diff of diffs) {
    const a = walkAff(diff, F, unk, constCoeff)
    if (!a) return null
    const row = variables.map((name) => a.v.get(name) ?? F.zero)
    row.push(F.neg(a.k))
    rows.push(row)
  }
  return rows
}

function hasSqrtConst(node: MathNode): boolean {
  let yes = false
  node.traverse((n: MathNode) => {
    if (fnName(n) === 'sqrt') {
      const arg = argsOf(n)[0]
      if (arg && constRat(arg)) yes = true
    }
  })
  return yes
}

function solveLinear(diffs: MathNode[], variables: string[], params: string[]): SystemResult | null {
  if (params.length) {
    const F = makeRF(params)
    const coeff = (node: MathNode): RF | null => constRF(node, params)
    const rows = matrixFrom(diffs, variables, F, coeff)
    if (!rows) return null
    return finish(rows, F, variables)
  }
  if (diffs.some(hasSqrtConst)) {
    const field = new RadField()
    const coeff = (node: MathNode): Rat[] | null => constRad(node, field)
    const rows = matrixFrom(diffs, variables, field, coeff)
    if (!rows) return null
    return finish(rows, field, variables)
  }
  const rows = matrixFrom(diffs, variables, RAT, constRat)
  if (!rows) return null
  return finish(rows, RAT, variables)
}

function constRad(node: MathNode, field: RadField): Rat[] | null {
  node = unwrap(node)
  if (fnName(node) === 'sqrt') {
    const arg = argsOf(node)[0]
    const r = arg && constRat(arg)
    if (!r) return null
    return field.fromSqrt(r)
  }
  const plain = constRat(node)
  if (plain) return field.fromRat(plain)
  if (node.type !== 'OperatorNode') return null
  const op = (node as unknown as { op: string }).op
  const args = argsOf(node)
  if ((op === '-' || op === '+') && args.length === 1) {
    const a = constRad(args[0]!, field)
    if (!a) return null
    return op === '-' ? field.neg(a) : a
  }
  if (args.length !== 2) return null
  const a = constRad(args[0]!, field)
  const b = constRad(args[1]!, field)
  if (!a || !b) return null
  if (op === '+') return field.add(a, b)
  if (op === '-') return field.sub(a, b)
  if (op === '*') return field.mul(a, b)
  if (op === '/') return field.isZero(b) ? null : field.div(a, b)
  return null
}

function constRF(node: MathNode, params: string[]): RF | null {
  node = unwrap(node)
  const asRat = constRat(node)
  if (asRat) return { n: pconst(asRat.n), d: pconst(asRat.d) }
  if (node.type === 'SymbolNode') {
    const name = (node as unknown as { name: string }).name
    const i = params.indexOf(name)
    if (i < 0) return null
    const exps = Array.from({ length: params.length }, () => 0)
    exps[i] = 1
    return { n: new Map([[monoKey(exps), 1n]]), d: pconst(1n) }
  }
  if (node.type !== 'OperatorNode') return null
  const op = (node as unknown as { op: string }).op
  const args = argsOf(node)
  if ((op === '-' || op === '+') && args.length === 1) {
    const a = constRF(args[0]!, params)
    if (!a) return null
    return op === '-' ? rfNeg(a) : a
  }
  if (args.length !== 2) return null
  if (op === '^') {
    const base = constRF(args[0]!, params)
    const exp = constRat(args[1]!)
    if (!base || !exp || exp.d !== 1n || exp.n < 0n || exp.n > 8n) return null
    let out: RF = { n: pconst(1n), d: pconst(1n) }
    for (let i = 0; i < Number(exp.n); i++) out = rfMul(out, base)
    return rfNorm(out.n, out.d)
  }
  const a = constRF(args[0]!, params)
  const b = constRF(args[1]!, params)
  if (!a || !b) return null
  if (op === '+') return rfAdd(a, b)
  if (op === '-') return rfSub(a, b)
  if (op === '*') return rfMul(a, b)
  if (op === '/') return rfZero(b) ? null : rfDiv(a, b)
  return null
}

function parseRatStr(s: string): Rat | null {
  const m = s.match(/^(-?\d+)(?:\/(-?\d+))?$/)
  if (!m) return null
  return rat(BigInt(m[1]!), m[2] ? BigInt(m[2]) : 1n)
}

/** Product of prime powers on one side of an exponential equation. */
function expSide(node: MathNode, unknowns: Set<string>): Map<bigint, Aff<Rat>> | null {
  node = unwrap(node)
  const acc = new Map<bigint, Aff<Rat>>()
  const addExp = (prime: bigint, e: Aff<Rat>, sign = 1) => {
    const cur = acc.get(prime) ?? aff(RAT)
    acc.set(prime, sign > 0 ? addA(cur, e, RAT) : subA(cur, e, RAT))
  }
  const takeInt = (n: bigint, exp: Aff<Rat>, sign = 1) => {
    if (n < 0n) return false
    if (n === 1n) return true
    for (const [p, e] of factorize(n)) {
      addExp(p, scaleA(exp, rat(BigInt(e)), RAT), sign)
    }
    return true
  }
  const walk = (n: MathNode, sign: number): boolean => {
    n = unwrap(n)
    const c = constRat(n)
    if (c) {
      if (c.n < 0n) return false
      return takeInt(c.n, aff(RAT, R1), sign) && takeInt(c.d, aff(RAT, R1), -sign)
    }
    if (n.type !== 'OperatorNode') return false
    const op = (n as unknown as { op: string }).op
    const args = argsOf(n)
    if (op === '*' && args.length === 2) return walk(args[0]!, sign) && walk(args[1]!, sign)
    if (op === '/' && args.length === 2) return walk(args[0]!, sign) && walk(args[1]!, -sign)
    if (op === '^' && args.length === 2) {
      const base = constRat(args[0]!)
      const exp = walkAff(args[1]!, RAT, unknowns, constRat)
      if (!base || base.n <= 0n || !exp) return false
      return takeInt(base.n, exp, sign) && takeInt(base.d, exp, -sign)
    }
    return false
  }
  if (!walk(node, 1)) return null
  return acc
}

function tryExponential(parsed: Parsed[], variables: string[]): SystemResult | null {
  const unk = new Set(variables)
  const rows: Rat[][] = []
  const primes = new Set<bigint>()
  const sides = parsed.map((p) => {
    const L = expSide(p.lhs, unk)
    const R = expSide(p.rhs, unk)
    return L && R ? { L, R } : null
  })
  if (sides.some((s) => !s)) return null
  for (const s of sides) {
    for (const p of s!.L.keys()) primes.add(p)
    for (const p of s!.R.keys()) primes.add(p)
  }
  if (!primes.size) return null
  for (const s of sides) {
    for (const p of primes) {
      const L = s!.L.get(p) ?? aff(RAT)
      const R = s!.R.get(p) ?? aff(RAT)
      const d = subA(L, R, RAT)
      const row = variables.map((name) => d.v.get(name) ?? R0)
      row.push(rneg(d.k))
      if (row.some((c) => !rzero(c))) rows.push(row)
    }
  }
  if (!rows.length) return null
  return finish(rows, RAT, variables)
}

function tryReciprocal(diffs: MathNode[], variables: string[]): SystemResult | null {
  const unk = new Set(variables)
  const rows: Rat[][] = []
  for (const diff of diffs) {
    const a = walkInv(diff, unk)
    if (!a) return null
    const row = variables.map((name) => a.v.get(name) ?? R0)
    row.push(rneg(a.k))
    rows.push(row)
  }
  const solved = finish(rows, RAT, variables)
  if (solved.status !== 'unique' || !solved.solutions) return solved.status === 'inconsistent' ? solved : null
  const vals = solved.solutions[0]!
  if (vals.some((v) => v === '0')) return null
  const inv = vals.map((v) => {
    const r = parseRatStr(v)
    return r ? rstr(rdiv(R1, r)) : null
  })
  if (inv.some((v) => !v)) return null
  return {
    status: 'unique',
    variables,
    solutions: [inv as string[]],
    display: variables.map((name, i) => `${name} = ${inv[i]}`).join(', '),
  }
}

function walkInv(node: MathNode, unknowns: Set<string>): Aff<Rat> | null {
  node = unwrap(node)
  const c = constRat(node)
  if (c) return aff(RAT, c)
  if (node.type === 'SymbolNode') return null
  if (node.type !== 'OperatorNode') return null
  const op = (node as unknown as { op: string }).op
  const args = argsOf(node)
  if ((op === '-' || op === '+') && args.length === 1) {
    const a = walkInv(args[0]!, unknowns)
    return a && (op === '-' ? scaleA(a, rat(-1n), RAT) : a)
  }
  if (args.length !== 2) return null
  if (op === '+' || op === '-') {
    const a = walkInv(args[0]!, unknowns)
    const b = walkInv(args[1]!, unknowns)
    if (!a || !b) return null
    return op === '+' ? addA(a, b, RAT) : subA(a, b, RAT)
  }
  if (op === '*') {
    const a = walkInv(args[0]!, unknowns)
    const b = walkInv(args[1]!, unknowns)
    if (!a || !b) return null
    if (isConst(a, RAT)) return scaleA(b, a.k, RAT)
    if (isConst(b, RAT)) return scaleA(a, b.k, RAT)
    return null
  }
  if (op === '/') {
    const b = unwrap(args[1]!)
    if (b.type === 'SymbolNode' && unknowns.has((b as unknown as { name: string }).name)) {
      const coeff = constRat(args[0]!)
      if (!coeff) return null
      const a = aff(RAT)
      a.v.set((b as unknown as { name: string }).name, coeff)
      return a
    }
    const a = walkInv(args[0]!, unknowns)
    const den = walkInv(args[1]!, unknowns)
    if (!a || !den || !isConst(den, RAT) || rzero(den.k)) return null
    return scaleA(a, rdiv(R1, den.k), RAT)
  }
  return null
}

type Quad = Map<string, Rat>

function qkey(vars: string[]): string {
  return [...vars].sort().join(',')
}
function qadd(a: Quad, b: Quad): Quad {
  const o = new Map(a)
  for (const [k, c] of b) {
    const s = radd(o.get(k) ?? R0, c)
    if (rzero(s)) o.delete(k)
    else o.set(k, s)
  }
  return o
}
function qscale(a: Quad, c: Rat): Quad {
  const o: Quad = new Map()
  for (const [k, v] of a) {
    const s = rmul(v, c)
    if (!rzero(s)) o.set(k, s)
  }
  return o
}

function walkQuad(node: MathNode, unknowns: Set<string>): Quad | null {
  node = unwrap(node)
  const c = constRat(node)
  if (c) return new Map(rzero(c) ? [] : [['', c]])
  if (node.type === 'SymbolNode') {
    const name = (node as unknown as { name: string }).name
    if (!unknowns.has(name)) return null
    return new Map([[name, R1]])
  }
  if (node.type !== 'OperatorNode') return null
  const op = (node as unknown as { op: string }).op
  const args = argsOf(node)
  if ((op === '-' || op === '+') && args.length === 1) {
    const a = walkQuad(args[0]!, unknowns)
    return a && (op === '-' ? qscale(a, rat(-1n)) : a)
  }
  if (args.length !== 2) return null
  if (op === '+' || op === '-') {
    const a = walkQuad(args[0]!, unknowns)
    const b = walkQuad(args[1]!, unknowns)
    if (!a || !b) return null
    return op === '+' ? qadd(a, b) : qadd(a, qscale(b, rat(-1n)))
  }
  if (op === '*') {
    const a = walkQuad(args[0]!, unknowns)
    const b = walkQuad(args[1]!, unknowns)
    if (!a || !b) return null
    return qmul(a, b)
  }
  if (op === '/') {
    const a = walkQuad(args[0]!, unknowns)
    const den = constRat(args[1]!)
    if (!a || !den || rzero(den)) return null
    return qscale(a, rdiv(R1, den))
  }
  if (op === '^') {
    const exp = constRat(args[1]!)
    if (!exp || exp.n !== 2n || exp.d !== 1n) return null
    const base = walkQuad(args[0]!, unknowns)
    if (!base) return null
    return qmul(base, base)
  }
  return null
}

function qmul(a: Quad, b: Quad): Quad | null {
  const o: Quad = new Map()
  for (const [ka, ca] of a) {
    for (const [kb, cb] of b) {
      const vars = [...(ka ? ka.split(',') : []), ...(kb ? kb.split(',') : [])]
      if (vars.length > 2) return null
      const key = qkey(vars)
      const s = radd(o.get(key) ?? R0, rmul(ca, cb))
      if (rzero(s)) o.delete(key)
      else o.set(key, s)
    }
  }
  return o
}

function qdegree(q: Quad): number {
  let m = 0
  for (const k of q.keys()) m = Math.max(m, k ? k.split(',').length : 0)
  return m
}

type AffRat = Aff<Rat>

function affMul(a: AffRat, b: AffRat): Quad | null {
  // (c + sum ai vi)(d + sum bj vj) as a quad in the free names
  const A: Quad = new Map([['', a.k]])
  for (const [n, c] of a.v) A.set(n, c)
  const B: Quad = new Map([['', b.k]])
  for (const [n, c] of b.v) B.set(n, c)
  return qmul(A, B)
}

function tryQuadratic(diffs: MathNode[], variables: string[]): SystemResult | null {
  const unk = new Set(variables)
  const linear: MathNode[] = []
  const quads: Quad[] = []
  for (const diff of diffs) {
    const q = walkQuad(diff, unk)
    if (!q) return null
    if (qdegree(q) <= 1) linear.push(diff)
    else quads.push(q)
  }
  if (!quads.length) return null
  const rows = matrixFrom(linear, variables, RAT, constRat)
  if (!rows) return null
  const lin = rows.length ? eliminate(rows, RAT) : { status: 'ok' as const, pivots: [] as number[], rows: [] as Rat[][] }
  if (lin.status === 'inconsistent') return { status: 'inconsistent', variables, display: 'no solution' }
  const freeIdx = variables.map((_, i) => i).filter((i) => !lin.pivots.includes(i))
  if (freeIdx.length !== 1) return null
  const fi = freeIdx[0]!
  const freeName = variables[fi]!
  const expr = new Map<string, AffRat>()
  variables.forEach((name, col) => {
    const kRow = lin.pivots.indexOf(col)
    if (kRow < 0) {
      const a = aff(RAT)
      a.v.set(freeName, R1)
      expr.set(name, a)
      return
    }
    const row = lin.rows[kRow]!
    const a = aff(RAT, row[variables.length]!)
    const c = rneg(row[fi]!)
    if (!rzero(c)) a.v.set(freeName, c)
    expr.set(name, a)
  })
  let poly: Quad = new Map()
  for (const q of quads) {
    const sub = substQuad(q, expr)
    if (!sub) return null
    poly = qadd(poly, sub)
  }
  const A = poly.get(`${freeName},${freeName}`) ?? R0
  const B = poly.get(freeName) ?? R0
  const C = poly.get('') ?? R0
  const roots = quadRoots(A, B, C)
  if (!roots) return rzero(A) && rzero(B) && rzero(C) ? finish(rows, RAT, variables) : { status: 'inconsistent', variables, display: 'no solution' }
  const solutions = roots.map((t) => variables.map((name) => rstr(evalAff(expr.get(name)!, freeName, t))))
  return {
    status: roots.length === 1 ? 'unique' : 'finite',
    variables,
    solutions,
    display:
      solutions.length === 1
        ? variables.map((name, i) => `${name} = ${solutions[0]![i]}`).join(', ')
        : solutions.map((sol) => `(${variables.join(', ')}) = (${sol.join(', ')})`).join(' or '),
  }
}

function evalAff(a: AffRat, free: string, t: Rat): Rat {
  return radd(a.k, rmul(a.v.get(free) ?? R0, t))
}

function substQuad(q: Quad, expr: Map<string, AffRat>): Quad | null {
  let out: Quad = new Map()
  for (const [key, c] of q) {
    if (!key) {
      out = qadd(out, new Map([['', c]]))
      continue
    }
    const names = key.split(',')
    let piece = affMul(expr.get(names[0]!) ?? aff(RAT), names[1] ? (expr.get(names[1]!) ?? aff(RAT)) : aff(RAT, R1))
    if (!piece) return null
    if (!names[1]) {
      // linear monomial: expr itself, not expr * 1 already handled... affMul(expr, 1) is expr
    }
    out = qadd(out, qscale(piece, c))
  }
  return out
}

function isSquare(n: bigint): bigint | null {
  if (n < 0n) return null
  let lo = 0n
  let hi = n
  while (lo <= hi) {
    const mid = (lo + hi) >> 1n
    const sq = mid * mid
    if (sq === n) return mid
    if (sq < n) lo = mid + 1n
    else hi = mid - 1n
  }
  return null
}

function quadRoots(A: Rat, B: Rat, C: Rat): Rat[] | null {
  if (rzero(A) && rzero(B)) return rzero(C) ? null : null
  if (rzero(A)) return [rdiv(rneg(C), B)]
  const disc = rsub(rmul(B, B), rmul(rat(4n), rmul(A, C)))
  if (disc.n < 0n) return null
  const num = isSquare(disc.n)
  const den = isSquare(disc.d)
  if (num == null || den == null) return null
  const s = rat(num, den)
  const twoA = rmul(rat(2n), A)
  return [rdiv(radd(rneg(B), s), twoA), rdiv(rsub(rneg(B), s), twoA)].filter((r, i, arr) => arr.findIndex((o) => req(o, r)) === i)
}

type SqrtSlot = { key: string; variable: string; scale: Rat; shift: Rat }

function trySqrtLinear(diffs: MathNode[], variables: string[]): SystemResult | null {
  const slots = new Map<string, SqrtSlot>()
  const rows: Rat[][] = []
  const keys: string[] = []
  for (const diff of diffs) {
    const a = walkSqrt(diff, new Set(variables), slots)
    if (!a) return null
    for (const key of a.v.keys()) if (!keys.includes(key)) keys.push(key)
  }
  if (!slots.size) return null
  for (const diff of diffs) {
    const a = walkSqrt(diff, new Set(variables), slots)!
    const row = keys.map((k) => a.v.get(k) ?? R0)
    row.push(rneg(a.k))
    rows.push(row)
  }
  const solved = finish(rows, RAT, keys)
  if (solved.status !== 'unique' || !solved.solutions) return null
  const u = solved.solutions[0]!.map(parseRatStr)
  if (u.some((v) => !v || v!.n < 0n)) return null
  const byVar = new Map<string, { scale: Rat; shift: Rat; u2: Rat }>()
  keys.forEach((key, i) => {
    const slot = slots.get(key)!
    const val = u[i]!
    if (byVar.has(slot.variable)) return
    byVar.set(slot.variable, { scale: slot.scale, shift: slot.shift, u2: rmul(val, val) })
  })
  if (byVar.size !== variables.length) return null
  const sol = variables.map((name) => {
    const s = byVar.get(name)!
    return rstr(rdiv(rsub(s.u2, s.shift), s.scale))
  })
  return {
    status: 'unique',
    variables,
    solutions: [sol],
    display: variables.map((name, i) => `${name} = ${sol[i]}`).join(', '),
  }
}

function walkSqrt(node: MathNode, unknowns: Set<string>, slots: Map<string, SqrtSlot>): Aff<Rat> | null {
  node = unwrap(node)
  const c = constRat(node)
  if (c) return aff(RAT, c)
  if (fnName(node) === 'sqrt') {
    const inside = walkAff(argsOf(node)[0]!, RAT, unknowns, constRat)
    if (!inside) return null
    const vars = [...inside.v.entries()].filter(([, c0]) => !rzero(c0))
    if (vars.length !== 1) return null
    const [variable, scale] = vars[0]!
    const key = `sqrt(${rstr(scale)}*${variable}+${rstr(inside.k)})`
    slots.set(key, { key, variable, scale, shift: inside.k })
    const a = aff(RAT)
    a.v.set(key, R1)
    return a
  }
  if (node.type === 'SymbolNode') return null
  if (node.type !== 'OperatorNode') return null
  const op = (node as unknown as { op: string }).op
  const args = argsOf(node)
  if ((op === '-' || op === '+') && args.length === 1) {
    const a = walkSqrt(args[0]!, unknowns, slots)
    return a && (op === '-' ? scaleA(a, rat(-1n), RAT) : a)
  }
  if (args.length !== 2) return null
  if (op === '+' || op === '-') {
    const a = walkSqrt(args[0]!, unknowns, slots)
    const b = walkSqrt(args[1]!, unknowns, slots)
    if (!a || !b) return null
    return op === '+' ? addA(a, b, RAT) : subA(a, b, RAT)
  }
  if (op === '*') {
    const a = walkSqrt(args[0]!, unknowns, slots)
    const b = walkSqrt(args[1]!, unknowns, slots)
    if (!a || !b) return null
    if (isConst(a, RAT)) return scaleA(b, a.k, RAT)
    if (isConst(b, RAT)) return scaleA(a, b.k, RAT)
    return null
  }
  if (op === '/') {
    const a = walkSqrt(args[0]!, unknowns, slots)
    const den = constRat(args[1]!)
    if (!a || !den || rzero(den)) return null
    return scaleA(a, rdiv(R1, den), RAT)
  }
  return null
}

function tryAbs(texts: string[], variables: string[]): SystemResult | null {
  if (!texts.some((t) => t.includes('abs(') || t.includes('|'))) return null
  const absVars = variables.filter((name) => texts.some((t) => t.includes(`abs(${name})`) || t.includes(`|${name}|`)))
  if (!absVars.length || absVars.length > 4) return null
  const sols: string[][] = []
  const masks = 1 << absVars.length
  for (let mask = 0; mask < masks; mask++) {
    const signed = texts.map((t) => {
      let s = t
      absVars.forEach((name, i) => {
        const rep = (mask >> i) & 1 ? `(${name})` : `(-${name})`
        s = s.replaceAll(`abs(${name})`, rep).replaceAll(`|${name}|`, rep)
      })
      return s
    })
    const parsed = signed.map(prepEq)
    if (parsed.some((p) => !p)) continue
    const solved = solveLinear(parsed.map((p) => p!.diff), variables, [])
    if (!solved || solved.status !== 'unique' || !solved.solutions) continue
    const vals = solved.solutions[0]!.map(parseRatStr)
    if (vals.some((v) => !v)) continue
    const ok = absVars.every((name, i) => {
      const v = vals[variables.indexOf(name)]!
      const nonneg = ((mask >> i) & 1) === 1
      return nonneg ? v.n >= 0n : v.n <= 0n
    })
    if (!ok) continue
    const strs = vals.map((v) => rstr(v!))
    if (!sols.some((s) => s.every((v, i) => v === strs[i]))) sols.push(strs)
  }
  if (!sols.length) return { status: 'inconsistent', variables, display: 'no solution' }
  return {
    status: sols.length === 1 ? 'unique' : 'finite',
    variables,
    solutions: sols,
    display: variables.map((name, i) => `${name} = ${sols[0]![i]}`).join(', '),
  }
}

function tryTrig(diffs: MathNode[], variables: string[]): SystemResult | null {
  const atoms: string[] = []
  const kind = new Map<string, { fn: 'sin' | 'cos'; name: string }>()
  const rows: Rat[][] = []
  for (const diff of diffs) {
    const a = walkTrig(diff, new Set(variables), atoms, kind)
    if (!a) return null
  }
  if (!atoms.length) return null
  for (const diff of diffs) {
    const a = walkTrig(diff, new Set(variables), atoms, kind)!
    const row = atoms.map((k) => a.v.get(k) ?? R0)
    row.push(rneg(a.k))
    rows.push(row)
  }
  const solved = finish(rows, RAT, atoms)
  if (solved.status !== 'unique' || !solved.solutions) return solved.status === 'inconsistent' ? solved : null
  const values = solved.solutions[0]!.map(parseRatStr)
  if (values.some((v) => !v)) return null
  const angles: Record<string, string[]> = {}
  const shown: { expr: string; value: string }[] = []
  atoms.forEach((atom, i) => {
    const meta = kind.get(atom)!
    const value = values[i]!
    if (value.n < 0n ? -value.n > value.d : value.n > value.d) return
    shown.push({ expr: atom, value: rstr(value) })
    angles[meta.name] = angleList(meta.fn, value)
  })
  if (shown.length !== atoms.length) return null
  return {
    status: 'unique',
    variables,
    atoms: shown,
    angles,
    display: shown.map((a) => `${a.expr} = ${a.value}`).join(', '),
  }
}

function angleList(fn: 'sin' | 'cos', v: Rat): string[] {
  const table: Record<string, string[]> =
    fn === 'sin'
      ? {
          '0': ['0', 'pi'],
          '1': ['pi/2'],
          '-1': ['3*pi/2'],
          '1/2': ['pi/6', '5*pi/6'],
          '-1/2': ['7*pi/6', '11*pi/6'],
        }
      : {
          '0': ['pi/2', '3*pi/2'],
          '1': ['0'],
          '-1': ['pi'],
          '1/2': ['pi/3', '5*pi/3'],
          '-1/2': ['2*pi/3', '4*pi/3'],
        }
  const hit = table[rstr(v)]
  if (hit) return hit
  if (fn === 'cos') {
    const a = `acos(${rstr(v)})`
    return req(v, R1) || req(v, rat(-1n)) ? [a] : [a, `2*pi - ${a}`]
  }
  const a = `asin(${rstr(v)})`
  if (req(v, R1)) return ['pi/2']
  if (req(v, rat(-1n))) return ['3*pi/2']
  return [a, `pi - ${a}`]
}

function walkTrig(
  node: MathNode,
  unknowns: Set<string>,
  atoms: string[],
  kind: Map<string, { fn: 'sin' | 'cos'; name: string }>,
): Aff<Rat> | null {
  node = unwrap(node)
  const c = constRat(node)
  if (c) return aff(RAT, c)
  const fn = fnName(node)
  if (fn === 'sin' || fn === 'cos') {
    const arg = argsOf(node)[0]
    if (!arg || arg.type !== 'SymbolNode') return null
    const name = (arg as unknown as { name: string }).name
    if (!unknowns.has(name)) return null
    const key = `${fn}(${name})`
    if (!atoms.includes(key)) atoms.push(key)
    kind.set(key, { fn, name })
    const a = aff(RAT)
    a.v.set(key, R1)
    return a
  }
  if (node.type === 'SymbolNode') return null
  if (node.type !== 'OperatorNode') return null
  const op = (node as unknown as { op: string }).op
  const args = argsOf(node)
  if ((op === '-' || op === '+') && args.length === 1) {
    const a = walkTrig(args[0]!, unknowns, atoms, kind)
    return a && (op === '-' ? scaleA(a, rat(-1n), RAT) : a)
  }
  if (args.length !== 2) return null
  if (op === '+' || op === '-') {
    const a = walkTrig(args[0]!, unknowns, atoms, kind)
    const b = walkTrig(args[1]!, unknowns, atoms, kind)
    if (!a || !b) return null
    return op === '+' ? addA(a, b, RAT) : subA(a, b, RAT)
  }
  if (op === '*') {
    const a = walkTrig(args[0]!, unknowns, atoms, kind)
    const b = walkTrig(args[1]!, unknowns, atoms, kind)
    if (!a || !b) return null
    if (isConst(a, RAT)) return scaleA(b, a.k, RAT)
    if (isConst(b, RAT)) return scaleA(a, b.k, RAT)
    return null
  }
  if (op === '/') {
    const a = walkTrig(args[0]!, unknowns, atoms, kind)
    const den = constRat(args[1]!)
    if (!a || !den || rzero(den)) return null
    return scaleA(a, rdiv(R1, den), RAT)
  }
  return null
}

/** `sys`, `sys3`, `sys 3`, `system 3`. The count needs no enter: the fields open as soon as it's typed. */
export function sysCommand(text: string): { count: number } | { hint: string } | null {
  const m = text.trim().match(/^sys(?:tem)?\s*(\d+)?\s*$/i)
  if (!m) return null
  if (!m[1]) return { hint: 'how many equations? sys2, sys3… up to sys5' }
  const n = Number(m[1])
  if (!Number.isInteger(n) || n < 1 || n > 5) return { hint: 'up to 5 equations' }
  return { count: n }
}

export function isSysCommand(text: string): boolean {
  return /^\s*sys(?:tem)?(?:\s|\d|$)/i.test(text) && sysCommand(text) != null
}

export type SystemAnswer = { display: string; exact?: string; message?: boolean }

/**
 * Solve the equations that parse, including while later fields are empty or still being typed.
 * An underdetermined system shows only what the typed equations already fix until every field is an equation.
 */
export function solveLive(lines: string[], defaults?: DefaultUnits): SystemAnswer | null {
  const pending = lines.some((line) => !prepEq(line))
  const ready = lines.map((line) => line.trim()).filter((line) => prepEq(line))
  if (!ready.length) return null
  const result = solveWithUnits(ready, defaults) ?? solveSystem(ready)
  if (!result) return null
  if (result.status === 'inconsistent' || result.status === 'mismatch') return { display: result.display, message: true }
  if (result.status === 'infinite' && pending) return result.pinned ? { display: result.pinned } : null
  return { display: result.display }
}

/** Solve up to five equations. Null when the system isn't one this solver reads. */
export function solveSystem(equations: string[]): SystemResult | null {
  if (equations.length < 1 || equations.length > 5) return null
  const parsed = equations.map(prepEq)
  if (parsed.some((p) => !p)) return null
  const ready = parsed as Parsed[]
  const names: string[] = []
  for (const p of ready) symbolsOf(p.diff, names)
  // as many equations as names (`F = m*a`, `m = 2`, `a = 3`): every name is an unknown, lowercase or not
  const { unknowns, params } = names.length <= ready.length ? { unknowns: names, params: [] } : classify(names)
  if (!unknowns.length || unknowns.length > 8) return null
  const diffs = ready.map((p) => p.diff)
  if (!params.length) {
    const exp = tryExponential(ready, unknowns)
    if (exp) return exp
    const inv = tryReciprocal(diffs, unknowns)
    if (inv) return inv
    const sq = trySqrtLinear(diffs, unknowns)
    if (sq) return sq
    const ab = tryAbs(equations, unknowns)
    if (ab) return ab
    const trig = tryTrig(diffs, unknowns)
    if (trig) return trig
  }
  const linear = solveLinear(diffs, unknowns, params)
  if (linear) return linear
  if (!params.length) return tryQuadratic(diffs, unknowns)
  return null
}

// ---- units: `x + y = 10 m`, `F = m*a` with `a = 9.8 m/s^2` ----

type UnitResult = SystemResult | { status: 'mismatch'; display: string; pinned?: undefined }

const UNIT_WORD = String.raw`[A-Za-zΩµμ°][A-Za-zΩµμ°]*`
const UNIT_POW = String.raw`(?:\s*(?:\^\s*-?\d+|[²³]))?`
// a number, then a unit that may be a product or quotient (`9.8 m/s^2`, `3 kg*m/s²`)
const QUANTITY = new RegExp(
  String.raw`(?<![\w.])((?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?)(\s*)(${UNIT_WORD}${UNIT_POW}(?:\s*[*/·]\s*${UNIT_WORD}${UNIT_POW})*)`,
  'g',
)
const DIMS = 7

/** An exact rational for an SI float, kept within mathjs safe integers: `(3048/10^4)`. */
function siRatText(x: number): string | null {
  const m = Math.abs(x)
    .toPrecision(15)
    .match(/^(\d+)(?:\.(\d+))?(?:e([+-]\d+))?$/)
  if (!m) return null
  const frac = (m[2] ?? '').replace(/0+$/, '')
  let digits = (m[1]! + frac).replace(/^0+(?=\d)/, '')
  let exp = Number(m[3] ?? 0) - frac.length
  while (digits.length > 1 && digits.endsWith('0')) {
    digits = digits.slice(0, -1)
    exp++
  }
  const pow = (k: number) => {
    const parts: string[] = []
    for (; k > 0; k -= 12) parts.push(`10^${Math.min(k, 12)}`)
    return parts.join('*')
  }
  const sign = x < 0 ? '-' : ''
  if (exp === 0) return `(${sign}${digits})`
  return exp > 0 ? `(${sign}${digits}*${pow(exp)})` : `(${sign}${digits}/(${pow(-exp)}))`
}

type UnitPrep = { plain: string[]; marked: string[]; quantities: SiQuantity[] }

/** Swap each `number unit` for its SI value, and for a marker the dimension check reads. Null when no line has a unit. */
function prepUnits(lines: string[]): UnitPrep | null {
  // a name used on its own anywhere is a variable, so `m` in `m = 2 kg` never reads as meters
  const bare = new Set<string>()
  for (const line of lines) {
    for (const w of line.replace(QUANTITY, ' ').match(/[A-Za-z_][A-Za-z0-9_]*/g) ?? []) bare.add(w)
  }
  const quantities: SiQuantity[] = []
  const plain: string[] = []
  const marked: string[] = []
  for (const line of lines) {
    let p = ''
    let q = ''
    let at = 0
    for (const m of line.matchAll(QUANTITY)) {
      const unitText = keptUnit(m[3]!, bare)
      if (!unitText) continue
      const qty = siQuantity(`${m[1]} ${unitText}`)
      const value = qty && siRatText(qty.si)
      if (!qty || !value) continue
      const start = m.index!
      const end = start + m[1]!.length + m[2]!.length + unitText.length
      const before = line.slice(at, start)
      p += `${before}${value}`
      q += `${before}(__q${quantities.length})`
      quantities.push(qty)
      at = end
    }
    plain.push(p + line.slice(at))
    marked.push(q + line.slice(at))
  }
  return quantities.length ? { plain, marked, quantities } : null
}

/**
 * The leading run of unit words. A word that is also a variable ends the run (`10 m/t` with `t = 2 s`),
 * unless a plain unit follows it: `3 m/s^2` is still meters beside a variable `m`, but `2 m` is 2 times it.
 */
function keptUnit(text: string, bare: Set<string>): string | null {
  const re = new RegExp(String.raw`(^|\s*[*/·]\s*)(${UNIT_WORD})(${UNIT_POW})`, 'gy')
  const parts: Array<{ text: string; variable: boolean }> = []
  for (const m of text.matchAll(re)) {
    const word = m[2]!
    if (!(readsAsUnit(word) || siQuantity(`1 ${word}`))) break
    parts.push({ text: m[0], variable: bare.has(word.replace(/[°µμΩ]/g, '')) })
  }
  while (parts.length && parts.at(-1)!.variable) parts.pop()
  return parts.map((p) => p.text).join('') || null
}

class DimClash extends Error {}

function dimsEqual(a: number[], b: number[]): boolean {
  return a.every((x, i) => x === b[i])
}

/** Each unknown's SI dimension, read off the typed quantities; null when a sum or a side mixes units. */
function inferDims(marked: string[], quantities: SiQuantity[], unknowns: string[]): Map<string, number[]> | null {
  const zero = Array.from({ length: DIMS }, () => 0)
  const assigned = new Map<string, number[]>()
  const known = (name: string): number[] | undefined => {
    const q = /^__q(\d+)$/.exec(name)
    if (q) return quantities[Number(q[1])]!.dim
    if (assigned.has(name)) return assigned.get(name)
    return unknowns.includes(name) ? undefined : zero
  }
  let changed = false
  let settle = false

  const dimOf = (node: MathNode): number[] | undefined => {
    node = unwrap(node)
    if (node.type === 'ConstantNode') return zero
    if (node.type === 'SymbolNode') {
      const d = known((node as unknown as { name: string }).name)
      return d ?? (settle ? zero : undefined)
    }
    const name = fnName(node)
    if (name) {
      const args = argsOf(node)
      if (name === 'abs' && args.length === 1) return dimOf(args[0]!)
      if (name === 'sqrt' && args.length === 1) {
        const d = dimOf(args[0]!)
        if (!d) return undefined
        if (d.some((x) => x % 2)) throw new DimClash()
        return d.map((x) => x / 2)
      }
      for (const a of args) unify(a, zero)
      return zero
    }
    if (node.type !== 'OperatorNode') throw new DimClash()
    const op = (node as unknown as { op: string }).op
    const args = argsOf(node)
    if (args.length === 1) return dimOf(args[0]!)
    const [a, b] = [args[0]!, args[1]!]
    if (op === '+' || op === '-') {
      const da = dimOf(a)
      const db = dimOf(b)
      if (da && db && !dimsEqual(da, db)) throw new DimClash()
      const d = da ?? db
      if (d) {
        unify(a, d)
        unify(b, d)
      }
      return d
    }
    if (op === '*' || op === '/') {
      const da = dimOf(a)
      const db = dimOf(b)
      if (!da || !db) return undefined
      return da.map((x, i) => (op === '*' ? x + db[i]! : x - db[i]!))
    }
    if (op === '^') {
      const e = constRat(b)
      const da = dimOf(a)
      if (!e) {
        unify(a, zero)
        return zero
      }
      if (!da) return undefined
      const out = da.map((x) => (x * Number(e.n)) / Number(e.d))
      if (out.some((x) => !Number.isInteger(x))) throw new DimClash()
      return out
    }
    throw new DimClash()
  }

  const unify = (node: MathNode, d: number[]): void => {
    node = unwrap(node)
    if (node.type === 'ConstantNode') {
      if (!dimsEqual(d, zero)) throw new DimClash()
      return
    }
    if (node.type === 'SymbolNode') {
      const name = (node as unknown as { name: string }).name
      const have = known(name)
      if (have) {
        if (!dimsEqual(have, d)) throw new DimClash()
        return
      }
      assigned.set(name, d)
      changed = true
      return
    }
    const name = fnName(node)
    if (name) {
      const args = argsOf(node)
      if (name === 'abs' && args.length === 1) unify(args[0]!, d)
      else if (name === 'sqrt' && args.length === 1) unify(args[0]!, d.map((x) => x * 2))
      else if (!dimsEqual(d, zero)) throw new DimClash()
      return
    }
    if (node.type !== 'OperatorNode') return
    const op = (node as unknown as { op: string }).op
    const args = argsOf(node)
    if (args.length === 1) return unify(args[0]!, d)
    const [a, b] = [args[0]!, args[1]!]
    if (op === '+' || op === '-') {
      unify(a, d)
      unify(b, d)
      return
    }
    if (op === '*' || op === '/') {
      const da = dimOf(a)
      const db = dimOf(b)
      if (da && db) {
        if (!dimsEqual(da.map((x, i) => (op === '*' ? x + db[i]! : x - db[i]!)), d)) throw new DimClash()
      } else if (da) unify(b, op === '*' ? d.map((x, i) => x - da[i]!) : da.map((x, i) => x - d[i]!))
      else if (db) unify(a, op === '*' ? d.map((x, i) => x - db[i]!) : d.map((x, i) => x + db[i]!))
      return
    }
    if (op === '^') {
      const e = constRat(b)
      if (!e || rzero(e)) return
      const base = d.map((x) => (x * Number(e.d)) / Number(e.n))
      if (base.some((x) => !Number.isInteger(x))) throw new DimClash()
      unify(a, base)
    }
  }

  const sides = marked.map(prepEq)
  if (sides.some((s) => !s)) return null
  const eqs = sides as Parsed[]
  const pass = () => {
    for (const eq of eqs) {
      const dl = dimOf(eq.lhs)
      const dr = dimOf(eq.rhs)
      if (dl && dr && !dimsEqual(dl, dr)) throw new DimClash()
      if (dl) unify(eq.rhs, dl)
      if (dr) unify(eq.lhs, dr)
    }
  }
  try {
    for (let round = 0; round < 12; round++) {
      changed = false
      pass()
      if (!changed) break
    }
    // what nothing pinned down is a plain number; one more pass checks every side agrees
    settle = true
    for (const name of unknowns) if (!assigned.has(name)) assigned.set(name, zero)
    pass()
  } catch (e) {
    if (e instanceof DimClash) return null
    throw e
  }
  return assigned
}

/** Solve with units by working in SI; null when no equation has a unit, so the plain solver takes over. */
export function solveWithUnits(equations: string[], defaults?: DefaultUnits): UnitResult | null {
  const prep = prepUnits(equations)
  if (!prep) return null
  const result = solveSystem(prep.plain)
  const mismatch: UnitResult = { status: 'mismatch', display: "units don't match" }
  if (!result) return null
  if (result.status === 'inconsistent') return result
  if (result.atoms || result.angles) return null
  const dims = inferDims(prep.marked, prep.quantities, result.variables)
  if (!dims) return mismatch
  // an unknown shows in the unit typed for that dimension, else the preferred or named unit
  const unitFor = (d: number[]) => prep.quantities.find((q) => q.unitId && dimsEqual(q.dim, d))?.unitId
  const show = (name: string, exact: string): string | null => {
    let n: number
    try {
      const v = math.evaluate(exact)
      if (typeof v !== 'number' || !Number.isFinite(v)) return null
      n = v
    } catch {
      return null
    }
    const d = dims.get(name)!
    const value = dimsEqual(d, Array.from({ length: DIMS }, () => 0)) ? { kind: 'number' as const, n } : siValue(n, d, unitFor(d), defaults)
    return value?.kind === 'number' ? formatValue(value) : null
  }
  const { variables } = result
  if (result.status === 'unique' || result.status === 'finite') {
    const rows = result.solutions!.map((sol) => sol.map((exact, i) => show(variables[i]!, exact)))
    if (rows.some((row) => row.some((s) => s == null))) return null
    const display =
      rows.length === 1
        ? variables.map((name, i) => `${name} = ${rows[0]![i]}`).join(', ')
        : rows
            .map((row) => (variables.length === 1 ? `${variables[0]} = ${row[0]}` : `(${variables.join(', ')}) = (${row.join(', ')})`))
            .join(' or ')
    return { ...result, display }
  }
  // underdetermined: only the pinned values have a unit to show; the rest stay free
  const free = result.free ?? []
  const pinnedParts: string[] = []
  variables.forEach((name, i) => {
    const expr = result.general?.[i]
    if (!expr || free.includes(name) || symbolsOf(math.parse(expr)).some((s) => free.includes(s))) return
    const shown = show(name, expr)
    if (shown) pinnedParts.push(`${name} = ${shown}`)
  })
  const pinned = pinnedParts.join(', ')
  return { ...result, pinned: pinned || undefined, display: [pinned, `free: ${free.join(', ')}`].filter(Boolean).join(', ') }
}
