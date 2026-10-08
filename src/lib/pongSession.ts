import {
  advance,
  botStep,
  COURT_H,
  decodePong,
  encodePong,
  movePaddle,
  newBotMind,
  newGame,
  PONG_VERSION,
  SILENCE_MS,
  SnapshotBuffer,
  snapshotOf,
  STATE_HZ,
  type PongMsg,
  type BotMind,
  type PongState,
  type Side,
  type Skill,
  type Snapshot,
} from './pong'

// hi goes out this often until the other side's hi comes in, since its court may not have been listening yet;
// a hi from a side that hasn't heard ours gets one back, so whichever court came up first still hears the other
const HI_MS = 400
// the guest's paddle goes out when it moves, at most this often, and at least every KEEP_MS
const PADDLE_MS = 1000 / 60
// frame times land a hair either side of a 60hz frame; without this the paddle would go every other frame
const PADDLE_SLACK_MS = 1
const STATE_MS = 1000 / STATE_HZ
const KEEP_MS = 250

export type SessionView = {
  // what to draw: the host's own state, or the guest's interpolated copy with its own paddle on top
  snap: Snapshot | null
  started: boolean
  again: { mine: boolean; theirs: boolean }
  // set once the game is over for good: the line the lobby shows
  ended: string | null
}

// one side of a game, without the page: the court component feeds it keys, messages and frame times,
// and draws what tick() returns
export class PongSession {
  private game: PongState
  private carry = 0
  private held = { up: false, down: false }
  private mineY = COURT_H / 2
  private theirY = COURT_H / 2
  // the other side's hi came in
  private greeted = false
  // host: the guest said hi; guest: the host's state is arriving
  private started = false
  private heard: number
  private lastTick: number
  private lastSent = -Infinity
  // host: when the next state is due. due times step by STATE_MS, so 60 and 120hz frames both send STATE_HZ
  private nextState = -Infinity
  private lastHi: number
  private sentY = -1
  private snaps = new SnapshotBuffer()
  private again = { mine: false, theirs: false }
  private ended: string | null = null

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
    } else this.hi()
  }

  private hi(): void {
    this.post({ t: 'hi', v: PONG_VERSION, ...(this.greeted ? { ok: true } : {}) })
  }

  private post(m: PongMsg): void {
    this.send(encodePong(m))
  }

  private end(note: string): void {
    if (!this.ended) this.ended = note
  }

  private over(): boolean {
    return this.role === 'host' ? this.game.phase === 'over' : this.snaps.latest()?.phase === 'over'
  }

  private restart(): void {
    this.game = newGame(this.rand() < 0.5 ? 1 : -1)
    this.carry = 0
    this.bot = newBotMind()
    this.again = { mine: false, theirs: false }
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
      this.restart()
      return
    }
    this.again = { ...this.again, mine: true }
    this.post({ t: 'again' })
    if (this.role === 'host' && this.again.theirs) this.restart()
  }

  receive(text: string, now: number): void {
    if (this.ended) return
    const m = decodePong(text)
    if (!m) return
    this.heard = now
    switch (m.t) {
      case 'hi':
        if (m.v !== PONG_VERSION) {
          this.post({ t: 'bye' })
          this.end('update both Q Calcs to play each other')
          return
        }
        this.greeted = true
        if (this.role === 'host') this.started = true
        if (!m.ok) this.hi()
        return
      case 'p':
        if (this.role === 'host') this.theirY = m.y
        return
      case 's':
        if (this.role !== 'guest') return
        this.started = true
        this.snaps.push(m, now)
        // the host restarted: a new game clears the rematch asks
        if (m.phase !== 'over') this.again = { mine: false, theirs: false }
        return
      case 'again':
        // only an ask made at the end counts; one from mid game would start the next game early
        if (!this.over()) return
        this.again = { ...this.again, theirs: true }
        if (this.role === 'host' && this.again.mine && this.game.phase === 'over') this.restart()
        return
      case 'bye':
        this.end('your friend left')
    }
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
      this.hi()
    }
    this.mineY = movePaddle(this.mineY, (this.held.down ? 1 : 0) - (this.held.up ? 1 : 0), dt)
    if (this.solo) ({ y: this.theirY, mind: this.bot } = botStep(this.game, this.theirY, this.bot, this.solo, dt, this.rand))
    if (this.role === 'host') {
      this.game = { ...this.game, paddles: [this.mineY, this.theirY] }
      // the serve waits until the guest's court is up
      if (this.started) ({ state: this.game, carry: this.carry } = advance(this.game, this.carry, dt, this.rand))
      if (!this.solo && now >= this.nextState) {
        // a frame that came very late doesn't make the next few go early
        this.nextState = Math.max(this.nextState, now - STATE_MS / 2) + STATE_MS
        this.post({ t: 's', ...snapshotOf(this.game, now) })
      }
      return this.view(snapshotOf(this.game, now))
    }
    const moved = this.mineY !== this.sentY
    if ((moved && now - this.lastSent >= PADDLE_MS - PADDLE_SLACK_MS) || now - this.lastSent >= KEEP_MS) {
      this.lastSent = now
      this.sentY = this.mineY
      this.post({ t: 'p', y: Math.round(this.mineY * 10) / 10 })
    }
    const seen = this.snaps.sample(now)
    return this.view(seen ? { ...seen, paddles: [seen.paddles[0], this.mineY] } : null)
  }

  private view(snap: Snapshot | null): SessionView {
    return { snap, started: this.started, again: this.again, ended: this.ended }
  }
}
