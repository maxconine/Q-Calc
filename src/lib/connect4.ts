import type { Side } from './pong'

// two-player connect 4 over the peer link. turn based, so neither side is in charge: each sends its own
// moves and both apply every move to the same board. a move the board doesn't allow is dropped

export const C4_VERSION = 1
export const C4_HINT = '↵ play connect 4'

export const COLS = 7
export const ROWS = 6
export const CELLS = COLS * ROWS
export const LINE = 4

export type { Side }
export type Cell = Side | null

// cells run left to right, bottom row first: cell (col, row) is board[row * COLS + col]
export type C4State = {
  board: Cell[]
  // who dropped first this game; the host in game 0, then alternating
  first: Side
  turn: Side
  moves: number
  // the cell the last move filled
  last?: number
  // set once the game is over: the winner and their four (or more) in a row; a full board with no winner is a draw
  winner?: Side
  line?: number[]
  draw?: boolean
}

export function isConnect4Command(text: string): boolean {
  return /^connect[\s-]*(4|four)$/i.test(text.trim())
}

export const other = (s: Side): Side => (s === 'host' ? 'guest' : 'host')

// game numbers count from 0; the host goes first in even ones
export function firstFor(game: number): Side {
  return game % 2 === 0 ? 'host' : 'guest'
}

export function newBoard(first: Side = 'host'): C4State {
  return { board: Array<Cell>(CELLS).fill(null), first, turn: first, moves: 0 }
}

export const cellAt = (col: number, row: number) => row * COLS + col

export function isOver(s: C4State): boolean {
  return s.winner != null || s.draw === true
}

export function validColumn(col: unknown): col is number {
  return typeof col === 'number' && Number.isInteger(col) && col >= 0 && col < COLS
}

// the row a disc dropped in col lands on, or -1 when the column is full
export function landingRow(s: C4State, col: number): number {
  if (!validColumn(col)) return -1
  for (let row = 0; row < ROWS; row++) if (s.board[cellAt(col, row)] == null) return row
  return -1
}

export function canDrop(s: C4State, col: number, who: Side): boolean {
  return !isOver(s) && s.turn === who && landingRow(s, col) >= 0
}

const DIRS: Array<[number, number]> = [
  [1, 0],
  [0, 1],
  [1, 1],
  [1, -1],
]

// every cell in a row of LINE or more through the cell at (col, row), all of its colour; empty when there isn't one
export function lineThrough(board: Cell[], col: number, row: number): number[] {
  const who = board[cellAt(col, row)]
  if (!who) return []
  const out = new Set<number>()
  for (const [dc, dr] of DIRS) {
    const run = [cellAt(col, row)]
    for (const sign of [1, -1]) {
      for (let c = col + dc * sign, r = row + dr * sign; c >= 0 && c < COLS && r >= 0 && r < ROWS; c += dc * sign, r += dr * sign) {
        if (board[cellAt(c, r)] !== who) break
        run.push(cellAt(c, r))
      }
    }
    if (run.length >= LINE) for (const i of run) out.add(i)
  }
  return [...out].sort((a, b) => a - b)
}

// who drops in col; null when it isn't theirs to make (wrong turn, full or unknown column, game over)
export function drop(s: C4State, col: number, who: Side): C4State | null {
  if (!canDrop(s, col, who)) return null
  const row = landingRow(s, col)
  const board = s.board.slice()
  const last = cellAt(col, row)
  board[last] = who
  const moves = s.moves + 1
  const line = lineThrough(board, col, row)
  if (line.length) return { ...s, board, moves, last, winner: who, line }
  if (moves === CELLS) return { ...s, board, moves, last, draw: true }
  return { ...s, board, moves, last, turn: other(who) }
}

// the wire. every type starts c4 so a pong court on the other end ignores it, and the other way round;
// bye is the one both games share
export type C4Msg =
  // ok: the sender has already heard the other side's hi, so it needs no answer
  | { t: 'c4'; v: number; ok?: boolean }
  // move n (counting from 0) of game g drops in column c
  | { t: 'c4m'; g: number; n: number; c: number }
  // asks for a rematch once game g is over
  | { t: 'c4a'; g: number }
  // keeps the link from looking dead while someone thinks
  | { t: 'c4k' }
  | { t: 'bye' }

// what can come in: a c4 message, or pong's hello from a friend who opened pong instead
export type C4In = C4Msg | { t: 'pong' }

export function encodeC4(msg: C4Msg): string {
  return JSON.stringify(msg)
}

const count = (v: unknown): v is number => typeof v === 'number' && Number.isInteger(v) && v >= 0 && v <= 1e6

// anything malformed is null, never a throw: this is text from another machine
export function decodeC4(raw: string): C4In | null {
  if (typeof raw !== 'string' || raw.length > 256) return null
  let m: unknown
  try {
    m = JSON.parse(raw)
  } catch {
    return null
  }
  if (!m || typeof m !== 'object' || Array.isArray(m)) return null
  const o = m as Record<string, unknown>
  switch (o.t) {
    case 'c4':
      return typeof o.v === 'number' && Number.isFinite(o.v) ? { t: 'c4', v: o.v, ...(o.ok === true ? { ok: true } : {}) } : null
    case 'c4m':
      return count(o.g) && count(o.n) && o.n < CELLS && validColumn(o.c) ? { t: 'c4m', g: o.g, n: o.n, c: o.c } : null
    case 'c4a':
      return count(o.g) ? { t: 'c4a', g: o.g } : null
    case 'c4k':
      return { t: 'c4k' }
    case 'bye':
      return { t: 'bye' }
    // pong's hello
    case 'hi':
      return { t: 'pong' }
    default:
      return null
  }
}
