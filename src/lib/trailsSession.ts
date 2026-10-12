import { matchWinner, newRound, QUEUE_MAX, scoreRound, steer, step, TICK_MS, type Dir, type Player, type Round } from './trails'
import {
  COUNT_MS,
  decodeTrails,
  encodeTrails,
  hashStart,
  hashStep,
  MAX_DELAY,
  MAX_FRAMES,
  POINT_MS,
  TRAILS_VERSION,
  type Frame,
  type NetPhase,
  type Snap,
  type TrailsMsg,
} from './trailsNet'

// one side of an online match, without the page: the arena feeds it keys, messages and frame times, and draws
// what tick() returns.
//
// lockstep: both sides run the same deterministic round and swap only their turns. a turn pressed now is
// stamped with a tick `delay` ticks ahead and sent at once, so it has usually arrived by the time either side
// gets there; every tick gets a frame, turn or not, so neither side ever waits on silence. a tick is only
// stepped once both frames for it are in. if one is late the round holds still ("waiting for friend…") and
// picks up when it lands; the side that was ahead ends up a little behind, which is how the two clocks settle
// into step without being synchronised.
//
// each message repeats the frames the other side hasn't acknowledged, so a lost or late one heals with the
// next. the guest sends a running hash of the round now and then; if the host's differs it sends its whole
// game (a snapshot) and the guest carries on from that. the same snapshot starts a match and a rematch

// hello goes out this often until the other side's comes in, since its arena may not have been listening yet
const HI_MS = 400
// with no frames going (the count, between rounds, after the match) each side says it's still there this often
export const KEEP_MS = 1000
// nothing from the other side for this long and the game is over
export const TRAILS_SILENCE_MS = 5000
// a stall shorter than this isn't worth a banner
export const STALL_SHOW_MS = 300
// held up this long, say so again: whatever is missing may have been lost on the way
const RESEND_MS = 250
// the host measures the link before the first round: this many pings, this far apart, for at most this long
const PINGS = 3
const PING_MS = 150
const PING_WAIT_MS = 900
// what a frame needs on top of the trip itself: both sides only look once a display frame
const MARGIN_MS = 30
// one-way, when nothing has been measured
const DEFAULT_LAG_MS = 60
// the guest's hash goes out every this many ticks
export const HASH_EVERY = 16
// the guest still talking in an old epoch this long (and a round trip and a half) after a snapshot never got it
const RESYNC_MS = 700

const COUNT_TOTAL_MS = COUNT_MS * 3
const opposite = (d: Dir) => ((d + 2) % 4) as Dir
const turns = (heading: Dir, to: Dir) => to !== heading && to !== opposite(heading)

export type TrailsPhase = 'wait' | NetPhase

export type TrailsView = {
  // what to draw; each cycle's queue holds the turn it's about to take, so heads glide the right way
  round: Round
  score: [number, number]
  // wait: the other side isn't there yet, or the host hasn't started the match
  phase: TrailsPhase
  count: number
  // who took the last round; null is a draw
  last: Player | null
  winner: Player | null
  // how far the heads are into the next cell, 0–1
  t: number
  // how long ago the round ended
  crashMs: number | null
  // the other side's turns are late and the round is holding still
  stalled: boolean
  again: { mine: boolean; theirs: boolean }
  // set once the game is over for good: the line the lobby shows
  ended: string | null
}

export class TrailsSession {
  readonly role: 'host' | 'guest'
  // the host is the left cycle
  readonly me: Player
  private readonly send: (text: string) => void

  private greeted = false
  private greetedAt = 0
  private heard: number
  private lastHi: number
  private lastSent: number
  private lastTick: number
  private ended: string | null = null

  // host: round trips measured, newest last
  private rtts: number[] = []
  private lastPing = -Infinity
  private snapAt = -Infinity
  // the round before this one: its hashes, for a guest check that comes in after it ended, and my frames,
  // in case the other side never got the last of them
  private prev: { r: number; hashes: number[]; mine: Frame[]; sealed: number } | null = null
  // host: a guest hash for a tick not reached yet
  private check: [tick: number, hash: number] | null = null

  private epoch = 0
  private r = -1
  private delay = 2
  private score: [number, number] = [0, 0]
  private round: Round = newRound()
  private phase: TrailsPhase = 'wait'
  // when point or over began
  private phaseAt = 0
  // when the count ends
  private runAt = 0
  private last: Player | null = null
  private acc = 0
  private again = { mine: false, theirs: false }

  // turns by tick: mine as sent, theirs as heard
  private mine: Frame[] = []
  private theirs: Array<Frame | undefined> = []
  // my frames are fixed up to this tick
  private sealed = -1
  // the first tick of theirs I don't hold
  private theirNext = 0
  // the last tick of mine they've said they hold
  private acked = -1
  // turns pressed and not yet given a tick
  private queue: Dir[] = []
  // the way I'll be going once every sealed turn is taken
  private heading: Dir = 1
  // the running hash after each tick
  private hashes: number[] = []
  private hashDue: [number, number] | null = null
  private dirty = false
  private stallAt: number | null = null

  constructor(role: 'host' | 'guest', send: (text: string) => void, now: number) {
    this.role = role
    this.me = role === 'host' ? 0 : 1
    this.send = send
    this.heard = now
    this.lastHi = now
    this.lastSent = now
    this.lastTick = now
    this.hi()
  }

  private post(m: TrailsMsg, now?: number): void {
    if (now != null) this.lastSent = now
    this.send(encodeTrails(m))
  }

  private hi(): void {
    this.post({ t: 'tr', v: TRAILS_VERSION, ...(this.greeted ? { ok: true } : {}) })
  }

  private end(note: string): void {
    if (this.ended == null) this.ended = note
  }

  // ---- the round

  private reset(r: number, round: Round, hash: number): void {
    if (this.r >= 0) this.prev = { r: this.r, hashes: this.hashes, mine: this.mine, sealed: this.sealed }
    this.r = r
    this.round = round
    this.mine = []
    this.theirs = []
    this.sealed = round.tick - 1
    this.theirNext = round.tick
    this.acked = round.tick - 1
    this.queue = []
    this.heading = round.cycles[this.me].dir
    this.hashes = []
    this.hashes[round.tick] = hash
    this.hashDue = this.role === 'guest' ? [round.tick, hash] : null
    this.check = null
    this.stallAt = null
    this.acc = 0
  }

  private nextRound(runAt: number): void {
    this.reset(this.r + 1, newRound(), hashStart(this.r + 1, this.score))
    this.phase = 'count'
    this.runAt = runAt
  }

  // fixes my frames up to tick `to`: each takes the oldest turn waiting, or none
  private sealTo(to: number): void {
    while (this.sealed < to) {
      const turn = this.queue.shift() ?? null
      this.mine[++this.sealed] = turn
      if (turn != null) this.heading = turn
      this.dirty = true
    }
  }

  private frame(p: Player, tick: number): Frame | undefined {
    return p === this.me ? this.mine[tick] : this.theirs[tick]
  }

  private stepOnce(now: number): void {
    let round = this.round
    for (const p of [0, 1] as const) {
      const turn = this.frame(p, round.tick)
      if (turn != null) round = steer(round, p, turn)
    }
    round = step(round)
    this.round = round
    const hash = hashStep(this.hashes[round.tick - 1]!, round)
    this.hashes[round.tick] = hash
    if (round.result) {
      this.score = scoreRound(this.score, round.result)
      this.last = round.result.winner
      this.phase = matchWinner(this.score) != null ? 'over' : 'point'
      this.phaseAt = now
      // the guest says how it ended, so a different ending is caught before the next round
      if (this.role === 'guest') {
        this.hashDue = [round.tick, hash]
        this.dirty = true
      }
    } else {
      this.sealTo(round.tick + this.delay - 1)
      if (this.role === 'guest' && round.tick % HASH_EVERY === 0) this.hashDue = [round.tick, hash]
    }
    if (this.check && (this.check[0] === round.tick || round.result)) {
      const [tick, theirs] = this.check
      this.check = null
      if (tick !== round.tick || theirs !== hash) this.snapshot(now)
    }
  }

  // ---- the host's side of keeping the two in step

  private lag(): number {
    if (!this.rtts.length) return DEFAULT_LAG_MS
    const sorted = [...this.rtts].sort((a, b) => a - b)
    return sorted[Math.floor((sorted.length - 1) / 2)]! / 2
  }

  private ping(now: number): void {
    this.lastPing = now
    this.post({ t: 'trp', c: Math.max(0, Math.round(now)) }, now)
  }

  // a new match: the delay is set from the ping, and the guest starts from the snapshot
  private start(now: number): void {
    this.delay = Math.min(MAX_DELAY, Math.max(1, Math.ceil((this.lag() + MARGIN_MS) / TICK_MS)))
    this.score = [0, 0]
    this.last = null
    this.again = { mine: false, theirs: false }
    this.nextRound(now + COUNT_TOTAL_MS)
    this.snapshot(now)
  }

  // the whole game as the host has it. whatever the guest sent before it now counts for nothing
  private snapshot(now: number): void {
    this.epoch++
    this.snapAt = now
    this.check = null
    // only frames with none missing before them go; the guest makes the rest again
    this.theirs.length = this.theirNext
    const lag = this.lag()
    const tick = this.round.tick
    const running = this.phase === 'run' || this.phase === 'count'
    let wait = 0
    if (this.phase === 'count') wait = this.runAt - now - lag
    else if (this.phase === 'point') wait = this.phaseAt + POINT_MS - now - lag
    // more frames than one message holds go again later as ordinary ones
    const mine = running ? this.mine.slice(tick, this.sealed + 1).slice(0, MAX_FRAMES) : []
    const theirs = running ? (this.theirs.slice(tick, this.theirNext) as Frame[]).slice(0, MAX_FRAMES) : []
    this.theirNext = tick + theirs.length
    this.theirs.length = this.theirNext
    const snap: Snap = {
      e: this.epoch,
      r: this.r,
      d: this.delay,
      score: this.score,
      phase: this.phase === 'wait' ? 'count' : this.phase,
      wait: Math.max(0, wait),
      round: this.round,
      frames: [mine, theirs],
      hash: this.hashes[tick]!,
    }
    this.post({ t: 'trs', ...snap }, now)
  }

  // the guest's hash for a tick, against mine
  private verify(r: number, [tick, hash]: [number, number], now: number): void {
    let same: boolean
    if (r === this.r) {
      if (tick > this.round.tick && !this.round.result) {
        this.check = [tick, hash]
        return
      }
      same = this.hashes[tick] === hash
    } else if (this.prev && r === this.prev.r) same = this.prev.hashes[tick] === hash
    // the guest is a moment ahead, into the next round
    else if (r === this.r + 1 && this.phase === 'point') return
    else same = false
    if (!same) this.snapshot(now)
  }

  // the guest is still in an old epoch: once the snapshot has had time to land, it didn't
  private stale(e: number, now: number): void {
    if (this.phase !== 'wait' && e < this.epoch && now - this.snapAt > RESYNC_MS + this.lag() * 3) this.snapshot(now)
  }

  // ---- the guest's side

  private adopt(m: Snap, now: number): void {
    const tick = m.round.tick
    this.epoch = m.e
    this.delay = m.d
    this.score = [m.score[0], m.score[1]]
    this.reset(m.r, m.round, m.hash)
    const [theirs, mine] = m.frames
    theirs.forEach((f, j) => (this.theirs[tick + j] = f))
    this.theirNext = tick + theirs.length
    mine.forEach((f, j) => {
      this.mine[tick + j] = f
      if (f != null && turns(this.heading, f)) this.heading = f
    })
    this.sealed = tick + mine.length - 1
    this.acked = this.sealed
    this.phase = m.phase
    this.last = m.round.result?.winner ?? null
    if (m.phase === 'count') this.runAt = now + m.wait
    else if (m.phase === 'point') this.phaseAt = now + m.wait - POINT_MS
    else if (m.phase === 'over') this.phaseAt = now
    else this.sealTo(tick + this.delay - 1)
    if (m.phase !== 'over') this.again = { mine: false, theirs: false }
    this.lastTick = now
    this.dirty = true
  }

  // ---- what the arena calls

  // a turn pressed: it takes the next free tick. the same way or straight back does nothing, as offline
  turn(dir: Dir): void {
    if (this.ended != null || (this.phase !== 'count' && this.phase !== 'run') || this.queue.length >= QUEUE_MAX) return
    if (turns(this.queue[this.queue.length - 1] ?? this.heading, dir)) this.queue.push(dir)
  }

  // enter: asks for a rematch once the match is over; the host starts it when both have asked
  rematch(now: number): void {
    if (this.ended != null || this.phase !== 'over' || this.again.mine) return
    if (this.role === 'host' && this.again.theirs) {
      this.start(now)
      return
    }
    this.again = { ...this.again, mine: true }
    this.post({ t: 'tra', e: this.epoch }, now)
  }

  // back to the menu: the other side is told rather than left to time out
  leave(): void {
    if (this.ended != null) return
    this.post({ t: 'bye' })
    this.end('')
  }

  receive(text: string, now: number): void {
    if (this.ended != null) return
    const m = decodeTrails(text)
    if (!m) return
    this.heard = now
    switch (m.t) {
      case 'tr':
        if (m.v !== TRAILS_VERSION) {
          this.post({ t: 'bye' })
          this.end('update both Q Calcs to play each other')
          return
        }
        if (!this.greeted) {
          this.greeted = true
          this.greetedAt = now
        }
        if (!m.ok) this.hi()
        return
      case 'other':
        this.post({ t: 'bye' })
        this.end(`your friend opened ${m.game} · both type trails`)
        return
      case 'trp':
        this.post({ t: 'trq', c: m.c }, now)
        return
      case 'trq': {
        const rtt = now - m.c
        if (this.role !== 'host' || rtt < 0 || rtt > TRAILS_SILENCE_MS) return
        this.rtts.push(rtt)
        if (this.rtts.length > 5) this.rtts.shift()
        return
      }
      case 'trk':
        if (this.role === 'host') this.stale(m.e, now)
        return
      case 'trf': {
        if (m.e !== this.epoch || this.phase === 'wait') {
          if (this.role === 'host') this.stale(m.e, now)
          return
        }
        if (m.r === this.r) {
          this.acked = Math.max(this.acked, Math.min(m.a, this.sealed))
          // first heard is kept: a frame never changes once sent. nothing from far beyond where the round is
          const limit = this.round.tick + MAX_FRAMES
          m.i.forEach((f, j) => {
            const tick = m.f + j
            if (tick >= this.round.tick && tick < limit && this.theirs[tick] === undefined) this.theirs[tick] = f
          })
          while (this.theirs[this.theirNext] !== undefined) this.theirNext++
          // my round is over and theirs is stuck short of it: the last frames go again
          if (this.round.result && m.a < this.sealed) this.dirty = true
        } else if (this.prev && m.r === this.prev.r && m.a < this.prev.sealed) {
          const from = Math.max(m.a + 1, this.prev.sealed - MAX_FRAMES + 1, 0)
          this.post({ t: 'trf', e: this.epoch, r: m.r, f: from, i: this.prev.mine.slice(from, this.prev.sealed + 1), a: -1 }, now)
        }
        if (m.h && this.role === 'host') this.verify(m.r, m.h, now)
        return
      }
      case 'trs':
        if (this.role === 'guest' && this.greeted && m.e > this.epoch) this.adopt(m, now)
        return
      case 'tra':
        // only an ask made at the end of this match counts
        if (m.e !== this.epoch || this.phase !== 'over') return
        if (this.role === 'host' && this.again.mine) this.start(now)
        else this.again = { ...this.again, theirs: true }
        return
      case 'bye':
        this.end('your friend left')
    }
  }

  tick(now: number): TrailsView {
    // a hidden window's clock crawls: carry on from where it is rather than race to catch up
    const dt = Math.min(TICK_MS * 3, Math.max(0, now - this.lastTick))
    this.lastTick = now
    if (this.ended != null) return this.view(now)
    if (now - this.heard > TRAILS_SILENCE_MS) {
      this.end('lost the connection')
      return this.view(now)
    }
    if (!this.greeted) {
      if (now - this.lastHi >= HI_MS) {
        this.lastHi = now
        this.hi()
      }
      return this.view(now)
    }
    if (this.phase === 'wait' && this.role === 'host') {
      if (this.rtts.length >= PINGS || (this.rtts.length > 0 && now - this.greetedAt >= PING_WAIT_MS)) this.start(now)
      else if (now - this.lastPing >= PING_MS) this.ping(now)
    }
    if (this.phase === 'point' && now - this.phaseAt >= POINT_MS) this.nextRound(this.phaseAt + POINT_MS + COUNT_TOTAL_MS)
    if (this.phase === 'count') {
      // the first frames are fixed before the count ends, each as far ahead of its tick as any later one
      this.sealTo(Math.min(this.delay - 1, Math.floor((now - this.runAt) / TICK_MS) + this.delay - 1))
      if (now >= this.runAt) {
        this.phase = 'run'
        this.acc = Math.min(TICK_MS, now - this.runAt)
      }
    } else if (this.phase === 'run') {
      this.acc += dt
      while (this.acc >= TICK_MS && this.phase === 'run') {
        if (this.theirs[this.round.tick] === undefined) {
          // held at the brink, so it steps the moment the frame lands
          this.acc = TICK_MS
          this.stallAt ??= now
          break
        }
        this.stallAt = null
        this.acc -= TICK_MS
        this.stepOnce(now)
      }
    }
    if (this.phase !== 'run') this.stallAt = null
    else if (this.stallAt != null && now - this.lastSent >= RESEND_MS) {
      this.dirty = true
      // where the guest is stuck, in case it's stuck because the two disagree
      if (this.role === 'guest') this.hashDue = [this.round.tick, this.hashes[this.round.tick]!]
    }
    if (this.dirty) this.sendFrames(now)
    else if (now - this.lastSent >= KEEP_MS) {
      if (this.role === 'host') this.ping(now)
      else this.post({ t: 'trk', e: this.epoch }, now)
    }
    return this.view(now)
  }

  // my frames they haven't acknowledged, what I hold of theirs, and a hash if one is due
  private sendFrames(now: number): void {
    this.dirty = false
    if (this.phase === 'wait') return
    const from = Math.max(this.acked + 1, this.sealed - MAX_FRAMES + 1, 0)
    const h = this.hashDue
    this.hashDue = null
    this.post({ t: 'trf', e: this.epoch, r: this.r, f: from, i: this.mine.slice(from, this.sealed + 1), a: this.theirNext - 1, ...(h ? { h } : {}) }, now)
  }

  private view(now: number): TrailsView {
    let round = this.round
    if (this.phase === 'run' || this.phase === 'count') {
      for (const p of [0, 1] as const) {
        const turn = this.frame(p, round.tick)
        if (turn != null) round = steer(round, p, turn)
      }
    }
    const over = this.phase === 'point' || this.phase === 'over'
    return {
      round,
      score: this.score,
      phase: this.phase,
      count: this.phase === 'count' ? Math.min(3, Math.max(1, Math.ceil((this.runAt - now) / COUNT_MS))) : 0,
      last: this.last,
      winner: matchWinner(this.score),
      t: this.phase === 'run' ? Math.min(1, this.acc / TICK_MS) : 0,
      crashMs: over ? now - this.phaseAt : null,
      stalled: this.stallAt != null && now - this.stallAt >= STALL_SHOW_MS,
      again: this.again,
      ended: this.ended,
    }
  }

  // for tests: where the game stands
  state(): { r: number; tick: number; hash: number; score: [number, number]; epoch: number; delay: number; phase: TrailsPhase } {
    return {
      r: this.r,
      tick: this.round.tick,
      hash: this.hashes[this.round.tick] ?? 0,
      score: [this.score[0], this.score[1]],
      epoch: this.epoch,
      delay: this.delay,
      phase: this.phase,
    }
  }
}
