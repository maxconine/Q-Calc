import { LINES, SIZE, isOver, legalCells, type Side, type UtState } from './ultimate'

// the computer for solo ultimate tic-tac-toe. the tree is too wide for alpha-beta to see far, so it plays
// monte carlo tree search: random games from each candidate, more of them down the lines that keep winning

export type UtSkill = 'easy' | 'normal' | 'hard'
export const UT_SKILLS: readonly UtSkill[] = ['easy', 'normal', 'hard']

// the most each level may think, in ms and in playouts: the search runs on the page's thread, and the
// computer's pause is only half a second
const BUDGET: Record<UtSkill, { ms: number; playouts: number }> = {
  easy: { ms: 15, playouts: 120 },
  normal: { ms: 120, playouts: 6000 },
  hard: { ms: 350, playouts: Infinity },
}
// easy: how often it plays a random cell instead, and how often it sees a big-board win it should take
const EASY = { wander: 0.3, seesWin: 0.7 }
const EXPLORE = Math.SQRT2

// a position of its own, as small ints: 1 and 2 are the players, 0 empty; a board's mark is 3 for a draw
class Pos {
  cells: Int8Array
  boards: Int8Array
  turn: number
  forced: number
  // 0 while the game goes on, else 1, 2 or 3 for a draw
  result: number

  constructor(cells: Int8Array, boards: Int8Array, turn: number, forced: number, result: number) {
    this.cells = cells
    this.boards = boards
    this.turn = turn
    this.forced = forced
    this.result = result
  }

  static from(s: UtState, me: Side): Pos {
    const code = (v: Side | 'draw' | null) => (v == null ? 0 : v === 'draw' ? 3 : v === me ? 1 : 2)
    const result = s.winner ? code(s.winner) : s.draw ? 3 : 0
    return new Pos(Int8Array.from(s.cells, code), Int8Array.from(s.boards, code), s.turn === me ? 1 : 2, s.forced ?? -1, result)
  }

  clone(): Pos {
    return new Pos(this.cells.slice(), this.boards.slice(), this.turn, this.forced, this.result)
  }

  moves(out: number[]): number[] {
    out.length = 0
    if (this.result) return out
    for (let b = 0; b < SIZE; b++) {
      if (this.boards[b] || (this.forced >= 0 && this.forced !== b)) continue
      for (let k = b * SIZE; k < b * SIZE + SIZE; k++) if (!this.cells[k]) out.push(k)
    }
    return out
  }

  play(cell: number): void {
    const who = this.turn
    this.cells[cell] = who
    const b = (cell / SIZE) | 0
    const base = b * SIZE
    if (threeOf(this.cells, base, who)) {
      this.boards[b] = who
      if (threeOf(this.boards, 0, who)) {
        this.result = who
        return
      }
    } else {
      let full = true
      for (let k = base; k < base + SIZE; k++) if (!this.cells[k]) full = false
      if (full) this.boards[b] = 3
    }
    let done = true
    for (let k = 0; k < SIZE; k++) if (!this.boards[k]) done = false
    if (done) {
      this.result = 3
      return
    }
    const next = cell % SIZE
    this.forced = this.boards[next] ? -1 : next
    this.turn = 3 - who
  }
}

function threeOf(marks: Int8Array, base: number, who: number): boolean {
  for (const [a, b, c] of LINES) if (marks[base + a] === who && marks[base + b] === who && marks[base + c] === who) return true
  return false
}

type Node = {
  move: number
  parent: Node | null
  // who made move: the one this node's wins count for
  by: number
  kids: Node[]
  untried: number[]
  wins: number
  visits: number
}

function node(move: number, parent: Node | null, by: number, pos: Pos): Node {
  return { move, parent, by, kids: [], untried: pos.moves([]).slice(), wins: 0, visits: 0 }
}

// plays one random game on from pos (which it changes) and says how it ended
function playout(pos: Pos, rand: () => number, buf: number[]): number {
  while (!pos.result) {
    const moves = pos.moves(buf)
    pos.play(moves[Math.floor(rand() * moves.length)]!)
  }
  return pos.result
}

// a cell that wins the whole game now, for whoever is to move, or -1
function winningCell(pos: Pos): number {
  for (const m of pos.moves([])) {
    const p = pos.clone()
    p.play(m)
    if (p.result === pos.turn) return m
  }
  return -1
}

// the cell the computer takes, playing me; -1 when it isn't its turn or there's nothing to take
export function botCell(
  s: UtState,
  me: Side,
  skill: UtSkill,
  rand: () => number = Math.random,
  clock: () => number = () => performance.now(),
): number {
  const legal = legalCells(s)
  if (isOver(s) || s.turn !== me || !legal.length) return -1
  if (legal.length === 1) return legal[0]!
  const root = Pos.from(s, me)

  const win = winningCell(root)
  if (win >= 0 && (skill !== 'easy' || rand() < EASY.seesWin)) return win
  if (skill === 'easy' && rand() < EASY.wander) return legal[Math.floor(rand() * legal.length)]!

  const { ms, playouts } = BUDGET[skill]
  const deadline = clock() + ms
  const tree = node(-1, null, 2, root)
  const buf: number[] = []
  for (let n = 0; n < playouts && (n % 32 !== 0 || clock() < deadline); n++) {
    // down the tree by ucb1 until a node with a move left to try
    const pos = root.clone()
    let at = tree
    while (!at.untried.length && at.kids.length) {
      const logN = Math.log(at.visits)
      let best = at.kids[0]!
      let bestScore = -Infinity
      for (const k of at.kids) {
        const score = k.wins / k.visits + EXPLORE * Math.sqrt(logN / k.visits)
        if (score > bestScore) {
          bestScore = score
          best = k
        }
      }
      at = best
      pos.play(at.move)
    }
    // one new node, then a random game from it
    if (at.untried.length) {
      const i = Math.floor(rand() * at.untried.length)
      const move = at.untried[i]!
      at.untried[i] = at.untried[at.untried.length - 1]!
      at.untried.pop()
      const by = pos.turn
      pos.play(move)
      const kid = node(move, at, by, pos)
      at.kids.push(kid)
      at = kid
    }
    const result = playout(pos, rand, buf)
    for (let up: Node | null = at; up; up = up.parent) {
      up.visits++
      if (result === up.by) up.wins++
      else if (result === 3) up.wins += 0.5
    }
  }
  // the most visited move is the one the search trusts most
  let pick = tree.kids[0]
  for (const k of tree.kids) if (!pick || k.visits > pick.visits) pick = k
  return pick ? pick.move : legal[0]!
}
