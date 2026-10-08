// two-player pong over the peer link. the host is authoritative: it runs step() and sends state;
// the guest sends its paddle and draws the host's state a little behind, interpolated

export const PONG_VERSION = 1
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
export const STATE_HZ = 30
// how far behind the host's clock the guest draws, so there's always a later snapshot to ease toward
export const INTERP_MS = 80
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

function centreBall(): Ball {
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

function overlapsPaddle(y: number, paddleY: number): boolean {
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

// ---- wire messages, as json text over the peer link

export type Snapshot = {
  // host clock, ms
  at: number
  ball: Ball
  paddles: [number, number]
  score: [number, number]
  phase: Phase
  winner?: Side
}

export type PongMsg =
  // ok: the sender has already heard the other side's hi, so it needs no answer
  | { t: 'hi'; v: number; ok?: boolean }
  | { t: 'p'; y: number }
  | ({ t: 's' } & Snapshot)
  | { t: 'again' }
  | { t: 'bye' }

const PHASES: Phase[] = ['serve', 'play', 'over']
const round1 = (n: number) => Math.round(n * 10) / 10

// state is the one sent 30 times a second, so it goes as short arrays
export function encodePong(msg: PongMsg): string {
  if (msg.t !== 's') return JSON.stringify(msg)
  const { ball, paddles, score, phase, winner, at } = msg
  return JSON.stringify({
    t: 's',
    at: Math.round(at),
    b: [ball.x, ball.y, ball.vx, ball.vy].map(round1),
    p: paddles.map(round1),
    s: score,
    ph: PHASES.indexOf(phase),
    ...(winner ? { w: winner === 'host' ? 0 : 1 } : {}),
  })
}

const finite = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v)
const numbers = (v: unknown, n: number): v is number[] => Array.isArray(v) && v.length === n && v.every(finite)

// anything malformed is null, never a throw: this is text from another machine
export function decodePong(raw: string): PongMsg | null {
  let m: unknown
  try {
    m = JSON.parse(raw)
  } catch {
    return null
  }
  if (!m || typeof m !== 'object') return null
  const o = m as Record<string, unknown>
  switch (o.t) {
    case 'hi':
      return finite(o.v) ? { t: 'hi', v: o.v, ...(o.ok === true ? { ok: true } : {}) } : null
    case 'p':
      return finite(o.y) ? { t: 'p', y: clampPaddle(o.y) } : null
    case 'again':
      return { t: 'again' }
    case 'bye':
      return { t: 'bye' }
    case 's': {
      if (!finite(o.at) || !numbers(o.b, 4) || !numbers(o.p, 2) || !numbers(o.s, 2) || !finite(o.ph)) return null
      const phase = PHASES[o.ph]
      if (!phase) return null
      const [x, y, vx, vy] = o.b
      const score = o.s.map((n) => clamp(Math.floor(n), 0, 99)) as [number, number]
      return {
        t: 's',
        at: o.at,
        ball: { x: clamp(x, -BALL_SIZE, COURT_W + BALL_SIZE), y: clamp(y, 0, COURT_H), vx: clamp(vx, -MAX_SPEED, MAX_SPEED), vy: clamp(vy, -MAX_SPEED, MAX_SPEED) },
        paddles: [clampPaddle(o.p[0]), clampPaddle(o.p[1])],
        score,
        phase,
        ...(o.w === 0 || o.w === 1 ? { winner: o.w === 0 ? 'host' : 'guest' } : {}),
      }
    }
    default:
      return null
  }
}

export function snapshotOf(s: PongState, at: number): Snapshot {
  return { at, ball: s.ball, paddles: s.paddles, score: s.score, phase: s.phase, winner: s.winner }
}

// ---- the guest's view of the host's snapshots

export class SnapshotBuffer {
  private snaps: Snapshot[] = []
  // local time minus host time, the smallest seen: the least delayed arrival is the best guess at the offset
  private offset = Infinity
  private recent: number[] = []

  push(snap: Snapshot, localNow: number): void {
    const last = this.snaps[this.snaps.length - 1]
    if (last && snap.at <= last.at) return
    this.snaps.push(snap)
    if (this.snaps.length > 32) this.snaps.shift()
    this.recent.push(localNow - snap.at)
    if (this.recent.length > 60) this.recent.shift()
    this.offset = Math.min(...this.recent)
  }

  latest(): Snapshot | undefined {
    return this.snaps[this.snaps.length - 1]
  }

  // the host's ball and paddle as they were INTERP_MS ago; a new serve or a point jumps rather than slides
  sample(localNow: number): Snapshot | undefined {
    const snaps = this.snaps
    if (!snaps.length) return undefined
    const at = localNow - this.offset - INTERP_MS
    let i = snaps.length - 1
    while (i > 0 && snaps[i - 1]!.at > at) i--
    const b = snaps[i]!
    const a = snaps[i - 1]
    if (!a || at >= b.at) {
      // past the newest: carry the ball on a little, then hold
      const ahead = Math.min(Math.max(0, at - b.at), 100) / 1000
      if (b.phase !== 'play' || !ahead) return b
      return { ...b, ball: { ...b.ball, x: b.ball.x + b.ball.vx * ahead, y: clamp(b.ball.y + b.ball.vy * ahead, BALL_SIZE / 2, COURT_H - BALL_SIZE / 2) } }
    }
    if (at <= a.at) return a
    const k = (at - a.at) / (b.at - a.at)
    const lerp = (p: number, q: number) => p + (q - p) * k
    const jump = a.phase !== b.phase || a.score[0] !== b.score[0] || a.score[1] !== b.score[1]
    return {
      ...b,
      at,
      ball: jump ? (k < 0.5 ? a.ball : b.ball) : { ...b.ball, x: lerp(a.ball.x, b.ball.x), y: lerp(a.ball.y, b.ball.y) },
      paddles: [lerp(a.paddles[0], b.paddles[0]), lerp(a.paddles[1], b.paddles[1])],
      score: k < 0.5 ? a.score : b.score,
      phase: k < 0.5 ? a.phase : b.phase,
      winner: k < 0.5 ? a.winner : b.winner,
    }
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
