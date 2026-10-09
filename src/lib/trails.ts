// trails: two cycles on a grid, each leaving a solid wall behind it. a crash into a wall, a trail or the
// other cycle ends your round; the last one moving takes it, and first to WIN_ROUNDS takes the match.
// pure and deterministic: the panel owns the clock, keys and drawing, and feeds rand in for the computer

export const TRAILS_HINT = '↵ play trails'

export const COLS = 64
export const ROWS = 26
export const WIN_ROUNDS = 3
// one cell per tick
export const TICK_MS = 72
// turns pressed between ticks wait their turn, this many deep
export const QUEUE_MAX = 2

// up, right, down, left: +2 (mod 4) is the reverse
export type Dir = 0 | 1 | 2 | 3
export const DX = [0, 1, 0, -1] as const
export const DY = [-1, 0, 1, 0] as const
export type Player = 0 | 1

export type Cycle = {
  x: number
  y: number
  dir: Dir
  queue: Dir[]
  alive: boolean
  // cell indexes from the start to the head, for drawing the wall as one line
  path: number[]
  // where it hit, when it hit; may sit just outside the grid
  crash?: { x: number; y: number }
}

// winner null is a draw
export type RoundResult = { winner: Player | null }

export type Round = {
  // 0 empty, 1 player 0's wall, 2 player 1's
  grid: Uint8Array
  cycles: [Cycle, Cycle]
  tick: number
  result: RoundResult | null
}

export const LEVELS = ['easy', 'normal', 'hard'] as const
export type Level = (typeof LEVELS)[number]

// 'trails', 'trail' or 'cycles', alone
export function isTrailsCommand(text: string): boolean {
  return /^(?:trails?|cycles)$/i.test(text.trim())
}

export const cellIndex = (x: number, y: number) => y * COLS + x
const inside = (x: number, y: number) => x >= 0 && y >= 0 && x < COLS && y < ROWS
const opposite = (d: Dir) => ((d + 2) % 4) as Dir

function startCycle(x: number, y: number, dir: Dir): Cycle {
  return { x, y, dir, queue: [], alive: true, path: [cellIndex(x, y)] }
}

// point-symmetric starts, a row apart, so neither side has the better corner and straight on isn't a crash
export function newRound(): Round {
  const a = startCycle(8, ROWS / 2 - 1, 1)
  const b = startCycle(COLS - 1 - 8, ROWS / 2, 3)
  const grid = new Uint8Array(COLS * ROWS)
  grid[a.path[0]!] = 1
  grid[b.path[0]!] = 2
  return { grid, cycles: [a, b], tick: 0, result: null }
}

const withCycle = (r: Round, p: Player, c: Cycle): Round => {
  const cycles: [Cycle, Cycle] = [r.cycles[0], r.cycles[1]]
  cycles[p] = c
  return { ...r, cycles }
}

// a turn pressed now; it's taken on a later tick. the same way or straight back (from the last turn
// already waiting, or the way it's going) does nothing, and a full queue drops it
export function queueTurn(r: Round, p: Player, dir: Dir): Round {
  const c = r.cycles[p]
  if (r.result || !c.alive || c.queue.length >= QUEUE_MAX) return r
  const last = c.queue[c.queue.length - 1] ?? c.dir
  if (dir === last || dir === opposite(last)) return r
  return withCycle(r, p, { ...c, queue: [...c.queue, dir] })
}

// the computer's choice for this tick, in place of anything queued
export function steer(r: Round, p: Player, dir: Dir): Round {
  const c = r.cycles[p]
  if (r.result || !c.alive) return r
  return withCycle(r, p, { ...c, queue: dir === c.dir || dir === opposite(c.dir) ? [] : [dir] })
}

// one cell on, both at once. both into the same cell is a draw, as is both crashing on the same tick
export function step(r: Round): Round {
  if (r.result) return r
  const grid = r.grid.slice()
  const moved = r.cycles.map((c) => {
    const [dir = c.dir, ...queue] = c.queue
    const x = c.x + DX[dir]
    const y = c.y + DY[dir]
    return { c, dir, queue, x, y, hit: !inside(x, y) || r.grid[cellIndex(x, y)] !== 0 }
  })
  const [a, b] = moved as [(typeof moved)[0], (typeof moved)[0]]
  if (!a.hit && !b.hit && a.x === b.x && a.y === b.y) {
    a.hit = true
    b.hit = true
  }
  const cycles = moved.map((m, p): Cycle => {
    if (m.hit) return { ...m.c, dir: m.dir, queue: [], alive: false, crash: { x: m.x, y: m.y } }
    const at = cellIndex(m.x, m.y)
    grid[at] = p + 1
    return { ...m.c, x: m.x, y: m.y, dir: m.dir, queue: m.queue, path: [...m.c.path, at] }
  }) as [Cycle, Cycle]
  let result: RoundResult | null = null
  if (a.hit && b.hit) result = { winner: null }
  else if (a.hit) result = { winner: 1 }
  else if (b.hit) result = { winner: 0 }
  return { grid, cycles, tick: r.tick + 1, result }
}

export function scoreRound(score: readonly [number, number], result: RoundResult): [number, number] {
  const next: [number, number] = [score[0], score[1]]
  if (result.winner != null) next[result.winner]++
  return next
}

export function matchWinner(score: readonly [number, number]): Player | null {
  if (score[0] >= WIN_ROUNDS) return 0
  if (score[1] >= WIN_ROUNDS) return 1
  return null
}

// a seeded rand in [0, 1), for tests and replays
export function seeded(seed: number): () => number {
  let s = seed >>> 0
  return () => {
    s = (s + 0x6d2b79f5) >>> 0
    let t = s
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

// ---- the computer ----

const free = (grid: Uint8Array, x: number, y: number) => inside(x, y) && grid[cellIndex(x, y)] === 0

// empty cells reachable from (x, y), which is itself taken; stops counting at cap
function room(grid: Uint8Array, x: number, y: number, cap = Infinity): number {
  const seen = new Uint8Array(grid.length)
  const stack = [cellIndex(x, y)]
  seen[stack[0]!] = 1
  let n = 0
  while (stack.length && n < cap) {
    const i = stack.pop()!
    const cx = i % COLS
    const cy = (i - cx) / COLS
    for (let d = 0; d < 4; d++) {
      const nx = cx + DX[d]!
      const ny = cy + DY[d]!
      if (!free(grid, nx, ny)) continue
      const j = cellIndex(nx, ny)
      if (seen[j]) continue
      seen[j] = 1
      n++
      stack.push(j)
    }
  }
  return n
}

// cells each head reaches first, mine minus theirs; ties count for no one. both heads are taken cells.
// separated tells whether the two regions ever met
function territory(grid: Uint8Array, me: { x: number; y: number }, them: { x: number; y: number }): { score: number; separated: boolean } {
  const owner = new Int8Array(grid.length) // 0 unseen, 1 mine, 2 theirs, 3 tie
  const dist = new Int32Array(grid.length)
  let frontier: number[] = [cellIndex(me.x, me.y), cellIndex(them.x, them.y)]
  owner[frontier[0]!] = 1
  owner[frontier[1]!] = 2
  let mine = 0
  let theirs = 0
  let met = false
  let depth = 0
  while (frontier.length) {
    depth++
    const next: number[] = []
    for (const i of frontier) {
      const who = owner[i]!
      if (who === 3) continue
      const cx = i % COLS
      const cy = (i - cx) / COLS
      for (let d = 0; d < 4; d++) {
        const nx = cx + DX[d]!
        const ny = cy + DY[d]!
        if (!free(grid, nx, ny)) continue
        const j = cellIndex(nx, ny)
        if (owner[j] === 0) {
          owner[j] = who
          dist[j] = depth
          next.push(j)
          if (who === 1) mine++
          else theirs++
        } else if (owner[j] !== who) {
          met = true
          if (owner[j] !== 3 && dist[j] === depth) {
            if (owner[j] === 1) mine--
            else theirs--
            owner[j] = 3
          }
        }
      }
    }
    frontier = next
  }
  return { score: mine - theirs, separated: !met }
}

const BOT = {
  // chance of a careless (but never fatal) turn each tick, and how far it looks into a pocket
  easy: { slip: 0.05, look: 24 },
  normal: { slip: 0, look: Infinity },
  hard: { slip: 0, look: Infinity },
} as const

const safeDirs = (grid: Uint8Array, c: Cycle): Dir[] =>
  ([0, 1, 2, 3] as Dir[]).filter((d) => d !== opposite(c.dir) && free(grid, c.x + DX[d], c.y + DY[d]))

// how a move to (x, y) looks from the cell's walls: hugging one wastes less room
const openSides = (grid: Uint8Array, x: number, y: number) => {
  let n = 0
  for (let d = 0; d < 4; d++) if (free(grid, x + DX[d]!, y + DY[d]!)) n++
  return n
}

function withCell(grid: Uint8Array, x: number, y: number, v: number): Uint8Array {
  const g = grid.slice()
  g[cellIndex(x, y)] = v
  return g
}

type Cell = { x: number; y: number }
const WIN = 1000

const openCells = (grid: Uint8Array, c: Cell): Cell[] => {
  const out: Cell[] = []
  for (let d = 0; d < 4; d++) if (free(grid, c.x + DX[d]!, c.y + DY[d]!)) out.push({ x: c.x + DX[d]!, y: c.y + DY[d]! })
  return out
}

// hard's lookahead, taking turns: it has moved to me (already in grid), they answer their best way, then
// it moves again, depth more times. the score is cells it reaches first, at the end of the line
const HARD_DEPTH = 1
function answer(grid: Uint8Array, me: Cell, them: Cell, meV: 1 | 2, themV: 1 | 2, depth: number): number {
  let worst = Infinity
  for (const t of openCells(grid, them)) {
    const g = withCell(grid, t.x, t.y, themV)
    const v = depth === 0 ? territory(g, me, t).score : bestMove(g, me, t, meV, themV, depth - 1)
    worst = Math.min(worst, v)
    if (worst <= -WIN) break
  }
  return worst === Infinity ? WIN : worst
}

function bestMove(grid: Uint8Array, me: Cell, them: Cell, meV: 1 | 2, themV: 1 | 2, depth: number): number {
  let best = -Infinity
  for (const m of openCells(grid, me)) {
    best = Math.max(best, answer(withCell(grid, m.x, m.y, meV), m, them, meV, themV, depth))
    if (best >= WIN) break
  }
  return best === -Infinity ? -WIN : best
}

// walled in alone: the most cells it can be sure of, trying every path FILL_DEPTH moves deep and counting
// the room left at the end of each
const FILL_DEPTH = 3
function fill(grid: Uint8Array, x: number, y: number, depth: number): number {
  if (depth === 0) return room(grid, x, y)
  let best = 0
  for (const m of openCells(grid, { x, y })) best = Math.max(best, 1 + fill(withCell(grid, m.x, m.y, 3), m.x, m.y, depth - 1))
  return best
}

// the way the computer (player p) goes this tick. a move straight into a wall or trail is only ever picked
// when every move is one.
// easy: the room each way, looked at only so far, and now and then a careless turn
// normal: the cells it would reach before you
// hard: the same, two moves ahead, assuming you answer each of its moves your best way, which is how it
// cuts you off; walled off from you, it searches for the tidiest way to fill its side
export function botTurn(r: Round, p: Player, level: Level, rand: () => number): Dir {
  const me = r.cycles[p]
  const them = r.cycles[(1 - p) as Player]
  const safe = safeDirs(r.grid, me)
  if (safe.length === 0) return me.dir
  if (safe.length === 1) return safe[0]!
  const bot = BOT[level]
  if (bot.slip && rand() < bot.slip) return safe[Math.floor(rand() * safe.length)]!

  let best = safe[0]!
  let bestScore = -Infinity
  for (const d of safe) {
    const x = me.x + DX[d]
    const y = me.y + DY[d]
    const grid = withCell(r.grid, x, y, p + 1)
    let score: number
    if (level === 'easy' || !them.alive) {
      score = room(grid, x, y, bot.look)
    } else {
      const theirMoves = safeDirs(r.grid, them)
      // could they take the same cell next tick? that's a draw at best
      const contested = theirMoves.some((t) => them.x + DX[t] === x && them.y + DY[t] === y)
      if (level === 'normal') {
        const t = territory(grid, { x, y }, them)
        score = t.separated ? t.score - openSides(grid, x, y) * 0.3 : t.score
        if (contested) score -= 500
      } else {
        const sides = territory(grid, { x, y }, them)
        // walled off: its room counted by how well it can fill it, less theirs
        if (sides.separated) score = sides.score - room(grid, x, y) + fill(grid, x, y, FILL_DEPTH) - openSides(grid, x, y) * 0.3
        else score = answer(grid, { x, y }, them, (p + 1) as 1 | 2, (2 - p) as 1 | 2, HARD_DEPTH)
        // the lookahead takes turns, so it can't see a head-on; only a sure loss is worse
        if (contested) score = Math.min(score, -WIN + 100)
      }
    }
    // a hair's preference for straight on, so it doesn't wiggle when all else is equal
    if (d === me.dir) score += 0.1
    if (score > bestScore) {
      bestScore = score
      best = d
    }
  }
  return best
}
