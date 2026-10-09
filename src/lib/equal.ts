// equal: guess the hidden 8-character equation in six tries. this is the pure part: checking a guess,
// colouring it the wordle way, making puzzles, the daily pick and the saved stats

export const LEN = 8
export const TRIES = 6
export const KEYS = '0123456789+-*/='

export type Mark = 'hit' | 'near' | 'miss'

export const EQUAL_HINT = '↵ play equal'

// 'equal' or 'nerdle', whole words only, so nothing a calculator would read
export function isEqualCommand(text: string): boolean {
  return /^(?:equal|nerdle)$/i.test(text.trim())
}

// a run of digits that's a fair number: no leading zeros
const NUM = /^(?:0|[1-9]\d*)$/

// the left side, evaluated with * and / before + and -; null when it isn't a whole-number sum
// (a malformed side, or a division that doesn't come out exact)
export function evaluate(expr: string): number | null {
  const parts = expr.split(/([+\-*/])/)
  if (parts.length < 3) return null
  for (let i = 0; i < parts.length; i += 2) if (!NUM.test(parts[i])) return null
  let total = 0
  let sign = 1
  let term = Number(parts[0])
  for (let i = 1; i < parts.length; i += 2) {
    const op = parts[i]
    const n = Number(parts[i + 1])
    if (op === '*') term *= n
    else if (op === '/') {
      if (n === 0 || term % n !== 0) return null
      term /= n
    } else {
      total += sign * term
      sign = op === '+' ? 1 : -1
      term = n
    }
  }
  return total + sign * term
}

// why a guess can't be played, or null when it can
export function guessProblem(guess: string): string | null {
  if (guess.length < LEN) return 'not enough characters'
  if (guess.length > LEN || [...guess].some((c) => !KEYS.includes(c))) return 'only digits, + - × ÷ and ='
  const sides = guess.split('=')
  if (sides.length !== 2) return 'it needs exactly one ='
  const [left, right] = sides
  if (!NUM.test(right)) return 'the right side is just a number'
  if (!/[+\-*/]/.test(left)) return 'the left side needs a sum'
  const value = evaluate(left)
  if (value == null) {
    if (/(?:^|[+\-*/])0\d/.test(left)) return 'no leading zeros'
    if (/^[+\-*/]|[+\-*/]$|[+\-*/]{2}/.test(left)) return 'that isn’t a sum'
    return 'division has to come out exact'
  }
  if (value !== Number(right)) return 'that doesn’t add up'
  return null
}

export const isValidEquation = (s: string) => guessProblem(s) == null

// green for the right place, yellow for elsewhere, grey otherwise. repeats are counted: a character only
// turns yellow as many times as the answer has spare copies of it after the greens
export function score(guess: string, answer: string): Mark[] {
  const marks: Mark[] = Array.from({ length: guess.length }, () => 'miss')
  const spare = new Map<string, number>()
  for (let i = 0; i < guess.length; i++) {
    if (guess[i] === answer[i]) marks[i] = 'hit'
    else spare.set(answer[i], (spare.get(answer[i]) ?? 0) + 1)
  }
  for (let i = 0; i < guess.length; i++) {
    if (marks[i] === 'hit') continue
    const left = spare.get(guess[i]) ?? 0
    if (left > 0) {
      marks[i] = 'near'
      spare.set(guess[i], left - 1)
    }
  }
  return marks
}

const RANK: Record<Mark, number> = { miss: 0, near: 1, hit: 2 }

// what the keypad knows about each character: its best mark so far
export function keyMarks(guesses: string[], answer: string): Record<string, Mark> {
  const out: Record<string, Mark> = {}
  for (const g of guesses) {
    score(g, answer).forEach((m, i) => {
      const was = out[g[i]]
      if (!was || RANK[m] > RANK[was]) out[g[i]] = m
    })
  }
  return out
}

// a small seeded generator (mulberry32), so a date always gives the same puzzle
export function rng(seed: number): () => number {
  let a = seed >>> 0
  return () => {
    a = (a + 0x6d2b79f5) >>> 0
    let t = a
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

const pick = <T>(rand: () => number, xs: readonly T[]): T => xs[Math.floor(rand() * xs.length)]

// a random number of that many digits, never starting with 0 (and never 0 itself)
function number(rand: () => number, digits: number): string {
  let s = String(1 + Math.floor(rand() * 9))
  while (s.length < digits) s += Math.floor(rand() * 10)
  return s
}

const OPS = ['+', '-', '*', '/'] as const

// one puzzle: build a left side by chance and keep it when its answer fills the rest of the 8 exactly.
// no zero operands, no answer of 0, and nothing as dull as ×1 or ÷1
export function generate(rand: () => number): string {
  for (let tries = 0; tries < 100000; tries++) {
    const rightLen = pick(rand, [1, 2, 2, 2, 3, 3])
    const leftLen = LEN - 1 - rightLen
    const ops = pick(rand, leftLen >= 5 ? [1, 1, 2, 2] : [1])
    let digits = leftLen - ops
    if (digits < ops + 1) continue
    // split the digits over ops+1 numbers of 1 to 3 digits each
    const lens = Array.from({ length: ops + 1 }, () => 1)
    digits -= ops + 1
    while (digits > 0) {
      const i = Math.floor(rand() * lens.length)
      if (lens[i] < 3) {
        lens[i]++
        digits--
      }
    }
    const nums = lens.map((d) => number(rand, d))
    const signs = Array.from({ length: ops }, () => pick(rand, OPS))
    if (signs.some((op, i) => (op === '*' || op === '/') && (nums[i + 1] === '1' || nums[i] === '1'))) continue
    let left = nums[0]
    signs.forEach((op, i) => (left += op + nums[i + 1]))
    const value = evaluate(left)
    if (value == null || value <= 0) continue
    const eq = `${left}=${value}`
    if (eq.length === LEN && isValidEquation(eq)) return eq
  }
  return '12+35=47'
}

// local dates, as yyyy-mm-dd
export function dateKey(d: Date): string {
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`
}

export function dayBefore(key: string): string {
  const [y, m, d] = key.split('-').map(Number)
  return dateKey(new Date(y, m - 1, d - 1))
}

// puzzle numbers count from the first day
const EPOCH = Date.UTC(2026, 0, 1)
export function dailyNumber(key: string): number {
  const [y, m, d] = key.split('-').map(Number)
  return Math.round((Date.UTC(y, m - 1, d) - EPOCH) / 86400000) + 1
}

function hash(s: string): number {
  let h = 2166136261
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619)
  return h >>> 0
}

// everyone gets the same equation on the same local date
export function dailyAnswer(key: string): string {
  return generate(rng(hash(`equal:${key}`)))
}

// ---- saved: today's game, the streak and how the wins went

export type DailyGame = { date: string; guesses: string[] }

export type EqualStats = {
  played: number
  wins: number
  streak: number
  best: number
  // the last daily won, for the streak
  lastWin: string | null
  // wins by how many guesses they took
  dist: number[]
}

export type EqualSave = { daily: DailyGame | null; stats: EqualStats }

export const emptyStats = (): EqualStats => ({ played: 0, wins: 0, streak: 0, best: 0, lastWin: null, dist: Array(TRIES).fill(0) })

export function isWon(guesses: string[], answer: string): boolean {
  return guesses.includes(answer)
}

export function isDone(guesses: string[], answer: string): boolean {
  return isWon(guesses, answer) || guesses.length >= TRIES
}

// a finished daily, into the stats. a win the day after a win keeps the streak going
export function recordDaily(stats: EqualStats, date: string, guesses: string[], answer: string): EqualStats {
  const won = isWon(guesses, answer)
  const dist = stats.dist.slice()
  if (won) dist[guesses.length - 1]++
  const streak = won ? (stats.lastWin === dayBefore(date) ? stats.streak + 1 : 1) : 0
  return {
    played: stats.played + 1,
    wins: stats.wins + (won ? 1 : 0),
    streak,
    best: Math.max(stats.best, streak),
    lastWin: won ? date : stats.lastWin,
    dist,
  }
}

// the streak as it stands today: it lapses once a day goes by without a win
export function currentStreak(stats: EqualStats, today: string): number {
  return stats.lastWin === today || stats.lastWin === dayBefore(today) ? stats.streak : 0
}

const SAVE_KEY = 'qcalc-equal'

export function loadSave(): EqualSave {
  try {
    const raw = JSON.parse(localStorage.getItem(SAVE_KEY) ?? 'null') as Partial<EqualSave> | null
    const stats = { ...emptyStats(), ...(raw?.stats ?? {}) }
    if (!Array.isArray(stats.dist) || stats.dist.length !== TRIES) stats.dist = Array(TRIES).fill(0)
    const d = raw?.daily
    const daily = d && typeof d.date === 'string' && Array.isArray(d.guesses) ? { date: d.date, guesses: d.guesses.filter(isValidEquation).slice(0, TRIES) } : null
    return { daily, stats }
  } catch {
    return { daily: null, stats: emptyStats() }
  }
}

export function storeSave(save: EqualSave): void {
  try {
    localStorage.setItem(SAVE_KEY, JSON.stringify(save))
  } catch {
    // private window: it just isn't remembered
  }
}

// ---- sharing

const SQUARE: Record<Mark, string> = { hit: '🟩', near: '🟨', miss: '⬛' }

export function shareText(guesses: string[], answer: string, title: string): string {
  const tries = isWon(guesses, answer) ? String(guesses.length) : 'X'
  const rows = guesses.map((g) => score(g, answer).map((m) => SQUARE[m]).join(''))
  return [`${title} ${tries}/${TRIES}`, '', ...rows].join('\n')
}

// how a character shows on a tile or key
export const glyph = (c: string) => (c === '*' ? '×' : c === '/' ? '÷' : c === '-' ? '−' : c)
