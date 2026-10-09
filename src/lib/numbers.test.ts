import { describe, expect, it } from 'vitest'
import { gameCommand, gameHint } from './games'
import {
  checkExpr,
  countdownPoints,
  dealCountdown,
  deal24,
  FRESH_STATS,
  isNumbersCommand,
  LARGE,
  NUMBERS_HINT,
  parseExpr,
  record24,
  recordCountdown,
  solve24,
  solveCountdown,
  solves24,
} from './numbers'

// a small seeded generator, so deals come out the same every run
function seeded(seed: number) {
  let s = seed >>> 0
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0
    return s / 2 ** 32
  }
}

describe('isNumbersCommand', () => {
  it('opens on the names', () => {
    for (const t of ['24 game', '24game', 'Game of 24', 'countdown', 'Count down', ' COUNTDOWN ', 'numbers game', 'numbers  game', '24 / countdown'])
      expect(isNumbersCommand(t), t).toBe(true)
  })

  it('leaves calculator input alone, bare 24 above all', () => {
    for (const t of ['24', ' 24 ', '24*2', 'game', 'games', 'numbers', 'count', '24 games', 'countdowns', '2 countdown'])
      expect(isNumbersCommand(t), t).toBe(false)
  })

  it('is a game that needs no link', () => {
    expect(gameCommand('countdown')).toBe('numbers')
    expect(gameCommand('24 game')).toBe('numbers')
    expect(gameCommand('24')).toBe(null)
    expect(gameHint('numbers', false)).toBe(NUMBERS_HINT)
  })
})

describe('solve24', () => {
  it('finds a way when there is one, and the way checks out', () => {
    for (const nums of [[4, 7, 8, 8], [1, 5, 5, 5], [3, 3, 8, 8], [6, 6, 6, 6], [1, 2, 3, 4], [13, 13, 13, 12]]) {
      const way = solve24(nums)
      expect(way, nums.join(' ')).not.toBe(null)
      expect(solves24(way!, nums), way!).toBe(true)
    }
  })

  it('knows the impossible ones', () => {
    for (const nums of [[1, 1, 1, 1], [1, 1, 1, 2], [13, 13, 13, 13], [1, 1, 5, 9]]) expect(solve24(nums), nums.join(' ')).toBe(null)
  })

  it('only deals solvable puzzles from 1–13', () => {
    const rng = seeded(7)
    for (let i = 0; i < 50; i++) {
      const { nums, solution } = deal24(rng)
      expect(nums).toHaveLength(4)
      for (const n of nums) expect(n >= 1 && n <= 13).toBe(true)
      expect(solves24(solution, nums)).toBe(true)
    }
  })
})

describe('the player’s expression', () => {
  it('reads the usual symbols and brackets', () => {
    expect(solves24('(7 - 8/8) * 4', [4, 7, 8, 8])).toBe(true)
    expect(solves24('(7 − 8 ÷ 8) × 4', [4, 7, 8, 8])).toBe(true)
    expect(solves24('4x(7-8/8)', [8, 8, 7, 4])).toBe(true)
    expect(solves24('8 / (3 - 8/3)', [3, 3, 8, 8])).toBe(true)
  })

  it('wants exactly the given numbers in 24', () => {
    expect(checkExpr('6*4', [4, 7, 8, 8], '24').problem).toMatch(/isn’t one/)
    expect(checkExpr('8+8+8', [4, 7, 8, 8], '24').problem).toMatch(/too often/)
    expect(checkExpr('8*8', [4, 7, 8, 8], '24').problem).toMatch(/use 4, 7 too/)
    expect(checkExpr('84-7-8', [4, 7, 8, 8], '24').problem).toMatch(/84/)
    expect(solves24('8*3', [4, 7, 8, 8])).toBe(false)
  })

  it('turns away other operators, eval-ish text and negatives', () => {
    for (const t of ['2^3', '4!', 'alert(1)', '2**3', 'Math.max(1)', '3 % 2', '1e3']) expect(checkExpr(t, [1, 2, 3, 4], '24').problem, t).toBeTruthy()
    expect(parseExpr('-4 + 28').kind).toBe('error')
    expect(parseExpr('4 4').kind).toBe('error')
    expect(parseExpr('4 + )').kind).toBe('error')
    expect(checkExpr('4/(2-2)', [4, 2, 2, 1], '24').problem).toMatch(/divide by 0/)
  })

  it('is just unfinished while typing', () => {
    for (const t of ['', '4 +', '(4 + 7', '(4 + 7) *', '((']) {
      const c = checkExpr(t, [4, 7, 8, 8], '24')
      expect(c.problem, t).toBe(null)
      expect(c.valid).toBe(false)
    }
    expect(checkExpr('4 + 7', [4, 7, 8, 8], '24').value).toEqual({ n: 11, d: 1 })
  })

  it('countdown: each number at most once, every step a positive whole number', () => {
    const nums = [100, 50, 3, 6, 7, 1]
    expect(checkExpr('100 * 7 - 50', nums, 'countdown').valid).toBe(true)
    expect(checkExpr('100 + 100', nums, 'countdown').problem).toMatch(/too often/)
    expect(checkExpr('7 / 3', nums, 'countdown').problem).toMatch(/whole/)
    expect(checkExpr('(7 / 3) * 6', nums, 'countdown').problem).toMatch(/whole/)
    expect(checkExpr('3 - 7 + 100', nums, 'countdown').problem).toMatch(/above 0/)
    expect(checkExpr('7 - 7', [7, 7], 'countdown').problem).toMatch(/above 0/)
    // 24 allows fractions on the way
    expect(checkExpr('(7 / 3) * 6', [7, 3, 6, 1], '24').value).toEqual({ n: 14, d: 1 })
  })
})

describe('countdown', () => {
  it('solves a classic, and finds the closest when exact is out of reach', () => {
    const best = solveCountdown([25, 50, 75, 100, 3, 6], 952)
    expect(best.value).toBe(952)
    expect(checkExpr(best.expr, [25, 50, 75, 100, 3, 6], 'countdown').value).toEqual({ n: 952, d: 1 })
    const near = solveCountdown([1, 1, 1, 1, 1, 1], 500)
    expect(near.value).toBe(9)
  })

  it('deals the right mix and a target in range it can reach', () => {
    const rng = seeded(3)
    for (const large of [0, 1, 2, 3, 4]) {
      const d = dealCountdown(large, rng)
      expect(d.nums).toHaveLength(6)
      expect(d.nums.filter((n) => (LARGE as readonly number[]).includes(n))).toHaveLength(large)
      expect(d.target >= 101 && d.target <= 999).toBe(true)
      expect(d.best.value).toBe(d.target)
      expect(checkExpr(d.best.expr, d.nums, 'countdown').value?.n).toBe(d.target)
    }
  })

  it('scores by closeness', () => {
    expect(countdownPoints(500, 500)).toBe(10)
    expect(countdownPoints(500, 495)).toBe(7)
    expect(countdownPoints(500, 505)).toBe(7)
    expect(countdownPoints(500, 510)).toBe(5)
    expect(countdownPoints(500, 511)).toBe(0)
    expect(countdownPoints(500, null)).toBe(0)
  })
})

describe('stats', () => {
  it('keeps streaks and bests', () => {
    let s = record24(record24(FRESH_STATS, true), true)
    expect(s.streak24).toBe(2)
    s = record24(s, false)
    expect([s.streak24, s.best24, s.solved24]).toEqual([0, 2, 2])
    let c = recordCountdown(recordCountdown(FRESH_STATS, 10), 7)
    expect([c.rounds, c.points, c.exactStreak, c.bestExact]).toEqual([2, 17, 0, 1])
    c = recordCountdown(c, 10)
    expect(c.exactStreak).toBe(1)
  })
})
