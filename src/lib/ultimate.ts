import type { Side } from './pong'

// ultimate tic-tac-toe over the peer link: nine small boards in a big one. where you play in a small board
// sends the other side to that board of the big one; sent to a board that's already won or full, they play
// anywhere. three small boards in a row wins; a drawn small board counts for nobody. turn based like
// connect 4: each side sends its own moves and both apply every move to the same board

export const UT_VERSION = 1
export const UT_HINT = '↵ play ultimate tic-tac-toe'

// boards and the cells in each run left to right, top row first. cell c of board b is cells[b * 9 + c]
export const SIZE = 9
export const CELLS = SIZE * SIZE

export type { Side }
export type Cell = Side | null
// a small board once it's decided: who took it, or a draw
export type BoardMark = Side | 'draw' | null

export type UtState = {
  cells: Cell[]
  boards: BoardMark[]
  // who moved first this game; the host in game 0, then alternating
  first: Side
  turn: Side
  moves: number
  // the board the side to move has to play in; null when it can play in any open board
  forced: number | null
  // the cell the last move took
  last?: number
  // set once the game is over: the winner and their three boards in a row; no move left and no winner is a draw
  winner?: Side
  line?: number[]
  draw?: boolean
}

// every three in a row on a 3×3 grid
export const LINES: readonly (readonly [number, number, number])[] = [
  [0, 1, 2],
  [3, 4, 5],
  [6, 7, 8],
  [0, 3, 6],
  [1, 4, 7],
  [2, 5, 8],
  [0, 4, 8],
  [2, 4, 6],
]

export function isUltimateCommand(text: string): boolean {
  return /^(ultimate([\s-]*(tic[\s-]*tac[\s-]*toe|ttt))?|uttt)$/i.test(text.trim())
}

export const other = (s: Side): Side => (s === 'host' ? 'guest' : 'host')

// game numbers count from 0; the host goes first in even ones
export function firstFor(game: number): Side {
  return game % 2 === 0 ? 'host' : 'guest'
}

export function newGame(first: Side = 'host'): UtState {
  return { cells: Array<Cell>(CELLS).fill(null), boards: Array<BoardMark>(SIZE).fill(null), first, turn: first, moves: 0, forced: null }
}

export const boardOf = (cell: number) => Math.floor(cell / SIZE)
export const spotOf = (cell: number) => cell % SIZE
export const cellIn = (board: number, spot: number) => board * SIZE + spot

export function isOver(s: UtState): boolean {
  return s.winner != null || s.draw === true
}

export function validCell(c: unknown): c is number {
  return typeof c === 'number' && Number.isInteger(c) && c >= 0 && c < CELLS
}

// whether the side to move may play in board b
export function boardOpen(s: UtState, b: number): boolean {
  return s.boards[b] == null && (s.forced == null || s.forced === b)
}

export function canPlay(s: UtState, cell: number, who: Side): boolean {
  return !isOver(s) && s.turn === who && validCell(cell) && s.cells[cell] == null && boardOpen(s, boardOf(cell))
}

// every cell the side to move may take
export function legalCells(s: UtState): number[] {
  if (isOver(s)) return []
  const out: number[] = []
  for (let b = 0; b < SIZE; b++) {
    if (!boardOpen(s, b)) continue
    for (let k = 0; k < SIZE; k++) if (s.cells[cellIn(b, k)] == null) out.push(cellIn(b, k))
  }
  return out
}

// the three in a row of who's marks through spot, or null
function lineOf<T>(marks: readonly T[], who: T, spot: number): number[] | null {
  for (const l of LINES) if (l.includes(spot) && l.every((k) => marks[k] === who)) return [...l]
  return null
}

// who takes cell; null when it isn't theirs to take (wrong turn, taken or closed cell, game over)
export function play(s: UtState, cell: number, who: Side): UtState | null {
  if (!canPlay(s, cell, who)) return null
  const cells = s.cells.slice()
  cells[cell] = who
  const b = boardOf(cell)
  const boards = s.boards.slice()
  const small = cells.slice(b * SIZE, b * SIZE + SIZE)
  if (lineOf(small, who, spotOf(cell))) boards[b] = who
  else if (small.every((c) => c != null)) boards[b] = 'draw'
  const moves = s.moves + 1
  const base = { ...s, cells, boards, moves, last: cell }
  if (boards[b] === who) {
    const line = lineOf(boards, who as BoardMark, b)
    if (line) return { ...base, forced: null, winner: who, line }
  }
  if (boards.every((m) => m != null)) return { ...base, forced: null, draw: true }
  const next = spotOf(cell)
  return { ...base, turn: other(who), forced: boards[next] == null ? next : null }
}

// the wire. every type starts ut so pong and connect 4 on the other end ignore it; bye is the one all share
export type UtMsg =
  // ok: the sender has already heard the other side's hi, so it needs no answer
  | { t: 'ut'; v: number; ok?: boolean }
  // move n (counting from 0) of game g takes cell c
  | { t: 'utm'; g: number; n: number; c: number }
  // asks for a rematch once game g is over
  | { t: 'uta'; g: number }
  // keeps the link from looking dead while someone thinks
  | { t: 'utk' }
  | { t: 'bye' }

// what can come in: a ut message, or another game's hello from a friend who opened that instead
export type UtIn = UtMsg | { t: 'other'; game: string }

export function encodeUt(msg: UtMsg): string {
  return JSON.stringify(msg)
}

const count = (v: unknown): v is number => typeof v === 'number' && Number.isInteger(v) && v >= 0 && v <= 1e6

// anything malformed is null, never a throw: this is text from another machine
export function decodeUt(raw: string): UtIn | null {
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
    case 'ut':
      return typeof o.v === 'number' && Number.isFinite(o.v) ? { t: 'ut', v: o.v, ...(o.ok === true ? { ok: true } : {}) } : null
    case 'utm':
      return count(o.g) && count(o.n) && o.n < CELLS && validCell(o.c) ? { t: 'utm', g: o.g, n: o.n, c: o.c } : null
    case 'uta':
      return count(o.g) ? { t: 'uta', g: o.g } : null
    case 'utk':
      return { t: 'utk' }
    case 'bye':
      return { t: 'bye' }
    // the other turn-based and real-time games' hellos
    case 'hi':
      return { t: 'other', game: 'pong' }
    case 'c4':
      return { t: 'other', game: 'connect 4' }
    default:
      return null
  }
}
