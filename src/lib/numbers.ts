// the numbers game: 24 (four numbers, make exactly 24) and countdown (six numbers, get near a target).
// pure logic: commands, puzzles, a strict little parser, the solvers and the scoring. NumbersPanel plays it

export const NUMBERS_HINT = '↵ play 24 or countdown'

// '24 game', 'game of 24', 'countdown', 'numbers game', and the games-list name '24 / countdown'.
// bare '24' stays a calculation
const NUMBERS_RE = /^(?:24 ?game|game of 24|count ?down|numbers game|24 ?\/ ?countdown)$/i

export function isNumbersCommand(text: string): boolean {
  return NUMBERS_RE.test(text.trim().replace(/\s+/g, ' '))
}

export type Mode = '24' | 'countdown'
export type Rng = () => number

export const TARGET_24 = 24
export const SECONDS_24 = 60
export const SECONDS_COUNTDOWN = 45
export const LARGE = [25, 50, 75, 100] as const
export const DEFAULT_LARGE = 2

// ---- exact fractions, so 8 ÷ 3 × 3 is 8 and not 7.999…

export type Frac = { n: number; d: number }

const gcd = (a: number, b: number): number => {
  a = Math.abs(a)
  b = Math.abs(b)
  while (b) [a, b] = [b, a % b]
  return a || 1
}

export function frac(n: number, d = 1): Frac {
  if (d < 0) [n, d] = [-n, -d]
  const g = gcd(n, d)
  return { n: n / g, d: d / g }
}

const add = (a: Frac, b: Frac) => frac(a.n * b.d + b.n * a.d, a.d * b.d)
const sub = (a: Frac, b: Frac) => frac(a.n * b.d - b.n * a.d, a.d * b.d)
const mul = (a: Frac, b: Frac) => frac(a.n * b.n, a.d * b.d)
const div = (a: Frac, b: Frac) => frac(a.n * b.d, a.d * b.n)

export const isWhole = (f: Frac) => f.d === 1

export function fracText(f: Frac): string {
  const neg = f.n < 0 ? '−' : ''
  return f.d === 1 ? `${neg}${Math.abs(f.n)}` : `${neg}${Math.abs(f.n)}/${f.d}`
}

// ---- expressions as the solvers write them: fewest brackets, the game's own symbols

type Op = '+' | '−' | '×' | '÷'
type Built = { e: string; op: Op | null }

function join(a: Built, op: Op, b: Built): Built {
  const loose = (x: Built) => x.op === '+' || x.op === '−'
  const wrap = (x: Built, need: boolean) => (need ? `(${x.e})` : x.e)
  let left = a.e
  let right = b.e
  if (op === '−') right = wrap(b, loose(b))
  if (op === '×') {
    left = wrap(a, loose(a))
    right = wrap(b, loose(b))
  }
  if (op === '÷') {
    left = wrap(a, loose(a))
    right = wrap(b, b.op != null)
  }
  return { e: `${left} ${op} ${right}`, op }
}

// ---- 24

type Item24 = Built & { v: Frac }

// one way to make target from all of nums, or null when there is none
export function solve24(nums: readonly number[], target = TARGET_24): string | null {
  const goal = frac(target)
  const rec = (items: Item24[]): string | null => {
    if (items.length === 1) return items[0]!.v.n === goal.n && items[0]!.v.d === goal.d ? items[0]!.e : null
    for (let i = 0; i < items.length; i++) {
      for (let j = i + 1; j < items.length; j++) {
        const a = items[i]!
        const b = items[j]!
        const rest = items.filter((_, k) => k !== i && k !== j)
        const next: Item24[] = [
          { v: add(a.v, b.v), ...join(a, '+', b) },
          { v: mul(a.v, b.v), ...join(a, '×', b) },
          { v: sub(a.v, b.v), ...join(a, '−', b) },
          { v: sub(b.v, a.v), ...join(b, '−', a) },
        ]
        if (b.v.n !== 0) next.push({ v: div(a.v, b.v), ...join(a, '÷', b) })
        if (a.v.n !== 0) next.push({ v: div(b.v, a.v), ...join(b, '÷', a) })
        for (const c of next) {
          const found = rec([...rest, c])
          if (found) return found
        }
      }
    }
    return null
  }
  return rec(nums.map((n) => ({ v: frac(n), e: String(n), op: null })))
}

const pick = (rng: Rng, n: number) => Math.min(n - 1, Math.floor(rng() * n))

// four numbers from 1–13 that make 24
export function deal24(rng: Rng = Math.random): { nums: number[]; solution: string } {
  for (;;) {
    const nums = Array.from({ length: 4 }, () => 1 + pick(rng, 13))
    const solution = solve24(nums)
    if (solution) return { nums, solution }
  }
}

// ---- countdown: whole numbers, each at most once, every step a positive whole number

export type Best = { value: number; expr: string }

type ItemCd = Built & { v: number }

// the closest anyone can get to target (exact when it's reachable), with one way to get there
export function solveCountdown(nums: readonly number[], target: number): Best {
  let best: Best = { value: nums[0] ?? 0, expr: String(nums[0] ?? 0) }
  for (const n of nums) if (Math.abs(n - target) < Math.abs(best.value - target)) best = { value: n, expr: String(n) }
  const seen = new Set<string>()
  const rec = (items: ItemCd[]) => {
    if (best.value === target || items.length < 2) return
    const key = items.map((x) => x.v).sort((a, b) => a - b).join(',')
    if (seen.has(key)) return
    seen.add(key)
    for (let i = 0; i < items.length; i++) {
      for (let j = i + 1; j < items.length; j++) {
        let a = items[i]!
        let b = items[j]!
        if (a.v < b.v) [a, b] = [b, a]
        const rest = items.filter((_, k) => k !== i && k !== j)
        const next: ItemCd[] = [{ v: a.v + b.v, ...join(a, '+', b) }]
        if (b.v !== 1) next.push({ v: a.v * b.v, ...join(a, '×', b) })
        if (a.v > b.v && a.v - b.v !== b.v) next.push({ v: a.v - b.v, ...join(a, '−', b) })
        if (b.v !== 1 && a.v % b.v === 0 && a.v / b.v !== b.v) next.push({ v: a.v / b.v, ...join(a, '÷', b) })
        for (const c of next) {
          if (Math.abs(c.v - target) < Math.abs(best.value - target)) best = { value: c.v, expr: c.e }
          if (best.value === target) return
          rec([...rest, c])
          if (best.value === target) return
        }
      }
    }
  }
  rec(nums.map((n) => ({ v: n, e: String(n), op: null })))
  return best
}

// six numbers (large from 25 50 75 100, small from two of each 1–10, as on the show) and a target 101–999
// that they can make exactly
export function dealCountdown(large = DEFAULT_LARGE, rng: Rng = Math.random): { nums: number[]; target: number; best: Best } {
  const nLarge = Math.max(0, Math.min(4, Math.round(large)))
  for (let tries = 0; ; tries++) {
    const bigs: number[] = [...LARGE]
    const smalls = Array.from({ length: 20 }, (_, i) => 1 + (i % 10))
    const nums: number[] = []
    for (let i = 0; i < nLarge; i++) nums.push(bigs.splice(pick(rng, bigs.length), 1)[0]!)
    while (nums.length < 6) nums.push(smalls.splice(pick(rng, smalls.length), 1)[0]!)
    const target = 101 + pick(rng, 899)
    const best = solveCountdown(nums, target)
    // nearly every draw is exact; after a few misses, settle for the target being one of the numbers' best
    if (best.value === target) return { nums, target, best }
    if (tries >= 30) return { nums, target: best.value >= 101 && best.value <= 999 ? best.value : target, best }
  }
}

// the show's points: 10 exact, 7 within 5, 5 within 10
export function countdownPoints(target: number, value: number | null): number {
  if (value == null) return 0
  const off = Math.abs(target - value)
  return off === 0 ? 10 : off <= 5 ? 7 : off <= 10 ? 5 : 0
}

// ---- the player's expression: digits, + − × ÷ (or - * / x), brackets. no unary minus, no eval

type Node = { k: 'num'; v: number } | { k: 'op'; op: Op; a: Node; b: Node }

export type Parsed =
  | { kind: 'empty' }
  | { kind: 'partial' }
  | { kind: 'error'; message: string }
  | { kind: 'ok'; node: Node; used: number[] }

const OPS: Record<string, Op> = { '+': '+', '-': '−', '−': '−', '–': '−', '*': '×', '×': '×', x: '×', X: '×', '·': '×', '/': '÷', '÷': '÷' }

type Tok = { t: 'num'; v: number } | { t: 'op'; op: Op } | { t: '(' } | { t: ')' }

export function parseExpr(text: string): Parsed {
  const toks: Tok[] = []
  for (let i = 0; i < text.length; ) {
    const c = text[i]!
    if (/\s/.test(c)) i++
    else if (/[0-9]/.test(c)) {
      let j = i
      while (j < text.length && /[0-9]/.test(text[j]!)) j++
      toks.push({ t: 'num', v: Number(text.slice(i, j)) })
      i = j
    } else if (c === '(' || c === ')') {
      toks.push({ t: c })
      i++
    } else if (OPS[c]) {
      toks.push({ t: 'op', op: OPS[c]! })
      i++
    } else return { kind: 'error', message: `only + − × ÷ and brackets, not ${c}` }
  }
  if (!toks.length) return { kind: 'empty' }
  let at = 0
  let ranOut = false
  const used: number[] = []
  class Bad extends Error {}
  const fail = (message: string): never => {
    throw new Bad(message)
  }
  const factor = (): Node => {
    const tok = toks[at]
    if (!tok) {
      ranOut = true
      return fail('unfinished')
    }
    if (tok.t === 'num') {
      at++
      used.push(tok.v)
      return { k: 'num', v: tok.v }
    }
    if (tok.t === '(') {
      at++
      const inner = expr()
      if (!toks[at]) {
        ranOut = true
        return fail('unfinished')
      }
      if (toks[at]!.t !== ')') return fail('a bracket is missing')
      at++
      return inner
    }
    if (tok.t === 'op' && tok.op === '−') return fail('no negative numbers')
    return fail(tok.t === ')' ? 'a ) with nothing before it' : `${tok.op} needs a number before it`)
  }
  const level = (ops: Op[], next: () => Node) => (): Node => {
    let node = next()
    for (;;) {
      const tok = toks[at]
      if (!tok || tok.t !== 'op' || !ops.includes(tok.op)) return node
      at++
      node = { k: 'op', op: tok.op, a: node, b: next() }
    }
  }
  const term = level(['×', '÷'], factor)
  const expr = level(['+', '−'], term)
  try {
    const node = expr()
    if (at < toks.length) {
      const tok = toks[at]!
      return { kind: 'error', message: tok.t === ')' ? 'a ( is missing' : 'an operator is missing' }
    }
    return { kind: 'ok', node, used }
  } catch (err) {
    if (!(err instanceof Bad)) throw err
    return ranOut ? { kind: 'partial' } : { kind: 'error', message: err.message }
  }
}

// value of a parsed tree. strict: countdown's rules, every step a positive whole number
function evaluate(node: Node, strict: boolean): Frac | string {
  if (node.k === 'num') return frac(node.v)
  const a = evaluate(node.a, strict)
  if (typeof a === 'string') return a
  const b = evaluate(node.b, strict)
  if (typeof b === 'string') return b
  if (node.op === '÷' && b.n === 0) return 'can’t divide by 0'
  const v = node.op === '+' ? add(a, b) : node.op === '−' ? sub(a, b) : node.op === '×' ? mul(a, b) : div(a, b)
  if (strict && !isWhole(v)) return '÷ must come out whole'
  if (strict && v.n <= 0) return 'every step must stay above 0'
  return v
}

// whether used fits nums: all of them exactly once (24), or each at most once (countdown)
export function numbersProblem(used: readonly number[], nums: readonly number[], all: boolean): string | null {
  const left = [...nums]
  for (const n of used) {
    const at = left.indexOf(n)
    if (at < 0) return nums.includes(n) ? `${n} is used too often` : `${n} isn’t one of the numbers`
    left.splice(at, 1)
  }
  if (all && left.length) return `use ${left.join(', ')} too`
  return null
}

export type Check = {
  // the value so far, when it parses
  value: Frac | null
  // what's wrong, if anything; partial input isn't wrong yet
  problem: string | null
  // a finished, legal answer
  valid: boolean
}

export function checkExpr(text: string, nums: readonly number[], mode: Mode): Check {
  const p = parseExpr(text)
  if (p.kind === 'empty' || p.kind === 'partial') return { value: null, problem: null, valid: false }
  if (p.kind === 'error') return { value: null, problem: p.message, valid: false }
  const v = evaluate(p.node, mode === 'countdown')
  if (typeof v === 'string') return { value: null, problem: v, valid: false }
  const problem = numbersProblem(p.used, nums, mode === '24')
  return { value: v, problem, valid: problem == null }
}

export function solves24(text: string, nums: readonly number[]): boolean {
  const c = checkExpr(text, nums, '24')
  return c.valid && c.value != null && c.value.n === TARGET_24 && c.value.d === 1
}

// ---- kept between games

export type NumbersStats = {
  mode: Mode
  large: number
  streak24: number
  best24: number
  solved24: number
  rounds: number
  points: number
  exactStreak: number
  bestExact: number
}

const STATS_KEY = 'qcalc-numbers'

export const FRESH_STATS: NumbersStats = { mode: '24', large: DEFAULT_LARGE, streak24: 0, best24: 0, solved24: 0, rounds: 0, points: 0, exactStreak: 0, bestExact: 0 }

export function loadStats(): NumbersStats {
  try {
    const raw = localStorage.getItem(STATS_KEY)
    const saved = raw ? (JSON.parse(raw) as Partial<NumbersStats>) : {}
    const out = { ...FRESH_STATS }
    for (const k of Object.keys(out) as (keyof NumbersStats)[]) {
      const v = saved[k]
      if (k === 'mode') out.mode = v === 'countdown' ? 'countdown' : '24'
      else if (typeof v === 'number' && Number.isFinite(v) && v >= 0) (out as Record<string, unknown>)[k] = v
    }
    return out
  } catch {
    return { ...FRESH_STATS }
  }
}

export function saveStats(stats: NumbersStats): void {
  try {
    localStorage.setItem(STATS_KEY, JSON.stringify(stats))
  } catch {
    // private window: it just isn't remembered
  }
}

// a 24 round ends: solved keeps the streak going, anything else (time, giving up) ends it
export function record24(s: NumbersStats, solved: boolean): NumbersStats {
  const streak24 = solved ? s.streak24 + 1 : 0
  return { ...s, streak24, best24: Math.max(s.best24, streak24), solved24: s.solved24 + (solved ? 1 : 0) }
}

export function recordCountdown(s: NumbersStats, points: number): NumbersStats {
  const exactStreak = points === 10 ? s.exactStreak + 1 : 0
  return { ...s, rounds: s.rounds + 1, points: s.points + points, exactStreak, bestExact: Math.max(s.bestExact, exactStreak) }
}
