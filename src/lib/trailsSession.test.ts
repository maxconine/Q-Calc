import { describe, expect, it } from 'vitest'
import { encodeC4 } from './connect4'
import { encodePong } from './pong'
import { newRound, queueTurn, seeded, step, TICK_MS, WIN_ROUNDS, type Dir, type Round } from './trails'
import { decodeTrails, encodeTrails, hashStart, hashStep, MAX_FRAMES, TRAILS_VERSION, type Snap, type TrailsMsg } from './trailsNet'
import { STALL_SHOW_MS, TRAILS_SILENCE_MS, TrailsSession, type TrailsView } from './trailsSession'

type To = 'host' | 'guest'
type Wire = { at: number; to: To; text: string }

// two sessions joined by a link that delivers each message after its own delay (so they can overtake each
// other), can lose them, and can hold one back to deliver late. stepped at 60fps, the guest half a frame off
function pair(
  opts: {
    lag?: number
    jitter?: number
    seed?: number
    guestVersion?: number
    // true loses the message; a number delivers it that many ms late instead
    fault?: (to: To, m: TrailsMsg, now: number) => boolean | number
  } = {},
) {
  const lag = opts.lag ?? 40
  const rand = seeded(opts.seed ?? 1)
  let now = 1000
  const queue: Wire[] = []
  const sent: Array<Wire & { m: TrailsMsg }> = []
  const route = (to: To) => (text: string) => {
    const m = decodeTrails(text) as TrailsMsg
    sent.push({ at: now, to, text, m })
    const fault = opts.fault?.(to, m, now) ?? false
    if (fault === true) return
    queue.push({ at: now + lag + rand() * (opts.jitter ?? 0) + (fault || 0), to, text })
  }
  const host = new TrailsSession('host', route('guest'), now)
  const guestSend = route('host')
  const guest = new TrailsSession(
    'guest',
    opts.guestVersion == null ? guestSend : (t) => guestSend(t.replace(`"v":${TRAILS_VERSION}`, `"v":${opts.guestVersion}`)),
    now,
  )
  let hv: TrailsView = host.tick(now)
  let gv: TrailsView = guest.tick(now)
  const stalls = { host: 0, guest: 0 }
  let cut = false
  // what each side is told to do, by time
  const script: Array<{ at: number; who: To; dir: Dir }> = []
  const deliver = (to: To) => {
    const due = queue.filter((m) => m.to === to && m.at <= now).sort((a, b) => a.at - b.at)
    for (const m of due) {
      queue.splice(queue.indexOf(m), 1)
      if (!cut) (to === 'host' ? host : guest).receive(m.text, now)
    }
  }
  const run = (ms: number, until?: () => boolean) => {
    const end = now + ms
    while (now < end && !until?.()) {
      now += 1000 / 120
      for (const s of script.splice(0).filter((s) => s.at <= now || (script.push(s), false))) (s.who === 'host' ? host : guest).turn(s.dir)
      // the two display clocks are half a frame apart
      const side: To = Math.round((now - 1000) / (1000 / 120)) % 2 ? 'host' : 'guest'
      deliver(side)
      if (side === 'host') hv = host.tick(now)
      else gv = guest.tick(now)
      if (hv.stalled) stalls.host++
      if (gv.stalled) stalls.guest++
    }
  }
  return {
    host,
    guest,
    run,
    sent,
    stalls,
    script,
    now: () => now,
    cut: () => (cut = true),
    views: () => ({ hv, gv }),
    toGuest: (m: TrailsMsg) => guest.receive(encodeTrails(m), now),
  }
}

// losing five keepalives running is a dead link, rightly; the soak is after the game's own messages
const KEEPALIVE = new Set(['trp', 'trq', 'trk'])
const SOAK = Number(process.env.TRAILS_SOAK ?? 6)
const paths = (v: TrailsView) => v.round.cycles.map((c) => c.path.join(','))
const running = (g: ReturnType<typeof pair>) => () => g.views().hv.phase === 'run' && g.views().gv.phase === 'run'

// both sides agree on everything that matters
function expectSame(g: ReturnType<typeof pair>) {
  // one side starts the next round a moment before the other
  g.run(400, () => g.host.state().r === g.guest.state().r)
  const h = g.host.state()
  const s = g.guest.state()
  expect(s.r).toBe(h.r)
  expect(s.score).toEqual(h.score)
  // one may be a tick or two on from the other: the walls so far must still be the same walls
  const { hv, gv } = g.views()
  for (const p of [0, 1] as const) {
    const a = hv.round.cycles[p].path
    const b = gv.round.cycles[p].path
    const n = Math.min(a.length, b.length)
    expect(b.slice(0, n)).toEqual(a.slice(0, n))
    expect(Math.abs(a.length - b.length)).toBeLessThanOrEqual(8)
  }
}

// a player who turns at random now and then, so rounds end in different ways
function wander(g: ReturnType<typeof pair>, seed: number, forMs: number) {
  const rand = seeded(seed)
  for (let at = g.now(); at < g.now() + forMs; at += 90 + rand() * 500) {
    g.script.push({ at, who: rand() < 0.5 ? 'host' : 'guest', dir: Math.floor(rand() * 4) as Dir })
  }
}

describe('TrailsSession', () => {
  it('waits for both arenas, measures the link, then counts in together', () => {
    const g = pair({ lag: 60 })
    expect(g.views().hv.phase).toBe('wait')
    g.run(3000, () => g.views().gv.phase === 'count')
    expect(g.views().gv.phase).toBe('count')
    expect(g.views().hv.phase).toBe('count')
    // 60ms each way plus the margin is two ticks
    expect(g.host.state().delay).toBe(2)
    expect(g.guest.state().delay).toBe(2)
    g.run(3000, running(g))
    expect(g.views().hv.phase).toBe('run')
  })

  it('picks the input delay from the ping', () => {
    for (const [lag, delay] of [
      [20, 1],
      [60, 2],
      [120, 3],
    ] as const) {
      const g = pair({ lag })
      g.run(4000, running(g))
      expect(g.host.state().delay).toBe(delay)
    }
  })

  it('resends hello until it is heard', () => {
    let lost = 0
    const g = pair({ fault: (_, m) => m.t === 'tr' && lost++ < 2 })
    g.run(4000, running(g))
    expect(g.views().gv.phase).toBe('run')
  })

  it('refuses a different version', () => {
    const g = pair({ guestVersion: TRAILS_VERSION + 1 })
    g.run(500)
    expect(g.views().hv.ended).toBe('update both Q Calcs to play each other')
    expect(g.sent.some((s) => s.to === 'guest' && s.m.t === 'bye')).toBe(true)
  })

  it('tells the players when the other side opened another game', () => {
    for (const [text, game] of [
      [encodePong({ t: 'hi', v: 1 }), 'pong'],
      [encodeC4({ t: 'c4', v: 1 }), 'connect 4'],
      [JSON.stringify({ t: 'hi', v: 1, u: 3 }), 'ember'],
    ] as const) {
      const out: string[] = []
      const s = new TrailsSession('host', (t) => out.push(t), 0)
      s.receive(text, 10)
      expect(s.tick(20).ended).toBe(`your friend opened ${game} · both type trails`)
      expect(out.at(-1)).toBe('{"t":"bye"}')
    }
  })

  it('every message is a tr type, so other games ignore them', () => {
    const g = pair()
    g.run(6000)
    expect(g.sent.length).toBeGreaterThan(50)
    for (const s of g.sent) expect(JSON.parse(s.text).t).toMatch(/^tr/)
  })

  it('stays in step across latency and jitter', () => {
    for (const [lag, jitter] of [
      [20, 10],
      [60, 40],
      [120, 60],
    ] as const) {
      const g = pair({ lag, jitter, seed: lag })
      g.run(4000, running(g))
      wander(g, lag, 20_000)
      for (let i = 0; i < 40; i++) {
        g.run(500)
        expectSame(g)
      }
      // nothing was ever put right: the two never disagreed
      expect(g.host.state().epoch).toBe(1)
      expect(g.host.state().r).toBeGreaterThan(0)
    }
  })

  it('runs smoothly on a steady link', () => {
    const g = pair({ lag: 80, jitter: 15 })
    g.run(4000, running(g))
    g.run(3000)
    expect(g.stalls).toEqual({ host: 0, guest: 0 })
    // about a tick's worth of time per tick: nobody is being held back
    expect(g.host.state().tick).toBeGreaterThan((3000 / TICK_MS) * 0.9)
  })

  it('a turn lands on the same tick on both sides, the input delay after it was pressed', () => {
    const g = pair({ lag: 60 })
    g.run(4000, running(g))
    g.run(400)
    const pressedAt = g.host.state().tick
    g.host.turn(0)
    g.guest.turn(2)
    g.run(1000)
    const { hv, gv } = g.views()
    expect(paths(gv).map((p) => p.slice(0, 200))).toEqual(paths(hv).map((p) => p.slice(0, 200)))
    // the host went right from (8, 12) and turned up `delay` ticks after the press: the cell it turned at
    const path = hv.round.cycles[0].path
    const corner = path.findIndex((cell, i) => i > 0 && path[i + 1] === cell - 64)
    expect(corner).toBe(pressedAt + g.host.state().delay)
    expect(gv.round.cycles[0].path[corner + 1]).toBe(path[corner + 1])
  })

  it('turns pressed during the count are taken as the round starts, the same on both sides', () => {
    const g = pair()
    g.run(4000, () => g.views().gv.phase === 'count' && g.views().hv.phase === 'count')
    g.host.turn(2)
    g.host.turn(3)
    // a third is dropped, as offline
    g.host.turn(0)
    g.guest.turn(0)
    g.run(4000, () => g.host.state().tick >= 6 && g.guest.state().tick >= 6)
    for (const v of [g.views().hv, g.views().gv]) {
      // down on the first tick, then left
      expect(v.round.cycles[0].path.slice(0, 3)).toEqual([12 * 64 + 8, 13 * 64 + 8, 13 * 64 + 7])
      expect(v.round.cycles[1].path.slice(0, 2)).toEqual([13 * 64 + 55, 12 * 64 + 55])
    }
  })

  it('a lost frame heals with the next message', () => {
    let lost = 0
    const g = pair({ fault: (_, m) => m.t === 'trf' && lost++ % 5 === 0 })
    g.run(4000, running(g))
    wander(g, 9, 8000)
    g.run(8000)
    expect(lost).toBeGreaterThan(40)
    expectSame(g)
    expect(g.host.state().epoch).toBe(1)
  })

  it('never sticks, whatever a bad link loses', () => {
    for (let seed = 1; seed <= SOAK; seed++) {
      const loss = seeded(seed * 77)
      let bad = true
      const g = pair({ lag: 20 + (seed % 12) * 12, jitter: 60, seed, fault: (_, m) => bad && loss() < 0.15 && !KEEPALIVE.has(m.t) })
      wander(g, seed, 45_000)
      g.run(45_000)
      // the link comes good: both must be playing the same game, and still moving
      bad = false
      g.run(3000)
      expect(g.views().hv.ended, `seed ${seed}`).toBe(null)
      expectSame(g)
      if (g.views().hv.phase === 'over') continue
      const was = [g.host.state().r, g.host.state().tick]
      g.run(2500)
      expect([g.host.state().r, g.host.state().tick], `seed ${seed}`).not.toEqual(was)
      expectSame(g)
    }
  })

  it('a stall shows, and ends when the late frame arrives', () => {
    // every guest frame message from tick 20 on is held back most of a second, once
    let from: number | null = null
    const g = pair({
      lag: 30,
      fault: (to, m, now) => {
        if (to !== 'host' || m.t !== 'trf' || m.f + m.i.length < 20) return false
        from ??= now
        return now - from < 900 ? from + 900 - now : false
      },
    })
    g.run(4000, running(g))
    g.run(8000, () => g.views().hv.stalled)
    const stuckAt = g.host.state().tick
    expect(g.views().hv.stalled).toBe(true)
    expect(g.views().hv.phase).toBe('run')
    g.run(300)
    // held still, not running on alone
    expect(g.host.state().tick).toBe(stuckAt)
    g.run(3000, () => !g.views().hv.stalled)
    g.run(500)
    expect(g.views().hv.stalled).toBe(false)
    expect(g.host.state().tick).toBeGreaterThan(stuckAt + 3)
    expectSame(g)
    expect(g.host.state().epoch).toBe(1)
    expect(STALL_SHOW_MS).toBeGreaterThan(TICK_MS * 2)
  })

  it('a frame that was dropped and turns up much later changes nothing', () => {
    // one message is held for 600ms: by then its frames have long since arrived with later ones
    let held = false
    const g = pair({
      lag: 40,
      jitter: 30,
      fault: (to, m) => {
        if (held || to !== 'guest' || m.t !== 'trf' || m.f < 8) return false
        held = true
        return 600
      },
    })
    g.run(4000, running(g))
    wander(g, 5, 6000)
    g.run(6000)
    expect(held).toBe(true)
    expectSame(g)
    expect(g.host.state().epoch).toBe(1)
  })

  it('a stale or far-future frame is not taken', () => {
    const g = pair()
    g.run(4000, running(g))
    g.run(500)
    const { r, tick } = g.guest.state()
    // another round's, another epoch's, and one from far beyond where the round is
    g.toGuest({ t: 'trf', e: 1, r: r + 1, f: tick + 4, i: [0], a: -1 })
    g.toGuest({ t: 'trf', e: 7, r, f: tick + 4, i: [0], a: -1 })
    g.toGuest({ t: 'trf', e: 1, r, f: tick + MAX_FRAMES + 5, i: [0], a: -1 })
    g.run(1500)
    expectSame(g)
    expect(g.host.state().epoch).toBe(1)
  })

  it('puts a desync right from the host', () => {
    const g = pair({ lag: 50, jitter: 20 })
    g.run(4000, running(g))
    g.run(300)
    // the guest is told the host turns up, a few ticks on, when it never does
    const { r, tick } = g.guest.state()
    g.toGuest({ t: 'trf', e: 1, r, f: tick + 6, i: [0], a: -1 })
    g.run(700)
    expect(g.views().gv.round.cycles[0].dir).not.toBe(g.views().hv.round.cycles[0].dir)
    g.run(3000, () => g.host.state().epoch > 1)
    expect(g.host.state().epoch).toBe(2)
    g.run(1000)
    expect(g.guest.state().epoch).toBe(2)
    expect(g.views().gv.round.cycles[0].dir).toBe(1)
    expectSame(g)
    // and it stays right, to the end of the match
    wander(g, 11, 60_000)
    g.run(60_000, () => g.views().hv.phase === 'over' && g.views().gv.phase === 'over')
    expect(g.guest.state().score).toEqual(g.host.state().score)
    expect(g.host.state().epoch).toBe(2)
  })

  it('a round that ended differently is put right before the next', () => {
    const g = pair({ lag: 30 })
    g.run(4000, running(g))
    g.run(300)
    // the guest sees the host turn into its own way out: the guest's round goes on a different way
    const { r, tick } = g.guest.state()
    g.toGuest({ t: 'trf', e: 1, r, f: tick + 3, i: [2], a: -1 })
    g.toGuest({ t: 'trf', e: 1, r, f: tick + 4, i: [3], a: -1 })
    g.run(20_000, () => g.host.state().epoch > 1 && g.guest.state().epoch > 1)
    expect(g.guest.state().epoch).toBeGreaterThan(1)
    g.run(1000)
    expectSame(g)
  })

  it('a snapshot that never arrives is sent again', () => {
    let lost = 0
    const g = pair({ fault: (_, m) => m.t === 'trs' && lost++ < 1 })
    g.run(6000, running(g))
    expect(lost).toBeGreaterThan(1)
    expect(g.views().gv.phase).toBe('run')
    expectSame(g)
  })

  it('plays a whole match with the same score on both sides, then a rematch', () => {
    const g = pair({ lag: 70, jitter: 50, seed: 4 })
    g.run(4000, running(g))
    wander(g, 21, 120_000)
    const over = () => g.views().hv.phase === 'over' && g.views().gv.phase === 'over'
    let rounds = 0
    let was = g.host.state().r
    g.run(120_000, () => {
      if (g.host.state().r !== was) {
        was = g.host.state().r
        rounds++
        expectSame(g)
      }
      return over()
    })
    expect(over()).toBe(true)
    const { hv, gv } = g.views()
    expect(gv.score).toEqual(hv.score)
    expect(Math.max(...hv.score)).toBe(WIN_ROUNDS)
    expect(gv.winner).toBe(hv.winner)
    expect(rounds).toBeGreaterThanOrEqual(WIN_ROUNDS - 1)
    expect(g.host.state().epoch).toBe(1)

    // one ask isn't enough; a turn does nothing once it's over
    g.script.length = 0
    g.guest.rematch(g.now())
    g.guest.turn(0)
    g.run(500)
    expect(g.views().hv.again).toEqual({ mine: false, theirs: true })
    expect(g.views().gv.again).toEqual({ mine: true, theirs: false })
    expect(g.views().hv.phase).toBe('over')
    g.host.rematch(g.now())
    g.run(500)
    for (const v of [g.views().hv, g.views().gv]) {
      expect(v.phase).toBe('count')
      expect(v.score).toEqual([0, 0])
      expect(v.again).toEqual({ mine: false, theirs: false })
    }
    g.run(4000, running(g))
    g.run(1000)
    expectSame(g)
  })

  it('the host asking first works too', () => {
    const g = pair()
    g.run(4000, running(g))
    wander(g, 8, 120_000)
    g.run(120_000, () => g.views().hv.phase === 'over' && g.views().gv.phase === 'over')
    g.script.length = 0
    g.host.rematch(g.now())
    g.run(300)
    expect(g.views().gv.again.theirs).toBe(true)
    g.guest.rematch(g.now())
    g.run(300)
    expect(g.views().hv.phase).toBe('count')
    expect(g.views().gv.phase).toBe('count')
  })

  it('an ask made mid match does not count', () => {
    const g = pair()
    g.run(4000, running(g))
    g.guest.rematch(g.now())
    g.host.receive(encodeTrails({ t: 'tra', e: 1 }), g.now())
    g.run(300)
    expect(g.views().hv.again).toEqual({ mine: false, theirs: false })
    expect(g.host.state().r).toBe(0)
  })

  it('keeps the link warm between rounds and ends after silence', () => {
    const g = pair()
    g.run(4000, running(g))
    wander(g, 8, 120_000)
    g.run(120_000, () => g.views().hv.phase === 'over' && g.views().gv.phase === 'over')
    // nothing to play, but both keep talking
    g.run(TRAILS_SILENCE_MS * 3)
    expect(g.views().hv.ended).toBe(null)
    expect(g.views().gv.ended).toBe(null)
    g.cut()
    g.run(TRAILS_SILENCE_MS + 200)
    expect(g.views().hv.ended).toBe('lost the connection')
    expect(g.views().gv.ended).toBe('lost the connection')
  })

  it('times out waiting for a side that never answers', () => {
    const s = new TrailsSession('guest', () => {}, 0)
    expect(s.tick(TRAILS_SILENCE_MS - 1).ended).toBe(null)
    expect(s.tick(TRAILS_SILENCE_MS + 1).ended).toBe('lost the connection')
  })

  it('says bye on leaving, and the other side is told', () => {
    const g = pair()
    g.run(4000, running(g))
    g.guest.leave()
    g.run(300)
    expect(g.views().hv.ended).toBe('your friend left')
    expect(g.guest.tick(g.now()).ended).toBe('')
  })
})

describe('the trails wire', () => {
  const roundAfter = (turns: Array<[number, 0 | 1, Dir]>, ticks: number): Round => {
    let r = newRound()
    for (let t = 0; t < ticks; t++) {
      for (const [at, p, dir] of turns) if (at === t) r = queueTurn(r, p, dir)
      r = step(r)
    }
    return r
  }
  const snap = (round: Round, over: Partial<Snap> = {}): TrailsMsg => ({
    t: 'trs',
    e: 3,
    r: 2,
    d: 2,
    score: [1, 0],
    phase: round.result ? 'point' : 'run',
    wait: 0,
    round,
    frames: [[null, 2], [0]],
    hash: 12345,
    ...over,
  })

  it('frames are a character a tick', () => {
    const text = encodeTrails({ t: 'trf', e: 0, r: 3, f: 120, i: [null, null, 1], a: 119 })
    expect(text).toBe('{"t":"trf","r":3,"f":120,"i":"..1","a":119}')
    expect(decodeTrails(text)).toEqual({ t: 'trf', e: 0, r: 3, f: 120, i: [null, null, 1], a: 119 })
    const hashed = { t: 'trf', e: 2, r: 3, f: 0, i: [], a: -1, h: [16, 4000000000] } as TrailsMsg
    expect(decodeTrails(encodeTrails(hashed))).toEqual(hashed)
  })

  it('a snapshot carries the round whole', () => {
    const round = roundAfter(
      [
        [3, 0, 0],
        [9, 0, 1],
        [5, 1, 0],
        [10, 1, 3],
      ],
      40,
    )
    const text = encodeTrails(snap(round))
    // walls go as their corners, not every cell
    expect(text.length).toBeLessThan(200)
    const back = decodeTrails(text)
    expect(back?.t).toBe('trs')
    if (back?.t !== 'trs') return
    expect(back.round.tick).toBe(40)
    expect([...back.round.grid]).toEqual([...round.grid])
    expect(back.round.cycles.map((c) => c.path)).toEqual(round.cycles.map((c) => c.path))
    expect(back.round.cycles.map((c) => [c.x, c.y, c.dir, c.alive])).toEqual(round.cycles.map((c) => [c.x, c.y, c.dir, c.alive]))
    expect(back.frames).toEqual([[null, 2], [0]])
    expect(back.score).toEqual([1, 0])
  })

  it('a snapshot of a finished round keeps who crashed, and where', () => {
    // the left cycle turns up and runs into the top wall
    const round = roundAfter([[0, 0, 0]], 20)
    expect(round.result).toEqual({ winner: 1 })
    const back = decodeTrails(encodeTrails(snap(round)))
    if (back?.t !== 'trs') throw new Error('not a snapshot')
    expect(back.round.result).toEqual({ winner: 1 })
    expect(back.round.cycles[0].alive).toBe(false)
    expect(back.round.cycles[0].crash).toEqual(round.cycles[0].crash)
    expect(back.round.tick).toBe(round.tick)
  })

  it('the hash follows the whole path, not just where the heads are', () => {
    const run = (turns: Array<[number, 0 | 1, Dir]>) => {
      let r = newRound()
      let h = hashStart(0, [0, 0])
      for (let t = 0; t < 12; t++) {
        for (const [at, p, dir] of turns) if (at === t) r = queueTurn(r, p, dir)
        r = step(r)
        h = hashStep(h, r)
      }
      return { r, h }
    }
    // up then right, or right then up: the same cell by two ways
    const a = run([
      [2, 0, 0],
      [3, 0, 1],
    ])
    const b = run([
      [3, 0, 0],
      [4, 0, 1],
    ])
    expect([a.r.cycles[0].x, a.r.cycles[0].y]).toEqual([b.r.cycles[0].x, b.r.cycles[0].y])
    expect(a.h).not.toBe(b.h)
    expect(hashStart(1, [0, 0])).not.toBe(hashStart(0, [0, 0]))
    expect(hashStart(0, [1, 0])).not.toBe(hashStart(0, [0, 1]))
  })

  it('rejects junk', () => {
    const round = roundAfter([[3, 0, 0]], 10)
    const good = JSON.parse(encodeTrails(snap(round))) as Record<string, unknown>
    expect(decodeTrails(JSON.stringify(good))?.t).toBe('trs')
    const bad: unknown[] = [
      '',
      'nope',
      'null',
      '[]',
      '{"t":"trx"}',
      '{"t":"tr"}',
      '{"t":"tr","v":"1"}',
      '{"t":"trp","c":-1}',
      '{"t":"trq","c":1.5}',
      '{"t":"trk"}',
      '{"t":"tra","e":"0"}',
      // frames: a turn that isn't one, a tick out of range, too many, a wrong type, a bad ack or hash
      '{"t":"trf","r":0,"f":0,"i":"..4","a":0}',
      '{"t":"trf","r":0,"f":0,"i":"..x","a":0}',
      '{"t":"trf","r":0,"f":-1,"i":".","a":0}',
      '{"t":"trf","r":0,"f":99999,"i":".","a":0}',
      '{"t":"trf","r":0,"f":1.5,"i":".","a":0}',
      `{"t":"trf","r":0,"f":0,"i":"${'.'.repeat(MAX_FRAMES + 1)}","a":0}`,
      '{"t":"trf","r":0,"f":0,"i":[1],"a":0}',
      '{"t":"trf","r":-1,"f":0,"i":".","a":0}',
      '{"t":"trf","r":0,"f":0,"i":".","a":-2}',
      '{"t":"trf","r":0,"f":0,"i":"."}',
      '{"t":"trf","r":0,"f":0,"i":".","a":0,"h":[1]}',
      '{"t":"trf","r":0,"f":0,"i":".","a":0,"h":[1,-5]}',
      '{"t":"trf","e":"1","r":0,"f":0,"i":".","a":0}',
      // snapshots
      { ...good, d: 0 },
      { ...good, d: 9 },
      { ...good, e: 0 },
      { ...good, s: [1, 9] },
      { ...good, s: [1] },
      { ...good, ph: 7 },
      // the phase and the round disagree
      { ...good, ph: 2 },
      { ...good, ph: 0 },
      { ...good, w: -1 },
      { ...good, h: 2 ** 33 },
      { ...good, q: ['..', '9'] },
      { ...good, q: ['..'] },
      // walls: too short for the tick, a diagonal, off the grid, through each other, not a wall at all
      { ...good, k: 11 },
      { ...good, c: [[1, [0, 65]], (good.c as unknown[])[1]] },
      { ...good, c: [[1, [0, 99999]], (good.c as unknown[])[1]] },
      { ...good, c: [(good.c as unknown[])[0], (good.c as unknown[])[0]] },
      { ...good, c: [[5, [0, 10]], (good.c as unknown[])[1]] },
      { ...good, c: [[1, 'x'], (good.c as unknown[])[1]] },
      { ...good, c: [(good.c as unknown[])[0]] },
      { ...good, c: [[1, [776, 786], [99, 99]], (good.c as unknown[])[1]] },
      // oversized
      JSON.stringify({ t: 'tr', v: 1, pad: 'x'.repeat(20_000) }),
    ]
    for (const b of bad) expect(decodeTrails(typeof b === 'string' ? b : JSON.stringify(b)), JSON.stringify(b).slice(0, 80)).toBe(null)
    // and nothing thrown for the wrong type altogether
    expect(decodeTrails(42 as unknown as string)).toBe(null)
  })

  it('a session shrugs off junk and what is not for it', () => {
    const out: string[] = []
    const s = new TrailsSession('guest', (t) => out.push(t), 0)
    for (const text of ['', '{', '{"t":"trf","r":0,"f":0,"i":"..9","a":0}', '{"t":"s","at":1}', '{"t":"trs"}']) s.receive(text, 10)
    expect(s.tick(20).ended).toBe(null)
    expect(s.tick(20).phase).toBe('wait')
    // junk doesn't count as hearing from the other side
    expect(s.tick(TRAILS_SILENCE_MS + 1).ended).toBe('lost the connection')
  })
})
