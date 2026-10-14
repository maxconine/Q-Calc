import { describe, expect, it } from 'vitest'
import {
  BALL_SIZE,
  COURT_H,
  decodePong,
  encodePong,
  fly,
  GUEST_FACE_X,
  HOST_FACE_X,
  landingY,
  PADDLE_H,
  PADDLE_SPEED,
  PONG_VERSION,
  secondsTo,
  SILENCE_MS,
  WIN_SCORE,
  type Side,
  type Snapshot,
} from './pong'
import { PongSession, type SessionView } from './pongSession'

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

type LinkOpts = {
  // one way, ms
  lag?: number
  // up to this much more, at random; the link still keeps order, as a websocket does
  jitter?: number
  // nothing gets through in either direction for ms from `from`, then it all comes at once
  stall?: { from: number; ms: number }
  // the guest's clock minus the host's
  skew?: number
  fps?: number
  guestVersion?: number
  dropFirst?: number
  drop?: (to: Side, text: string) => boolean
  seed?: number
}

// two sessions joined by a fake relay, stepped frame by frame
function pair(opts: LinkOpts = {}) {
  const lag = opts.lag ?? 5
  const skew = opts.skew ?? 0
  const net = seeded(opts.seed ?? 1)
  let now = 1000
  let dropped = 0
  const queue: Array<{ at: number; to: Side; text: string }> = []
  const sent: Array<{ at: number; to: Side; text: string }> = []
  const last: Record<Side, number> = { host: 0, guest: 0 }
  const route = (to: Side) => (text: string) => {
    sent.push({ at: now, to, text })
    if (dropped < (opts.dropFirst ?? 0)) {
      dropped++
      return
    }
    if (opts.drop?.(to, text)) return
    let at = Math.max(last[to], now + lag + net() * (opts.jitter ?? 0))
    const stall = opts.stall
    if (stall && at >= stall.from && at < stall.from + stall.ms) at = stall.from + stall.ms
    last[to] = at
    queue.push({ at, to, text })
  }
  let r = 0
  // a fixed sequence: serves go out at an angle, so still paddles miss
  const rand = () => [0.9, 0.1, 0.8, 0.2][r++ % 4]!
  const host = new PongSession('host', route('guest'), now, rand)
  const guestSend = route('host')
  const guest = new PongSession(
    'guest',
    opts.guestVersion == null ? guestSend : (t) => guestSend(t.replace(`"v":${PONG_VERSION}`, `"v":${opts.guestVersion}`)),
    now + skew,
    rand,
  )
  let hv: SessionView = host.tick(now)
  let gv: SessionView = guest.tick(now + skew)
  const run = (ms: number, each?: (hv: SessionView, gv: SessionView) => void, cut = false) => {
    const end = now + ms
    while (now < end) {
      now += 1000 / (opts.fps ?? 60)
      if (!cut) {
        const due = queue.filter((m) => m.at <= now)
        queue.splice(0, queue.length, ...queue.filter((m) => m.at > now))
        for (const m of due) {
          if (m.to === 'host') host.receive(m.text, now)
          else guest.receive(m.text, now + skew)
        }
      } else queue.length = 0
      hv = host.tick(now)
      gv = guest.tick(now + skew)
      each?.(hv, gv)
    }
  }
  return { host, guest, run, sent, views: () => ({ hv, gv }), now: () => now }
}

const mine = (s: Snapshot, side: Side) => s.paddles[side === 'host' ? 0 : 1]
const faceOf = (side: Side) => (side === 'host' ? HOST_FACE_X : GUEST_FACE_X)
const coming = (s: Snapshot, side: Side) => s.phase === 'play' && (side === 'host' ? s.ball.vx < 0 : s.ball.vx > 0)

// a player who watches only its own screen: once a ball is coming it picks a spot on the paddle and gets there
// just in time, and now and then doesn't bother
function player(side: Side, rand: () => number, lazy = 0.25, spareMs = 40) {
  let wasComing = false
  let aim = 0
  let idle = false
  return (v: SessionView, s: PongSession) => {
    const snap = v.snap
    if (!snap) return
    const now = coming(snap, side)
    if (now && !wasComing) {
      aim = (rand() * 2 - 1) * PADDLE_H * 0.35
      idle = rand() < lazy
    }
    wasComing = now
    let gap = 0
    if (now && !idle) {
      const want = landingY(snap.ball, faceOf(side)) - aim
      const due = secondsTo(snap.ball, faceOf(side)) * 1000
      if (due <= (Math.abs(want - mine(snap, side)) / PADDLE_SPEED) * 1000 + spareMs) gap = want - mine(snap, side)
    }
    s.key('up', gap < -4)
    s.key('down', gap > 4)
  }
}

// watches one side's screen: every time the ball reaches that side's paddle there, was the call the one the
// screen showed? and does the score only ever go up a point at a time?
function referee(side: Side) {
  const reach = PADDLE_H / 2 + BALL_SIZE / 2
  let prev: Snapshot | null = null
  const r = { hits: 0, misses: 0, wrong: [] as string[], scores: [] as string[] }
  const watch = (snap: Snapshot | null) => {
    if (!snap) return
    if (prev && coming(prev, side)) {
      const face = faceOf(side)
      const hit = snap.phase === 'play' && !coming(snap, side) && snap.score.join() === prev.score.join()
      const past = coming(snap, side) && (side === 'guest' ? snap.ball.x > face + 0.01 : snap.ball.x < face - 0.01)
      if (hit || (past && (side === 'guest' ? prev.ball.x <= face : prev.ball.x >= face))) {
        const y = fly(prev.ball, secondsTo(prev.ball, face)).y
        const off = Math.abs(y - mine(snap, side))
        if (hit) r.hits++
        else r.misses++
        if (off < reach - 1.5 && !hit) r.wrong.push(`in place (${off.toFixed(1)} off) but missed`)
        if (off > reach + 1.5 && hit) r.wrong.push(`out of place (${off.toFixed(1)} off) but hit`)
      }
    }
    const was = prev?.score
    if (was && snap.score.join() !== was.join()) {
      const up = [snap.score[0] - was[0], snap.score[1] - was[1]].sort().join()
      if (up !== '0,1' && snap.score.join() !== '0,0') r.scores.push(`${was.join(':')} → ${snap.score.join(':')}`)
    }
    prev = snap
  }
  return { r, watch }
}

// both players playing their screens until someone wins
function match(opts: LinkOpts, ms = 150_000) {
  const g = pair(opts)
  const rand = seeded((opts.seed ?? 1) * 7)
  const hp = player('host', rand)
  const gp = player('guest', rand)
  const hr = referee('host')
  const gr = referee('guest')
  let overAt = -1
  g.run(ms, (hv, gv) => {
    hp(hv, g.host)
    gp(gv, g.guest)
    hr.watch(hv.snap)
    gr.watch(gv.snap)
    if (overAt < 0 && hv.snap?.phase === 'over' && gv.snap?.phase === 'over') overAt = g.now()
  })
  return { g, host: hr.r, guest: gr.r, overAt }
}

describe('PongSession online', () => {
  it('waits for both courts, then serves', () => {
    const g = pair({ dropFirst: 2 })
    expect(g.views().hv.started).toBe(false)
    g.run(1000)
    // the first hellos were lost; the resent ones got through
    expect(g.views().hv.started).toBe(true)
    expect(g.views().gv.started).toBe(true)
    g.run(1000)
    expect(g.views().hv.snap?.phase).toBe('play')
  })

  it('both sides fly the same ball, each sees the other paddle', () => {
    const g = pair({ lag: 60, skew: 12_345.6 })
    g.run(1500)
    const { hv, gv } = g.views()
    expect(gv.snap?.phase).toBe('play')
    // on a shared clock: the same place on both screens, not 80ms behind
    expect(Math.abs(hv.snap!.ball.x - gv.snap!.ball.x)).toBeLessThan(2)
    g.guest.key('up', true)
    g.host.key('down', true)
    g.run(1000)
    expect(g.views().gv.snap?.paddles[1]).toBe(PADDLE_H / 2)
    expect(g.views().hv.snap?.paddles[1]).toBeCloseTo(PADDLE_H / 2)
    expect(g.views().hv.snap?.paddles[0]).toBe(COURT_H - PADDLE_H / 2)
    expect(g.views().gv.snap?.paddles[0]).toBeCloseTo(COURT_H - PADDLE_H / 2)
  })

  it.each([
    ['no lag', { lag: 0 }],
    ['60ms', { lag: 60, skew: -4321 }],
    ['150ms', { lag: 150, skew: 98_765 }],
    ['60ms with jitter', { lag: 60, jitter: 40, skew: 777, seed: 2 }],
    ['150ms with jitter and a stall', { lag: 150, jitter: 30, stall: { from: 9000, ms: 300 }, seed: 3 }],
    ['a lopsided link', { lag: 20, jitter: 120, seed: 4 }],
  ] as const)('%s: what each side sees at its own paddle is what counts, and the scores agree', (_, opts) => {
    const { g, host, guest, overAt } = match(opts)
    expect(overAt).toBeGreaterThan(0)
    for (const r of [host, guest]) {
      expect(r.wrong).toEqual([])
      expect(r.scores).toEqual([])
      expect(r.hits).toBeGreaterThan(5)
    }
    // every point was a miss someone saw on their own screen
    expect(host.misses + guest.misses).toBe(g.views().hv.snap!.score[0] + g.views().hv.snap!.score[1])
    const { hv, gv } = g.views()
    expect(hv.snap?.score).toEqual(gv.snap?.score)
    expect(Math.max(...hv.snap!.score)).toBe(WIN_SCORE)
    expect(gv.snap?.winner).toBe(hv.snap?.winner)
  })

  it('at 150ms a guest who gets there just as the ball does on its screen keeps the rally going', () => {
    const g = pair({ lag: 150, skew: 50 })
    const rand = seeded(5)
    // nobody ever gives up, and each gets there with a frame or two to spare
    const hp = player('host', rand, 0, 25)
    const gp = player('guest', rand, 0, 25)
    g.run(60_000, (hv, gv) => {
      hp(hv, g.host)
      gp(gv, g.guest)
    })
    expect(g.views().hv.snap?.score).toEqual([0, 0])
    expect(g.views().gv.snap?.score).toEqual([0, 0])
  })

  it('sends fewer messages than the host-runs-everything version did', () => {
    const { g, overAt } = match({ lag: 60 })
    const play = g.sent.filter((m) => m.at > 3000 && m.at < overAt)
    const perSecond = play.length / ((overAt - 3000) / 1000)
    // that one sent 60 to 90 a second
    expect(perSecond).toBeLessThan(30)
  })

  it('a leg that goes missing is sent again', () => {
    let legs = 0
    const g = pair({ lag: 40, drop: (to, text) => to === 'guest' && text.startsWith('{"t":"l"') && legs++ === 1 })
    const rand = seeded(6)
    const hp = player('host', rand)
    const gp = player('guest', rand)
    g.run(150_000, (hv, gv) => {
      hp(hv, g.host)
      gp(gv, g.guest)
    })
    expect(legs).toBeGreaterThan(2)
    expect(g.views().hv.snap?.phase).toBe('over')
    expect(g.views().gv.snap?.score).toEqual(g.views().hv.snap?.score)
  })

  it('plays to the win, then rematches when both ask, and the next game agrees too', () => {
    const g = pair({ lag: 80, jitter: 20 })
    // both paddles out of the way, so every serve scores
    g.host.key('up', true)
    g.guest.key('down', true)
    g.run(60_000)
    const { hv, gv } = g.views()
    expect(hv.snap?.phase).toBe('over')
    expect(Math.max(...hv.snap!.score)).toBe(WIN_SCORE)
    expect(gv.snap?.phase).toBe('over')
    expect(gv.snap?.score).toEqual(hv.snap?.score)
    expect(gv.snap?.winner).toBe(hv.snap?.winner)

    g.guest.rematch()
    g.run(300)
    expect(g.views().hv.again).toEqual({ mine: false, theirs: true })
    expect(g.views().hv.snap?.phase).toBe('over')
    g.host.rematch()
    g.run(400)
    expect(g.views().hv.snap?.score).toEqual([0, 0])
    expect(g.views().gv.snap?.score).toEqual([0, 0])
    expect(g.views().gv.again).toEqual({ mine: false, theirs: false })
    expect(g.views().hv.again).toEqual({ mine: false, theirs: false })
    g.run(60_000)
    expect(g.views().hv.snap?.phase).toBe('over')
    expect(g.views().gv.snap?.score).toEqual(g.views().hv.snap?.score)
    expect(Math.max(...g.views().gv.snap!.score)).toBe(WIN_SCORE)
  })

  it('rematch does nothing mid game', () => {
    const g = pair()
    g.run(1500)
    g.guest.rematch()
    g.run(200)
    expect(g.views().hv.again.theirs).toBe(false)
  })

  it('a rematch ask from mid game does not carry over to the end', () => {
    const g = pair()
    g.run(1500)
    g.host.receive(encodePong({ t: 'again' }), 2500)
    g.host.key('up', true)
    g.guest.key('down', true)
    g.run(60_000)
    expect(g.views().hv.snap?.phase).toBe('over')
    expect(g.views().hv.again.theirs).toBe(false)
    g.host.rematch()
    g.run(200)
    expect(g.views().hv.snap?.phase).toBe('over')
  })

  it('ends when the other side goes quiet', () => {
    const g = pair()
    g.run(1500)
    g.run(SILENCE_MS + 500, undefined, true)
    expect(g.views().hv.ended).toBe('lost the connection')
    expect(g.views().gv.ended).toBe('lost the connection')
  })

  it('ends when the other side says bye', () => {
    const g = pair()
    g.run(1500)
    g.host.receive(encodePong({ t: 'bye' }), 3000)
    g.run(100)
    expect(g.views().hv.ended).toBe('your friend left')
  })

  it('tells an up to date player that the friend needs to update, and the other way round', () => {
    const old = pair({ guestVersion: PONG_VERSION - 1 })
    old.run(500)
    expect(old.views().hv.ended).toBe('your friend needs to update Q Calc to play')
    expect(old.views().gv.ended).toBe('your friend left')
    const later = pair({ guestVersion: PONG_VERSION + 1 })
    later.run(500)
    expect(later.views().hv.ended).toBe('update Q Calc to play your friend')
  })

  it('a friend on the last version gets a bye, and their messages do nothing', () => {
    const sent: string[] = []
    const s = new PongSession('guest', (t) => sent.push(t), 0)
    s.receive('{"t":"s","at":1,"b":[1,2,3,4],"p":[0,0],"s":[0,0],"ph":1}', 10)
    s.receive('{"t":"p","y":40}', 10)
    expect(s.tick(20).started).toBe(false)
    s.receive('{"t":"hi","v":1,"g":"pong"}', 30)
    expect(s.tick(40).ended).toBe('your friend needs to update Q Calc to play')
    expect(decodePong(sent[sent.length - 1]!)).toEqual({ t: 'bye' })
  })

  it('starts when the guest court came up first and missed nothing but its own hi', () => {
    // the guest's page mounted first: its hi went to a host page that wasn't listening yet
    let guestHis = 0
    const g = pair({ drop: (to, text) => to === 'host' && text.includes('"hi"') && guestHis++ === 0 })
    g.run(2000)
    expect(g.views().hv.started).toBe(true)
    expect(g.views().hv.snap?.phase).toBe('play')
  })

  it('stops saying hi once both sides have heard each other', () => {
    const g = pair({ dropFirst: 1 })
    g.run(3000)
    const lateHis = g.sent.filter((m) => m.at > 2500 && m.text.includes('"hi"'))
    expect(lateHis).toEqual([])
  })

  it.each([60, 120, 144])('keeps the same message rate at %i fps', (fps) => {
    const g = pair({ fps })
    g.guest.key('down', true)
    g.run(3000)
    const second = g.sent.filter((m) => m.at > 2000 && m.at <= 3000)
    // keepalives from both, plus a leg or two
    expect(second.length).toBeGreaterThanOrEqual(8)
    expect(second.length).toBeLessThanOrEqual(12)
  })

  it('the paddle message stays on the court', () => {
    const sent: string[] = []
    const s = new PongSession('guest', (t) => sent.push(t), 0)
    s.receive(encodePong({ t: 'hi', v: PONG_VERSION, ok: true }), 0)
    s.key('down', true)
    for (let t = 16; t < 2000; t += 16) s.tick(t)
    const last = decodePong(sent[sent.length - 1]!)
    expect(last).toMatchObject({ t: 'p', y: COURT_H - PADDLE_H / 2, d: 1 })
  })
})
