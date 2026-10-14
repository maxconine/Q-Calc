import { otherGameNote } from './hello'
import {
  advance,
  BALL_SIZE,
  botStep,
  bounceOff,
  centreBall,
  clampPaddle,
  COURT_H,
  COURT_W,
  decodePong,
  encodePong,
  fade,
  FarPaddle,
  fly,
  GUEST_FACE_X,
  HOST_FACE_X,
  HostClock,
  LATE_MS,
  movePaddle,
  newBotMind,
  newer,
  newGame,
  overlapsPaddle,
  PADDLE_SPEED,
  PONG_VERSION,
  secondsTo,
  serve,
  SERVE_PAUSE_S,
  SILENCE_MS,
  SNAP_PX,
  snapshotOf,
  towards,
  winnerOf,
  wireLeg,
  type Ball,
  type BotMind,
  type Dir,
  type Leg,
  type Phase,
  type PongMsg,
  type PongState,
  type Side,
  type Skill,
  type Snapshot,
} from './pong'

// hi goes out this often until the other side's hi comes in, since its court may not have been listening yet;
// a hi from a side that hasn't heard ours gets one back, so whichever court came up first still hears the other
const HI_MS = 400
// a paddle goes out when its way changes, at most this often, and at least every KEEP_MS. the other side carries it
// on at paddle speed in between, so the changes are all it needs
const PADDLE_MS = 1000 / 30
// frame times land a hair either side of a frame; without this a change could wait a whole extra frame
const PADDLE_SLACK_MS = 1
const KEEP_MS = 250
// the other side is still on an older leg than the newest one sent from here, this long after: it went missing
const RESEND_MS = 1000
// the guest's guess at the host's clock improves as quicker round trips come in; it moves toward the better one
// at a tenth of real time, so the ball runs a touch fast or slow for a moment rather than jumping. further off than
// this and it jumps
const SLEW = 0.1
const SLEW_MAX_MS = 250

export type SessionView = {
  // what to draw
  snap: Snapshot | null
  started: boolean
  again: { mine: boolean; theirs: boolean }
  // set once the game is over for good: the line the lobby shows
  ended: string | null
}

// one side of a game, without the page: the court component feeds it keys, messages and frame times,
// and draws what tick() returns
export class PongSession {
  // solo: the whole game, stepped here. online it only holds the serve's way for the next game
  private game: PongState
  private carry = 0
  private held = { up: false, down: false }
  private mineY = COURT_H / 2
  // solo: the computer's paddle
  private theirY = COURT_H / 2
  // the other side's hi came in
  private greeted = false
  // host: the guest said hi; guest: the first leg came
  private started = false
  private heard: number
  private lastTick: number
  private lastHi: number
  private again = { mine: false, theirs: false }
  private ended: string | null = null

  // online: the newest leg of the ball's flight, and whether it was judged here
  private leg: Leg | null = null
  private legMine = false
  // the ball went past this side's paddle; the point goes when it leaves the court
  private missed = false
  // the far bounce as guessed from the far paddle as drawn here, until the far side's leg comes
  private guess: Leg | null = null
  private guessed = false
  private ease = { x: 0, y: 0, at: 0 }
  private far = new FarPaddle()
  // guest: the host's clock, and the offset to it in use
  private clock = new HostClock()
  private offset = 0
  // host: the guest's latest clock stamp and when it came, to echo
  private echo: { c: number; at: number } | null = null
  private sent: { y: number; d: Dir; at: number } = { y: COURT_H / 2, d: 0, at: -Infinity }
  private resentAt = -Infinity

  readonly role: Side
  // a game against the computer: no link, the computer plays the guest's paddle
  readonly solo: Skill | null
  private bot: BotMind = newBotMind()
  private readonly send: (text: string) => void
  private readonly rand: () => number

  constructor(role: Side, send: (text: string) => void, now: number, rand: () => number = Math.random, solo: Skill | null = null) {
    this.role = solo ? 'host' : role
    this.solo = solo
    this.send = send
    this.rand = rand
    this.game = newGame(rand() < 0.5 ? 1 : -1)
    this.heard = now
    this.lastTick = now
    this.lastHi = now
    if (solo) {
      this.greeted = true
      this.started = true
    } else this.hi(now)
  }

  private hi(now: number): void {
    this.post({ t: 'hi', v: PONG_VERSION, g: 'pong', ...(this.greeted ? { ok: true } : {}) }, now)
  }

  private post(m: PongMsg, now: number): void {
    if (m.t !== 'bye') {
      m.c = now
      if (this.echo) {
        m.e = this.echo.c
        m.h = now - this.echo.at
      }
    }
    this.send(encodePong(m))
  }

  private end(note: string): void {
    if (!this.ended) this.ended = note
  }

  // the shared clock: the host's own, the guest's guess at it
  private shared(now: number): number {
    return this.role === 'host' ? now : now + this.offset
  }

  // guest: moves the offset in use toward the clock's best guess, by at most SLEW of ms
  private follow(ms: number): void {
    const gap = this.clock.offset() - this.offset
    this.offset += !this.leg || Math.abs(gap) > SLEW_MAX_MS ? gap : Math.min(ms * SLEW, Math.max(-ms * SLEW, gap))
  }

  private over(): boolean {
    return this.solo ? this.game.phase === 'over' : this.leg?.kind === 'over'
  }

  private restart(now: number): void {
    this.game = newGame(this.rand() < 0.5 ? 1 : -1)
    this.carry = 0
    this.bot = newBotMind()
    this.again = { mine: false, theirs: false }
    if (!this.solo) this.produce(this.serveLeg((this.leg?.game ?? 0) + 1, 0, this.shared(now), this.game.serveDir, [0, 0]), now)
  }

  private serveLeg(game: number, n: number, from: number, dir: 1 | -1, score: [number, number]): Leg {
    return { game, n, at: from + SERVE_PAUSE_S * 1000, kind: 'serve', ball: serve(dir, this.rand), score }
  }

  key(dir: 'up' | 'down', down: boolean): void {
    this.held[dir] = down
  }

  release(): void {
    this.held = { up: false, down: false }
  }

  // enter: asks for a rematch once the game is over; the host restarts when both have asked
  rematch(): void {
    if (this.ended || !this.over() || this.again.mine) return
    // the computer always wants another
    if (this.solo) {
      this.restart(this.lastTick)
      return
    }
    this.again = { ...this.again, mine: true }
    this.post({ t: 'again' }, this.lastTick)
    if (this.role === 'host' && this.again.theirs) this.restart(this.lastTick)
  }

  receive(text: string, now: number): void {
    if (this.ended) return
    const m = decodePong(text)
    if (!m) return
    this.heard = now
    if (m.t !== 'other' && m.t !== 'bye') {
      if (this.role === 'host' && m.c != null) this.echo = { c: m.c, at: now }
      else if (this.role === 'guest') {
        this.clock.heard(m, now)
        this.follow(0)
      }
    }
    switch (m.t) {
      case 'hi':
        if (m.v !== PONG_VERSION) {
          this.post({ t: 'bye' }, now)
          this.end(m.v < PONG_VERSION ? 'your friend needs to update Q Calc to play' : 'update Q Calc to play your friend')
          return
        }
        this.greeted = true
        if (!m.ok) this.hi(now)
        if (this.role === 'host' && !this.started) {
          this.started = true
          // the serve waits until the guest's court is up
          this.produce(this.serveLeg(1, 0, now, this.game.serveDir, [0, 0]), now)
        }
        return
      case 'p': {
        const t = this.shared(now)
        this.far.report(m.y, m.d, m.at, t, now)
        const leg = this.leg
        // still behind on a leg sent from here, well after it should have landed: send it again
        if (leg && this.legMine && newer(leg, { ...leg, game: m.game, n: m.n }) && m.at - leg.at > RESEND_MS && t - this.resentAt > RESEND_MS) {
          this.resentAt = t
          this.post({ t: 'l', ...leg }, now)
        }
        return
      }
      case 'l':
        this.take(m, now)
        return
      case 'again':
        // only an ask made at the end counts; one from mid game would start the next game early
        if (!this.over()) return
        this.again = { ...this.again, theirs: true }
        if (this.role === 'host' && this.again.mine) this.restart(now)
        return
      case 'other':
        this.post({ t: 'bye' }, now)
        this.end(otherGameNote(m.game, 'pong'))
        return
      case 'bye':
        this.end('your friend left')
    }
  }

  // a leg judged on this side: kept as the wire will carry it, so both sides fly the same ball
  private produce(leg: Leg, now: number): void {
    this.setLeg(wireLeg(leg))
    this.legMine = true
    this.post({ t: 'l', ...this.leg! }, now)
  }

  private setLeg(leg: Leg): void {
    this.leg = leg
    this.missed = false
    this.guess = null
    this.guessed = false
  }

  // a leg judged on the other side
  private take(m: Leg, now: number): void {
    if (!newer(m, this.leg)) return
    const t = this.shared(now)
    const before = this.shown(t, now)
    let leg: Leg = { game: m.game, n: m.n, at: m.at, kind: m.kind, ball: m.ball, score: m.score }
    // after a stall, take it up from LATE_MS in rather than from wherever it has got to
    if (leg.kind !== 'over' && towards(leg.ball) === this.role && t - leg.at > LATE_MS) leg = { ...leg, at: t - LATE_MS }
    const fresh = this.leg?.game !== leg.game
    this.setLeg(leg)
    this.legMine = false
    this.started = true
    // a new game clears the rematch asks
    if (fresh) this.again = { mine: false, theirs: false }
    this.ease = { x: 0, y: 0, at: now }
    const after = this.shown(t, now)
    if (fresh || before.phase !== 'play' || after.phase !== 'play') return
    const x = before.ball.x - after.ball.x
    const y = before.ball.y - after.ball.y
    if (Math.hypot(x, y) <= SNAP_PX) this.ease = { x, y, at: now }
  }

  // the ball as drawn at shared time t, our time now
  private shown(t: number, now: number): { ball: Ball; phase: Phase } {
    const leg = this.leg
    if (!leg || leg.kind === 'over') return { ball: centreBall(), phase: leg ? 'over' : 'serve' }
    if (leg.kind === 'serve' && t < leg.at) return { ball: centreBall(), phase: 'serve' }
    let ball: Ball
    if (this.guess) {
      // a guessed return stops at this side's face: only a real leg is judged here
      const stop = this.guess.at + secondsTo(this.guess.ball, this.role === 'host' ? HOST_FACE_X : GUEST_FACE_X) * 1000
      ball = fly(this.guess.ball, (Math.min(t, stop) - this.guess.at) / 1000)
    } else ball = fly(leg.ball, Math.max(0, t - leg.at) / 1000)
    const k = fade(now - this.ease.at)
    const half = BALL_SIZE / 2
    return { ball: { ...ball, x: ball.x + this.ease.x * k, y: Math.min(COURT_H - half, Math.max(half, ball.y + this.ease.y * k)) }, phase: 'play' }
  }

  // the ball heading here: hit or miss, against this paddle as it is now
  private judge(t: number, now: number): void {
    const leg = this.leg
    if (!leg || leg.kind === 'over' || towards(leg.ball) !== this.role || t < leg.at) return
    const host = this.role === 'host'
    const face = host ? HOST_FACE_X : GUEST_FACE_X
    const dir = host ? 1 : -1
    if (!this.missed) {
      const cross = leg.at + secondsTo(leg.ball, face) * 1000
      if (t < cross) return
      const b = fly(leg.ball, (cross - leg.at) / 1000)
      if (overlapsPaddle(b.y, this.mineY)) {
        this.produce({ ...leg, n: leg.n + 1, at: cross, kind: 'play', ball: bounceOff({ ...b, x: face }, this.mineY, dir) }, now)
        return
      }
      this.missed = true
    }
    const out = leg.at + secondsTo(leg.ball, host ? -BALL_SIZE / 2 : COURT_W + BALL_SIZE / 2) * 1000
    if (t < out) return
    const score: [number, number] = host ? [leg.score[0], leg.score[1] + 1] : [leg.score[0] + 1, leg.score[1]]
    if (winnerOf(score)) this.produce({ ...leg, n: leg.n + 1, at: out, kind: 'over', ball: centreBall(), score }, now)
    // served back toward this side, which lost the point
    else this.produce(this.serveLeg(leg.game, leg.n + 1, out, host ? -1 : 1, score), now)
  }

  // the ball heading away: once it reaches the far face, guess the far side's bounce from its paddle as drawn here
  private guessFar(t: number, now: number): void {
    const leg = this.leg
    if (!leg || leg.kind === 'over' || this.guessed || towards(leg.ball) === this.role || t < leg.at) return
    const host = this.role === 'host'
    const face = host ? GUEST_FACE_X : HOST_FACE_X
    const cross = leg.at + secondsTo(leg.ball, face) * 1000
    if (t < cross) return
    this.guessed = true
    const b = fly(leg.ball, (cross - leg.at) / 1000)
    const farY = this.far.shown(t, now)
    if (overlapsPaddle(b.y, farY)) this.guess = { ...leg, at: cross, ball: bounceOff({ ...b, x: face }, farY, host ? -1 : 1) }
  }

  tick(now: number): SessionView {
    const dt = Math.min(0.1, Math.max(0, now - this.lastTick) / 1000)
    this.lastTick = now
    if (this.ended) return this.view(null)
    if (!this.solo && now - this.heard > SILENCE_MS) {
      this.end('lost the connection')
      return this.view(null)
    }
    if (!this.greeted && now - this.lastHi >= HI_MS) {
      this.lastHi = now
      this.hi(now)
    }
    const d = ((this.held.down ? 1 : 0) - (this.held.up ? 1 : 0)) as Dir
    this.mineY = movePaddle(this.mineY, d, dt)
    if (this.solo) {
      ;({ y: this.theirY, mind: this.bot } = botStep(this.game, this.theirY, this.bot, this.solo, dt, this.rand))
      this.game = { ...this.game, paddles: [this.mineY, this.theirY] }
      ;({ state: this.game, carry: this.carry } = advance(this.game, this.carry, dt, this.rand))
      return this.view(snapshotOf(this.game, now))
    }
    if (this.role === 'guest') this.follow(dt * 1000)
    const t = this.shared(now)
    this.judge(t, now)
    this.guessFar(t, now)
    if (this.greeted) this.sendPaddle(d, t, now)
    const { ball, phase } = this.shown(t, now)
    const theirs = this.far.shown(t, now)
    const score = this.leg?.score ?? [0, 0]
    const winner = phase === 'over' ? winnerOf(score) : undefined
    return this.view({
      at: t,
      ball,
      paddles: this.role === 'host' ? [this.mineY, theirs] : [theirs, this.mineY],
      score,
      phase,
      ...(winner ? { winner } : {}),
    })
  }

  // this paddle, when its way changes or it has strayed from where the other side would carry it, and as a keepalive
  private sendPaddle(d: Dir, t: number, now: number): void {
    const since = now - this.sent.at
    const carried = clampPaddle(this.sent.y + (this.sent.d * PADDLE_SPEED * since) / 1000)
    const changed = d !== this.sent.d || Math.abs(carried - this.mineY) > 1
    if (!((changed && since >= PADDLE_MS - PADDLE_SLACK_MS) || since >= KEEP_MS)) return
    this.sent = { y: this.mineY, d, at: now }
    this.post({ t: 'p', at: t, y: this.mineY, d, game: this.leg?.game ?? 0, n: this.leg?.n ?? 0 }, now)
  }

  private view(snap: Snapshot | null): SessionView {
    return { snap, started: this.started, again: this.again, ended: this.ended }
  }
}
