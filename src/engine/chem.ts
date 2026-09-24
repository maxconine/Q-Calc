// chemical equation balancing: element symbols by pattern, no periodic table

type Counts = Map<string, bigint>

interface Species {
  /** formula and charge as typed, for spotting duplicates and variable names */
  key: string
  base: string
  display: string
  atoms: Counts
  charge: bigint
  /** digits, parens, hydrate dots, charges or states: things plain math never writes */
  marked: boolean
}

export interface Reaction {
  arrow: string
  left: Species[]
  right: Species[]
}

export type Balance =
  | { kind: 'balanced'; coefficients: bigint[]; display: string }
  | { kind: 'underdetermined' }
  | { kind: 'impossible' }

const SUB_DIGITS = '₀₁₂₃₄₅₆₇₈₉'
const SUP_DIGITS = '⁰¹²³⁴⁵⁶⁷⁸⁹'
const HYDRATE_DOTS = '·•∙⋅.'
const STATES = ['(aq)', '(s)', '(l)', '(g)']
const ARROW_RE = /->|→|⟶|=>|=/g

const sub = (digits: string) => digits.replace(/\d/g, (d) => SUB_DIGITS[Number(d)]!)
const sup = (digits: string) => digits.replace(/\d/g, (d) => SUP_DIGITS[Number(d)]!)

function add(into: Counts, from: Counts, times: bigint): void {
  for (const [el, n] of from) into.set(el, (into.get(el) ?? 0n) + n * times)
}

class Scanner {
  i = 0
  readonly s: string
  constructor(s: string) {
    this.s = s
  }
  peek(k = 0): string {
    return this.s[this.i + k] ?? ''
  }
  digits(): string {
    const m = /^\d+/.exec(this.s.slice(this.i))
    if (!m) return ''
    this.i += m[0].length
    return m[0]
  }
  ws(): void {
    while (/\s/.test(this.peek())) this.i++
  }
  done(): boolean {
    return this.i >= this.s.length
  }
}

/** Subscript count after an element or group; 0 is not a count. */
function count(sc: Scanner): { n: bigint; text: string } | null {
  const text = sc.digits()
  if (!text) return { n: 1n, text: '' }
  const n = BigInt(text)
  return n > 0n ? { n, text } : null
}

const CLOSE: Record<string, string> = { '(': ')', '[': ']' }

function groups(sc: Scanner): { atoms: Counts; display: string; plain: string } | null {
  const atoms: Counts = new Map()
  let display = ''
  let plain = ''
  for (;;) {
    const c = sc.peek()
    if (/[A-Z]/.test(c)) {
      const el = /[a-z]/.test(sc.peek(1)) ? c + sc.peek(1) : c
      sc.i += el.length
      const k = count(sc)
      if (!k) return null
      atoms.set(el, (atoms.get(el) ?? 0n) + k.n)
      display += el + sub(k.text)
      plain += el + k.text
    } else if (c in CLOSE && !STATES.some((st) => sc.s.startsWith(st, sc.i))) {
      sc.i++
      const inner = groups(sc)
      if (!inner || sc.peek() !== CLOSE[c]) return null
      sc.i++
      const k = count(sc)
      if (!k) return null
      add(atoms, inner.atoms, k.n)
      display += c + inner.display + CLOSE[c] + sub(k.text)
      plain += c + inner.plain + CLOSE[c] + k.text
    } else break
  }
  if (!atoms.size) return null
  return { atoms, display, plain }
}

function formula(sc: Scanner): { atoms: Counts; display: string; plain: string; marked: boolean } | null {
  const first = groups(sc)
  if (!first) return null
  const atoms: Counts = new Map(first.atoms)
  let { display, plain } = first
  let marked = /[\d([]/.test(first.plain)
  while (HYDRATE_DOTS.includes(sc.peek()) && /[\dA-Z([]/.test(sc.peek(1))) {
    sc.i++
    const mult = sc.digits()
    const n = mult ? BigInt(mult) : 1n
    const part = groups(sc)
    if (!part || n === 0n) return null
    add(atoms, part.atoms, n)
    display += `·${mult}${part.display}`
    plain += `·${mult}${part.plain}`
    marked = true
  }
  return { atoms, display, plain, marked }
}

function signedCharge(mag: string, sign: string): bigint {
  const n = mag ? BigInt(mag) : 1n
  return sign === '-' ? -n : n
}

/** `^2+`, `^+2`, `^{2+}`, `²⁺`; a bare `+`/`-` only straight after a letter or bracket. */
function charge(sc: Scanner, lastChar: string): { n: bigint; ok: boolean } {
  const rest = sc.s.slice(sc.i)
  let m = /^\^(?:\{(\d*)([+-])\}|\((\d*)([+-])\)|([+-])(\d+)|(\d*)([+-]))/.exec(rest)
  if (m) {
    sc.i += m[0].length
    const mag = m[1] ?? m[3] ?? m[6] ?? m[7] ?? ''
    const sign = m[2] ?? m[4] ?? m[5] ?? m[8]!
    return { n: signedCharge(mag, sign), ok: true }
  }
  m = /^(?:([⁰¹²³⁴⁵⁶⁷⁸⁹]*)([⁺⁻])|([⁺⁻])([⁰¹²³⁴⁵⁶⁷⁸⁹]+))/.exec(rest)
  if (m) {
    sc.i += m[0].length
    const mag = (m[1] ?? m[4] ?? '').replace(/[⁰¹²³⁴⁵⁶⁷⁸⁹]/g, (d) => String(SUP_DIGITS.indexOf(d)))
    return { n: signedCharge(mag, (m[2] ?? m[3]) === '⁻' ? '-' : '+'), ok: true }
  }
  const c = sc.peek()
  if (c !== '+' && c !== '-') return { n: 0n, ok: true }
  if (c === '-' && sc.peek(1) === '>') return { n: 0n, ok: true }
  // `+` followed by another species is the separator, not a charge
  const after = sc.s.slice(sc.i + 1).trimStart()
  if (c === '+' && after !== '' && !after.startsWith('+')) return { n: 0n, ok: true }
  // `Fe2+` could be Fe₂⁺ or Fe²⁺, so a bare sign after a digit is refused
  if (/\d/.test(lastChar)) return { n: 0n, ok: false }
  sc.i++
  return { n: c === '-' ? -1n : 1n, ok: true }
}

/** A leading coefficient is read and ignored: the line gets balanced from scratch. */
function species(sc: Scanner): Species | null {
  const coef = sc.digits()
  if (coef) sc.ws()
  const f = formula(sc)
  if (!f) return null
  const q = charge(sc, sc.s[sc.i - 1] ?? '')
  if (!q.ok) return null
  const state = STATES.find((st) => sc.s.startsWith(st, sc.i)) ?? ''
  sc.i += state.length
  const mag = q.n < 0n ? -q.n : q.n
  const sign = q.n < 0n ? '-' : '+'
  const magText = mag === 1n ? '' : String(mag)
  const chargeDisplay = q.n === 0n ? '' : sup(magText) + (sign === '-' ? '⁻' : '⁺')
  const chargePlain = q.n === 0n ? '' : `^${magText}${sign}`
  return {
    key: f.plain + chargePlain,
    base: f.plain,
    display: f.display + chargeDisplay + state,
    atoms: f.atoms,
    charge: q.n,
    marked: f.marked || q.n !== 0n || state !== '',
  }
}

function side(text: string): Species[] | null {
  const sc = new Scanner(text)
  const out: Species[] = []
  sc.ws()
  for (;;) {
    const sp = species(sc)
    if (!sp) return null
    out.push(sp)
    sc.ws()
    if (sc.done()) return out
    if (sc.peek() !== '+') return null
    sc.i++
    sc.ws()
  }
}

/** Unicode subscripts are read as digits so a shown answer can be typed back in. */
export function parseReaction(text: string): Reaction | null {
  const s = text.replace(/[₀-₉]/g, (d) => String(SUB_DIGITS.indexOf(d))).trim()
  const arrows = [...s.matchAll(ARROW_RE)]
  if (arrows.length !== 1) return null
  const a = arrows[0]!
  const left = side(s.slice(0, a.index))
  const right = side(s.slice(a.index + a[0].length))
  if (!left || !right) return null
  return { arrow: a[0], left, right }
}

function gcd(a: bigint, b: bigint): bigint {
  a = a < 0n ? -a : a
  b = b < 0n ? -b : b
  while (b) [a, b] = [b, a % b]
  return a
}

function reduce(v: bigint[]): bigint[] {
  const g = v.reduce(gcd, 0n)
  return g > 1n ? v.map((x) => x / g) : v
}

/** Integer null space basis by fraction-free elimination to reduced row echelon form. */
export function nullSpace(matrix: bigint[][], cols: number): bigint[][] {
  const m = matrix.map((r) => [...r])
  const pivots: number[] = []
  let r = 0
  for (let c = 0; c < cols && r < m.length; c++) {
    const p = m.findIndex((row, i) => i >= r && row[c] !== 0n)
    if (p < 0) continue
    ;[m[r], m[p]] = [m[p]!, m[r]!]
    const pr = m[r]!
    for (let i = 0; i < m.length; i++) {
      const row = m[i]!
      if (i === r || row[c] === 0n) continue
      const a = pr[c]!
      const b = row[c]!
      m[i] = reduce(row.map((x, j) => x * a - pr[j]! * b))
    }
    pivots.push(c)
    r++
  }
  const lcm = pivots.reduce((l, c, i) => {
    const p = m[i]![c]!
    const ap = p < 0n ? -p : p
    return (l / gcd(l, ap)) * ap
  }, 1n)
  const basis: bigint[][] = []
  for (let f = 0; f < cols; f++) {
    if (pivots.includes(f)) continue
    const v = new Array<bigint>(cols).fill(0n)
    v[f] = lcm
    pivots.forEach((c, i) => {
      v[c] = (-m[i]![f]! * lcm) / m[i]![c]!
    })
    basis.push(reduce(v))
  }
  return basis
}

function allPositive(v: bigint[]): boolean {
  return v.every((x) => x > 0n)
}

/**
 * Whether some combination of the basis is positive in every species, by Fourier–Motzkin
 * on the strict system `y·b_j > 0`. Too many constraints gives up, and blank beats a guess.
 */
function hasPositiveCombination(basis: bigint[][]): boolean {
  const k = basis.length
  let rows = basis[0]!.map((_, j) => basis.map((b) => b[j]!))
  for (let v = k - 1; v >= 0; v--) {
    const pos = rows.filter((r) => r[v]! > 0n)
    const neg = rows.filter((r) => r[v]! < 0n)
    const next = rows.filter((r) => r[v] === 0n)
    for (const p of pos) {
      for (const n of neg) next.push(reduce(p.map((x, i) => x * -n[v]! + n[i]! * p[v]!)))
    }
    rows = [...new Map(next.map((r) => [r.join(','), r])).values()]
    if (rows.length > 5000) return false
  }
  return rows.length === 0
}

export function balance(reaction: Reaction): Balance {
  const { left, right } = reaction
  const all = [...left, ...right]
  // a repeated species (on one side or both) adds a free direction that isn't a reaction
  if (new Set(all.map((s) => s.key)).size !== all.length) return { kind: 'impossible' }
  const elements = [...new Set(all.flatMap((s) => [...s.atoms.keys()]))]
  const signOf = (i: number) => (i < left.length ? 1n : -1n)
  const rows = elements.map((el) => all.map((s, i) => (s.atoms.get(el) ?? 0n) * signOf(i)))
  if (all.some((s) => s.charge !== 0n)) rows.push(all.map((s, i) => s.charge * signOf(i)))
  const basis = nullSpace(rows, all.length)
  if (basis.length === 0) return { kind: 'impossible' }
  if (basis.length > 1) return hasPositiveCombination(basis) ? { kind: 'underdetermined' } : { kind: 'impossible' }
  let v = basis[0]!
  if (v[0]! < 0n) v = v.map((x) => -x)
  if (!allPositive(v)) return { kind: 'impossible' }
  if (!rows.every((row) => row.reduce((s, x, j) => s + x * v[j]!, 0n) === 0n)) return { kind: 'impossible' }
  const half = (list: Species[], off: number) =>
    list.map((s, i) => (v[i + off] === 1n ? s.display : `${v[i + off]} ${s.display}`)).join(' + ')
  return {
    kind: 'balanced',
    coefficients: v,
    display: `${half(left, 0)} → ${half(right, left.length)}`,
  }
}

function marked(r: Reaction): boolean {
  return r.left.length > 1 || r.right.length > 1 || [...r.left, ...r.right].some((s) => s.marked)
}

/**
 * A reaction written with an arrow that looks chemical; `100 C -> F` doesn't.
 * These answer blank rather than falling back to other readings.
 */
export function isReactionInput(text: string): boolean {
  const r = parseReaction(text)
  return Boolean(r && r.arrow !== '=' && marked(r))
}

/**
 * The shown answer for a reaction line, or null to leave the line to the math engine.
 * With `=` the line is only chemistry when it balances and uses no known variable names,
 * so `x + y = 3`, `H = CO` or `CO2 = C + O2` with those defined stay math.
 */
export function chemAnswer(text: string, isName: (name: string) => boolean = () => false): string | null {
  if (!/->|→|⟶|=/.test(text)) return null
  const r = parseReaction(text)
  if (!r) return null
  const strict = r.arrow === '='
  if (strict && [...r.left, ...r.right].some((s) => isName(s.base))) return null
  if (!strict && !marked(r)) return null
  const b = balance(r)
  if (b.kind === 'balanced') return b.display
  if (strict) return null
  return b.kind === 'underdetermined' ? 'underdetermined' : ''
}

/** Copy text for a shown balanced reaction: ascii digits, caret charges and `->`. Anything else is unchanged. */
export function chemCopyText(shown: string): string {
  if (!shown.includes(' → ')) return shown
  const plain = shown
    .replace(/[₀-₉]/g, (d) => String(SUB_DIGITS.indexOf(d)))
    .replace(/([⁰¹²³⁴⁵⁶⁷⁸⁹]*)([⁺⁻])/g, (_, mag: string, sign: string) => {
      const m = mag.replace(/[⁰¹²³⁴⁵⁶⁷⁸⁹]/g, (d) => String(SUP_DIGITS.indexOf(d)))
      return `^${m}${sign === '⁻' ? '-' : '+'}`
    })
    .replace(' → ', ' -> ')
  return parseReaction(plain) ? plain : shown
}
