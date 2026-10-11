import { describe, expect, it } from 'vitest'
import { cellIn, isOver, legalCells, newGame, other, play, type Side, type UtState } from './ultimate'
import { botCell, type UtSkill } from './ultimateBot'

// a seeded random, so a failing game can be played again
function seeded(seed: number): () => number {
  let x = seed >>> 0 || 1
  return () => {
    x ^= x << 13
    x ^= x >>> 17
    x ^= x << 5
    return (x >>> 0) / 2 ** 32
  }
}

// one full game, the host by skill (or at random when null), the guest likewise; returns who won
function game(host: UtSkill | null, guest: UtSkill | null, rand: () => number, first: Side = 'host'): Side | 'draw' {
  let s = newGame(first)
  // a clock that runs fast, so each move gets a couple of hundred playouts and the games stay quick
  const clock = (() => {
    let t = 0
    return () => (t += 20)
  })()
  while (!isOver(s)) {
    const skill = s.turn === 'host' ? host : guest
    const legal = legalCells(s)
    const cell = skill ? botCell(s, s.turn, skill, rand, clock) : legal[Math.floor(rand() * legal.length)]!
    s = play(s, cell, s.turn)!
  }
  return s.winner ?? 'draw'
}

describe('ultimate computer', () => {
  it('takes the cell that wins the game', () => {
    const s0 = newGame()
    const s: UtState = {
      ...s0,
      boards: ['guest', 'guest', null, null, 'host', null, 'host', null, null],
      cells: s0.cells.map((_, i) => (i === cellIn(2, 4) || i === cellIn(2, 6) ? 'guest' : null)),
      turn: 'guest',
      forced: 2,
    }
    for (const skill of ['normal', 'hard'] as const) expect(botCell(s, 'guest', skill, seeded(1))).toBe(cellIn(2, 2))
  })

  it('only moves on its own turn and only to legal cells', () => {
    const s = play(newGame(), cellIn(4, 4), 'host')!
    expect(botCell(s, 'host', 'hard', seeded(2))).toBe(-1)
    const c = botCell(s, 'guest', 'normal', seeded(3))
    expect(legalCells(s)).toContain(c)
  })

  it('keeps to its playout budget when the clock never runs out', () => {
    const t0 = performance.now()
    botCell(newGame(), 'host', 'normal', seeded(4), () => 0)
    expect(performance.now() - t0).toBeLessThan(2000)
  })

  it('beats random play nearly always, even on a short search', () => {
    const rand = seeded(5)
    let wins = 0
    for (let g = 0; g < 6; g++) if (game('normal', null, rand, g % 2 ? 'guest' : 'host') === 'host') wins++
    expect(wins).toBeGreaterThanOrEqual(5)
  }, 30_000)

  it('plays each side', () => {
    const s = newGame('guest')
    expect(legalCells(s)).toContain(botCell(s, 'guest', 'easy', seeded(7)))
    expect(other('guest')).toBe('host')
  })
})
