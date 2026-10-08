import { describe, expect, it } from 'vitest'
import {
  BALL_SIZE,
  COURT_H,
  COURT_W,
  PADDLE_INSET,
  PADDLE_W,
  SKILLS,
  STEP_S,
  WIN_SCORE,
  botStep,
  clampPaddle,
  landingY,
  newBotMind,
  newGame,
  step,
  type PongState,
  type Skill,
} from './pong'
import { PongSession } from './pongSession'

// a small seeded generator, so the games below come out the same every run
function seeded(seed: number): () => number {
  let a = seed >>> 0
  return () => {
    a = (a + 0x6d2b79f5) >>> 0
    let t = Math.imul(a ^ (a >>> 15), 1 | a)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

const FACE = COURT_W - PADDLE_INSET - PADDLE_W - BALL_SIZE / 2

describe('landingY', () => {
  it('goes straight when nothing is in the way', () => {
    expect(landingY({ x: 100, y: 100, vx: 100, vy: 10 }, 200)).toBeCloseTo(110)
  })

  it('folds off the bottom and top walls', () => {
    const half = BALL_SIZE / 2
    // heads 100px past the bottom edge: comes back up 100px
    expect(landingY({ x: 0, y: COURT_H - half - 50, vx: 100, vy: 150 }, 100)).toBeCloseTo(COURT_H - half - 100)
    expect(landingY({ x: 0, y: half + 50, vx: 100, vy: -150 }, 100)).toBeCloseTo(half + 100)
  })

  it('stays on the court however far it goes', () => {
    const rand = seeded(7)
    for (let i = 0; i < 500; i++) {
      const y = landingY({ x: 0, y: rand() * COURT_H, vx: 50 + rand() * 600, vy: (rand() * 2 - 1) * 900 }, COURT_W)
      expect(y).toBeGreaterThanOrEqual(BALL_SIZE / 2 - 1e-9)
      expect(y).toBeLessThanOrEqual(COURT_H - BALL_SIZE / 2 + 1e-9)
    }
  })
})

describe('botStep', () => {
  const playing = (ball: PongState['ball']): PongState => ({ ...newGame(1), phase: 'play', ball })

  it('drifts back to the middle while the ball goes the other way', () => {
    let y = 30
    let mind = newBotMind()
    const s = playing({ x: 300, y: 200, vx: -300, vy: 0 })
    for (let i = 0; i < 200; i++) ({ y, mind } = botStep(s, y, mind, 'hard', STEP_S, () => 0.5))
    expect(y).toBeCloseTo(COURT_H / 2, 0)
  })

  it('goes to meet a ball coming its way, no faster than a paddle can', () => {
    let y = COURT_H / 2
    let mind = newBotMind()
    const s = playing({ x: FACE - 20, y: 40, vx: 300, vy: 0 })
    const before = y
    ;({ y, mind } = botStep(s, y, mind, 'hard', 0.01, () => 0.5))
    // 420 px/s for 10ms
    expect(before - y).toBeCloseTo(4.2)
    for (let i = 0; i < 200; i++) ({ y, mind } = botStep(s, y, mind, 'hard', STEP_S, () => 0.5))
    // it stops inside its 2px dead zone
    expect(Math.abs(y - clampPaddle(40))).toBeLessThan(2.5)
  })

  it('on easy, ignores a ball still far away', () => {
    const s = playing({ x: 60, y: 40, vx: 300, vy: 0 })
    const { y } = botStep(s, COURT_H / 2, newBotMind(), 'easy', 0.01, () => 0.5)
    expect(y).toBe(COURT_H / 2)
  })
})

// a perfect left player against the computer: how many of the computer's points the left wins, over many games
function pointsAgainst(skill: Skill, games: number, seed: number): { won: number; lost: number } {
  const rand = seeded(seed)
  let won = 0
  let lost = 0
  for (let g = 0; g < games; g++) {
    let s = newGame(rand() < 0.5 ? 1 : -1)
    let bot = COURT_H / 2
    let mind = newBotMind()
    for (let i = 0; i < 120 * 60 * 10 && s.phase !== 'over'; i++) {
      ;({ y: bot, mind } = botStep(s, bot, mind, skill, STEP_S, rand))
      s = step({ ...s, paddles: [clampPaddle(s.ball.y), bot] }, STEP_S, rand)
    }
    won += s.score[0]
    lost += s.score[1]
  }
  return { won, lost }
}

describe('the computer, by level', () => {
  const results = Object.fromEntries(SKILLS.map((k) => [k, pointsAgainst(k, 12, 42)])) as Record<Skill, { won: number; lost: number }>

  it('every game finishes', () => {
    for (const k of SKILLS) expect(results[k].won).toBe(12 * WIN_SCORE)
  })

  it('gets harder from easy to hard, and even hard can be beaten', () => {
    // a perfect player never misses, so every rally ends with the computer missing: a longer rally is a better computer
    const rallies = (k: Skill) => rallyLength(k, 300, 9)
    const [easy, normal, hard] = SKILLS.map(rallies)
    expect(easy).toBeLessThan(normal!)
    expect(normal).toBeLessThan(hard!)
    expect(easy).toBeGreaterThan(1)
  })
})

// how many times, on average, the computer returns the ball before it misses
function rallyLength(skill: Skill, rallies: number, seed: number): number {
  const rand = seeded(seed)
  let returns = 0
  for (let r = 0; r < rallies; r++) {
    let s: PongState = { ...newGame(1), serveIn: 0 }
    let bot = COURT_H / 2
    let mind = newBotMind()
    let wasComing = false
    for (let i = 0; i < 120 * 120; i++) {
      ;({ y: bot, mind } = botStep(s, bot, mind, skill, STEP_S, rand))
      const next = step({ ...s, paddles: [clampPaddle(s.ball.y), bot] }, STEP_S, rand)
      if (next.score[0] !== s.score[0]) break
      const coming = next.phase === 'play' && next.ball.vx > 0
      if (wasComing && !coming && next.phase === 'play') returns++
      wasComing = coming
      s = next
    }
  }
  return returns / rallies
}

describe('PongSession solo', () => {
  it('sends nothing, starts at once, never times out, and ↵ starts the next game straight away', () => {
    const sent: string[] = []
    const rand = seeded(3)
    const session = new PongSession('host', (t) => sent.push(t), 0, rand, 'easy')
    let now = 0
    let view = session.tick(now)
    expect(view.started).toBe(true)
    // an easy computer against a still paddle: someone reaches 7 in a few minutes
    for (let i = 0; i < 60 * 60 * 10 && view.snap?.phase !== 'over'; i++) view = session.tick((now += 1000 / 60))
    expect(view.ended).toBeNull()
    expect(view.snap?.phase).toBe('over')
    session.rematch()
    view = session.tick((now += 1000 / 60))
    expect(view.snap?.phase).toBe('serve')
    expect(view.snap?.score).toEqual([0, 0])
    expect(sent).toEqual([])
  })
})
