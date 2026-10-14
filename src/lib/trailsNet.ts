import { otherHello } from './hello'
import { COLS, ROWS, WIN_ROUNDS, type Cycle, type Dir, type Round, type RoundResult } from './trails'

// trails over the peer link, as json text. both sides run the same round and only swap turns: one small
// frame message a tick, each naming the tick its turns land on. every type starts `tr`, so pong, connect 4
// and ember & frost ignore them; the hello is tagged with its game, and every game's is recognised here
// (hello.ts) so a mismatched pair is told

export const TRAILS_VERSION = 1
// how the start of a round is timed, here so the panel and the online game count the same
export const COUNT_MS = 550
export const POINT_MS = 1300
// turns land this many ticks after they're pressed, at most; the host picks from the link's ping
export const MAX_DELAY = 4
// a round can't outlast the grid: each tick fills a cell per cycle
export const MAX_TICK = COLS * ROWS
// frames one message may carry
export const MAX_FRAMES = 48

const MAX_TEXT = 12_000
const MAX_COUNT = 1e6
const MAX_WAIT_MS = 10_000
const U32 = 0xffffffff

// a player's turn for one tick; null is straight on
export type Frame = Dir | null
export type NetPhase = 'count' | 'run' | 'point' | 'over'
const PHASES: NetPhase[] = ['count', 'run', 'point', 'over']

// the host's whole game, enough for the guest to carry on from it: how a match starts, restarts, and is put
// right after a desync
export type Snap = {
  // epoch: goes up with every snapshot, so what the guest sent before it is told from what it sent after
  e: number
  // the round's number since the link opened
  r: number
  // the input delay, in ticks
  d: number
  score: [number, number]
  phase: NetPhase
  // ms left of count or point, from when this arrives
  wait: number
  round: Round
  // each side's turns from round.tick on, as far as the host holds them: host then guest
  frames: [Frame[], Frame[]]
  hash: number
}

export type TrailsMsg =
  // ok: the sender has already heard the other side's hello, so it needs no answer
  | { t: 'tr'; v: number; g?: 'trails'; ok?: boolean }
  // the host's ping, and keepalive: c is its clock, sent straight back in trq
  | { t: 'trp'; c: number }
  | { t: 'trq'; c: number }
  // the guest's keepalive
  | { t: 'trk'; e: number }
  // turns for ticks f, f+1…; a: the last tick of yours I hold with none missing before it;
  // h: the guest's running hash after that many ticks, for the host to check
  | { t: 'trf'; e: number; r: number; f: number; i: Frame[]; a: number; h?: [tick: number, hash: number] }
  | ({ t: 'trs' } & Snap)
  // a rematch asked for
  | { t: 'tra'; e: number }
  | { t: 'bye' }

// what arrives may also be another game's hello
export type TrailsIn = TrailsMsg | { t: 'other'; game: string }

// ---- the running hash: every position since the round began, so two sides that agree on it agree on the grid

const mix = (h: number, n: number) => Math.imul(h ^ (n & 0xffff), 16777619) >>> 0

export function hashStart(r: number, score: readonly [number, number]): number {
  return mix(mix(mix(0x811c9dc5, r), score[0]), score[1])
}

export function hashStep(h: number, round: Round): number {
  let out = mix(h, round.tick)
  for (const c of round.cycles) out = mix(mix(mix(mix(out, c.x), c.y), c.dir), c.alive ? 1 : 2)
  return out
}

// ---- a round as text: each wall as the cells where it starts, turns and ends

function corners(path: number[]): number[] {
  const out: number[] = []
  path.forEach((cell, i) => {
    const before = path[i - 1]
    const after = path[i + 1]
    if (before == null || after == null || cell - before !== after - cell) out.push(cell)
  })
  return out
}

const count = (v: unknown, max = MAX_COUNT): v is number => typeof v === 'number' && Number.isInteger(v) && v >= 0 && v <= max
const isDir = (v: unknown): v is Dir => v === 0 || v === 1 || v === 2 || v === 3

// the cells from corner to corner, written into grid as v; null if they don't make a wall
function wall(grid: Uint8Array, cornersIn: unknown, v: number): number[] | null {
  if (!Array.isArray(cornersIn) || cornersIn.length < 1 || cornersIn.length > MAX_TICK) return null
  const path: number[] = []
  const put = (cell: number): boolean => {
    if (grid[cell] !== 0) return false
    grid[cell] = v
    path.push(cell)
    return true
  }
  let at: number | null = null
  for (const c of cornersIn) {
    if (!count(c, COLS * ROWS - 1)) return null
    if (at == null) {
      if (!put(c)) return null
    } else {
      const dx = (c % COLS) - (at % COLS)
      const dy = Math.floor(c / COLS) - Math.floor(at / COLS)
      if ((dx === 0) === (dy === 0)) return null
      const by = dx ? Math.sign(dx) : Math.sign(dy) * COLS
      for (let cell: number = at + by; ; cell += by) {
        if (!put(cell)) return null
        if (cell === c) break
      }
    }
    at = c
  }
  return path
}

function packCycle(c: Cycle): unknown[] {
  const out: unknown[] = [c.dir, corners(c.path)]
  if (!c.alive) out.push([c.crash?.x ?? c.x, c.crash?.y ?? c.y])
  return out
}

function unpackCycle(grid: Uint8Array, raw: unknown, v: number): Cycle | null {
  if (!Array.isArray(raw) || raw.length < 2 || raw.length > 3 || !isDir(raw[0])) return null
  const path = wall(grid, raw[1], v)
  if (!path) return null
  const head = path[path.length - 1]!
  const cycle: Cycle = { x: head % COLS, y: Math.floor(head / COLS), dir: raw[0], queue: [], alive: true, path }
  if (raw.length === 2) return cycle
  const hit: unknown = raw[2]
  if (!Array.isArray(hit) || hit.length !== 2) return null
  const [x, y] = hit as unknown[]
  // a crash may sit one cell outside the grid
  if (typeof x !== 'number' || typeof y !== 'number' || !Number.isInteger(x) || !Number.isInteger(y)) return null
  if (x < -1 || x > COLS || y < -1 || y > ROWS) return null
  return { ...cycle, alive: false, crash: { x, y } }
}

function unpackRound(tick: unknown, raw: unknown): Round | null {
  if (!count(tick, MAX_TICK) || !Array.isArray(raw) || raw.length !== 2) return null
  const grid = new Uint8Array(COLS * ROWS)
  const a = unpackCycle(grid, raw[0], 1)
  const b = a && unpackCycle(grid, raw[1], 2)
  if (!a || !b) return null
  // a cycle that's still going has moved every tick; one that crashed stopped a tick short
  if (a.path.length !== tick + (a.alive ? 1 : 0) || b.path.length !== tick + (b.alive ? 1 : 0)) return null
  let result: RoundResult | null = null
  if (!a.alive && !b.alive) result = { winner: null }
  else if (!a.alive) result = { winner: 1 }
  else if (!b.alive) result = { winner: 0 }
  return { grid, cycles: [a, b], tick, result }
}

const packFrames = (frames: readonly Frame[]) => frames.map((f) => (f == null ? '.' : String(f))).join('')

function unpackFrames(raw: unknown): Frame[] | null {
  if (typeof raw !== 'string' || raw.length > MAX_FRAMES || !/^[.0-3]*$/.test(raw)) return null
  return [...raw].map((ch) => (ch === '.' ? null : (Number(ch) as Dir)))
}

export function encodeTrails(m: TrailsMsg): string {
  switch (m.t) {
    case 'trf':
      return JSON.stringify({ t: 'trf', ...(m.e ? { e: m.e } : {}), r: m.r, f: m.f, i: packFrames(m.i), a: m.a, ...(m.h ? { h: m.h } : {}) })
    case 'trs':
      return JSON.stringify({
        t: 'trs',
        e: m.e,
        r: m.r,
        d: m.d,
        s: m.score,
        ph: PHASES.indexOf(m.phase),
        w: Math.max(0, Math.round(m.wait)),
        k: m.round.tick,
        c: m.round.cycles.map(packCycle),
        q: m.frames.map(packFrames),
        h: m.hash,
      })
    default:
      return JSON.stringify(m)
  }
}

// anything malformed is null, never a throw: this is text from another machine
export function decodeTrails(raw: string): TrailsIn | null {
  if (typeof raw !== 'string' || raw.length > MAX_TEXT) return null
  let parsed: unknown
  try {
    parsed = JSON.parse(raw)
  } catch {
    return null
  }
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return null
  const o = parsed as Record<string, unknown>
  const other = otherHello(o, 'trails')
  if (other) return { t: 'other', game: other }
  switch (o.t) {
    case 'tr':
      return count(o.v) ? { t: 'tr', v: o.v, ...(o.ok === true ? { ok: true } : {}) } : null
    case 'trp':
    case 'trq':
      return count(o.c, Number.MAX_SAFE_INTEGER) ? { t: o.t, c: o.c } : null
    case 'trk':
      return count(o.e) ? { t: 'trk', e: o.e } : null
    case 'tra':
      return count(o.e) ? { t: 'tra', e: o.e } : null
    case 'bye':
      return { t: 'bye' }
    case 'trf': {
      const e = o.e ?? 0
      const i = unpackFrames(o.i)
      if (!count(e) || !count(o.r) || !count(o.f, MAX_TICK) || !i || o.f + i.length > MAX_TICK + MAX_DELAY) return null
      if (typeof o.a !== 'number' || !Number.isInteger(o.a) || o.a < -1 || o.a > MAX_TICK + MAX_DELAY) return null
      let h: [number, number] | undefined
      if (o.h !== undefined) {
        if (!Array.isArray(o.h) || o.h.length !== 2 || !count(o.h[0], MAX_TICK) || !count(o.h[1], U32)) return null
        h = [o.h[0], o.h[1]]
      }
      return { t: 'trf', e, r: o.r, f: o.f, i, a: o.a, ...(h ? { h } : {}) }
    }
    case 'trs': {
      const phase = count(o.ph, 3) ? PHASES[o.ph] : undefined
      const round = unpackRound(o.k, o.c)
      if (!phase || !round || !count(o.e) || o.e < 1 || !count(o.r) || !count(o.d, MAX_DELAY) || o.d < 1) return null
      if (!count(o.w, MAX_WAIT_MS) || !count(o.h, U32)) return null
      if (!Array.isArray(o.s) || o.s.length !== 2 || !count(o.s[0], WIN_ROUNDS) || !count(o.s[1], WIN_ROUNDS)) return null
      if (!Array.isArray(o.q) || o.q.length !== 2) return null
      const mine = unpackFrames(o.q[0])
      const theirs = unpackFrames(o.q[1])
      if (!mine || !theirs) return null
      // a round that's over goes with the score it made, and one still going with neither
      if ((round.result != null) !== (phase === 'point' || phase === 'over')) return null
      if (phase === 'count' && round.tick !== 0) return null
      return { t: 'trs', e: o.e, r: o.r, d: o.d, score: [o.s[0], o.s[1]], phase, wait: o.w, round, frames: [mine, theirs], hash: o.h }
    }
    default:
      return null
  }
}
