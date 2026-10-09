// crack: a needle sweeps a safe dial; press as it crosses the dot. level n needs n hits in a row.
// pure and deterministic: time comes in as ms, randomness as a () => [0, 1) function

export const CRACK_HINT = '↵ crack the safe'

// 'crack', 'crack the safe', 'safe'. whole words only, so 'crack2' stays calculator input
export function isCrackCommand(text: string): boolean {
  return /^(?:crack(?: the safe)?|safe ?crack(?:er|ing)?)$/i.test(text.trim().replace(/\s+/g, ' '))
}

const TAU = Math.PI * 2

// how far either side of the dot's centre still counts, in radians (~9.5°, about the dot's drawn size)
export const HIT_WINDOW = 0.165
// a new dot never lands closer than this to the needle, either side (~50°)
export const MIN_GAP = 0.87
// …nor further ahead than this, so the wait for it stays short (270°)
export const MAX_AHEAD = TAU * 0.75
// radians per second at level 1, the step per level, and the ceiling
export const BASE_SPEED = 2.3
export const SPEED_STEP = 0.1
export const MAX_SPEED = 4.4
// the shake after a miss, and the click-open after a cleared level
export const FAIL_MS = 650
export const CLEAR_MS = 950
// a frame never moves the needle by more than this much time, so a stalled tab doesn't skip the dot
const MAX_STEP_MS = 50

export type CrackPhase = 'ready' | 'play' | 'fail' | 'clear'

export type CrackState = {
  level: number
  left: number // hits still needed this level
  angle: number // the needle, radians clockwise from 12 o'clock, in [0, 2π)
  dir: 1 | -1 // 1 = clockwise
  dot: number // the target, radians like angle
  ahead: number // how far the needle still has to travel to the dot's centre; negative once past it
  phase: CrackPhase
  at: number // ms of the last update
  since: number // ms the phase began
}

export type CrackEvent = 'start' | 'hit' | 'clear' | 'miss' | null

export function speedFor(level: number): number {
  return Math.min(MAX_SPEED, BASE_SPEED + SPEED_STEP * (Math.max(1, level) - 1))
}

const wrap = (a: number) => ((a % TAU) + TAU) % TAU

// a fresh dot somewhere ahead of the needle in its direction, never within MIN_GAP of it
function placeDot(s: CrackState, rand: () => number): CrackState {
  const ahead = MIN_GAP + rand() * (MAX_AHEAD - MIN_GAP)
  return { ...s, ahead, dot: wrap(s.angle + s.dir * ahead) }
}

// a level waiting for its first press; the needle keeps where it was
export function startLevel(level: number, now: number, rand: () => number, angle = 0, dir: 1 | -1 = 1): CrackState {
  const lv = Math.max(1, Math.floor(level))
  return placeDot(
    { level: lv, left: lv, angle: wrap(angle), dir, dot: 0, ahead: 0, phase: 'ready', at: now, since: now },
    rand,
  )
}

export function overDot(s: CrackState): boolean {
  return Math.abs(s.ahead) <= HIT_WINDOW
}

// moves the needle to now, fails the level if it has gone right past the dot, and ends the shake or the
// click-open when their time is up
export function tick(s: CrackState, now: number, rand: () => number): { state: CrackState; event: CrackEvent } {
  if (s.phase === 'fail' && now - s.since >= FAIL_MS)
    return { state: startLevel(s.level, now, rand, s.angle, s.dir), event: null }
  if (s.phase === 'clear' && now - s.since >= CLEAR_MS)
    return { state: startLevel(s.level + 1, now, rand, s.angle, s.dir), event: null }
  if (s.phase !== 'play') return { state: { ...s, at: now }, event: null }
  const dt = Math.min(MAX_STEP_MS, Math.max(0, now - s.at)) / 1000
  const move = speedFor(s.level) * dt
  const next = { ...s, angle: wrap(s.angle + s.dir * move), ahead: s.ahead - move, at: now }
  if (next.ahead < -HIT_WINDOW) return { state: { ...next, phase: 'fail', since: now }, event: 'miss' }
  return { state: next, event: null }
}

// space / ↵ / a click. in ready it starts the needle; in play it's a hit over the dot and a miss anywhere else
export function press(s: CrackState, now: number, rand: () => number): { state: CrackState; event: CrackEvent } {
  if (s.phase === 'ready') return { state: { ...s, phase: 'play', at: now, since: now }, event: 'start' }
  if (s.phase !== 'play') return { state: s, event: null }
  const { state: cur, event } = tick(s, now, rand)
  if (event === 'miss') return { state: cur, event }
  if (!overDot(cur)) return { state: { ...cur, phase: 'fail', since: now }, event: 'miss' }
  const left = cur.left - 1
  if (left === 0) return { state: { ...cur, left, ahead: 0, phase: 'clear', since: now }, event: 'clear' }
  const flipped: CrackState = { ...cur, left, dir: cur.dir === 1 ? -1 : 1 }
  return { state: placeDot(flipped, rand), event: 'hit' }
}

// the furthest level reached, kept across runs
const KEY = 'qcalc-crack'

export function parseBest(raw: string | null): number {
  const n = Number(raw)
  return Number.isInteger(n) && n >= 1 && n <= 9999 ? n : 1
}

export function loadBest(): number {
  try {
    return parseBest(localStorage.getItem(KEY))
  } catch {
    return 1
  }
}

export function saveBest(level: number): void {
  try {
    if (level > loadBest()) localStorage.setItem(KEY, String(level))
  } catch {
    // the mac app's non-persistent web store can reject writes; this run still remembers
  }
}
