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
  INTERP_MS,
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
  SnapshotBuffer,
  snapshotOf,
  step,
  STEP_S,
  WIN_SCORE,
  type PongState,
  type Snapshot,
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

describe('wire messages', () => {
  it('round trip state, rounded', () => {
    const s = playing({ ball: { x: 100.123, y: 50.46, vx: -300.04, vy: 12.5 }, paddles: [80.33, 90], score: [3, 6] })
    const msg = decodePong(encodePong({ t: 's', ...snapshotOf(s, 1234.6) }))
    expect(msg).toEqual({ t: 's', at: 1235, ball: { x: 100.1, y: 50.5, vx: -300, vy: 12.5 }, paddles: [80.3, 90], score: [3, 6], phase: 'play' })
  })

  it('keeps the winner', () => {
    const msg = decodePong(encodePong({ t: 's', ...snapshotOf({ ...newGame(), phase: 'over', winner: 'guest', score: [2, 7] }, 1) }))
    expect(msg).toMatchObject({ phase: 'over', winner: 'guest' })
  })

  it('round trip the small messages', () => {
    for (const m of [{ t: 'hi', v: 1 }, { t: 'hi', v: 1, ok: true }, { t: 'again' }, { t: 'bye' }, { t: 'p', y: 120 }] as const) {
      expect(decodePong(encodePong(m))).toEqual(m)
    }
  })

  it('clamps what another machine could get wrong', () => {
    expect(decodePong('{"t":"p","y":99999}')).toEqual({ t: 'p', y: COURT_H - PADDLE_H / 2 })
    expect(decodePong('{"t":"s","at":1,"b":[9999,-5,0,0],"p":[0,0],"s":[-3,400.7],"ph":1}')).toMatchObject({
      ball: { x: COURT_W + BALL_SIZE, y: 0 },
      score: [0, 99],
    })
    // a wild speed would fling the guest's carried-on ball off the court
    expect(decodePong('{"t":"s","at":1,"b":[0,0,1e300,-1e300],"p":[0,0],"s":[0,0],"ph":1}')).toMatchObject({
      ball: { vx: MAX_SPEED, vy: -MAX_SPEED },
    })
    expect(decodePong('{"t":"hi","v":1,"ok":"yes"}')).toEqual({ t: 'hi', v: 1 })
  })

  it('refuses anything malformed', () => {
    for (const raw of ['', 'nope', 'null', '[]', '{"t":"x"}', '{"t":"p"}', '{"t":"p","y":"3"}', '{"t":"hi"}',
      '{"t":"s","at":1,"b":[1,2,3],"p":[0,0],"s":[0,0],"ph":1}', '{"t":"s","at":1,"b":[1,2,3,4],"p":[0,0],"s":[0,0],"ph":7}',
      '{"t":"s","at":1,"b":[1,2,3,null],"p":[0,0],"s":[0,0],"ph":1}']) {
      expect(decodePong(raw)).toBeNull()
    }
  })
})

describe('SnapshotBuffer', () => {
  const snap = (at: number, x: number, over: Partial<Snapshot> = {}): Snapshot => ({
    at,
    ball: { x, y: 100, vx: 300, vy: 0 },
    paddles: [100, 100],
    score: [0, 0],
    phase: 'play',
    ...over,
  })

  it('draws INTERP_MS behind, easing between snapshots', () => {
    const buf = new SnapshotBuffer()
    // host clock runs 5000ms behind ours, plus some arrival jitter
    buf.push(snap(1000, 100), 6000)
    buf.push(snap(1033, 110), 6040)
    buf.push(snap(1066, 120), 6066)
    buf.push(snap(1100, 130), 6101)
    // the least delayed arrival sets the offset at 5000, so this local time draws host time 1050
    const mid = buf.sample(1050 + 5000 + INTERP_MS)
    expect(mid?.ball.x).toBeCloseTo(110 + (10 * 17) / 33, 1)
  })

  it('carries the ball a little past the newest, then holds', () => {
    const buf = new SnapshotBuffer()
    buf.push(snap(0, 100), 1000)
    expect(buf.sample(1000 + INTERP_MS + 50)?.ball.x).toBeCloseTo(100 + 300 * 0.05)
    expect(buf.sample(1000 + INTERP_MS + 5000)?.ball.x).toBeCloseTo(100 + 300 * 0.1)
  })

  it('jumps across a point instead of sliding the ball back to centre', () => {
    const buf = new SnapshotBuffer()
    buf.push(snap(0, 630), 1000)
    buf.push(snap(33, 320, { phase: 'serve', score: [1, 0] }), 1033)
    const early = buf.sample(1000 + 10 + INTERP_MS)
    const late = buf.sample(1000 + 25 + INTERP_MS)
    expect(early?.ball.x).toBe(630)
    expect(late?.ball.x).toBe(320)
    expect(late?.score).toEqual([1, 0])
  })

  it('ignores stale and repeated snapshots', () => {
    const buf = new SnapshotBuffer()
    buf.push(snap(100, 1), 0)
    buf.push(snap(50, 2), 0)
    buf.push(snap(100, 3), 0)
    expect(buf.latest()?.ball.x).toBe(1)
  })
})
