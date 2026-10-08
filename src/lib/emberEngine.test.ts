import { describe, expect, it } from 'vitest'
import { compileLevel, gateRect, initState, scoreOf, step, tileAt, traceBeams, validateLevel } from './emberEngine'
import {
  COLS,
  DEATH_TICKS,
  FREEZE_TICKS,
  IN_DOWN,
  IN_LEFT,
  IN_RIGHT,
  IN_UP,
  MELT_TICKS,
  PLAYER_H,
  PLAYER_W,
  PORTAL_COOLDOWN_TICKS,
  PUSH_SPEED,
  ROWS,
  RUN_SPEED,
  STEP_HZ,
  T_WALL,
  TILE,
  type GameEvent,
  type GameState,
  type Inputs,
  type Level,
  type LevelDef,
  type ThingDef,
} from './emberTypes'

// tiny levels: `rows` are the inner 30 columns of the bottom rows, sitting on `floor` (row 17).
// anything left out is empty, walled in at the sides and top
const row = (s: string) => '#' + s.padEnd(COLS - 2, '.').slice(0, COLS - 2) + '#'
function def(rows: string[], things: ThingDef[] = [], floor = '#'.repeat(COLS)): LevelDef {
  const map = ['#'.repeat(COLS), ...Array.from({ length: ROWS - 2 - rows.length }, () => row('')), ...rows.map(row), floor]
  return { id: 't', name: 't', par: 10, map, things }
}
const level = (rows: string[], things: ThingDef[] = [], floor?: string) => compileLevel(def(rows, things, floor))

// runs n ticks with the same inputs (or a function of the tick), collecting events
function run(L: Level, s: GameState, n: number, input: Inputs | ((t: number) => Inputs)): { s: GameState; ev: GameEvent[] } {
  const ev: GameEvent[] = []
  for (let t = 0; t < n; t++) {
    s = step(L, s, typeof input === 'function' ? input(t) : input)
    ev.push(...s.events)
  }
  return { s, ev }
}
const feet = (s: GameState, p: 0 | 1) => s.players[p].y + PLAYER_H
const R: Inputs = [IN_RIGHT, 0]

// fire stands at column 1 (tile x 2), frost and the doors far right, on floor row 17
const OPEN = ['.1........................R.2B']

describe('compile', () => {
  it('places spawns, doors and gems', () => {
    const L = level(['.1.r.b..R....B.............2..'])
    expect(L.spawns[0]).toEqual({ x: 2 * TILE + (TILE - PLAYER_W) / 2, y: 17 * TILE - PLAYER_H })
    expect(L.doors[0]).toEqual({ x: 9 * TILE, y: 15 * TILE, w: TILE, h: 2 * TILE })
    expect(L.gems).toEqual([
      { x: 4 * TILE + 10, y: 16 * TILE + 10, el: 'fire' },
      { x: 6 * TILE + 10, y: 16 * TILE + 10, el: 'frost' },
    ])
  })

  it('throws on malformed maps', () => {
    expect(() => compileLevel({ ...def(OPEN), map: def(OPEN).map.slice(1) })).toThrow(/rows/)
    expect(() => level(['.1.......R....B.............2.', '..x'])).toThrow(/unknown char 'x'/)
    expect(() => level(['.........R....B.............2.'])).toThrow(/missing fire spawn/)
    expect(() => level(['.1.1.....R....B.............2.'])).toThrow(/2 fire spawns/)
    expect(() => level(['.1.......R..................2.'])).toThrow(/frost door/)
  })

  it('converts things to px', () => {
    const L = level(OPEN, [
      { k: 'plate', x: 3, y: 16, ch: 'a' },
      { k: 'button', x: 5, y: 16, ch: 'b', secs: 2 },
      { k: 'fan', x: 6, y: 16, w: 1, h: 4 },
      { k: 'mover', x: 8, y: 10, w: 2, h: 1, to: { x: 8, y: 5 }, speed: 3 },
      { k: 'portal', x: 10, y: 16, pair: 'p' },
      { k: 'portal', x: 20, y: 16, pair: 'p' },
    ])
    expect(L.plates[0]).toEqual({ x: 60, y: 336, w: 40, h: 4, ch: 'a' })
    expect(L.buttons[0]).toEqual({ x: 100, y: 336, w: 20, h: 4, ch: 'b', ticks: 120 })
    expect(L.fans[0]).toEqual({ x: 120, y: 260, w: 20, h: 80 })
    expect(L.movers[0]).toMatchObject({ x: 160, y: 200, to: { x: 160, y: 100 }, speed: 60 })
    expect(L.portals.map((p) => [p.y, p.h, p.other])).toEqual([
      [300, 40, 1],
      [300, 40, 0],
    ])
  })

  it('validateLevel lists problems', () => {
    expect(validateLevel(def(OPEN))).toEqual([])
    const bad = def(['.1.......R....B.............2.'], [
      { k: 'portal', x: 4, y: 16, pair: 'solo' },
      { k: 'gate', x: 31, y: 3, w: 2, h: 1, open: ['a', '!b'] },
      { k: 'plate', x: 3, y: 16, ch: 'a' },
    ])
    const p = validateLevel(bad)
    expect(p.some((l) => /portal pair 'solo'/.test(l))).toBe(true)
    expect(p.some((l) => /out of bounds/.test(l))).toBe(true)
    expect(p.some((l) => /channel 'b'/.test(l))).toBe(true)
    expect(p.some((l) => /channel 'a'/.test(l))).toBe(false)
    expect(validateLevel({ ...def(OPEN), map: [...def(OPEN).map.slice(0, 17), '##'] }).some((l) => /row 17/.test(l))).toBe(true)
  })
})

describe('moving', () => {
  it('accelerates to run speed and stops', () => {
    const L = level(OPEN)
    let s = step(L, initState(L), R)
    expect(s.players[0].vx).toBeGreaterThan(0)
    expect(s.players[0].vx).toBeLessThan(RUN_SPEED)
    s = run(L, s, 20, R).s
    expect(s.players[0].vx).toBe(RUN_SPEED)
    expect(s.players[0].face).toBe(1)
    s = run(L, s, 20, [0, 0]).s
    expect(s.players[0].vx).toBe(0)
    expect(s.players[0].ground).toBe(true)
  })

  it('jumps about three tiles', () => {
    const L = level(OPEN)
    let s = initState(L)
    const y0 = s.players[0].y
    let top = y0
    const r = run(L, s, 60, (t) => (t < 40 ? [IN_UP, 0] : [0, 0]))
    expect(r.ev.filter((e) => e.k === 'jump').length).toBe(2) // held through the landing hops again
    s = initState(L)
    for (let t = 0; t < 40; t++) {
      s = step(L, s, [t < 30 ? IN_UP : 0, 0])
      top = Math.min(top, s.players[0].y)
    }
    expect(y0 - top).toBeGreaterThan(3 * TILE)
    expect(y0 - top).toBeLessThan(3.4 * TILE)
  })

  it('a short tap jumps lower', () => {
    const L = level(OPEN)
    let s = initState(L)
    let top = s.players[0].y
    for (let t = 0; t < 40; t++) {
      s = step(L, s, [t < 4 ? IN_UP : 0, 0])
      top = Math.min(top, s.players[0].y)
    }
    expect(initState(L).players[0].y - top).toBeLessThan(2 * TILE)
  })

  it('climbs three tiles but not four', () => {
    for (const [h, ok] of [
      [3, true],
      [4, false],
    ] as const) {
      // a wall h tiles tall right of fire; jump while pressed against it
      const rows = Array.from({ length: h - 1 }, () => '.....####.....................')
      rows.push('.1...####.................R.2B')
      const L = level(rows)
      let s = initState(L)
      let stood = false
      for (let t = 0; t < 120; t++) {
        s = step(L, s, [IN_RIGHT | (t > 30 && t < 60 ? IN_UP : 0), 0])
        if (s.players[0].ground && feet(s, 0) === (17 - h) * TILE) stood = true
      }
      expect(stood).toBe(ok)
      expect(s.players[0].x > 9 * TILE).toBe(ok)
    }
  })

  // a gap of n tiles in the floor over goo: for how many different jump ticks does a full jump at a run clear it
  function clears(n: number): number {
    const floor = '#'.repeat(12) + 'G'.repeat(n) + '#'.repeat(COLS - 12 - n)
    const L = level(['.1........................R.2B'], [], floor)
    let ways = 0
    for (let jumpAt = 0; jumpAt < 120; jumpAt++) {
      let s = initState(L)
      for (let t = 0; t < 200 && s.phase === 'play'; t++) s = step(L, s, [IN_RIGHT | (t >= jumpAt && t < jumpAt + 30 ? IN_UP : 0), 0])
      if (s.phase === 'play' && s.players[0].x > (12 + n) * TILE) ways++
    }
    return ways
  }

  it('clears a 3 tile gap at a run easily, 5 only just, 6 never', () => {
    expect(clears(3)).toBeGreaterThanOrEqual(15)
    expect(clears(5)).toBeGreaterThan(0)
    expect(clears(5)).toBeLessThanOrEqual(6)
    expect(clears(6)).toBe(0)
  })

  it('coyote time and jump buffering', () => {
    // walk off a ledge then press jump a few ticks later
    const L = level(['.1.......', '#####....', '#####........................2', '#####.....................R..B.'])
    let s = initState(L)
    let off = -1
    for (let t = 0; t < 60 && off < 0; t++) {
      s = step(L, s, R)
      if (!s.players[0].ground) off = t
    }
    expect(off).toBeGreaterThan(0)
    s = step(L, s, R)
    s = step(L, s, R)
    s = step(L, s, [IN_RIGHT | IN_UP, 0])
    expect(s.events.some((e) => e.k === 'jump')).toBe(true)
    // buffer: a press a few ticks before landing jumps on touchdown
    const L2 = level(OPEN)
    s = step(L2, initState(L2), [IN_UP, 0])
    while (!(s.players[0].vy > 0 && feet(s, 0) > 17 * TILE - 12)) s = step(L2, s, [0, 0])
    s = step(L2, s, [IN_UP, 0])
    expect(s.players[0].ground).toBe(false)
    const r = run(L2, s, 6, [0, 0])
    expect(r.ev.filter((e) => e.k === 'jump').length).toBe(1)
    // a press long before landing is forgotten
    s = step(L2, initState(L2), [IN_UP, 0])
    s = run(L2, s, 10, [0, 0]).s
    s = step(L2, s, [IN_UP, 0])
    expect(run(L2, s, 40, [0, 0]).ev.filter((e) => e.k === 'jump').length).toBe(0)
  })

  it('walls block', () => {
    const L = level(['.1...#......................2B', '.....#.....................R..'])
    const s = run(L, initState(L), 60, R).s
    expect(s.players[0].x).toBe(6 * TILE - PLAYER_W)
    expect(run(L, s, 1, [0, 0]).s.players[0].vx).toBe(0)
  })
})

describe('pools and death', () => {
  const pool = (c: string) => compileLevel(def(['.1...........................2', '.....................R..B.....'], [], '####' + c + c + '#'.repeat(26)))

  it('each pool kills the right element only', () => {
    for (const [c, fireDies, cause] of [
      ['L', false, 'lava'],
      ['W', true, 'water'],
      ['G', true, 'goo'],
      ['w', true, 'water'],
    ] as const) {
      const L = pool(c)
      const r = run(L, initState(L), 40, R)
      expect(r.s.phase === 'dead', c).toBe(fireDies)
      if (fireDies) expect(r.ev.find((e) => e.k === 'die')).toMatchObject({ p: 0, cause })
    }
    // frost walks left into the pool
    for (const [c, frostDies] of [
      ['L', true],
      ['W', false],
      ['G', true],
    ] as const) {
      const L = compileLevel(def(['.......2.....................1', '.....................R..B.....'], [], '####' + c + c + '#'.repeat(26)))
      const r = run(L, initState(L), 50, [0, IN_LEFT])
      expect(r.s.phase === 'dead', c).toBe(frostDies)
    }
  })

  it('fire wades through lava and climbs out the other side', () => {
    const L = pool('L')
    const s = run(L, initState(L), 80, R).s
    expect(s.phase).toBe('play')
    expect(s.players[0].x).toBeGreaterThan(8 * TILE)
    expect(feet(s, 0)).toBe(17 * TILE)
  })

  it('restarts after DEATH_TICKS with one more death', () => {
    const L = pool('W')
    let s = initState(L)
    while (s.phase === 'play') s = step(L, s, R)
    const t = s.tick
    s = run(L, s, DEATH_TICKS - 1, R).s
    expect(s.phase).toBe('dead')
    expect(s.tick).toBe(t)
    s = step(L, s, R)
    expect(s).toEqual(initState(L, 1))
  })
})

describe('gems and doors', () => {
  it('only the matching element collects a gem', () => {
    const L = level(['.1.b.r....................R.2B'])
    const r = run(L, initState(L), 40, R)
    expect(r.s.taken).toEqual([false, true])
    expect(r.s.players[0].gems).toBe(1)
    expect(r.ev.filter((e) => e.k === 'gem')).toEqual([{ k: 'gem', p: 0, x: 6 * TILE + 10, y: 16 * TILE + 10, el: 'fire' }])
  })

  it('needs both players in their doors to win, then scores', () => {
    const L = level(['.1R#...................#2B...r'])
    let r = run(L, initState(L), 30, [IN_RIGHT, 0])
    expect(r.s.players[0].inDoor).toBe(true)
    expect(r.s.phase).toBe('play')
    expect(r.ev.filter((e) => e.k === 'door').length).toBe(1)
    r = run(L, r.s, 30, [IN_RIGHT, IN_RIGHT])
    expect(r.s.phase).toBe('won')
    expect(r.ev.at(-1)).toEqual({ k: 'win' })
    const tick = r.s.tick
    const s = run(L, r.s, 10, [IN_LEFT, IN_LEFT]).s
    expect(s.tick).toBe(tick)
    expect(s.phaseT).toBe(10 + r.s.phaseT)
    expect(scoreOf(L, s)).toEqual({ secs: tick / STEP_HZ, gems: [0, 0], total: [1, 0], stars: 2 })
    expect(scoreOf(L, initState(L)).stars).toBe(0)
  })
})

describe('switches, gates, movers', () => {
  it('a plate opens a gate, which closes on release', () => {
    // fire walks onto the plate; the gate stands further right
    const L = level(OPEN, [
      { k: 'plate', x: 4, y: 16, ch: 'p' },
      { k: 'gate', x: 15, y: 14, w: 1, h: 3, open: 'p' },
    ])
    let r = run(L, initState(L), 18, R)
    expect(r.s.on).toEqual(['p'])
    expect(r.ev.find((e) => e.k === 'plate')).toMatchObject({ i: 0, on: true })
    r = run(L, r.s, 30, [0, 0])
    expect(r.s.gates[0]).toBe(1)
    expect(gateRect(L, r.s, 0).y).toBe(14 * TILE - 3 * TILE)
    r = run(L, r.s, 60, R)
    expect(r.s.on).toEqual([])
    expect(r.ev.find((e) => e.k === 'plate')).toMatchObject({ on: false })
    r = run(L, r.s, 30, [0, 0])
    expect(r.s.gates[0]).toBe(0)
  })

  it('a closing gate never crushes', () => {
    // frost starts under a gate that is open while fire is on the plate; fire steps off
    const L = level(['.1.........2..............R..B'], [
      { k: 'plate', x: 1, y: 16, ch: 'p' },
      { k: 'gate', x: 12, y: 13, w: 1, h: 3, open: 'p', dir: 'up' },
    ])
    let s = initState(L)
    s = run(L, s, 40, [IN_LEFT, 0]).s // fire stays on the plate (wall at the left)
    expect(s.gates[0]).toBe(1)
    s = run(L, s, 60, [IN_RIGHT, 0]).s
    s = run(L, s, 60, [0, 0]).s
    expect(s.gates[0]).toBeGreaterThan(0)
    const g = gateRect(L, s, 0)
    expect(g.y + g.h).toBeLessThanOrEqual(s.players[1].y + 0.01)
    expect(g.y + g.h).toBeGreaterThan(s.players[1].y - 1)
    // frost steps out and it shuts
    s = run(L, s, 60, [0, IN_RIGHT]).s
    expect(s.gates[0]).toBe(0)
  })

  it('a lever flips on the down key', () => {
    const L = level(OPEN, [
      { k: 'lever', x: 2, y: 16, ch: 'l' },
      { k: 'gate', x: 15, y: 14, w: 1, h: 3, open: '!l' },
    ])
    let s = initState(L)
    expect(s.gates[0]).toBe(1)
    s = step(L, s, [IN_DOWN, 0])
    expect(s.events).toContainEqual({ k: 'lever', i: 0, x: 50, y: 330 })
    expect(s.levers[0]).toBe(true)
    s = step(L, s, [IN_DOWN, 0]) // held: no flip
    expect(s.levers[0]).toBe(true)
    expect(s.on).toEqual(['l'])
  })

  it('a timed button stays on for its secs', () => {
    const L = level(['.1........................R.2B'], [{ k: 'button', x: 2, y: 16, ch: 'b', secs: 1 }])
    let r = run(L, initState(L), 2, [0, 0])
    expect(r.s.on).toEqual(['b'])
    r = run(L, r.s, 30, R)
    expect(r.s.on).toEqual(['b'])
    const offAt = run(L, r.s, 120, R).ev.findIndex((e) => e.k === 'button' && !e.on)
    expect(offAt).toBeGreaterThanOrEqual(0)
    r = run(L, r.s, STEP_HZ + 10, R)
    expect(r.s.on).toEqual([])
  })

  it('a mover carries its rider up and comes back down', () => {
    // a lift under fire, raised by frost's lever
    const L = level(['.....', '.....', '..........................2...', '.1.........................RB.'], [
      { k: 'mover', x: 1, y: 17, w: 3, h: 1, to: { x: 1, y: 13 } },
    ], '#'.repeat(COLS))
    // the mover starts embedded in the floor row; the player stands on it
    let s = initState(L)
    expect(feet(s, 0)).toBe(17 * TILE)
    const mv: number[] = []
    for (let t = 0; t < 180; t++) {
      s = step(L, s, [0, 0])
      mv.push(s.movers[0]!.y)
      expect(Math.abs(feet(s, 0) - s.movers[0]!.y)).toBeLessThan(1)
    }
    expect(Math.min(...mv)).toBe(13 * TILE)
    expect(mv.at(-1)).toBeGreaterThan(13 * TILE)
    expect(s.movers[0]!.dir).toBe(-1)
  })

  it('a lever-driven mover waits rather than crush a player below', () => {
    const L = level(['.12.......................R..B'], [
      { k: 'lever', x: 3, y: 16, ch: 'down' },
      { k: 'mover', x: 2, y: 12, w: 1, h: 1, to: { x: 2, y: 16 }, when: 'down' },
    ])
    let s = run(L, initState(L), 10, [0, 0]).s
    // frost pulls the lever; the mover comes down onto fire and stops above fire's head
    s = step(L, s, [0, IN_DOWN])
    s = run(L, s, 120, [0, 0]).s
    expect(s.movers[0]!.y + TILE).toBeLessThanOrEqual(s.players[0].y + 0.01)
    expect(s.movers[0]!.t).toBeGreaterThan(0.5)
    expect(s.movers[0]!.t).toBeLessThan(1)
  })
})

describe('boxes', () => {
  it('a player pushes a box at push speed', () => {
    const L = level(OPEN, [{ k: 'box', x: 5, y: 16 }])
    const r = run(L, initState(L), 90, R)
    expect(r.s.boxes[0]!.x).toBeGreaterThan(5 * TILE + 20)
    expect(r.s.players[0].x).toBeCloseTo(r.s.boxes[0]!.x - PLAYER_W)
    expect(r.s.players[0].vx).toBeLessThanOrEqual(PUSH_SPEED)
    expect(r.ev.filter((e) => e.k === 'push').length).toBe(1)
  })

  it('a box against a wall blocks', () => {
    const L = level(['.1...#....................R.2B'], [{ k: 'box', x: 4, y: 16 }])
    const s = run(L, initState(L), 90, R).s
    expect(s.boxes[0]!.x).toBe(5 * TILE)
    expect(s.players[0].x).toBe(5 * TILE - PLAYER_W)
  })

  it('a box falls onto a plate and holds it; players stand on boxes', () => {
    const L = level(OPEN, [
      { k: 'box', x: 5, y: 10 },
      { k: 'plate', x: 5, y: 16, w: 1, ch: 'p' },
    ])
    let s = run(L, initState(L), 60, [0, 0]).s
    expect(s.boxes[0]!.y).toBe(16 * TILE)
    expect(s.on).toEqual(['p'])
    // fire hops onto the box
    for (let t = 0; t < 40; t++) s = step(L, s, [(s.players[0].x < 5 * TILE ? IN_RIGHT : 0) | (t < 25 ? IN_UP : 0), 0])
    s = run(L, s, 30, [0, 0]).s
    expect(feet(s, 0)).toBe(16 * TILE)
    expect(s.players[0].ground).toBe(true)
  })

  it('boxes stack', () => {
    const L = level(OPEN, [
      { k: 'box', x: 8, y: 16 },
      { k: 'box', x: 8, y: 10 },
    ])
    const s = run(L, initState(L), 60, [0, 0]).s
    expect(s.boxes.map((b) => b.y)).toEqual([16 * TILE, 15 * TILE])
  })
})

describe('fans, ice and thin water', () => {
  it('a fan lifts a player to hover near the top of its column', () => {
    const L = level(['....1.....................R.2B'], [{ k: 'fan', x: 5, y: 16, w: 1, h: 6 }])
    const ys: number[] = []
    let s = initState(L)
    for (let t = 0; t < 180; t++) {
      s = step(L, s, [0, 0])
      if (t > 90) ys.push(feet(s, 0))
    }
    expect(Math.min(...ys)).toBeGreaterThan(11 * TILE - 40)
    expect(Math.max(...ys)).toBeLessThan(11 * TILE + 50)
    expect(s.players[0].ground).toBe(false)
  })

  it('a fan only runs while its channel is on', () => {
    const L = level(['.1..2.....................R..B'], [
      { k: 'fan', x: 5, y: 16, w: 1, h: 6, when: 'l' },
      { k: 'lever', x: 2, y: 16, ch: 'l' },
    ])
    let s = run(L, initState(L), 30, [0, 0]).s
    expect(s.players[1].ground).toBe(true)
    s = step(L, s, [IN_DOWN, 0])
    s = run(L, s, 30, [0, 0]).s
    expect(feet(s, 1)).toBeLessThan(15 * TILE)
  })

  it('ice melts for fire only', () => {
    const ice: ThingDef[] = [{ k: 'ice', x: 5, y: 15, w: 1, h: 2 }]
    let L = level(['.1........................R.2B'], ice)
    let r = run(L, initState(L), 30 + MELT_TICKS, R)
    expect(r.s.ice[0]).toBe(1)
    expect(r.ev.filter((e) => e.k === 'melt')).toEqual([{ k: 'melt', i: 0, x: 110, y: 320 }])
    r = run(L, r.s, 30, R)
    expect(r.s.players[0].x).toBeGreaterThan(6 * TILE)
    L = level(['.2........................R.1B'], ice)
    r = run(L, initState(L), 200, [0, IN_RIGHT])
    expect(r.s.ice[0]).toBe(0)
    expect(r.s.players[1].x).toBe(5 * TILE - PLAYER_W)
  })

  it('thin water freezes near frost, carries fire, and thaws later', () => {
    const floor = '#'.repeat(6) + 'www' + '#'.repeat(COLS - 9)
    const L = level(['.12.......................R..B'], [], floor)
    expect(tileAt(L, initState(L), 7, 17)).toBe(5)
    // both walk right; frost leads, fire stops on the middle tile
    const ev: GameEvent[] = []
    let s = initState(L)
    for (let t = 0; t < 50; t++) {
      s = step(L, s, [s.players[0].x < 7 * TILE ? IN_RIGHT : 0, IN_RIGHT])
      ev.push(...s.events)
    }
    expect(ev.filter((e) => e.k === 'freeze').length).toBe(3)
    expect(tileAt(L, s, 7, 17)).toBe(T_WALL)
    expect(s.phase).toBe('play')
    // frost walks on away
    const r = run(L, s, FREEZE_TICKS + 60, [0, IN_RIGHT])
    expect(r.ev.some((e) => e.k === 'thaw')).toBe(true)
    expect(r.ev.find((e) => e.k === 'die')).toMatchObject({ p: 0, cause: 'water' })
  })
})

describe('light and portals', () => {
  // emitter at tile (3,16) shining right; a mirror at (10,16) turns it up to a sensor at (10,10)
  const light = (more: ThingDef[] = []) =>
    level(['.1........................R.2B'], [
      { k: 'emitter', x: 3, y: 16, dir: 'right' },
      { k: 'mirror', x: 10, y: 16, slant: '/' },
      { k: 'sensor', x: 10, y: 10, ch: 's' },
      { k: 'gate', x: 20, y: 14, w: 1, h: 3, open: 's' },
      ...more,
    ])

  it('a mirror sends the beam to a sensor', () => {
    const L = light()
    const s0 = initState(L)
    expect(s0.on).toEqual(['s'])
    expect(s0.gates[0]).toBe(1)
    expect(traceBeams(L, s0)).toEqual([
      { x1: 70, y1: 330, x2: 210, y2: 330 },
      { x1: 210, y1: 330, x2: 210, y2: 210 },
    ])
  })

  it('a box blocks the beam', () => {
    const L = light([{ k: 'box', x: 6, y: 16 }])
    const s = initState(L)
    expect(s.on).toEqual([])
    expect(traceBeams(L, s)).toEqual([{ x1: 70, y1: 330, x2: 120, y2: 330 }])
  })

  it('a player blocks the beam, and the down key turns the mirror', () => {
    const L = light()
    let r = run(L, initState(L), 20, R)
    expect(r.ev).toContainEqual({ k: 'sensor', i: 0, on: false, x: 210, y: 210 })
    let s = r.s
    while (s.players[0].x < 10 * TILE) s = step(L, s, R)
    s = step(L, s, [IN_DOWN, 0])
    expect(s.mirrors).toEqual(['\\'])
    expect(s.events).toContainEqual({ k: 'mirror', i: 0, x: 210, y: 330 })
    r = run(L, s, 40, R)
    expect(r.s.on).toEqual([])
    expect(traceBeams(L, r.s).at(-1)).toEqual({ x1: 210, y1: 330, x2: 210, y2: 340 })
  })

  it('portals teleport with a cooldown', () => {
    const L = level(['.1........................R.2B'], [
      { k: 'portal', x: 6, y: 16, pair: 'a' },
      { k: 'portal', x: 15, y: 16, pair: 'a' },
    ])
    let r = run(L, initState(L), 40, R)
    expect(r.ev.filter((e) => e.k === 'portal').length).toBe(1)
    expect(r.s.players[0].x).toBeGreaterThan(15 * TILE)
    expect(r.s.players[0].vx).toBe(RUN_SPEED)
    // the cooldown holds while still in the arrival portal; once it's over the way back works
    let s = r.s
    while (s.players[0].portalCd > 0) s = step(L, s, [0, 0])
    r = run(L, s, 40, [IN_LEFT, 0])
    expect(r.ev.filter((e) => e.k === 'portal').length).toBe(1)
    expect(r.s.players[0].x).toBeLessThan(6 * TILE)
    s = initState(L)
    while (!s.events.some((e) => e.k === 'portal')) s = step(L, s, R)
    expect(s.players[0].portalCd).toBe(PORTAL_COOLDOWN_TICKS)
  })
})

describe('determinism and purity', () => {
  const busy = () =>
    level(['....b..r', '..12...........w..........R..B'], [
      { k: 'box', x: 6, y: 16 },
      { k: 'plate', x: 9, y: 16, ch: 'p' },
      { k: 'gate', x: 12, y: 14, w: 1, h: 3, open: 'p' },
      { k: 'mover', x: 18, y: 12, w: 2, h: 1, to: { x: 22, y: 12 } },
      { k: 'fan', x: 24, y: 16, w: 1, h: 3 },
      { k: 'emitter', x: 1, y: 10, dir: 'right' },
      { k: 'sensor', x: 30, y: 10, ch: 's' },
      { k: 'ice', x: 14, y: 16, w: 1, h: 1 },
    ])
  const script = (t: number): Inputs => [
    (t % 50 < 30 ? IN_RIGHT : IN_LEFT) | (t % 37 === 0 ? IN_UP : 0),
    (t % 70 < 40 ? IN_RIGHT : 0) | (t % 23 === 0 ? IN_UP : 0) | (t % 90 === 5 ? IN_DOWN : 0),
  ]

  it('the same inputs give the same states, and step never mutates its input', () => {
    const L = busy()
    const a: GameState[] = []
    let s = initState(L)
    for (let t = 0; t < 400; t++) {
      const before = JSON.stringify(s)
      const n = step(L, s, script(t))
      expect(JSON.stringify(s)).toBe(before)
      s = n
      a.push(s)
    }
    s = initState(L)
    for (let t = 0; t < 400; t++) {
      s = step(L, s, script(t))
      expect(s).toEqual(a[t])
    }
  })

  it('states survive a JSON round trip and step the same afterwards', () => {
    const L = busy()
    let s = initState(L)
    for (let t = 0; t < 200; t++) {
      const j = JSON.parse(JSON.stringify(s)) as GameState
      expect(j).toEqual(s)
      const n = step(L, s, script(t))
      expect(step(L, j, script(t))).toEqual(n)
      s = n
    }
  })
})
