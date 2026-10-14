import { describe, expect, it } from 'vitest'
import {
  advance,
  BALL_SIZE,
  bounceOff,
  clampPaddle,
  COURT_H,
  COURT_W,
  decodePong,
  encodePong,
  EASE_MS,
  FarPaddle,
  fly,
  GUEST_FACE_X,
  HOST_FACE_X,
  HostClock,
  isPongCommand,
  MAX_BOUNCE,
  MAX_SPEED,
  movePaddle,
  newGame,
  PADDLE_H,
  PADDLE_INSET,
  PADDLE_SPEED,
  PADDLE_W,
  SERVE_PAUSE_S,
  SERVE_SPEED,
  newer,
  secondsTo,
  step,
  STEP_S,
  WIN_SCORE,
  winnerOf,
  wireLeg,
  type Leg,
  type PongState,
} from './pong'

const half = () => 0.5
const playing = (over: Partial<PongState> = {}): PongState => ({ ...newGame(), phase: 'play', serveIn: 0, ...over })
function stepFor(s: PongState, seconds: number): PongState {
  let state = s
  for (let t = 0; t < seconds; t += STEP_S) state = step(state, STEP_S, half)
  return state
}

describe('isPongCommand', () => {
  it('is only the word pong', () => {
    expect(isPongCommand('pong')).toBe(true)
    expect(isPongCommand('  Pong ')).toBe(true)
    expect(isPongCommand('ping pong')).toBe(false)
    expect(isPongCommand('pong2')).toBe(false)
    expect(isPongCommand('po')).toBe(false)
  })
})

describe('paddles', () => {
  it('stay on the court', () => {
    expect(clampPaddle(-50)).toBe(PADDLE_H / 2)
    expect(clampPaddle(COURT_H + 50)).toBe(COURT_H - PADDLE_H / 2)
  })

  it('move at paddle speed', () => {
    expect(movePaddle(100, 1, 0.1)).toBeCloseTo(100 + PADDLE_SPEED * 0.1)
    expect(movePaddle(100, -3, 0.1)).toBeCloseTo(100 - PADDLE_SPEED * 0.1)
    expect(movePaddle(100, 0, 0.1)).toBe(100)
  })
})

describe('serve', () => {
  it('waits, then serves toward serveDir at serve speed', () => {
    let s = newGame(-1)
    s = step(s, SERVE_PAUSE_S / 2, half)
    expect(s.phase).toBe('serve')
    s = step(s, SERVE_PAUSE_S, half)
    expect(s.phase).toBe('play')
    expect(s.ball.vx).toBeCloseTo(-SERVE_SPEED)
    expect(s.ball.vy).toBeCloseTo(0)
  })

  it('angles within the serve cone', () => {
    const s = step(newGame(1), SERVE_PAUSE_S + 0.01, () => 0.999)
    expect(Math.hypot(s.ball.vx, s.ball.vy)).toBeCloseTo(SERVE_SPEED)
    expect(s.ball.vy).toBeGreaterThan(0)
    expect(Math.abs(Math.atan2(s.ball.vy, s.ball.vx))).toBeLessThan(0.45)
  })
})

describe('step', () => {
  it('bounces off the top and bottom walls', () => {
    const s = playing({ ball: { x: 300, y: BALL_SIZE / 2 + 1, vx: 0, vy: -300 } })
    const next = step(s, STEP_S)
    expect(next.ball.vy).toBeGreaterThan(0)
    expect(next.ball.y).toBeGreaterThanOrEqual(BALL_SIZE / 2)
    const low = step(playing({ ball: { x: 300, y: COURT_H - BALL_SIZE / 2 - 1, vx: 0, vy: 300 } }), STEP_S)
    expect(low.ball.vy).toBeLessThan(0)
  })

  it('returns off the host paddle, flat from the centre and faster', () => {
    const face = PADDLE_INSET + PADDLE_W
    const s = playing({ ball: { x: face + BALL_SIZE / 2 + 1, y: 130, vx: -400, vy: 0 }, paddles: [130, 130] })
    const next = step(s, STEP_S)
    expect(next.ball.vx).toBeCloseTo(400 * 1.06)
    expect(next.ball.vy).toBeCloseTo(0)
    expect(next.ball.x).toBeCloseTo(face + BALL_SIZE / 2)
  })

  it('returns off the guest paddle, steeper near the edge', () => {
    const face = COURT_W - PADDLE_INSET - PADDLE_W
    const s = playing({ ball: { x: face - BALL_SIZE / 2 - 1, y: 150, vx: 400, vy: 0 }, paddles: [130, 130] })
    const next = step(s, STEP_S)
    expect(next.ball.vx).toBeLessThan(0)
    expect(next.ball.vy).toBeGreaterThan(0)
  })

  it('misses when the paddle is elsewhere, and the other side scores', () => {
    let s = playing({ ball: { x: 60, y: 30, vx: -500, vy: 0 }, paddles: [220, 130] })
    s = stepFor(s, 0.5)
    expect(s.score).toEqual([0, 1])
    expect(s.phase).toBe('serve')
    // served back toward the host, who lost the point
    expect(s.serveDir).toBe(-1)
    expect(s.ball).toMatchObject({ x: COURT_W / 2, y: COURT_H / 2 })
  })

  it('does not hit the back of a paddle', () => {
    const s = playing({ ball: { x: PADDLE_INSET - 2, y: 130, vx: 300, vy: 0 }, paddles: [130, 130] })
    expect(step(s, STEP_S).ball.vx).toBe(300)
  })

  it('never tunnels through a paddle at top speed', () => {
    const face = PADDLE_INSET + PADDLE_W
    for (let x = face + BALL_SIZE / 2; x < face + BALL_SIZE / 2 + 7; x += 0.37) {
      const s = playing({ ball: { x, y: 130, vx: -MAX_SPEED, vy: 0 }, paddles: [130, 130] })
      expect(stepFor(s, 0.05).ball.vx).toBeGreaterThan(0)
    }
  })

  it('ends at the winning score', () => {
    let s = playing({ score: [WIN_SCORE - 1, 3], ball: { x: COURT_W - 20, y: 30, vx: 600, vy: 0 }, paddles: [130, 220] })
    s = stepFor(s, 0.2)
    expect(s.phase).toBe('over')
    expect(s.winner).toBe('host')
    expect(stepFor(s, 1)).toBe(s)
  })

  it('advance steps whole slices and carries the rest', () => {
    const { carry } = advance(newGame(), 0, STEP_S * 2.5)
    expect(carry).toBeCloseTo(STEP_S * 0.5)
    // a long stall is capped, not replayed
    const long = advance(playing({ ball: { x: 320, y: 130, vx: 100, vy: 0 } }), 0, 10, half)
    expect(long.state.ball.x).toBeCloseTo(320 + 100 * 0.25, 0)
  })
})

describe('bounceOff', () => {
  it('caps the angle and the speed', () => {
    const b = bounceOff({ x: 0, y: 200, vx: -MAX_SPEED, vy: 0 }, 100, 1)
    expect(Math.atan2(b.vy, b.vx)).toBeCloseTo(MAX_BOUNCE)
    expect(Math.hypot(b.vx, b.vy)).toBeCloseTo(MAX_SPEED)
  })
})

describe('fly', () => {
  it('folds off the walls the way step does', () => {
    const ball = { x: 320, y: 40, vx: 400, vy: -650 }
    let s = playing({ ball, paddles: [130, 130] })
    for (let i = 0; i < 60; i++) s = step(s, STEP_S)
    const b = fly(ball, 60 * STEP_S)
    expect(b.x).toBeCloseTo(s.ball.x, 6)
    expect(b.y).toBeCloseTo(s.ball.y, 6)
    expect(b.vy).toBe(s.ball.vy)
  })

  it('times the ball to a face', () => {
    expect(secondsTo({ x: 100, y: 0, vx: 200, vy: 0 }, GUEST_FACE_X)).toBeCloseTo((GUEST_FACE_X - 100) / 200)
    expect(secondsTo({ x: 100, y: 0, vx: -200, vy: 0 }, HOST_FACE_X)).toBeCloseTo((100 - HOST_FACE_X) / 200)
    expect(secondsTo({ x: 10, y: 0, vx: -200, vy: 0 }, HOST_FACE_X)).toBe(0)
  })
})

describe('legs', () => {
  const leg: Leg = { game: 2, n: 5, at: 1234.56, kind: 'play', ball: { x: 100.123, y: 50.46, vx: -300.04, vy: 12.5 }, score: [3, 6] }

  it('round trip, rounded the same way the sender keeps them', () => {
    const msg = decodePong(encodePong({ t: 'l', ...leg, c: 99.99 }))
    expect(msg).toEqual({ t: 'l', ...wireLeg(leg), c: 100 })
    expect(wireLeg(leg)).toMatchObject({ at: 1234.6, ball: { x: 100.1, y: 50.5, vx: -300, vy: 12.5 } })
  })

  it('go by game, then by leg', () => {
    expect(newer({ ...leg, n: 6 }, leg)).toBe(true)
    expect(newer({ ...leg, game: 3, n: 0 }, leg)).toBe(true)
    expect(newer(leg, leg)).toBe(false)
    expect(newer({ ...leg, n: 4 }, leg)).toBe(false)
    expect(newer(leg, null)).toBe(true)
    expect(winnerOf([WIN_SCORE, 2])).toBe('host')
    expect(winnerOf([2, WIN_SCORE])).toBe('guest')
    expect(winnerOf([6, 6])).toBeUndefined()
  })
})

describe('wire messages', () => {
  it('round trip the small messages', () => {
    for (const m of [
      { t: 'hi', v: 2 },
      { t: 'hi', v: 2, ok: true, c: 5, e: 1.5, h: 2 },
      { t: 'again', c: 7 },
      { t: 'bye' },
      { t: 'p', at: 10.5, y: 120, d: -1, game: 1, n: 4 },
    ] as const) {
      expect(decodePong(encodePong(m))).toEqual(m)
    }
  })

  it('clamps what another machine could get wrong', () => {
    expect(decodePong('{"t":"p","at":1,"y":99999,"d":0,"r":0,"n":0}')).toMatchObject({ y: COURT_H - PADDLE_H / 2 })
    expect(decodePong('{"t":"l","r":1,"n":1,"at":1,"k":0,"b":[9999,-5,1e300,-1e300],"s":[-3,400.7]}')).toMatchObject({
      ball: { x: COURT_W + BALL_SIZE, y: BALL_SIZE / 2, vx: MAX_SPEED, vy: -MAX_SPEED },
      score: [0, 99],
    })
    expect(decodePong('{"t":"hi","v":1,"ok":"yes","c":"x"}')).toEqual({ t: 'hi', v: 1 })
  })

  it('refuses anything malformed', () => {
    for (const raw of ['', 'nope', 'null', '[]', '{"t":"x"}', '{"t":"p"}', '{"t":"p","y":3}', '{"t":"hi"}',
      // the last version's messages
      '{"t":"p","y":3}', '{"t":"s","at":1,"b":[1,2,3,4],"p":[0,0],"s":[0,0],"ph":1}',
      '{"t":"p","at":1,"y":3,"d":2,"r":0,"n":0}', '{"t":"p","at":1,"y":3,"d":0,"r":-1,"n":0}',
      '{"t":"l","r":1,"n":1,"at":1,"k":0,"b":[1,2,3],"s":[0,0]}', '{"t":"l","r":1,"n":1,"at":1,"k":7,"b":[1,2,3,4],"s":[0,0]}',
      '{"t":"l","r":1,"n":1.5,"at":1,"k":0,"b":[1,2,3,4],"s":[0,0]}', '{"t":"l","r":1,"n":1,"at":1,"k":0,"b":[1,2,3,null],"s":[0,0]}',
      // a ball in play that goes nowhere would never be judged
      '{"t":"l","r":1,"n":1,"at":1,"k":0,"b":[1,2,0,4],"s":[0,0]}']) {
      expect(decodePong(raw)).toBeNull()
    }
  })
})

describe('HostClock', () => {
  it('reads the offset from the quickest round trip', () => {
    const clock = new HostClock()
    expect(clock.offset()).toBe(0)
    // the host runs 5000ms ahead; one way takes 40ms, then 25ms
    clock.heard({ c: 5000 + 960 }, 1000)
    expect(clock.offset()).toBe(4960)
    clock.heard({ c: 5000 + 1960, e: 1900, h: 20 }, 2000)
    expect(clock.offset()).toBeCloseTo(5000)
    // a slow, lopsided trip doesn't move it
    clock.heard({ c: 5000 + 2900, e: 2700, h: 10 }, 3000)
    expect(clock.offset()).toBeCloseTo(5000)
    clock.heard({ c: 5000 + 3975, e: 3940, h: 10 }, 4000)
    expect(clock.offset()).toBeCloseTo(5000)
  })
})

describe('FarPaddle', () => {
  it('carries the paddle on at paddle speed, and eases a report in', () => {
    const p = new FarPaddle()
    p.report(100, 1, 1000, 1000, -1000)
    expect(p.where(1100)).toBeCloseTo(100 + PADDLE_SPEED * 0.1)
    expect(p.where(10_000)).toBe(COURT_H - PADDLE_H / 2)
    // it stopped at 120 50ms ago: drawn where it was, sliding to 120
    p.report(120, 0, 1050, 1100, 100)
    expect(p.shown(1100, 100)).toBeCloseTo(100 + PADDLE_SPEED * 0.1)
    expect(p.shown(1100, 100 + 6 * EASE_MS)).toBeCloseTo(120, 0)
    // an older report is dropped
    p.report(30, 0, 1020, 1100, 400)
    expect(p.where(1200)).toBe(120)
  })
})
