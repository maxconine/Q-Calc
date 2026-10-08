import { describe, expect, it } from 'vitest'
import { drop, isOver, newBoard, type C4State, type Side } from './connect4'
import { BOT_BUDGET_MS, botColumn, C4_SKILLS, type C4Skill } from './connect4Bot'
import { BOT_THINK_MS, Connect4Session } from './connect4Session'

function seeded(seed: number): () => number {
  let a = seed >>> 0
  return () => {
    a = (a + 0x6d2b79f5) >>> 0
    let t = Math.imul(a ^ (a >>> 15), 1 | a)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

// plays the columns in turn from a fresh board, host first
function board(cols: number[]): C4State {
  let s = newBoard('host')
  for (const c of cols) s = drop(s, c, s.turn)!
  return s
}

describe('botColumn', () => {
  it('takes a win that is there', () => {
    // host: 0 1 2 on the bottom row; it's host's turn again after guest's 6 6 6
    const s = board([0, 6, 1, 6, 2, 5])
    for (const k of ['normal', 'hard'] as const) expect(botColumn(s, 'host', k)).toBe(3)
  })

  it('blocks a four the other side is about to make', () => {
    // host has 0 1 2; guest to move must block 3
    const s = board([0, 6, 1, 6, 2])
    for (const k of ['normal', 'hard'] as const) expect(botColumn(s, 'guest', k)).toBe(3)
  })

  it('only plays a column with room, on its own turn', () => {
    const full = board([0, 0, 0, 0, 0, 0])
    let t = 0
    const fast = () => (t += 40)
    for (const k of C4_SKILLS) for (let i = 0; i < 20; i++) expect(botColumn(full, 'host', k, seeded(i), fast)).not.toBe(0)
    expect(botColumn(newBoard('host'), 'guest', 'hard')).toBe(-1)
  })

  it('stops searching on hard when its time is up, inside the pause', () => {
    expect(BOT_BUDGET_MS).toBeLessThan(BOT_THINK_MS)
    for (const s of [newBoard('host'), board([3, 3, 2, 4])]) {
      const t = performance.now()
      expect(botColumn(s, 'host', 'hard')).toBeGreaterThanOrEqual(0)
      // the clock is read every 1024 positions, so it can run a hair over
      expect(performance.now() - t).toBeLessThan(BOT_BUDGET_MS + 100)
    }
  })

  it('still finds the win when it has no time at all', () => {
    let t = 0
    const s = board([0, 6, 1, 6, 2, 5])
    expect(botColumn(s, 'host', 'hard', Math.random, () => (t += 1e6))).toBe(3)
  })
})

function match(a: C4Skill, b: C4Skill, games: number, seed: number): { a: number; b: number; draws: number } {
  const rand = seeded(seed)
  const out = { a: 0, b: 0, draws: 0 }
  for (let g = 0; g < games; g++) {
    // each side goes first in half the games
    let s = newBoard(g % 2 === 0 ? 'host' : 'guest')
    const skill = (who: Side) => (who === 'host' ? a : b)
    // a clock that runs fast, so hard gets a few thousand positions a move instead of a third of a second
    let t = 0
    const clock = () => (t += 40)
    while (!isOver(s)) s = drop(s, botColumn(s, s.turn, skill(s.turn), rand, clock), s.turn)!
    if (s.winner === 'host') out.a++
    else if (s.winner === 'guest') out.b++
    else out.draws++
  }
  return out
}

describe('the levels', () => {
  it('normal beats easy most games, and hard beats normal most games', { timeout: 60_000 }, () => {
    const ne = match('normal', 'easy', 20, 1)
    expect(ne.a).toBeGreaterThan(ne.b * 3)
    const hn = match('hard', 'normal', 8, 2)
    expect(hn.a).toBeGreaterThan(hn.b)
  })
})

describe('Connect4Session solo', () => {
  it('the computer answers after its pause, sends nothing, and ↵ starts the next game with it going first', () => {
    const sent: string[] = []
    const s = new Connect4Session('host', (t) => sent.push(t), 0, 'easy', seeded(4))
    expect(s.view().started).toBe(true)
    expect(s.play(3, 10)).toBe(true)
    expect(s.view().thinking).toBe(true)
    expect(s.tick(10 + BOT_THINK_MS - 1).state.moves).toBe(1)
    expect(s.tick(10 + BOT_THINK_MS).state.moves).toBe(2)
    expect(s.view().thinking).toBe(false)
    // play it out: the human keeps dropping in the first open column
    let now = 1000
    while (!isOver(s.view().state)) {
      const st = s.view().state
      if (st.turn === 'host') s.play([0, 1, 2, 3, 4, 5, 6].find((c) => st.board[35 + c] == null)!, now)
      s.tick((now += BOT_THINK_MS))
    }
    s.rematch(now)
    const next = s.view()
    expect(next.state.moves).toBe(0)
    expect(next.state.first).toBe('guest')
    expect(next.thinking).toBe(true)
    expect(s.tick(now + BOT_THINK_MS).state.moves).toBe(1)
    expect(sent).toEqual([])
  })
})
