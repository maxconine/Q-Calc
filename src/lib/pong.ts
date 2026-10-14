import { otherHello } from './hello'
// two-player pong over the peer link, and solo against the computer. solo runs step(); online, each side judges the
// ball at its own paddle and sends the next leg of its flight (see below)

// 2: each side judges its own paddle. 1 had the host judge both
export const PONG_VERSION = 2
export const PONG_HINT = '↵ play pong'

export const COURT_W = 640
export const COURT_H = 260
export const PADDLE_W = 8
export const PADDLE_H = 52
export const PADDLE_INSET = 18
// px per second
export const PADDLE_SPEED = 420
export const BALL_SIZE = 8
export const SERVE_SPEED = 300
export const MAX_SPEED = 720
export const SPEED_UP = 1.06
// steepest return off a paddle's edge
export const MAX_BOUNCE = (55 * Math.PI) / 180
export const MAX_SERVE_ANGLE = (25 * Math.PI) / 180
export const SERVE_PAUSE_S = 0.9
export const WIN_SCORE = 7
// fixed simulation step; at max speed the ball moves 6px a step, under a paddle's width
export const STEP_S = 1 / 120
// nothing from the other side for this long and the game is over
export const SILENCE_MS = 3000

export type Side = 'host' | 'guest'
export type Phase = 'serve' | 'play' | 'over'

export type Ball = { x: number; y: number; vx: number; vy: number }

// host on the left, guest on the right; paddle ys are centres
export type PongState = {
  ball: Ball
  paddles: [host: number, guest: number]
  score: [host: number, guest: number]
  phase: Phase
  // seconds until the serve, while phase is serve
  serveIn: number
  // +1 serves toward the guest, -1 toward the host
  serveDir: 1 | -1
  winner?: Side
}

export function isPongCommand(text: string): boolean {
  return /^pong$/i.test(text.trim())
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v))

export function clampPaddle(y: number): number {
  return clamp(y, PADDLE_H / 2, COURT_H - PADDLE_H / 2)
}

// dir is -1 up, 0 still, 1 down
export function movePaddle(y: number, dir: number, dt: number): number {
  return clampPaddle(y + Math.sign(dir) * PADDLE_SPEED * dt)
}

export function centreBall(): Ball {
  return { x: COURT_W / 2, y: COURT_H / 2, vx: 0, vy: 0 }
}

export function newGame(serveDir: 1 | -1 = 1): PongState {
  return {
    ball: centreBall(),
    paddles: [COURT_H / 2, COURT_H / 2],
    score: [0, 0],
    phase: 'serve',
    serveIn: SERVE_PAUSE_S,
    serveDir,
  }
}

// rand in [0, 1) picks the serve angle
export function serve(dir: 1 | -1, rand: () => number): Ball {
  const angle = (rand() * 2 - 1) * MAX_SERVE_ANGLE
  return { x: COURT_W / 2, y: COURT_H / 2, vx: Math.cos(angle) * SERVE_SPEED * dir, vy: Math.sin(angle) * SERVE_SPEED }
}

// the return angle follows where the ball met the paddle: centre sends it flat, the edges steep
export function bounceOff(ball: Ball, paddleY: number, dir: 1 | -1): Ball {
  const reach = PADDLE_H / 2 + BALL_SIZE / 2
  const offset = clamp((ball.y - paddleY) / reach, -1, 1)
  const speed = Math.min(MAX_SPEED, Math.hypot(ball.vx, ball.vy) * SPEED_UP)
  const angle = offset * MAX_BOUNCE
  return { ...ball, vx: Math.cos(angle) * speed * dir, vy: Math.sin(angle) * speed }
}

export function overlapsPaddle(y: number, paddleY: number): boolean {
  return Math.abs(y - paddleY) <= PADDLE_H / 2 + BALL_SIZE / 2
}

// one fixed step of the ball: walls, paddles, points. paddles are moved by the caller first
export function step(s: PongState, dt: number, rand: () => number = Math.random): PongState {
  if (s.phase === 'over') return s
  if (s.phase === 'serve') {
    const serveIn = s.serveIn - dt
    if (serveIn > 0) return { ...s, serveIn }
    return { ...s, phase: 'play', serveIn: 0, ball: serve(s.serveDir, rand) }
  }
  const half = BALL_SIZE / 2
  let { x, y, vx, vy } = s.ball
  const px = x
  x += vx * dt
  y += vy * dt
  if (y < half) {
    y = 2 * half - y
    vy = Math.abs(vy)
  } else if (y > COURT_H - half) {
    y = 2 * (COURT_H - half) - y
    vy = -Math.abs(vy)
  }
  let ball: Ball = { x, y, vx, vy }
  const leftFace = PADDLE_INSET + PADDLE_W
  const rightFace = COURT_W - PADDLE_INSET - PADDLE_W
  // only the face that faces the court, and only as the ball crosses it
  if (vx < 0 && px - half >= leftFace && x - half < leftFace && overlapsPaddle(y, s.paddles[0])) {
    ball = bounceOff({ ...ball, x: leftFace + half }, s.paddles[0], 1)
  } else if (vx > 0 && px + half <= rightFace && x + half > rightFace && overlapsPaddle(y, s.paddles[1])) {
    ball = bounceOff({ ...ball, x: rightFace - half }, s.paddles[1], -1)
  }
  if (ball.x + half < 0) return point(s, 'guest')
  if (ball.x - half > COURT_W) return point(s, 'host')
  return { ...s, ball }
}

function point(s: PongState, scorer: Side): PongState {
  const score: [number, number] = scorer === 'host' ? [s.score[0] + 1, s.score[1]] : [s.score[0], s.score[1] + 1]
  const won = score[0] >= WIN_SCORE || score[1] >= WIN_SCORE
  return {
    ...s,
    score,
    ball: centreBall(),
    phase: won ? 'over' : 'serve',
    serveIn: won ? 0 : SERVE_PAUSE_S,
    // toward whoever just lost the point
    serveDir: scorer === 'host' ? 1 : -1,
    winner: won ? scorer : undefined,
  }
}

// steps whole STEP_S slices out of real time; the rest carries to the next frame
export function advance(s: PongState, carry: number, dt: number, rand?: () => number): { state: PongState; carry: number } {
  let left = carry + Math.min(dt, 0.25)
  let state = s
  while (left >= STEP_S) {
    state = step(state, STEP_S, rand)
    left -= STEP_S
  }
  return { state, carry: left }
}

// what a court draws
export type Snapshot = {
  // shared clock, ms
  at: number
  ball: Ball
  paddles: [number, number]
  score: [number, number]
  phase: Phase
  winner?: Side
}

// ---- online play. each side judges its own paddle: while the ball heads toward a side, that side alone decides
// the hit or the miss, against its own paddle where it is now, and sends the next leg of the ball's flight. the
// other side flies the ball on from that leg by a shared clock, the host's, and guesses the far bounce from the
// far paddle as it draws it until the real leg comes in. the score only changes in a leg, and only the side the
// ball was heading to sends one, so the two can't both count a point

// where the ball's centre meets each paddle's face
export const HOST_FACE_X = PADDLE_INSET + PADDLE_W + BALL_SIZE / 2
export const GUEST_FACE_X = COURT_W - PADDLE_INSET - PADDLE_W - BALL_SIZE / 2
// a correction fades rather than jumps, to a third in this long
export const EASE_MS = 50
// one bigger than this jumps instead: sliding that far would read as the ball flying backward
export const SNAP_PX = 96
// a leg that reaches the side it's heading to later than this, after a stall, is taken up from this far in, so
// the side that has to judge it still sees it coming
export const LATE_MS = 200

export type Dir = -1 | 0 | 1
export type LegKind = 'play' | 'serve' | 'over'

// a stretch of the ball's flight between two judgements. game counts games (a rematch is the next one), n the legs
// within one. at is shared time in ms: when the leg starts, or for a serve when the ball leaves the centre
export type Leg = { game: number; n: number; at: number; kind: LegKind; ball: Ball; score: [number, number] }

export const towards = (b: Ball): Side => (b.vx > 0 ? 'guest' : 'host')

export const fade = (ms: number): number => Math.exp(-Math.max(0, ms) / EASE_MS)

export function newer(a: Leg, b: Leg | null): boolean {
  return !b || a.game > b.game || (a.game === b.game && a.n > b.n)
}

export function winnerOf(score: [number, number]): Side | undefined {
  return score[0] >= WIN_SCORE ? 'host' : score[1] >= WIN_SCORE ? 'guest' : undefined
}

// the ball s seconds on, off the walls but through the paddles, which are judged apart
export function fly(b: Ball, s: number): Ball {
  const half = BALL_SIZE / 2
  const span = COURT_H - 2 * half
  const m = (((b.y - half + b.vy * s) % (2 * span)) + 2 * span) % (2 * span)
  const down = m <= span
  return { x: b.x + b.vx * s, y: half + (down ? m : 2 * span - m), vx: b.vx, vy: down ? b.vy : -b.vy }
}

// seconds until the ball's centre reaches x; 0 if it's already there or past
export function secondsTo(b: Ball, x: number): number {
  if (!b.vx) return Infinity
  return Math.max(0, (x - b.x) / b.vx)
}

// ---- wire messages, as json text over the peer link

// every message carries the sender's own clock as c. the host's also echo the guest's latest c as e, with how long
// it held it as h: that's the guest's round trip, and from it the host's clock
export type Stamp = { c?: number; e?: number; h?: number }

export type PongMsg =
  // ok: the sender has already heard the other side's hi, so it needs no answer
  | ({ t: 'hi'; v: number; g?: 'pong'; ok?: boolean } & Stamp)
  // a paddle at shared time at and the way it's going, sent when that changes and as a keepalive. game and n: the
  // newest leg the sender has, so one that went missing can go again
  | ({ t: 'p'; at: number; y: number; d: Dir; game: number; n: number } & Stamp)
  | ({ t: 'l' } & Leg & Stamp)
  | ({ t: 'again' } & Stamp)
  | { t: 'bye' }

const KINDS: LegKind[] = ['play', 'serve', 'over']
const round1 = (n: number) => Math.round(n * 10) / 10

function stampOf(m: Stamp): Stamp {
  return {
    ...(m.c != null ? { c: round1(m.c) } : {}),
    ...(m.e != null ? { e: round1(m.e) } : {}),
    ...(m.h != null ? { h: round1(m.h) } : {}),
  }
}

// a leg as it goes over the wire; the side that sends one keeps this copy too, so both fly the very same ball
export function wireLeg(l: Leg): Leg {
  const { x, y, vx, vy } = l.ball
  return { game: l.game, n: l.n, at: round1(l.at), kind: l.kind, ball: { x: round1(x), y: round1(y), vx: round1(vx), vy: round1(vy) }, score: [l.score[0], l.score[1]] }
}

export function encodePong(msg: PongMsg): string {
  switch (msg.t) {
    case 'p':
      return JSON.stringify({ t: 'p', at: round1(msg.at), y: round1(msg.y), d: msg.d, r: msg.game, n: msg.n, ...stampOf(msg) })
    case 'l': {
      const l = wireLeg(msg)
      const { x, y, vx, vy } = l.ball
      return JSON.stringify({ t: 'l', r: l.game, n: l.n, at: l.at, k: KINDS.indexOf(l.kind), b: [x, y, vx, vy], s: l.score, ...stampOf(msg) })
    }
    case 'bye':
      return JSON.stringify(msg)
    default:
      return JSON.stringify({ ...msg, ...stampOf(msg) })
  }
}

const finite = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v)
const numbers = (v: unknown, n: number): v is number[] => Array.isArray(v) && v.length === n && v.every(finite)
const count = (v: unknown): v is number => Number.isInteger(v) && (v as number) >= 0 && (v as number) < 1e9

function readStamp(o: Record<string, unknown>): Stamp {
  return { ...(finite(o.c) ? { c: o.c } : {}), ...(finite(o.e) ? { e: o.e } : {}), ...(finite(o.h) ? { h: o.h } : {}) }
}

// anything malformed is null, never a throw: this is text from another machine
// what can come in: a pong message, or another game's hello from a friend who opened that instead
export type PongIn = PongMsg | { t: 'other'; game: string }

export function decodePong(raw: string): PongIn | null {
  let m: unknown
  try {
    m = JSON.parse(raw)
  } catch {
    return null
  }
  if (!m || typeof m !== 'object') return null
  const o = m as Record<string, unknown>
  const other = otherHello(o, 'pong')
  if (other) return { t: 'other', game: other }
  switch (o.t) {
    case 'hi':
      return finite(o.v) ? { t: 'hi', v: o.v, ...(o.ok === true ? { ok: true } : {}), ...readStamp(o) } : null
    case 'p':
      if (!finite(o.at) || !finite(o.y) || (o.d !== -1 && o.d !== 0 && o.d !== 1) || !count(o.r) || !count(o.n)) return null
      return { t: 'p', at: o.at, y: clampPaddle(o.y), d: o.d, game: o.r, n: o.n, ...readStamp(o) }
    case 'l': {
      if (!count(o.r) || !count(o.n) || !finite(o.at) || !numbers(o.b, 4) || !numbers(o.s, 2) || !finite(o.k)) return null
      const kind = KINDS[o.k]
      if (!kind) return null
      const [x, y, vx, vy] = o.b
      // a ball in flight has to be going somewhere, or nobody would ever judge it
      if (kind !== 'over' && Math.abs(vx) < 1) return null
      const half = BALL_SIZE / 2
      return {
        t: 'l',
        game: o.r,
        n: o.n,
        at: o.at,
        kind,
        ball: { x: clamp(x, -BALL_SIZE, COURT_W + BALL_SIZE), y: clamp(y, half, COURT_H - half), vx: clamp(vx, -MAX_SPEED, MAX_SPEED), vy: clamp(vy, -MAX_SPEED, MAX_SPEED) },
        score: o.s.map((n) => clamp(Math.floor(n), 0, 99)) as [number, number],
        ...readStamp(o),
      }
    }
    case 'again':
      return { t: 'again', ...readStamp(o) }
    case 'bye':
      return { t: 'bye' }
    default:
      return null
  }
}

export function snapshotOf(s: PongState, at: number): Snapshot {
  return { at, ball: s.ball, paddles: s.paddles, score: s.score, phase: s.phase, winner: s.winner }
}

// ---- the clock and the far paddle

// the guest's guess at the host's clock. a round trip pins the offset to within half its lopsidedness, so the
// quickest recent one wins; before the first, the host's stamps as they land, as if they took no time
export class HostClock {
  private trips: Array<{ rtt: number; offset: number }> = []
  private rough = -Infinity

  heard(s: Stamp, now: number): void {
    if (s.c == null) return
    this.rough = Math.max(this.rough, s.c - now)
    if (s.e == null || s.h == null) return
    const rtt = now - s.e - s.h
    if (!(rtt >= 0 && rtt < 10_000)) return
    this.trips.push({ rtt, offset: s.c + rtt / 2 - now })
    if (this.trips.length > 40) this.trips.shift()
  }

  // the host's clock minus ours
  offset(): number {
    let best: { rtt: number; offset: number } | undefined
    for (const t of this.trips) if (!best || t.rtt < best.rtt) best = t
    if (best) return best.offset
    return Number.isFinite(this.rough) ? this.rough : 0
  }
}

// the other side's paddle, carried on at paddle speed from its last report, which is exact while a key is held;
// a report that moves it eases in
export class FarPaddle {
  private y = COURT_H / 2
  private d: Dir = 0
  private at = 0
  private seen = -Infinity
  private off = 0
  private offAt = 0

  // where it is at shared time t, by the reports
  where(t: number): number {
    return clampPaddle(this.y + (this.d * PADDLE_SPEED * clamp(t - this.at, 0, 1000)) / 1000)
  }

  // where it's drawn at shared time t, our time now
  shown(t: number, now: number): number {
    return clampPaddle(this.where(t) + this.off * fade(now - this.offAt))
  }

  report(y: number, d: Dir, at: number, t: number, now: number): void {
    if (at < this.seen) return
    const was = this.shown(t, now)
    this.y = y
    this.d = d
    this.at = at
    this.seen = at
    const off = was - this.where(t)
    this.off = Math.abs(off) > SNAP_PX ? 0 : off
    this.offAt = now
  }
}

// ---- the computer, for solo games: it plays the guest's paddle on the right

export type Skill = 'easy' | 'normal' | 'hard'
export const SKILLS: readonly Skill[] = ['easy', 'normal', 'hard']

// how the computer plays at each level. reach: the share of the court, from its own side, where it starts
// following an incoming ball; speed: of a player's paddle; miss: how far off its aim can land, in paddle heights, for a
// ball at serve speed; a faster ball throws its aim off further, so even hard cracks once a rally gets quick
const BOT: Record<Skill, { reach: number; speed: number; miss: number }> = {
  easy: { reach: 0.45, speed: 0.55, miss: 0.7 },
  normal: { reach: 0.7, speed: 0.8, miss: 0.45 },
  hard: { reach: 1, speed: 1, miss: 0.3 },
}

// what the computer keeps between frames: where on its paddle it means to meet the ball, picked once per rally
export type BotMind = { aim: number; coming: boolean }

export function newBotMind(): BotMind {
  return { aim: 0, coming: false }
}

// where the ball will be, top to bottom, when it reaches x, folding its path off the walls
export function landingY(ball: Ball, x: number): number {
  if (ball.vx === 0) return ball.y
  const t = (x - ball.x) / ball.vx
  if (t <= 0) return ball.y
  const half = BALL_SIZE / 2
  const span = COURT_H - 2 * half
  const raw = ball.y - half + ball.vy * t
  const m = ((raw % (2 * span)) + 2 * span) % (2 * span)
  return half + (m <= span ? m : 2 * span - m)
}

// one frame of the computer's paddle. it heads for where the ball will land once the ball is near enough,
// and drifts back to the middle while the ball goes the other way
export function botStep(s: PongState, y: number, mind: BotMind, skill: Skill, dt: number, rand: () => number): { y: number; mind: BotMind } {
  const bot = BOT[skill]
  const coming = s.phase === 'play' && s.ball.vx > 0
  // a new rally toward it: decide how well this one goes
  const pace = Math.hypot(s.ball.vx, s.ball.vy) / SERVE_SPEED
  const next: BotMind = coming && !mind.coming ? { aim: (rand() * 2 - 1) * bot.miss * pace * PADDLE_H, coming } : { ...mind, coming }
  const face = COURT_W - PADDLE_INSET - PADDLE_W
  const seen = coming && s.ball.x >= face - bot.reach * COURT_W
  const target = seen ? landingY(s.ball, face - BALL_SIZE / 2) + next.aim : COURT_H / 2
  const gap = target - y
  // a dead zone, so it doesn't shiver once it's there
  if (Math.abs(gap) < 2) return { y, mind: next }
  const most = PADDLE_SPEED * bot.speed * dt
  return { y: clampPaddle(y + clamp(gap, -most, most)), mind: next }
}
