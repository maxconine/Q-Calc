import { describe, expect, it, vi } from 'vitest'
import { LEVELS } from './emberLevels'
import { decodeEmber, encodeEmber, STATE_HZ, SILENCE_MS } from './emberProtocol'
import { createLocalGame, createOnlineGame, type EmberGame, type EmberView } from './emberSession'
import { IN_LEFT, IN_RIGHT, IN_UP, RUN_ACCEL, type GameEvent } from './emberTypes'

// the session is tested on four small levels of its own: 0 open floor, 1 a water pit right of ember,
// 2 and 3 each player one step left of their door with a wall behind it
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
    fps?: number
    guestVersion?: number
    dropFirst?: number
    drop?: (to: 'host' | 'guest', text: string) => boolean
  } = {},
) {
  const lag = opts.lag ?? 5
  let now = 1000
  let dropped = 0
  const queue: Array<{ at: number; to: 'host' | 'guest'; text: string }> = []
  const sent: Array<{ at: number; to: 'host' | 'guest'; text: string }> = []
  const route = (to: 'host' | 'guest') => (text: string) => {
    sent.push({ at: now, to, text })
    if (dropped < (opts.dropFirst ?? 0)) {
      dropped++
      return
    }
    if (opts.drop?.(to, text)) return
    queue.push({ at: now + lag, to, text })
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
        const due = queue.filter((m) => m.at <= now)
        queue.splice(0, queue.length, ...queue.filter((m) => m.at > now))
        for (const m of due) (m.to === 'host' ? host : guest).receive(m.text, now)
      }
      hv = host.tick(now)
      gv = guest.tick(now)
      events.host.push(...hv.events)
      events.guest.push(...gv.events)
    }
  }
  // runs until both sides draw `level` in play, then a little more
  const startOn = (level: number, who: EmberGame = host) => {
    run(500)
    who.pick(level)
    run(200)
    who.play()
    run(500)
  }
  return { host, guest, run, sent, events, startOn, now: () => now, views: () => ({ hv, gv }) }
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
    g.host.pick(1)
    g.run(200)
    expect(g.views().gv.level).toBe(1)
    g.host.pick(9)
    g.run(200)
    expect(g.views().gv.level).toBe(3)
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
    g.startOn(2)
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
    expect(g.views().hv.level).toBe(3)
    expect(g.views().gv.level).toBe(3)
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
    l.game.pick(2)
    l.game.play()
    l.game.next()
    expect(l.game.tick(0).level).toBe(2)
    l.game.keys(0, IN_RIGHT)
    l.game.keys(1, IN_RIGHT)
    l.run(1500)
    expect(l.view().state!.phase).toBe('won')
    expect(l.events.filter((e) => e.k === 'win')).toHaveLength(1)
    l.game.next()
    l.run(20)
    expect(l.view().level).toBe(3)
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

describe('the wire', () => {
  it('keeps key messages tiny', () => {
    expect(encodeEmber({ t: 'k', a: 3, f: 120, i: [[118, 2]] })).toBe('{"t":"k","a":3,"f":120,"i":[[118,2]]}')
  })
})
