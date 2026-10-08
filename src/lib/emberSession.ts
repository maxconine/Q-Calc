import { compileLevel, initState, step } from './emberEngine'
import { LEVELS } from './emberLevels'
import { decodeEmber, encodeEmber, eventFits, fitsLevel, SILENCE_MS, STATE_HZ, type EmberMsg, type EmberNote, type KeyChange } from './emberProtocol'
import { EMBER_VERSION, STEP_S, type GameEvent, type GameState, type Inputs, type Level, type LevelDef, type PlayerIndex } from './emberTypes'

// drives ember & frost: which screen, which level, the fixed-step clock, and (online) the link.
// the panel feeds it keys and frame times and draws what tick() returns. no DOM here
//
// online, the host (ember) is authoritative: it steps the engine with its own keys and the guest's, and streams
// state at STATE_HZ. the guest (frost) runs the same engine ahead of the host's clock, so frost answers its keys
// at once: on each host state it rewinds to it and replays its own recorded keys up to its current frame.
// the guest stamps its keys with the frame they belong to and the host applies them at that frame, so a guest
// running far enough ahead predicts its own character exactly; the host reports how early the keys arrive and
// the guest moves its clock to keep them early

export type EmberScreen = 'wait' | 'select' | 'play'

export type EmberView = {
  mode: 'local' | 'online'
  screen: EmberScreen
  level: number // index into LEVELS: the one being played, or the one highlighted on select
  compiled: Level | null // compiled LEVELS[level] while playing
  state: GameState | null // latest state to draw (online guest: predicted, corrections eased in)
  prev: GameState | null // the state one step before `state`, for interpolation
  alpha: number // 0..1, how far between prev and state this frame sits
  events: GameEvent[] // everything that happened since the last tick() call
  me: PlayerIndex | null // online: the character this side plays; local: null (both)
  unlocked: number // levels 0..unlocked-1 can be picked
  // online: the line the lobby shows once the link is over
  ended: string | null
  // online: something the other side did or asked for, shown small (e.g. 'frost restarted')
  note: string | null
}

export interface EmberGame {
  tick(now: number): EmberView
  // held keys. local: per player; online: the player argument is ignored and this side's character is used
  keys(player: PlayerIndex, bits: number): void
  // drop all held keys (window blur)
  release(): void
  // select screen: move the highlight / start the highlighted level
  pick(index: number): void
  play(): void
  // in play: restart the attempt; after a win: the next level; back to select
  retry(): void
  next(): void
  menu(): void
  // online only: a message from the link
  receive(text: string, now: number): void
  // tell the other side we're leaving (online), stop
  leave(): void
}

// a frame after a long gap (a background tab) only catches up this much, so it never spirals
const MAX_GAP_S = 0.25
// frame times land a hair either side of a step; without this 60hz frames would step 0, 2, 0, 2...
const STEP_EPS = 1e-6
// hi goes out this often until the other side's hi comes in
const HI_MS = 400
// the guest's keys go out when they change, and at least this often so the host never thinks it's gone
const KEEP_MS = 250
// the host's select screen goes out when it changes, and at least this often
const SEL_MS = 300
const STATE_MS = 1000 / STATE_HZ
const NOTE_MS = 3000
// the guest's highlight move waits this long for the host to agree before an older select can undo it
const PICK_MS = 1000
// guest prediction: how far ahead of the newest host state the guest may run (also caps a replay)
const MAX_AHEAD = 40
// how far ahead the guest starts before the host has said how early its keys arrive
const START_LEAD = 6
// keys should arrive at least this many frames before the host needs them
const MIN_SLACK = 1
const AIM_SLACK = 3
// keys that keep arriving this early mean the guest can run a frame closer to the host
const LOOSE_SLACK = 6
const SLACK_WINDOW = 8
// a correction bigger than this jumps (a portal, a respawn); smaller ones ease out by SMOOTH per step
const SNAP_PX = 40
const SMOOTH = 0.8
// the guest plays these for its own character as it predicts them, and skips the host's copies
const OWN_KINDS = new Set<GameEvent['k']>(['jump', 'land', 'gem', 'portal', 'push'])
const NAMES = ['ember', 'frost'] as const
const NOTE_TEXT: Record<EmberNote, string> = {
  play: 'started the level',
  retry: 'restarted',
  next: 'went on to the next level',
  menu: 'went back to the levels',
}

const levelCount = () => Math.max(1, LEVELS.length)
const clampUnlocked = (u: number) => Math.min(levelCount(), Math.max(1, Math.floor(u) || 1))

const cache = new WeakMap<LevelDef, Level>()
// null when the level doesn't exist or doesn't compile; the select screen just won't start it
function compiledAt(i: number): Level | null {
  const def = LEVELS[i]
  if (!def) return null
  let level = cache.get(def)
  if (!level) {
    try {
      level = compileLevel(def)
    } catch {
      return null
    }
    cache.set(def, level)
  }
  return level
}

// whole fixed steps out of real time; what's left over is how far into the next step this frame sits
class Clock {
  private acc = 0
  private last: number

  constructor(now: number) {
    this.last = now
  }

  steps(now: number): number {
    this.acc += Math.min(MAX_GAP_S, Math.max(0, now - this.last) / 1000)
    this.last = now
    let n = 0
    while (this.acc >= STEP_S - STEP_EPS) {
      this.acc -= STEP_S
      n++
    }
    this.acc = Math.max(0, this.acc)
    return n
  }

  alpha(): number {
    return Math.min(1, this.acc / STEP_S)
  }

  reset(): void {
    this.acc = 0
  }
}

// ---- local: both characters on one keyboard

class LocalGame implements EmberGame {
  private screen: 'select' | 'play' = 'select'
  private level: number
  private unlocked: number
  private compiled: Level | null = null
  private state: GameState | null = null
  private prev: GameState | null = null
  private held: Inputs = [0, 0]
  private clock: Clock

  constructor(now: number, unlocked: number) {
    this.unlocked = clampUnlocked(unlocked)
    this.level = this.unlocked - 1
    this.clock = new Clock(now)
  }

  tick(now: number): EmberView {
    const n = this.clock.steps(now)
    const events: GameEvent[] = []
    if (this.screen === 'play' && this.compiled && this.state) {
      for (let i = 0; i < n; i++) {
        this.prev = this.state
        this.state = step(this.compiled, this.state, [this.held[0], this.held[1]])
        events.push(...this.state.events)
        if (this.state.phase === 'won' && this.prev.phase !== 'won') this.unlocked = clampUnlocked(Math.max(this.unlocked, this.level + 2))
      }
    }
    return {
      mode: 'local',
      screen: this.screen,
      level: this.level,
      compiled: this.screen === 'play' ? this.compiled : null,
      state: this.screen === 'play' ? this.state : null,
      prev: this.screen === 'play' ? this.prev : null,
      alpha: this.clock.alpha(),
      events,
      me: null,
      unlocked: this.unlocked,
      ended: null,
      note: null,
    }
  }

  keys(player: PlayerIndex, bits: number): void {
    this.held[player] = bits & 15
  }

  release(): void {
    this.held = [0, 0]
  }

  pick(index: number): void {
    if (this.screen === 'select') this.level = Math.min(this.unlocked - 1, Math.max(0, Math.floor(index) || 0))
  }

  play(): void {
    if (this.screen === 'select') this.start(this.level, 0)
  }

  private start(i: number, deaths: number): void {
    const level = compiledAt(i)
    if (!level) return
    this.level = i
    this.compiled = level
    this.state = initState(level, deaths)
    this.prev = this.state
    this.screen = 'play'
    this.clock.reset()
  }

  // a restart mid level keeps the death count; one after a win is a fresh go
  retry(): void {
    if (this.screen === 'play' && this.state) this.start(this.level, this.state.phase === 'won' ? 0 : this.state.deaths)
  }

  next(): void {
    if (this.screen !== 'play' || this.state?.phase !== 'won') return
    if (this.level + 1 < LEVELS.length) this.start(this.level + 1, 0)
    else this.menu()
  }

  menu(): void {
    this.screen = 'select'
    this.state = this.prev = this.compiled = null
  }

  receive(): void {}

  leave(): void {
    this.menu()
  }
}

// ---- online: the parts both sides share (handshake, silence, notes)

abstract class OnlineGame implements EmberGame {
  protected screen: EmberScreen = 'wait'
  protected level: number
  protected unlocked: number
  protected compiled: Level | null = null
  protected held = 0
  protected greeted = false
  protected now: number
  protected clock: Clock
  // events for the next view
  protected out: GameEvent[] = []
  private heard: number
  private lastHi: number
  private ended: string | null = null
  private note: string | null = null
  private noteAt = 0
  private readonly send: (text: string) => void
  abstract readonly me: PlayerIndex

  constructor(send: (text: string) => void, now: number, unlocked: number) {
    this.send = send
    this.now = this.heard = this.lastHi = now
    this.unlocked = clampUnlocked(unlocked)
    this.level = this.unlocked - 1
    this.clock = new Clock(now)
    this.hi()
  }

  protected post(m: EmberMsg): void {
    this.send(encodeEmber(m))
  }

  private hi(): void {
    this.post({ t: 'hi', v: EMBER_VERSION, u: this.unlocked, ...(this.greeted ? { ok: true } : {}) })
  }

  private end(line: string): void {
    if (!this.ended) this.ended = line
  }

  protected say(who: PlayerIndex, what: EmberNote | string): void {
    this.note = `${NAMES[who]} ${what in NOTE_TEXT ? NOTE_TEXT[what as EmberNote] : what}`
    this.noteAt = this.now
  }

  protected clampPick(i: number): number {
    return Math.min(this.unlocked - 1, Math.max(0, Math.floor(i) || 0))
  }

  protected unlock(upTo: number): void {
    this.unlocked = clampUnlocked(Math.max(this.unlocked, upTo))
  }

  protected abstract handle(m: EmberMsg, now: number): void
  protected abstract advance(steps: number, now: number): void
  protected abstract drawn(): { state: GameState | null; prev: GameState | null }

  receive(text: string, now: number): void {
    if (this.ended) return
    const m = decodeEmber(text)
    if (!m) return
    this.heard = now
    this.now = Math.max(this.now, now)
    if (m.t === 'hi') {
      if (m.v !== EMBER_VERSION) {
        this.post({ t: 'bye' })
        this.end('update both Q Calcs to play each other')
        return
      }
      const first = !this.greeted
      this.greeted = true
      this.unlock(m.u)
      if (first && this.screen === 'wait') this.screen = 'select'
      if (!m.ok) this.hi()
      return
    }
    if (m.t === 'bye') {
      this.end('your friend left')
      return
    }
    // nothing but hi counts until the versions have matched: another version's messages may mean other things
    if (this.greeted) this.handle(m, now)
  }

  tick(now: number): EmberView {
    this.now = now
    const n = this.clock.steps(now)
    if (!this.ended) {
      if (now - this.heard > SILENCE_MS) this.end('lost the connection')
      else {
        if (!this.greeted && now - this.lastHi >= HI_MS) {
          this.lastHi = now
          this.hi()
        }
        this.advance(n, now)
      }
    }
    if (this.note && now - this.noteAt > NOTE_MS) this.note = null
    const events = this.out
    this.out = []
    const playing = this.screen === 'play'
    const { state, prev } = playing ? this.drawn() : { state: null, prev: null }
    return {
      mode: 'online',
      screen: this.screen,
      level: this.level,
      compiled: playing ? this.compiled : null,
      state,
      prev,
      alpha: this.clock.alpha(),
      events,
      me: this.me,
      unlocked: this.unlocked,
      ended: this.ended,
      note: this.note,
    }
  }

  keys(_player: PlayerIndex, bits: number): void {
    this.held = bits & 15
  }

  release(): void {
    this.held = 0
  }

  leave(): void {
    if (this.ended) return
    this.post({ t: 'bye' })
    this.end('you left')
  }

  abstract pick(index: number): void
  abstract play(): void
  abstract retry(): void
  abstract next(): void
  abstract menu(): void
}

// ---- the host: ember, and the one true game

class HostGame extends OnlineGame {
  readonly me = 0
  private state: GameState | null = null
  private prev: GameState | null = null
  // steps since this attempt began; the guest's keys are stamped in these
  private frame = 0
  // bumps on every start and restart, so keys and states from an older attempt are told apart
  private attempt = 0
  private guestBits = 0
  // guest key changes not yet reached, by frame
  private queue: KeyChange[] = []
  private queuedTo = -1
  private slack: [number, number] | undefined
  // events since the last state message
  private pending: GameEvent[] = []
  // due times step by STATE_MS, so 60 and 120hz frames both send STATE_HZ
  private nextState = -Infinity
  private nextSel = -Infinity

  pick(index: number): void {
    if (this.screen !== 'select') return
    this.level = this.clampPick(index)
    this.nextSel = -Infinity
  }

  play(): void {
    if (this.screen !== 'select' || !this.start(this.level, 0)) return
    this.post({ t: 'note', w: 'play' })
  }

  retry(): void {
    if (!this.restart()) return
    this.post({ t: 'note', w: 'retry' })
  }

  next(): void {
    if (!this.forward()) return
    this.post({ t: 'note', w: 'next' })
  }

  menu(): void {
    if (!this.toSelect()) return
    this.post({ t: 'note', w: 'menu' })
  }

  private start(i: number, deaths: number): boolean {
    const level = compiledAt(i)
    if (!level) return false
    this.level = i
    this.compiled = level
    this.state = this.prev = initState(level, deaths)
    this.screen = 'play'
    this.frame = 0
    this.attempt++
    this.guestBits = 0
    this.queue = []
    this.queuedTo = -1
    this.slack = undefined
    this.pending = []
    this.nextState = -Infinity
    this.clock.reset()
    return true
  }

  private restart(): boolean {
    if (this.screen !== 'play' || !this.state) return false
    return this.start(this.level, this.state.phase === 'won' ? 0 : this.state.deaths)
  }

  private forward(): boolean {
    if (this.screen !== 'play' || this.state?.phase !== 'won') return false
    if (this.level + 1 < LEVELS.length) return this.start(this.level + 1, 0)
    return this.toSelect()
  }

  private toSelect(): boolean {
    if (this.screen !== 'play') return false
    this.screen = 'select'
    this.state = this.prev = this.compiled = null
    this.nextSel = -Infinity
    return true
  }

  protected handle(m: EmberMsg): void {
    switch (m.t) {
      case 'k': {
        if (this.screen !== 'play' || m.a !== this.attempt) return
        for (const c of m.i) {
          if (c[0] <= this.queuedTo) continue
          this.queue.push(c)
          this.queuedTo = c[0]
        }
        // how many frames before the host needs them the keys turned up; the guest steers its clock by it
        const slack = m.f - this.frame
        if (!this.slack || slack < this.slack[0]) this.slack = [slack, m.f]
        return
      }
      case 'pick':
        if (this.screen === 'select') this.pick(m.l)
        return
      case 'go':
        if (this.screen !== 'select') return
        if (this.start(this.clampPick(m.l), 0)) this.say(1, 'play')
        return
      case 'retry':
        if (this.restart()) this.say(1, 'retry')
        return
      case 'next':
        if (this.forward()) this.say(1, 'next')
        return
      case 'menu':
        if (this.toSelect()) this.say(1, 'menu')
    }
  }

  protected advance(steps: number, now: number): void {
    if (this.screen === 'play' && this.compiled && this.state) {
      for (let i = 0; i < steps; i++) {
        while (this.queue.length && this.queue[0]![0] <= this.frame) this.guestBits = this.queue.shift()![1]
        this.prev = this.state
        this.state = step(this.compiled, this.state, [this.held, this.guestBits])
        this.frame++
        this.out.push(...this.state.events)
        // the guest makes its own win from the phase, so a lost message can't lose it
        for (const e of this.state.events) if (e.k !== 'win') this.pending.push(e)
        if (this.state.phase === 'won' && this.prev.phase !== 'won') this.unlock(this.level + 2)
      }
      if (now >= this.nextState) {
        // a frame that came very late doesn't make the next few go early
        this.nextState = Math.max(this.nextState, now - STATE_MS / 2) + STATE_MS
        this.post({
          t: 's',
          l: this.level,
          a: this.attempt,
          f: this.frame,
          fi: this.held,
          ...(this.slack ? { sl: this.slack } : {}),
          g: this.state,
          e: this.pending,
        })
        this.pending = []
        this.slack = undefined
      }
    } else if (this.screen === 'select' && now >= this.nextSel) {
      this.nextSel = now + SEL_MS
      this.post({ t: 'sel', l: this.level, u: this.unlocked })
    }
  }

  protected drawn() {
    return { state: this.state, prev: this.prev }
  }
}

// ---- the guest: frost, predicted

type Offset = { x: number; y: number }

class GuestGame extends OnlineGame {
  readonly me = 1
  private attempt = -1
  // the newest host state and the frame it's at
  private auth: GameState | null = null
  private authF = 0
  private fireBits = 0
  private dirty = false
  // the predicted state at frame `frame`, and the one before it
  private cur: GameState | null = null
  private prev: GameState | null = null
  private frame = 0
  // a predicted step changed the phase: wait for the host to say so rather than show a death or win it may not confirm
  private holding = false
  // this side's key changes, [frame, bits], oldest first; kept back to the newest host state
  private log: KeyChange[] = []
  private recent: KeyChange[] = []
  private logged = -1
  private unsent = false
  private lastSent = -Infinity
  // where the next attempt starts relative to the host's frame
  private lead = START_LEAD
  // slack reports about guest frames before this were measured before the clock last moved
  private adjustedAt = 0
  private slacks: number[] = []
  private skip = 0
  // the drawn players sit this far from the predicted ones, easing to 0 after a correction
  private offsets: [Offset, Offset] = [
    { x: 0, y: 0 },
    { x: 0, y: 0 },
  ]
  private winSeen = false
  private picked: { l: number; until: number } | null = null

  pick(index: number): void {
    if (this.screen !== 'select') return
    this.level = this.clampPick(index)
    this.picked = { l: this.level, until: this.now + PICK_MS }
    this.post({ t: 'pick', l: this.level })
  }

  play(): void {
    if (this.screen === 'select') this.post({ t: 'go', l: this.level })
  }

  retry(): void {
    if (this.screen === 'play') this.post({ t: 'retry' })
  }

  next(): void {
    if (this.screen === 'play' && this.auth?.phase === 'won') this.post({ t: 'next' })
  }

  menu(): void {
    if (this.screen === 'play') this.post({ t: 'menu' })
  }

  protected handle(m: EmberMsg, now: number): void {
    switch (m.t) {
      case 'sel':
        this.unlock(m.u)
        if (this.screen !== 'select') {
          this.screen = 'select'
          this.auth = this.cur = this.prev = this.compiled = null
        }
        // an older select still in flight shouldn't undo this side's own move
        if (this.picked && now < this.picked.until && m.l !== this.picked.l) return
        this.picked = null
        this.level = m.l
        return
      case 's':
        this.onState(m)
        return
      case 'note':
        this.say(0, m.w)
    }
  }

  private onState(m: Extract<EmberMsg, { t: 's' }>): void {
    if (m.a < this.attempt) return
    const level = compiledAt(m.l)
    if (!level || !fitsLevel(level, m.g)) return
    if (m.a > this.attempt) {
      this.attempt = m.a
      this.level = m.l
      this.compiled = level
      this.screen = 'play'
      this.frame = m.f + this.lead
      this.cur = this.prev = null
      this.log = []
      this.recent = []
      this.logged = -1
      this.adjustedAt = this.frame
      this.slacks = []
      this.skip = 0
      this.winSeen = false
      this.offsets = [
        { x: 0, y: 0 },
        { x: 0, y: 0 },
      ]
    } else if (m.l !== this.level || m.f <= this.authF || this.screen !== 'play') return
    this.auth = m.g
    this.authF = m.f
    this.fireBits = m.fi
    this.dirty = true
    for (const e of m.e) {
      if (e.k === 'win' || !eventFits(level, e)) continue
      if ('p' in e && e.p === this.me && OWN_KINDS.has(e.k)) continue
      this.out.push(e)
    }
    if (m.g.phase === 'won' && !this.winSeen) {
      this.winSeen = true
      this.out.push({ k: 'win' })
      this.unlock(this.level + 2)
    }
    // the host fell back behind this side's clock (a stall here): jump ahead of it again
    if (this.frame < m.f) {
      this.frame = m.f + this.lead
      this.adjustedAt = this.frame
    }
    this.steer(m.sl)
    this.lead = Math.min(MAX_AHEAD, Math.max(2, this.frame - m.f))
  }

  // keys turning up late: run further ahead at once. keys turning up early for a while: ease back a frame
  private steer(sl: [number, number] | undefined): void {
    if (!sl || sl[1] < this.adjustedAt) return
    const [slack] = sl
    if (slack < MIN_SLACK) {
      this.frame = Math.min(this.authF + MAX_AHEAD, this.frame + AIM_SLACK - slack)
      this.adjustedAt = this.frame
      this.slacks = []
      return
    }
    this.slacks.push(slack)
    if (this.slacks.length < SLACK_WINDOW) return
    if (Math.min(...this.slacks) >= LOOSE_SLACK) {
      this.skip++
      this.adjustedAt = this.frame + 1
      this.slacks = []
    } else this.slacks.shift()
  }

  private bitsAt(f: number): number {
    for (let i = this.log.length - 1; i >= 0; i--) if (this.log[i]![0] <= f) return this.log[i]![1]
    return 0
  }

  // one predicted step; a phase change holds instead
  private predict(level: Level, s: GameState, bits: number): GameState | null {
    const n = step(level, s, [this.fireBits, bits])
    return n.phase === s.phase ? n : null
  }

  // rewind to the host's state and replay this side's keys up to the current frame
  private resim(level: Level, auth: GameState): void {
    const before = this.cur
    let s = auth
    let prev = auth
    this.holding = false
    // the log only needs to reach back to the newest host state
    while (this.log.length > 1 && this.log[1]![0] <= this.authF) this.log.shift()
    for (let f = this.authF; f < this.frame; f++) {
      const n = this.predict(level, s, this.bitsAt(f))
      if (!n) {
        this.holding = true
        prev = s
        break
      }
      prev = s
      s = n
    }
    this.cur = s
    this.prev = prev
    for (const p of [0, 1] as const) {
      const o = this.offsets[p]
      const a = before?.players[p]
      const b = s.players[p]
      // keep what's on screen where it was and let the difference ease out, unless it's a jump
      const x = a ? a.x + o.x - b.x : 0
      const y = a ? a.y + o.y - b.y : 0
      const far = !before || before.phase !== s.phase || Math.hypot(x, y) > SNAP_PX
      this.offsets[p] = far ? { x: 0, y: 0 } : { x, y }
    }
  }

  protected advance(steps: number, now: number): void {
    if (this.screen === 'play' && this.compiled && this.auth) {
      const level = this.compiled
      if (this.dirty) {
        this.dirty = false
        this.resim(level, this.auth)
      }
      const skip = Math.min(this.skip, steps)
      this.skip -= skip
      for (let i = skip; i < steps; i++) {
        // far ahead of anything the host has said: wait for it rather than guess on
        if (this.frame - this.authF >= MAX_AHEAD) break
        if (this.held !== this.logged) {
          const c: KeyChange = [this.frame, this.held]
          this.log.push(c)
          this.recent = [...this.recent, c].slice(-4)
          this.logged = this.held
          this.unsent = true
        }
        const s = this.cur!
        const n = this.holding ? null : this.predict(level, s, this.held)
        if (n) {
          this.prev = s
          this.cur = n
          for (const e of n.events) if ('p' in e && e.p === this.me && OWN_KINDS.has(e.k)) this.out.push(e)
        } else {
          this.holding = true
          this.prev = s
        }
        this.frame++
        for (const o of this.offsets) {
          o.x = Math.abs(o.x) < 0.05 ? 0 : o.x * SMOOTH
          o.y = Math.abs(o.y) < 0.05 ? 0 : o.y * SMOOTH
        }
      }
    }
    if (this.greeted && (this.unsent || now - this.lastSent >= KEEP_MS)) {
      this.unsent = false
      this.lastSent = now
      this.post({ t: 'k', a: Math.max(0, this.attempt), f: this.frame, i: this.screen === 'play' ? this.recent : [] })
    }
  }

  protected drawn() {
    const shift = (s: GameState | null): GameState | null => {
      if (!s) return null
      const [a, b] = this.offsets
      if (!a.x && !a.y && !b.x && !b.y) return s
      return {
        ...s,
        players: [
          { ...s.players[0], x: s.players[0].x + a.x, y: s.players[0].y + a.y },
          { ...s.players[1], x: s.players[1].x + b.x, y: s.players[1].y + b.y },
        ],
      }
    }
    return { state: shift(this.cur), prev: shift(this.prev) }
  }
}

export function createLocalGame(now: number, unlocked: number): EmberGame {
  return new LocalGame(now, unlocked)
}

// role host plays fire (0), guest plays frost (1). unlocked is this side's own; online uses the max of both
export function createOnlineGame(role: 'host' | 'guest', send: (text: string) => void, now: number, unlocked: number): EmberGame {
  return role === 'host' ? new HostGame(send, now, unlocked) : new GuestGame(send, now, unlocked)
}
