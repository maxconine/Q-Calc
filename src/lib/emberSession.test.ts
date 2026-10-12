import { describe, expect, it, vi } from 'vitest'
import { LEVELS } from './emberLevels'
import { decodeEmber, STATE_HZ, SILENCE_MS } from './emberProtocol'
import { createLocalGame, createOnlineGame, type EmberGame, type EmberView } from './emberSession'
import { IN_DOWN, IN_LEFT, IN_RIGHT, IN_UP, RUN_ACCEL, type GameEvent } from './emberTypes'

// the session is tested on small levels of its own: 0 open floor, 1 a water pit right of ember,
// 2 two stages: the doors a step away, then a checkpoint flag at column 8 and a water pit from column 14,
// 3 left of frost a trampoline under a one-way deck, then a crumbling floor over a drop,
// 4 a lamp beside ember, light-only and dark-only blocks, and left of frost an arrow overhead, a key and a lock,
// 5 and 6 each player one step left of their door with a wall behind it
vi.mock('./emberLevels', () => {
  const row = (cells: Record<number, string>) => {
    let s = '#'
    for (let c = 1; c < 31; c++) s += cells[c] ?? '.'
    return s + '#'
  }
  const map = (r16: Record<number, string>, floor = '#'.repeat(32)) => [
    '#'.repeat(32),
    ...Array.from({ length: 15 }, () => row({})),
    row(r16),
    floor,
  ]
  const win = { 1: '1', 2: 'R', 3: '#', 27: '#', 28: '2', 29: 'B', 30: '#' }
  return {
    LEVELS: [
      { id: 't-open', name: 'open', par: 20, stages: [{ map: map({ 2: '1', 13: 'R', 24: 'B', 27: '2' }), things: [] }] },
      { id: 't-pit', name: 'pit', par: 20, stages: [{ map: map({ 2: '1', 20: 'R', 26: 'B', 29: '2' }, '####WWWW' + '#'.repeat(24)), things: [] }] },
      {
        id: 't-two',
        name: 'two',
        par: 60,
        stages: [
          { map: map(win), things: [] },
          { map: map({ 2: '1', 4: '2', 8: 'C', 27: 'R', 29: 'B' }, '#'.repeat(14) + 'WWWW' + '#'.repeat(14)), things: [] },
        ],
      },
      {
        id: 't-tiles',
        name: 'tiles',
        par: 60,
        stages: [
          {
            map: [
              '#'.repeat(32),
              ...Array.from({ length: 13 }, () => row({})),
              row({ 19: '=', 20: '=', 21: '=', 22: '=', 23: '=' }),
              row({}),
              row({ 2: '1', 3: 'R', 5: 'B', 27: '2' }),
              '#'.repeat(8) + '~~~~~~' + '#'.repeat(6) + 'TTT' + '#'.repeat(9),
              row({}),
              row({}),
              '#'.repeat(32),
            ],
            things: [],
          },
        ],
      },
      {
        id: 't-things',
        name: 'things',
        par: 60,
        stages: [
          {
            map: [
              '#'.repeat(32),
              ...Array.from({ length: 12 }, () => row({})),
              row({ 25: '^' }),
              row({ 8: 'O', 9: 'D' }),
              row({}),
              row({ 2: '1', 10: 'R', 12: 'B', 20: 'k', 27: '2' }),
              '#'.repeat(32),
            ],
            things: [
              { k: 'lamp', x: 4, y: 16 },
              { k: 'lock', x: 18, y: 15, w: 1, h: 2 },
            ],
          },
        ],
      },
      { id: 't-win', name: 'win', par: 20, stages: [{ map: map(win), things: [] }] },
      { id: 't-last', name: 'last', par: 20, stages: [{ map: map(win), things: [] }] },
    ],
  }
})

const FRAME = 1000 / 60

// two online sessions joined by a queue that delivers after `lag` ms, with a fake clock
function pair(
  opts: {
    lag?: number
    // each message takes up to this much longer, so some overtake others
    jitter?: number
    // this share of the host's state messages never arrives
    loss?: number
    fps?: number
    guestVersion?: number
    dropFirst?: number
    drop?: (to: 'host' | 'guest', text: string) => boolean
    // called after every frame with both views
    frame?: (hv: EmberView, gv: EmberView) => void
  } = {},
) {
  const lag = opts.lag ?? 5
  let now = 1000
  let dropped = 0
  let seed = 99
  const rand = () => ((seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648)
  const queue: Array<{ at: number; to: 'host' | 'guest'; text: string }> = []
  const sent: Array<{ at: number; to: 'host' | 'guest'; text: string }> = []
  const route = (to: 'host' | 'guest') => (text: string) => {
    sent.push({ at: now, to, text })
    if (dropped < (opts.dropFirst ?? 0)) {
      dropped++
      return
    }
    if (opts.drop?.(to, text)) return
    if (opts.loss && to === 'guest' && text.startsWith('{"t":"s"') && rand() < opts.loss) return
    queue.push({ at: now + lag + (opts.jitter ? rand() * opts.jitter : 0), to, text })
  }
  const host = createOnlineGame('host', route('guest'), now)
  const guestSend = route('host')
  const guest = createOnlineGame(
    'guest',
    opts.guestVersion == null ? guestSend : (t) => guestSend(t.replace(/"v":\d+/, `"v":${opts.guestVersion}`)),
    now,
  )
  let hv: EmberView = host.tick(now)
  let gv: EmberView = guest.tick(now)
  const events = { host: [] as GameEvent[], guest: [] as GameEvent[] }
  const run = (ms: number, cut = false) => {
    const end = now + ms
    while (now < end - 1e-9) {
      now += 1000 / (opts.fps ?? 60)
      if (cut) queue.length = 0
      else {
        const due = queue.filter((m) => m.at <= now).sort((a, b) => a.at - b.at)
        queue.splice(0, queue.length, ...queue.filter((m) => m.at > now))
        for (const m of due) (m.to === 'host' ? host : guest).receive(m.text, now)
      }
      hv = host.tick(now)
      gv = guest.tick(now)
      events.host.push(...hv.events)
      events.guest.push(...gv.events)
      opts.frame?.(hv, gv)
    }
  }
  // one frame at a time until `done`, or `ms` at most
  const until = (done: () => boolean, ms = 5000) => {
    const end = now + ms
    while (!done() && now < end) run(1000 / (opts.fps ?? 60))
    return done()
  }
  // runs until both sides draw `level` in play, then a little more
  const startOn = (level: number, who: EmberGame = host) => {
    run(500)
    who.pick(level)
    run(200)
    who.play()
    run(500)
  }
  return { host, guest, run, until, sent, events, startOn, now: () => now, views: () => ({ hv, gv }) }
}

const pos = (v: EmberView, p: 0 | 1) => v.state!.players[p]

describe('online: the link', () => {
  it('waits for both sides, then both go to select', () => {
    const g = pair({ dropFirst: 2 })
    expect(g.views().hv.screen).toBe('wait')
    expect(g.views().gv.screen).toBe('wait')
    g.run(1000)
    expect(g.views().hv.screen).toBe('select')
    expect(g.views().gv.screen).toBe('select')
    expect(g.views().hv.me).toBe(0)
    expect(g.views().gv.me).toBe(1)
  })

  it('stops saying hi once both sides have heard each other', () => {
    const g = pair({ dropFirst: 1 })
    g.run(3000)
    expect(g.sent.filter((m) => m.at > 2500 && m.text.includes('"hi"'))).toEqual([])
  })

  it('refuses a different version', () => {
    const g = pair({ guestVersion: 99 })
    g.run(500)
    expect(g.views().hv.ended).toBe('update both Q Calcs to play each other')
    expect(g.views().gv.ended).toBe('your friend left')
  })

  it('ends when the other side says bye', () => {
    const g = pair()
    g.run(500)
    g.guest.leave()
    g.run(100)
    expect(g.views().hv.ended).toBe('your friend left')
  })

  it.each(['select', 'play'])('ends when the other side goes quiet on %s', (where) => {
    const g = pair()
    if (where === 'play') g.startOn(0)
    else g.run(500)
    // still alive well past the silence window while the link works, on either screen
    g.run(SILENCE_MS + 500)
    expect(g.views().hv.ended).toBe(null)
    expect(g.views().gv.ended).toBe(null)
    g.run(SILENCE_MS + 500, true)
    expect(g.views().hv.ended).toBe('lost the connection')
    expect(g.views().gv.ended).toBe('lost the connection')
  })

  it('ignores junk and garbled messages without throwing', () => {
    const g = pair()
    g.startOn(0)
    const valid = g.sent.filter((m) => m.at > g.now() - 300).map((m) => m.text)
    expect(valid.some((t) => t.startsWith('{"t":"s"'))).toBe(true)
    let seed = 7
    const rand = () => ((seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648)
    const junk: string[] = ['', 'null', '[]', '{}', '"s"', '{"t":"s"}', '{"t":"k","a":1,"f":-1,"i":[]}', '{"t":"pick","l":99}', '{"t":"go","l":1.5}', '1e999']
    for (let i = 0; i < 300; i++) {
      const base = valid[Math.floor(rand() * valid.length)]!
      const at = Math.floor(rand() * base.length)
      const pick = rand()
      if (pick < 0.3) junk.push(base.slice(0, at))
      else if (pick < 0.6) junk.push(base.slice(0, at) + String.fromCharCode(32 + Math.floor(rand() * 90)) + base.slice(at + 1))
      else if (pick < 0.8) junk.push(base.replace(/-?\d+(\.\d+)?/g, (n) => (rand() < 0.2 ? ['1e999', '-1', '9999999', '0.5', '"x"', 'null', '[]'][Math.floor(rand() * 7)]! : n)))
      else junk.push(Array.from({ length: Math.floor(rand() * 60) }, () => String.fromCharCode(Math.floor(rand() * 128))).join(''))
    }
    for (const text of junk) {
      expect(() => g.host.receive(text, g.now())).not.toThrow()
      expect(() => g.guest.receive(text, g.now())).not.toThrow()
      expect(() => g.run(1)).not.toThrow()
    }
    g.run(500)
    const { hv, gv } = g.views()
    expect(hv.ended).toBe(null)
    expect(gv.screen).toBe('play')
    for (const v of [hv, gv]) for (const p of v.state!.players) expect(Number.isFinite(p.x) && Number.isFinite(p.y)).toBe(true)
  })
})

describe('online: select', () => {
  it('shares the highlight, and every level can be picked', () => {
    const g = pair()
    g.run(500)
    g.guest.pick(2)
    expect(g.guest.tick(g.now()).level).toBe(2)
    g.run(200)
    expect(g.views().hv.level).toBe(2)
    expect(g.views().gv.level).toBe(2)
    expect(LEVELS).toHaveLength(7)
    g.host.pick(1)
    g.run(200)
    expect(g.views().gv.level).toBe(1)
    g.host.pick(9)
    g.run(200)
    expect(g.views().gv.level).toBe(6)
  })

  it.each(['host', 'guest'] as const)('the %s can start the highlighted level', (who) => {
    const g = pair()
    g.startOn(1, who === 'host' ? g.host : g.guest)
    const { hv, gv } = g.views()
    expect(hv.screen).toBe('play')
    expect(gv.screen).toBe('play')
    expect(hv.level).toBe(1)
    expect(gv.level).toBe(1)
    expect(gv.compiled).not.toBe(null)
    expect(gv.state?.phase).toBe('play')
    expect(who === 'host' ? gv.note : hv.note).toMatch(who === 'host' ? /^ember / : /^frost /)
  })

  it('menu from either side takes both back to select', () => {
    const g = pair()
    g.startOn(1)
    g.guest.menu()
    g.run(300)
    expect(g.views().hv.screen).toBe('select')
    expect(g.views().gv.screen).toBe('select')
    expect(g.views().hv.note).toBe('frost went back to the levels')
    g.startOn(1)
    g.host.menu()
    g.run(300)
    expect(g.views().gv.screen).toBe('select')
    expect(g.views().gv.note).toBe('ember went back to the levels')
  })
})

describe('online: play', () => {
  it.each([0, 80])('host and guest agree after both move, at %ims lag', (lag) => {
    const g = pair({ lag })
    g.startOn(0)
    g.host.keys(0, IN_RIGHT)
    g.guest.keys(1, IN_LEFT | IN_UP)
    g.run(300)
    g.guest.keys(1, IN_LEFT)
    g.run(200)
    g.host.keys(0, IN_RIGHT | IN_UP)
    g.guest.keys(1, 0)
    g.run(250)
    g.host.keys(0, 0)
    g.run(1500)
    const { hv, gv } = g.views()
    for (const p of [0, 1] as const) {
      expect(pos(gv, p).x).toBeCloseTo(pos(hv, p).x, 0)
      expect(pos(gv, p).y).toBeCloseTo(pos(hv, p).y, 0)
    }
    // they really moved
    expect(pos(hv, 0).x).toBeGreaterThan(hv.compiled!.stages[0]!.spawns[0].x + 40)
    expect(pos(hv, 1).x).toBeLessThan(hv.compiled!.stages[0]!.spawns[1].x - 40)
  })

  it('the guest moves its own character at once under lag; the host sees it later', () => {
    const g = pair({ lag: 80 })
    g.startOn(0)
    g.run(1000)
    const x0 = pos(g.views().gv, 1).x
    const h0 = pos(g.views().hv, 1).x
    g.guest.keys(1, IN_RIGHT)
    // 50ms is less than the 80ms the keys take to reach the host. from rest the engine accelerates, so
    // frost covers about RUN_ACCEL·t²/2 by now; half of that is plenty to show it moved before the host knew
    g.run(50)
    expect(pos(g.views().gv, 1).x).toBeGreaterThan(x0 + (RUN_ACCEL * 0.05 ** 2) / 4)
    expect(pos(g.views().hv, 1).x).toBe(h0)
    g.run(400)
    expect(pos(g.views().hv, 1).x).toBeGreaterThan(h0 + 10)
  })

  it('the guest emits its own jump once and the host its own', () => {
    const g = pair({ lag: 80 })
    g.startOn(0)
    g.run(500)
    g.events.guest.length = 0
    g.events.host.length = 0
    g.guest.keys(1, IN_UP)
    g.run(100)
    g.guest.keys(1, 0)
    g.host.keys(0, IN_UP)
    g.run(100)
    g.host.keys(0, 0)
    g.run(1500)
    const jumps = (es: GameEvent[], p: number) => es.filter((e) => e.k === 'jump' && e.p === p).length
    expect(jumps(g.events.guest, 1)).toBe(1)
    expect(jumps(g.events.guest, 0)).toBe(1)
    expect(jumps(g.events.host, 1)).toBe(1)
    expect(jumps(g.events.host, 0)).toBe(1)
  })

  it('a death on the host shows on the guest, and the level comes back', () => {
    const g = pair({ lag: 40 })
    g.startOn(1)
    g.host.keys(0, IN_RIGHT)
    let sawDead = false
    for (let i = 0; i < 120 && !sawDead; i++) {
      g.run(FRAME)
      sawDead = g.views().gv.state?.phase === 'dead'
    }
    expect(sawDead).toBe(true)
    expect(g.events.guest.some((e) => e.k === 'die' && 'p' in e && e.p === 0)).toBe(true)
    g.host.keys(0, 0)
    g.run(3000)
    expect(g.views().gv.state?.phase).toBe('play')
    expect(g.views().gv.state?.deaths).toBeGreaterThanOrEqual(1)
    expect(g.views().gv.state?.deaths).toBe(g.views().hv.state?.deaths)
  })

  it('a guest that walks into a pit freezes until the host confirms, never dying alone', () => {
    // frost's own predicted death waits for the host; the host here never hears frost's keys
    const g = pair({ lag: 40, drop: (to, text) => to === 'host' && text.startsWith('{"t":"k"') })
    g.startOn(0)
    g.guest.keys(1, IN_LEFT)
    g.run(2000)
    // the host never moved frost, so after the guest's prediction is corrected frost is back near its spawn
    expect(g.views().gv.state?.phase).toBe('play')
    expect(Math.abs(pos(g.views().gv, 1).x - g.views().hv.compiled!.stages[0]!.spawns[1].x)).toBeLessThan(45)
  })

  it('win, then next from the guest, then next past the last level goes to select', () => {
    const g = pair({ lag: 30 })
    g.startOn(5)
    g.host.keys(0, IN_RIGHT)
    g.guest.keys(1, IN_RIGHT)
    g.run(1500)
    expect(g.views().hv.state?.phase).toBe('won')
    expect(g.views().gv.state?.phase).toBe('won')
    expect(g.events.guest.filter((e) => e.k === 'win')).toHaveLength(1)
    expect(g.events.host.filter((e) => e.k === 'win')).toHaveLength(1)
    // let go first: the next level is the same one step from the doors, so held keys would win it at once
    g.host.release()
    g.guest.release()
    g.guest.next()
    g.run(300)
    expect(g.views().hv.level).toBe(6)
    expect(g.views().gv.level).toBe(6)
    expect(g.views().gv.state?.phase).toBe('play')
    expect(g.views().hv.note).toBe('frost went on to the next level')
    g.host.keys(0, IN_RIGHT)
    g.guest.keys(1, IN_RIGHT)
    g.run(1500)
    expect(g.views().gv.state?.phase).toBe('won')
    g.host.next()
    g.run(300)
    expect(g.views().hv.screen).toBe('select')
    expect(g.views().gv.screen).toBe('select')
  })

  it('next does nothing before a win; retry from the guest takes both back', () => {
    const g = pair({ lag: 30 })
    g.startOn(0)
    g.host.keys(0, IN_RIGHT)
    g.run(500)
    g.host.keys(0, 0)
    g.guest.next()
    g.run(200)
    expect(g.views().hv.level).toBe(0)
    const moved = pos(g.views().hv, 0).x
    expect(moved).toBeGreaterThan(g.views().hv.compiled!.stages[0]!.spawns[0].x + 20)
    g.guest.retry()
    g.run(300)
    expect(g.views().hv.note).toBe('frost went back to the checkpoint')
    expect(pos(g.views().hv, 0).x).toBeCloseTo(g.views().hv.compiled!.stages[0]!.spawns[0].x, 0)
    expect(pos(g.views().gv, 0).x).toBeCloseTo(g.views().hv.compiled!.stages[0]!.spawns[0].x, 0)
  })
})

describe('online: message rates', () => {
  it.each([60, 120, 144])('the host sends about %i fps worth of nothing but STATE_HZ states a second', (fps) => {
    const g = pair({ fps })
    g.startOn(0)
    g.run(2000)
    const t = g.now()
    const states = g.sent.filter((m) => m.to === 'guest' && m.at > t - 1000 && m.text.startsWith('{"t":"s"'))
    expect(states.length).toBeGreaterThanOrEqual(STATE_HZ - 2)
    expect(states.length).toBeLessThanOrEqual(STATE_HZ + 1)
  })

  it('the guest sends only keepalives while its keys stay the same', () => {
    const g = pair()
    g.startOn(0)
    g.guest.keys(1, IN_RIGHT)
    g.run(100)
    const t = g.now()
    g.run(2000)
    const fromGuest = g.sent.filter((m) => m.to === 'host' && m.at > t)
    expect(fromGuest.length).toBeLessThanOrEqual(9)
    expect(fromGuest.length).toBeGreaterThanOrEqual(6)
    g.guest.keys(1, IN_LEFT)
    g.run(FRAME * 2)
    expect(g.sent.filter((m) => m.to === 'host' && m.at > g.now() - FRAME * 2.5)).toHaveLength(1)
  })

  it('state messages stay small', () => {
    const g = pair()
    g.startOn(0)
    g.host.keys(0, IN_RIGHT)
    g.run(500)
    const states = g.sent.filter((m) => m.text.startsWith('{"t":"s"'))
    for (const m of states) expect(m.text.length).toBeLessThan(700)
    expect(decodeEmber(states[states.length - 1]!.text)?.t).toBe('s')
  })

  it('a dropped key message heals from the next one', () => {
    let n = 0
    const g = pair({ lag: 20, drop: (to, text) => to === 'host' && text.startsWith('{"t":"k"') && text.includes('[') && n++ % 2 === 0 })
    g.startOn(0)
    g.guest.keys(1, IN_LEFT)
    g.run(300)
    g.guest.keys(1, 0)
    g.run(1500)
    expect(pos(g.views().hv, 1).x).toBeCloseTo(pos(g.views().gv, 1).x, 0)
    expect(pos(g.views().hv, 1).x).toBeLessThan(g.views().hv.compiled!.stages[0]!.spawns[1].x - 20)
  })
})

describe('local game', () => {
  const at = () => {
    let now = 0
    const game = createLocalGame(now)
    let v = game.tick(now)
    const events: GameEvent[] = []
    const run = (ms: number, fps = 60) => {
      const end = now + ms
      while (now < end - 1e-9) {
        now += 1000 / fps
        v = game.tick(now)
        events.push(...v.events)
      }
    }
    return { game, run, events, view: () => v, gap: (ms: number) => ((now += ms), (v = game.tick(now))) }
  }

  it('starts on select with every level open, and clamps picks to the list', () => {
    const l = at()
    expect(l.view().screen).toBe('select')
    expect(l.view().mode).toBe('local')
    expect(l.view().me).toBe(null)
    l.game.pick(3)
    expect(l.game.tick(1).level).toBe(3)
    l.game.pick(99)
    expect(l.game.tick(1).level).toBe(LEVELS.length - 1)
    l.game.pick(-4)
    expect(l.game.tick(2).level).toBe(0)
    expect(createLocalGame(0, 999).tick(0).level).toBe(LEVELS.length - 1)
    expect(createLocalGame(0, 2).tick(0).level).toBe(2)
  })

  it.each([60, 120, 144, 30])('steps once per 1/60s at %i fps, and cuts a long gap short', (fps) => {
    const l = at()
    l.game.play()
    l.run(1000, fps)
    expect(l.view().screen).toBe('play')
    expect(Math.abs(l.view().state!.tick - 60)).toBeLessThanOrEqual(1)
    expect(l.view().alpha).toBeGreaterThanOrEqual(0)
    expect(l.view().alpha).toBeLessThanOrEqual(1)
    const t = l.view().state!.tick
    l.gap(5000)
    expect(l.view().state!.tick - t).toBe(15)
    expect(l.view().prev!.tick).toBe(l.view().state!.tick - 1)
  })

  it('collects events from every step', () => {
    const l = at()
    l.game.pick(1)
    l.game.play()
    l.game.keys(0, IN_RIGHT)
    l.run(1000)
    expect(l.events.some((e) => e.k === 'die' && 'p' in e && e.p === 0)).toBe(true)
    expect(l.view().state!.phase).toBe('dead')
  })

  it('next after a win plays the next level; after the last it goes back to select', () => {
    const l = at()
    l.game.pick(5)
    l.game.play()
    l.game.next()
    expect(l.game.tick(0).level).toBe(5)
    l.game.keys(0, IN_RIGHT)
    l.game.keys(1, IN_RIGHT)
    l.run(1500)
    expect(l.view().state!.phase).toBe('won')
    expect(l.events.filter((e) => e.k === 'win')).toHaveLength(1)
    l.game.next()
    l.run(20)
    expect(l.view().level).toBe(6)
    expect(l.view().state!.phase).toBe('play')
    l.run(1500)
    expect(l.view().state!.phase).toBe('won')
    l.game.next()
    l.run(20)
    expect(l.view().screen).toBe('select')
    expect(l.view().state).toBe(null)
  })

  it('retry restarts the attempt; menu goes back to select', () => {
    const l = at()
    l.game.play()
    l.game.keys(0, IN_RIGHT)
    l.run(500)
    const s = l.view().compiled!.stages[0]!.spawns[0]
    expect(l.view().state!.players[0].x).toBeGreaterThan(s.x + 20)
    l.game.release()
    l.game.retry()
    l.run(20)
    expect(l.view().state!.players[0].x).toBeCloseTo(s.x, 0)
    l.game.menu()
    expect(l.game.tick(9999).screen).toBe('select')
  })
})

// rough networks: a relay hop each way, messages overtaking each other, and one state in ten lost
const NETS = [
  { name: 'a clean link', lag: 30 },
  { name: 'a relay at 80 ms', lag: 80 },
  { name: '120 ms with jitter and reordering', lag: 120, jitter: 60 },
  { name: '80 ms, jitter, and a state in ten lost', lag: 80, jitter: 40, loss: 0.1 },
]
const count = (es: GameEvent[], k: GameEvent['k']) => es.filter((e) => e.k === k).length
// the largest move of one drawn player between two frames
function tracker(p: 0 | 1) {
  let last: { x: number; y: number; stage: number; deaths: number } | null = null
  const t = { max: 0, blended: 0, frame: (_hv: EmberView, _gv: EmberView) => {} }
  t.frame = (_hv, gv) => {
    const s = gv.state
    if (!s || !gv.prev) return void (last = null)
    // nothing may be drawn between two stages or across a respawn
    if (gv.prev.stage !== s.stage || gv.prev.deaths !== s.deaths) t.blended++
    const a = s.players[p]
    if (last && last.stage === s.stage && last.deaths === s.deaths) t.max = Math.max(t.max, Math.hypot(a.x - last.x, a.y - last.y))
    last = { x: a.x, y: a.y, stage: s.stage, deaths: s.deaths }
  }
  return t
}

describe.each(NETS)('online: stages and checkpoints over $name', ({ name: _n, ...net }) => {
  // both through the first stage's doors and standing in the second
  const toStageTwo = (g: ReturnType<typeof pair>) => {
    g.startOn(2)
    g.host.keys(0, IN_RIGHT)
    g.guest.keys(1, IN_RIGHT)
    expect(g.until(() => g.views().hv.state?.phase === 'clear')).toBe(true)
    g.host.release()
    g.guest.release()
    expect(g.until(() => g.views().hv.state?.stage === 1 && g.views().gv.state?.stage === 1)).toBe(true)
    g.run(400)
  }
  // on to the flag, both standing still
  const toFlag = (g: ReturnType<typeof pair>) => {
    toStageTwo(g)
    g.host.keys(0, IN_RIGHT)
    g.guest.keys(1, IN_RIGHT)
    expect(g.until(() => g.views().hv.state?.cp === 0)).toBe(true)
    g.host.release()
    g.guest.release()
    g.run(600)
  }
  const agree = (g: ReturnType<typeof pair>) => {
    const { hv, gv } = g.views()
    expect(gv.state!.stage).toBe(hv.state!.stage)
    expect(gv.state!.phase).toBe(hv.state!.phase)
    expect(gv.state!.deaths).toBe(hv.state!.deaths)
    expect(gv.state!.cp).toBe(hv.state!.cp)
    for (const p of [0, 1] as const) {
      expect(pos(gv, p).x).toBeCloseTo(pos(hv, p).x, 0)
      expect(pos(gv, p).y).toBeCloseTo(pos(hv, p).y, 0)
    }
  }

  it('a cleared stage leads both into the next, with one clock', () => {
    const t = tracker(1)
    const g = pair({ ...net, frame: t.frame })
    toStageTwo(g)
    agree(g)
    const { hv, gv } = g.views()
    expect(gv.compiled).toBe(hv.compiled)
    expect(count(g.events.guest, 'clear')).toBe(1)
    expect(count(g.events.guest, 'stage')).toBe(1)
    expect(count(g.events.host, 'stage')).toBe(1)
    expect(t.blended).toBe(0)
    // the clock ran through the stage change and the guest's is the host's, a few frames ahead
    expect(hv.state!.tick).toBeGreaterThan(30)
    expect(gv.state!.tick - hv.state!.tick).toBeGreaterThanOrEqual(0)
    expect(gv.state!.tick - hv.state!.tick).toBeLessThanOrEqual(40)
    // frost stood still in the new stage: it never slid in from where the old stage left it
    expect(t.max).toBeLessThan(12)
  })

  it('a death goes back to the checkpoint on both sides', () => {
    const t = tracker(1)
    const g = pair({ ...net, frame: t.frame })
    toFlag(g)
    agree(g)
    expect(count(g.events.guest, 'checkpoint')).toBe(1)
    const at = [pos(g.views().hv, 0).x, pos(g.views().hv, 1).x]
    const spawn = g.views().hv.compiled!.stages[1]!.spawns
    expect(at[0]).toBeGreaterThan(spawn[0].x + 40)
    g.host.keys(0, IN_RIGHT)
    expect(g.until(() => g.views().gv.state?.phase === 'dead')).toBe(true)
    g.host.release()
    expect(g.until(() => g.views().hv.state?.deaths === 1 && g.views().gv.state?.deaths === 1)).toBe(true)
    g.run(600)
    agree(g)
    const { hv, gv } = g.views()
    expect(gv.state!.cp).toBe(0)
    expect(gv.state!.phase).toBe('play')
    // back where the flag was taken (they were still walking then, so a step short of where they stopped), not at the stage's start
    for (const p of [0, 1] as const) {
      expect(pos(gv, p).x).toBeGreaterThan(spawn[p].x + 40)
      expect(Math.abs(pos(gv, p).x - at[p]!)).toBeLessThan(40)
    }
    expect(g.events.guest.filter((e) => e.k === 'respawn')).toEqual([{ k: 'respawn', cp: 0 }])
    expect(count(g.events.guest, 'die')).toBe(count(g.events.host, 'die'))
    expect(t.blended).toBe(0)
    expect(t.max).toBeLessThan(12)
    // the clock didn't go back
    expect(gv.state!.tick).toBeGreaterThanOrEqual(hv.state!.tick)
  })

  it.each(['host', 'guest'] as const)('retry from the %s goes to the checkpoint, restart to the top', (who) => {
    const g = pair(net)
    toFlag(g)
    const me = who === 'host' ? g.host : g.guest
    const other = () => (who === 'host' ? g.views().gv : g.views().hv)
    const name = who === 'host' ? 'ember' : 'frost'
    const at = pos(g.views().hv, 1).x
    g.guest.keys(1, IN_LEFT)
    g.run(400)
    g.guest.release()
    g.run(300)
    expect(pos(g.views().hv, 1).x).toBeLessThan(at - 20)
    const clock = g.views().hv.state!.tick
    me.retry()
    g.run(700)
    agree(g)
    expect(other().note).toBe(`${name} went back to the checkpoint`)
    expect(g.views().gv.state!.stage).toBe(1)
    expect(g.views().gv.state!.deaths).toBe(1)
    expect(Math.abs(pos(g.views().gv, 1).x - at)).toBeLessThan(40)
    expect(g.views().hv.state!.tick).toBeGreaterThan(clock)
    expect(count(g.events.guest, 'respawn')).toBe(1)
    me.restart()
    g.run(700)
    agree(g)
    expect(other().note).toBe(`${name} restarted the level`)
    for (const v of [g.views().hv, g.views().gv]) {
      expect(v.state!.stage).toBe(0)
      expect(v.state!.deaths).toBe(0)
      expect(v.state!.cp).toBe(-1)
      expect(v.state!.tick).toBeLessThan(80)
    }
  })

  it('the win shows the same time and deaths on both sides', () => {
    const g = pair(net)
    g.startOn(5)
    g.guest.keys(1, IN_LEFT)
    g.run(300)
    g.host.keys(0, IN_RIGHT)
    g.guest.keys(1, IN_RIGHT)
    expect(g.until(() => g.views().gv.state?.phase === 'won')).toBe(true)
    g.run(300)
    expect(g.views().gv.state!.tick).toBe(g.views().hv.state!.tick)
    expect(g.views().gv.state!.deaths).toBe(g.views().hv.state!.deaths)
    expect(count(g.events.guest, 'win')).toBe(1)
  })

  it('a lamp press, an arrow jump, a key and a lock all agree', () => {
    const t = tracker(1)
    const g = pair({ ...net, frame: t.frame })
    g.startOn(4)
    g.run(400)
    g.events.guest.length = g.events.host.length = 0
    // ember walks to the lamp and presses it: dark on both screens
    g.host.keys(0, IN_RIGHT)
    g.run(350)
    g.host.keys(0, IN_DOWN)
    expect(g.until(() => g.views().hv.state!.lit === false && g.views().gv.state!.lit === false, 1500)).toBe(true)
    g.host.release()
    expect(g.events.guest.filter((e) => e.k === 'light')).toEqual(g.events.host.filter((e) => e.k === 'light').map((e) => ({ ...e, x: Math.round((e as { x: number }).x), y: Math.round((e as { y: number }).y) })))
    expect(count(g.events.guest, 'light')).toBe(1)
    // frost jumps into the arrow and spends it at once, on its own screen first
    g.guest.keys(1, IN_LEFT | IN_UP)
    expect(g.until(() => pos(g.views().gv, 1).air === 1, 1500)).toBe(true)
    g.guest.keys(1, IN_LEFT)
    g.run(34)
    g.guest.keys(1, IN_LEFT | IN_UP)
    expect(g.until(() => g.events.guest.some((e) => e.k === 'airjump' && e.p === 1), 200)).toBe(true)
    g.guest.keys(1, IN_LEFT)
    // on to the key and into the lock
    expect(g.until(() => g.views().hv.state!.locks[0] === true, 4000)).toBe(true)
    g.guest.release()
    g.run(1200)
    agree(g)
    const { hv, gv } = g.views()
    for (const k of ['lit', 'keys', 'keyTaken', 'locks', 'on'] as const) expect(gv.state![k]).toEqual(hv.state![k])
    expect(gv.state!.keyTaken).toEqual([true])
    expect(gv.state!.keys).toBe(0)
    expect(gv.state!.arrows.map((a) => a > 0)).toEqual(hv.state!.arrows.map((a) => a > 0))
    for (const k of ['airjump', 'arrow', 'key', 'unlock'] as const) {
      expect(count(g.events.host, k)).toBe(1)
      expect(count(g.events.guest, k)).toBe(1)
    }
    expect(t.max).toBeLessThan(16)
  })

  it('the guest bounces, lands on a one-way and runs a crumbling floor without a hitch', () => {
    const t = tracker(1)
    const seen = { host: [] as number[], guest: [] as number[] }
    const g = pair({
      ...net,
      frame: (hv, gv) => {
        t.frame(hv, gv)
        if (hv.state && gv.state) {
          seen.host.push(pos(hv, 1).y)
          seen.guest.push(pos(gv, 1).y)
        }
      },
    })
    g.startOn(3)
    g.run(600)
    seen.host.length = seen.guest.length = 0
    g.events.guest.length = g.events.host.length = 0
    g.guest.keys(1, IN_LEFT)
    expect(g.until(() => g.events.host.some((e) => e.k === 'bounce' && e.p === 1))).toBe(true)
    // on over the crumbling floor, which gives way behind and under frost
    g.guest.keys(1, IN_LEFT)
    expect(g.until(() => count(g.events.host, 'break') > 0 && g.views().hv.state!.phase === 'play' && pos(g.views().hv, 1).x < 150, 8000)).toBe(true)
    g.guest.release()
    g.run(1500)
    agree(g)
    // frost went up off the trampoline on both screens
    const floor = Math.max(...seen.host.slice(0, 5))
    expect(Math.min(...seen.host)).toBeLessThan(floor - 60)
    expect(Math.min(...seen.guest)).toBeCloseTo(Math.min(...seen.host), 0)
    // the guest heard its own bounces as they happened, once each
    expect(g.events.guest.filter((e) => e.k === 'bounce' && e.p === 1).length).toBe(g.events.host.filter((e) => e.k === 'bounce' && e.p === 1).length)
    expect(count(g.events.guest, 'break')).toBe(count(g.events.host, 'break'))
    // the same blocks are gone on both screens; the guest's timers are its few frames ahead
    const ahead = g.views().gv.state!.crumble.map((c, i) => c - g.views().hv.state!.crumble[i]!)
    expect(g.views().gv.state!.crumble.map((c) => c > 0)).toEqual(g.views().hv.state!.crumble.map((c) => c > 0))
    expect(Math.max(...ahead)).toBeLessThanOrEqual(40)
    expect(Math.min(...ahead)).toBeGreaterThanOrEqual(0)
    // never yanked: the most frost moved in a frame is a frame of its top speed
    expect(t.max).toBeLessThan(16)
    expect(t.blended).toBe(0)
  })
})

describe('online: the slow part', () => {
  const isState = (text: string) => text.startsWith('{"t":"s"')
  const hasSlow = (text: string) => isState(text) && text.includes('"k":{')

  it('goes with a state only when it changed, and on a key frame now and then', () => {
    const g = pair({ lag: 40 })
    g.startOn(0)
    g.run(1000)
    const t = g.now()
    g.host.keys(0, IN_RIGHT)
    g.guest.keys(1, IN_LEFT)
    g.run(4000)
    const states = g.sent.filter((m) => m.at > t && isState(m.text))
    const slow = states.filter((m) => hasSlow(m.text))
    expect(states.length).toBeGreaterThan(110)
    expect(slow.length).toBeGreaterThanOrEqual(2)
    expect(slow.length).toBeLessThanOrEqual(4)
    // players only: a state is a hundred-odd bytes
    for (const m of states.filter((m) => !hasSlow(m.text))) expect(m.text.length).toBeLessThan(170)
  })

  it('a guest that never got it asks, and the host sends it again', () => {
    // the first states of the level all lose their slow part on the way
    let lost = 0
    const g = pair({ lag: 40, drop: (to, text) => to === 'guest' && hasSlow(text) && lost++ < 2 })
    g.run(500)
    g.host.play()
    expect(g.until(() => g.views().gv.screen === 'play', 400)).toBe(true)
    expect(lost).toBeGreaterThanOrEqual(3)
    g.run(300)
    expect(g.views().gv.state!.phase).toBe('play')
    // asking costs a message or two, not a stream of them
    expect(g.sent.filter((m) => m.to === 'host' && m.text.startsWith('{"t":"k"')).length).toBeLessThan(12)
  })

  it('a change lost twice running still arrives', () => {
    let cut = false
    let lost = 0
    const g = pair({ lag: 40, drop: (to, text) => to === 'guest' && cut && hasSlow(text) && lost++ < 2 })
    g.startOn(2)
    g.host.keys(0, IN_RIGHT)
    g.guest.keys(1, IN_RIGHT)
    expect(g.until(() => g.views().hv.state?.phase === 'clear')).toBe(true)
    g.host.release()
    g.guest.release()
    expect(g.until(() => g.views().hv.state!.phaseT > 60)).toBe(true)
    // both messages that carry the new stage are lost
    cut = true
    expect(g.until(() => g.views().hv.state?.stage === 1)).toBe(true)
    expect(g.until(() => g.views().gv.state?.stage === 1, 400)).toBe(true)
    expect(lost).toBeGreaterThanOrEqual(2)
    g.run(300)
    expect(count(g.events.guest, 'stage')).toBe(1)
    expect(pos(g.views().gv, 1).x).toBeCloseTo(pos(g.views().hv, 1).x, 0)
  })

  it('one lost state loses no event', () => {
    // the state that carries the respawn never arrives
    let lost = 0
    const g = pair({ lag: 40, drop: (to, text) => to === 'guest' && isState(text) && /"e":\[[^\]]*\[33,/.test(text) && lost++ < 1 })
    g.startOn(1)
    g.host.keys(0, IN_RIGHT)
    expect(g.until(() => g.views().hv.state?.phase === 'dead')).toBe(true)
    g.host.release()
    expect(g.until(() => g.views().gv.state?.deaths === 1)).toBe(true)
    g.run(200)
    expect(lost).toBe(1)
    expect(g.events.guest.filter((e) => e.k === 'respawn')).toEqual([{ k: 'respawn', cp: -1 }])
  })
})

describe('local: stages and checkpoints', () => {
  const at = () => {
    let now = 0
    const game = createLocalGame(now)
    let v = game.tick(now)
    const events: GameEvent[] = []
    let blended = 0
    const run = (ms: number) => {
      const end = now + ms
      while (now < end - 1e-9) {
        now += FRAME
        v = game.tick(now)
        events.push(...v.events)
        if (v.state && v.prev && (v.prev.stage !== v.state.stage || v.prev.deaths !== v.state.deaths)) blended++
      }
    }
    const until = (done: () => boolean, ms = 5000) => {
      const end = now + ms
      while (!done() && now < end) run(FRAME)
      return done()
    }
    return { game, run, until, events, view: () => v, blended: () => blended }
  }

  it('plays stage after stage on one clock, back to the flag after a death, and restarts from the top', () => {
    const l = at()
    l.game.pick(2)
    l.game.play()
    l.run(FRAME)
    l.game.keys(0, IN_RIGHT)
    l.game.keys(1, IN_RIGHT)
    expect(l.until(() => l.view().state!.stage === 1)).toBe(true)
    expect(count(l.events, 'clear')).toBe(1)
    expect(count(l.events, 'stage')).toBe(1)
    const cleared = l.view().state!.tick
    expect(cleared).toBeGreaterThan(5)
    expect(l.until(() => l.view().state!.cp === 0)).toBe(true)
    l.game.release()
    l.run(300)
    const x = [l.view().state!.players[0].x, l.view().state!.players[1].x]
    l.game.keys(0, IN_RIGHT)
    expect(l.until(() => l.view().state!.phase === 'dead')).toBe(true)
    l.game.release()
    expect(l.until(() => l.view().state!.phase === 'play')).toBe(true)
    expect(l.view().state!.deaths).toBe(1)
    expect(l.view().state!.stage).toBe(1)
    expect(Math.abs(l.view().state!.players[0].x - x[0]!)).toBeLessThan(40)
    expect(l.view().state!.tick).toBeGreaterThan(cleared)
    // retry: the same again at once, and it counts
    l.game.keys(1, IN_LEFT)
    l.run(300)
    l.game.release()
    const clock = l.view().state!.tick
    l.game.retry()
    l.run(50)
    expect(l.view().state!.deaths).toBe(2)
    expect(Math.abs(l.view().state!.players[1].x - x[1]!)).toBeLessThan(40)
    expect(l.view().state!.tick).toBeGreaterThanOrEqual(clock)
    expect(count(l.events, 'respawn')).toBe(2)
    expect(l.blended()).toBe(0)
    l.game.restart()
    l.run(50)
    expect(l.view().state!.stage).toBe(0)
    expect(l.view().state!.deaths).toBe(0)
    expect(l.view().state!.tick).toBeLessThan(10)
    expect(l.view().level).toBe(2)
  })
})
