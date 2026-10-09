import { describe, expect, it } from 'vitest'
import { gameCommand } from './games'
import {
  botTurn,
  cellIndex,
  COLS,
  DX,
  DY,
  isTrailsCommand,
  matchWinner,
  newRound,
  queueTurn,
  ROWS,
  scoreRound,
  seeded,
  steer,
  step,
  WIN_ROUNDS,
  type Cycle,
  type Dir,
  type Level,
  type Player,
  type Round,
} from './trails'

// a round with the cycles placed by hand on an empty grid
function placed(a: Partial<Cycle> & { x: number; y: number; dir: Dir }, b: Partial<Cycle> & { x: number; y: number; dir: Dir }): Round {
  const grid = new Uint8Array(COLS * ROWS)
  const cyc = (c: typeof a, p: Player): Cycle => {
    grid[cellIndex(c.x, c.y)] = p + 1
    return { queue: [], alive: true, path: [cellIndex(c.x, c.y)], ...c }
  }
  return { grid, cycles: [cyc(a, 0), cyc(b, 1)], tick: 0, result: null }
}

function wall(r: Round, cells: Array<[number, number]>, v = 2): Round {
  const grid = r.grid.slice()
  for (const [x, y] of cells) grid[cellIndex(x, y)] = v
  return { ...r, grid }
}

describe('trails', () => {
  it('moves one cell a tick and leaves a trail', () => {
    let r = newRound()
    const [a0, b0] = r.cycles
    r = step(step(r))
    expect(r.cycles[0]).toMatchObject({ x: a0.x + 2, y: a0.y, alive: true })
    expect(r.cycles[1]).toMatchObject({ x: b0.x - 2, y: b0.y, alive: true })
    expect(r.cycles[0].path).toHaveLength(3)
    for (const i of r.cycles[0].path) expect(r.grid[i]).toBe(1)
    for (const i of r.cycles[1].path) expect(r.grid[i]).toBe(2)
    expect(r.tick).toBe(2)
    expect(r.result).toBeNull()
  })

  it('starts fair: point-symmetric, and straight on is not a crash', () => {
    const [a, b] = newRound().cycles
    expect(a.x + b.x).toBe(COLS - 1)
    expect(a.y + b.y).toBe(ROWS - 1)
    expect(a.y).not.toBe(b.y)
  })

  it('turns 90° and never reverses', () => {
    let r = placed({ x: 10, y: 10, dir: 1 }, { x: 40, y: 20, dir: 3 })
    r = queueTurn(r, 0, 3) // straight back: ignored
    r = queueTurn(r, 0, 1) // the way it's going: ignored
    expect(r.cycles[0].queue).toEqual([])
    r = step(queueTurn(r, 0, 2))
    expect(r.cycles[0]).toMatchObject({ x: 10, y: 11, dir: 2 })
  })

  it('queues two quick turns, measured from the last one waiting, and drops a third', () => {
    let r = placed({ x: 10, y: 10, dir: 1 }, { x: 40, y: 20, dir: 3 })
    r = queueTurn(r, 0, 0) // up
    r = queueTurn(r, 0, 2) // down: the reverse of up, ignored
    r = queueTurn(r, 0, 3) // then left: a u-turn over two ticks
    r = queueTurn(r, 0, 0) // full
    expect(r.cycles[0].queue).toEqual([0, 3])
    r = step(r)
    expect(r.cycles[0]).toMatchObject({ x: 10, y: 9, dir: 0 })
    r = step(r)
    expect(r.cycles[0]).toMatchObject({ x: 9, y: 9, dir: 3, alive: true })
    expect(r.cycles[0].queue).toEqual([])
  })

  it('crashes into the wall', () => {
    let r = placed({ x: COLS - 1, y: 5, dir: 1 }, { x: 10, y: 20, dir: 1 })
    r = step(r)
    expect(r.cycles[0].alive).toBe(false)
    expect(r.cycles[0].crash).toEqual({ x: COLS, y: 5 })
    expect(r.result).toEqual({ winner: 1 })
    // once it's over, it stays over
    expect(step(r)).toBe(r)
  })

  it('crashes into its own trail and the other one', () => {
    // a tight loop into itself
    let r = placed({ x: 10, y: 10, dir: 1 }, { x: 40, y: 20, dir: 1 })
    r = step(r)
    r = step(queueTurn(r, 0, 2))
    r = step(queueTurn(r, 0, 3))
    r = step(queueTurn(r, 0, 0))
    expect(r.result).toEqual({ winner: 1 })
    // into the other's wall
    r = wall(placed({ x: 10, y: 10, dir: 1 }, { x: 40, y: 20, dir: 1 }), [[11, 10]])
    expect(step(r).result).toEqual({ winner: 1 })
    r = wall(placed({ x: 10, y: 10, dir: 1 }, { x: 40, y: 20, dir: 1 }), [[41, 20]], 1)
    expect(step(r).result).toEqual({ winner: 0 })
  })

  it('crashes into the other cycle', () => {
    // b drives into the cell a's head just left, which is taken
    const r2 = step(placed({ x: 10, y: 10, dir: 2 }, { x: 11, y: 10, dir: 3 }))
    expect(r2.cycles[1].alive).toBe(false)
    expect(r2.result).toEqual({ winner: 0 })
  })

  it('is a draw head-on into the same cell, and when both crash at once', () => {
    const r = step(placed({ x: 10, y: 10, dir: 1 }, { x: 12, y: 10, dir: 3 }))
    expect(r.result).toEqual({ winner: null })
    expect(r.cycles.every((c) => !c.alive)).toBe(true)
    // nose to nose: each hits the other's head
    expect(step(placed({ x: 10, y: 10, dir: 1 }, { x: 11, y: 10, dir: 3 })).result).toEqual({ winner: null })
    // both into walls on the same tick
    expect(step(placed({ x: 0, y: 3, dir: 3 }, { x: COLS - 1, y: 20, dir: 1 })).result).toEqual({ winner: null })
  })

  it('scores rounds to a match; draws count for no one', () => {
    let score: [number, number] = [0, 0]
    score = scoreRound(score, { winner: 0 })
    score = scoreRound(score, { winner: null })
    expect(score).toEqual([1, 0])
    expect(matchWinner(score)).toBeNull()
    for (let i = 1; i < WIN_ROUNDS; i++) score = scoreRound(score, { winner: 1 })
    expect(matchWinner(score)).toBeNull()
    score = scoreRound(score, { winner: 1 })
    expect(score).toEqual([1, WIN_ROUNDS])
    expect(matchWinner(score)).toBe(1)
  })

  it('steer sets the computer’s move and ignores reverses', () => {
    let r = placed({ x: 10, y: 10, dir: 1 }, { x: 40, y: 20, dir: 1 })
    r = steer(queueTurn(r, 1, 0), 1, 2)
    expect(r.cycles[1].queue).toEqual([2])
    expect(steer(r, 1, 3).cycles[1].queue).toEqual([])
  })

  it('never drives the computer into a wall or trail when a way out exists', () => {
    const rand = seeded(7)
    let checked = 0
    for (const level of ['easy', 'normal', 'hard'] as Level[]) {
      for (let n = 0; n < 150; n++) {
        // a random clutter, the bot somewhere free
        let r = placed({ x: 1 + Math.floor(rand() * (COLS - 2)), y: 1 + Math.floor(rand() * (ROWS - 2)), dir: Math.floor(rand() * 4) as Dir }, { x: 0, y: 0, dir: 1 })
        const me = r.cycles[0]
        const clutter: Array<[number, number]> = []
        for (let i = 0; i < 700; i++) {
          const x = Math.floor(rand() * COLS)
          const y = Math.floor(rand() * ROWS)
          if ((x !== me.x || y !== me.y) && (x || y)) clutter.push([x, y])
        }
        r = wall(r, clutter)
        const safe = ([0, 1, 2, 3] as Dir[]).filter((d) => {
          const x = me.x + DX[d]
          const y = me.y + DY[d]
          return d !== (me.dir + 2) % 4 && x >= 0 && y >= 0 && x < COLS && y < ROWS && r.grid[cellIndex(x, y)] === 0
        })
        const d = botTurn(r, 0, level, rand)
        if (safe.length) {
          checked++
          expect(safe).toContain(d)
        }
      }
    }
    expect(checked).toBeGreaterThan(100)
  })

  it('picks the open side over a dead end', () => {
    // a pocket straight ahead: right is boxed in, down is open
    let r = placed({ x: 10, y: 0, dir: 1 }, { x: 50, y: 20, dir: 3 })
    r = wall(r, [
      [12, 0],
      [11, 1],
    ])
    for (const level of ['normal', 'hard'] as Level[]) expect(botTurn(r, 0, level, seeded(1))).toBe(2)
  })

  // a whole round, bot against bot; players swap sides each round so neither start is favoured
  function duel(a: Level, b: Level, rounds: number, seed: number): { a: number; b: number; draws: number } {
    const rand = seeded(seed)
    const tally = { a: 0, b: 0, draws: 0 }
    for (let i = 0; i < rounds; i++) {
      const levels: [Level, Level] = i % 2 ? [b, a] : [a, b]
      let r = newRound()
      // a few loose moves first, so the rounds aren't all the same round
      const opening = Math.floor(rand() * 12)
      while (!r.result && r.tick < COLS * ROWS) {
        for (const p of [0, 1] as Player[]) {
          if (r.tick >= opening) r = steer(r, p, botTurn(r, p, levels[p], rand))
          else if (rand() < 0.15) r = steer(r, p, Math.floor(rand() * 4) as Dir)
        }
        r = step(r)
      }
      const w = r.result?.winner
      if (w == null) tally.draws++
      else if ((w === 0) === (i % 2 === 0)) tally.a++
      else tally.b++
    }
    return tally
  }

  it('plays harder the higher the level', () => {
    const hardEasy = duel('hard', 'easy', 12, 3)
    expect(hardEasy.a).toBeGreaterThan(hardEasy.b * 3)
    const normalEasy = duel('normal', 'easy', 16, 4)
    expect(normalEasy.a).toBeGreaterThan(normalEasy.b * 3)
    const hardNormal = duel('hard', 'normal', 12, 5)
    expect(hardNormal.a).toBeGreaterThan(hardNormal.b * 2)
  }, 20_000)

  it('opens from its names, and leaves maths alone', () => {
    for (const t of ['trails', 'Trail', ' cycles ', 'TRAILS']) {
      expect(isTrailsCommand(t)).toBe(true)
      expect(gameCommand(t)).toBe('trails')
    }
    for (const t of ['trails2', 'trail s', 'cycle', '2 trails', 'trails+1', 'sin(x)', 't']) expect(isTrailsCommand(t)).toBe(false)
  })
})
