import { cellAt, COLS, CELLS, isOver, LINE, ROWS, type C4State, type Side } from './connect4'

// the computer for solo connect 4. it searches ahead with alpha-beta on a board of its own (small ints, no
// copies), scoring a position it stops at by the fours each side could still make

export type C4Skill = 'easy' | 'normal' | 'hard'
export const C4_SKILLS: readonly C4Skill[] = ['easy', 'normal', 'hard']

// how many moves ahead each level looks at most. easy looks one ahead and then plays loosely besides;
// hard looks as deep as it gets to in its time
const DEPTH: Record<C4Skill, number> = { easy: 1, normal: 4, hard: 14 }
// the most a move may take: the search runs on the page's thread, and the computer's pause is only half a second
export const BOT_BUDGET_MS = 350
// easy: how often it plays a random column instead, and how often it sees a win or a threat it should block
const EASY = { wander: 0.35, seesWin: 0.75, seesBlock: 0.55 }

const WIN = 1_000_000
// centre first: the best moves are usually there, so alpha-beta cuts more
const ORDER = [3, 2, 4, 1, 5, 0, 6]

// every group of four cells in a line, once
const WINDOWS: number[][] = (() => {
  const out: number[][] = []
  for (const [dc, dr] of [
    [1, 0],
    [0, 1],
    [1, 1],
    [1, -1],
  ] as const) {
    for (let row = 0; row < ROWS; row++) {
      for (let col = 0; col < COLS; col++) {
        const endC = col + dc * (LINE - 1)
        const endR = row + dr * (LINE - 1)
        if (endC < 0 || endC >= COLS || endR < 0 || endR >= ROWS) continue
        out.push(Array.from({ length: LINE }, (_, k) => cellAt(col + dc * k, row + dr * k)))
      }
    }
  }
  return out
})()

// the windows through each cell, so a move is checked for a win without scanning the whole board
const THROUGH: number[][][] = Array.from({ length: CELLS }, (_, i) => WINDOWS.filter((w) => w.includes(i)))

// 1 is the computer, -1 its opponent, 0 empty
type Board = Int8Array

// thrown out of a search that ran out of time; the last depth that finished stands
const OUT_OF_TIME = Symbol('out of time')

class Search {
  readonly board: Board
  readonly heights = new Int8Array(COLS)
  nodes = 0
  deadline = Infinity
  clock: () => number = () => 0

  constructor(s: C4State, me: Side) {
    this.board = Int8Array.from(s.board, (c) => (c == null ? 0 : c === me ? 1 : -1))
    for (let col = 0; col < COLS; col++) {
      let h = 0
      while (h < ROWS && this.board[cellAt(col, h)] !== 0) h++
      this.heights[col] = h
    }
  }

  canPlay(col: number): boolean {
    return this.heights[col]! < ROWS
  }

  // drops for who, and says whether it made four
  play(col: number, who: 1 | -1): boolean {
    const i = cellAt(col, this.heights[col]!++)
    this.board[i] = who
    return THROUGH[i]!.some((w) => w.every((j) => this.board[j] === who))
  }

  undo(col: number): void {
    this.board[cellAt(col, --this.heights[col]!)] = 0
  }

  // the computer's view of a position nobody has won yet: fours it could still make, minus the opponent's
  score(): number {
    let total = 0
    for (const w of WINDOWS) {
      let mine = 0
      let theirs = 0
      for (const j of w) {
        const v = this.board[j]
        if (v === 1) mine++
        else if (v === -1) theirs++
      }
      if (mine && theirs) continue
      if (mine === 3) total += 50
      else if (mine === 2) total += 6
      else if (mine === 1) total += 1
      else if (theirs === 3) total -= 60
      else if (theirs === 2) total -= 6
      else if (theirs === 1) total -= 1
    }
    for (let row = 0; row < ROWS; row++) {
      const v = this.board[cellAt(3, row)]
      total += v === 1 ? 4 : v === -1 ? -4 : 0
    }
    return total
  }

  // negamax with alpha-beta: the best score `who` can force from here, looking `depth` moves ahead.
  // a win found sooner scores higher, so the computer takes the quick one and puts off a loss
  negamax(depth: number, alpha: number, beta: number, who: 1 | -1, moves: number): number {
    // the clock is only read every so often; reading it is slower than a node
    if ((++this.nodes & 1023) === 0 && this.clock() > this.deadline) throw OUT_OF_TIME
    if (moves === CELLS) return 0
    if (depth === 0) return who * this.score()
    let best = -Infinity
    for (const col of ORDER) {
      if (!this.canPlay(col)) continue
      const won = this.play(col, who)
      const v = won ? WIN + depth : -this.negamax(depth - 1, -beta, -alpha, (-who) as 1 | -1, moves + 1)
      this.undo(col)
      if (v > best) best = v
      if (v > alpha) alpha = v
      if (alpha >= beta) break
    }
    return best
  }
}

function legal(s: C4State): number[] {
  return ORDER.filter((col) => s.board[cellAt(col, ROWS - 1)] == null)
}

// a column that wins at once for who, or -1
function winningColumn(search: Search, who: 1 | -1): number {
  for (const col of ORDER) {
    if (!search.canPlay(col)) continue
    const won = search.play(col, who)
    search.undo(col)
    if (won) return col
  }
  return -1
}

// centre columns more often than edge ones
function looseColumn(cols: number[], rand: () => number): number {
  const weights = cols.map((c) => 4 - Math.abs(c - 3))
  let r = rand() * weights.reduce((a, b) => a + b, 0)
  for (let k = 0; k < cols.length; k++) {
    r -= weights[k]!
    if (r < 0) return cols[k]!
  }
  return cols[cols.length - 1]!
}

// the best columns looking `depth` moves ahead, best first in `order`'s order; ties are all kept, so games differ
function bestColumns(search: Search, cols: number[], depth: number, moves: number): { picks: number[]; best: number } {
  let best = -Infinity
  let picks: number[] = []
  for (const col of cols) {
    const won = search.play(col, 1)
    // a column can only matter if it's at least as good as the best so far, so the search stops proving how much
    // worse the others are; the window stays one point open below, so a column as good as the best still counts as one
    let v: number
    try {
      v = won ? WIN + depth + 1 : -search.negamax(depth - 1, -Infinity, -(best - 1), -1, moves + 1)
    } finally {
      search.undo(col)
    }
    if (v > best) {
      best = v
      picks = [col]
    } else if (v === best) picks.push(col)
  }
  return { picks, best }
}

// the column the computer drops in, playing as `me`; the game must not be over and it must be its turn
export function botColumn(
  s: C4State,
  me: Side,
  skill: C4Skill,
  rand: () => number = Math.random,
  clock: () => number = () => performance.now(),
): number {
  const cols = legal(s)
  if (isOver(s) || s.turn !== me || !cols.length) return -1
  const search = new Search(s, me)

  if (skill === 'easy') {
    const win = winningColumn(search, 1)
    if (win >= 0 && rand() < EASY.seesWin) return win
    const block = winningColumn(search, -1)
    if (block >= 0 && rand() < EASY.seesBlock) return block
    if (rand() < EASY.wander) return looseColumn(cols, rand)
  }

  // a four on the board right now needs no thinking
  const now = winningColumn(search, 1)
  if (now >= 0) return now

  // deeper and deeper until the level's depth or the time runs out; each depth tries the last one's best first.
  // a win or loss found for certain won't change by looking further, so that ends it too
  search.clock = clock
  search.deadline = clock() + BOT_BUDGET_MS
  let picks = cols
  for (let depth = 1; depth <= DEPTH[skill] && depth <= CELLS - s.moves; depth++) {
    const order = [...picks, ...cols.filter((c) => !picks.includes(c))]
    let found: { picks: number[]; best: number }
    try {
      found = bestColumns(search, order, depth, s.moves)
    } catch (e) {
      if (e !== OUT_OF_TIME) throw e
      break
    }
    picks = found.picks
    if (Math.abs(found.best) >= WIN) break
  }
  return picks[Math.floor(rand() * picks.length)]!
}
