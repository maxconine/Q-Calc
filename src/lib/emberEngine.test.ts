import { describe, expect, it } from 'vitest'
import { compileLevel, gateRect, initState, respawn, scoreOf, step, tileAt, traceBeams, validateLevel } from './emberEngine'
import {
  ARROW_RESPAWN_TICKS,
  CRUMBLE_GONE_TICKS,
  CRUMBLE_TICKS,
  DEATH_TICKS,
  FREEZE_TICKS,
  IN_DOWN,
  IN_LEFT,
  IN_RIGHT,
  IN_UP,
  JUMP_V,
  MELT_TICKS,
  PLAYER_H,
  PLAYER_W,
  PORTAL_COOLDOWN_TICKS,
  PUSH_SPEED,
  RUN_SPEED,
  STAGE_TICKS,
  STEP_HZ,
  T_CRUMBLE,
  T_WALL,
  TILE,
  TRAMP_MAX_V,
  TRAMP_MIN_V,
  VIEW_COLS as COLS,
  VIEW_ROWS as ROWS,
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
  return { id: 't', name: 't', par: 10, stages: [{ map, things }] }
}
// the same level with another map
const remap = (d: LevelDef, f: (map: string[]) => string[]): LevelDef => ({ ...d, stages: [{ ...d.stages[0]!, map: f(d.stages[0]!.map) }] })
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
    expect(L.stages[0]!.spawns[0]).toEqual({ x: 2 * TILE + (TILE - PLAYER_W) / 2, y: 17 * TILE - PLAYER_H })
    expect(L.stages[0]!.doors[0]).toEqual({ x: 9 * TILE, y: 15 * TILE, w: TILE, h: 2 * TILE })
    expect(L.stages[0]!.gems).toEqual([
      { x: 4 * TILE + 10, y: 16 * TILE + 10, el: 'fire' },
      { x: 6 * TILE + 10, y: 16 * TILE + 10, el: 'frost' },
    ])
  })

  it('throws on malformed maps', () => {
    expect(() => compileLevel(remap(def(OPEN), (m) => m.slice(1)))).toThrow(/rows/)
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
    expect(L.stages[0]!.plates[0]).toEqual({ x: 60, y: 336, w: 40, h: 4, ch: 'a', ceil: false })
    expect(L.stages[0]!.buttons[0]).toEqual({ x: 100, y: 336, w: 20, h: 4, ch: 'b', ticks: 120, ceil: false })
    expect(L.stages[0]!.fans[0]).toEqual({ x: 120, y: 260, w: 20, h: 80 })
    expect(L.stages[0]!.movers[0]).toMatchObject({ x: 160, y: 200, to: { x: 160, y: 100 }, speed: 60 })
    expect(L.stages[0]!.portals.map((p) => [p.y, p.h, p.other])).toEqual([
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
    expect(validateLevel(remap(def(OPEN), (m) => [...m.slice(0, 17), '##'])).some((l) => /row 17/.test(l))).toBe(true)
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
    // back at the stage's start, the clock still running
    expect(s).toEqual({ ...initState(L, 1), tick: t, events: [{ k: 'respawn', cp: -1 }] })
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
    expect(gateRect(L.stages[0]!, r.s, 0).y).toBe(14 * TILE - 3 * TILE)
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
    const g = gateRect(L.stages[0]!, s, 0)
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
    expect(tileAt(L.stages[0]!, initState(L), 7, 17)).toBe(5)
    // both walk right; frost leads, fire stops on the middle tile
    const ev: GameEvent[] = []
    let s = initState(L)
    for (let t = 0; t < 50; t++) {
      s = step(L, s, [s.players[0].x < 7 * TILE ? IN_RIGHT : 0, IN_RIGHT])
      ev.push(...s.events)
    }
    expect(ev.filter((e) => e.k === 'freeze').length).toBe(3)
    expect(tileAt(L.stages[0]!, s, 7, 17)).toBe(T_WALL)
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
    expect(traceBeams(L.stages[0]!, s0)).toEqual([
      { x1: 70, y1: 330, x2: 210, y2: 330 },
      { x1: 210, y1: 330, x2: 210, y2: 210 },
    ])
  })

  it('a box blocks the beam', () => {
    const L = light([{ k: 'box', x: 6, y: 16 }])
    const s = initState(L)
    expect(s.on).toEqual([])
    expect(traceBeams(L.stages[0]!, s)).toEqual([{ x1: 70, y1: 330, x2: 120, y2: 330 }])
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
    expect(traceBeams(L.stages[0]!, r.s).at(-1)).toEqual({ x1: 210, y1: 330, x2: 210, y2: 340 })
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

// ---- ember 2: the foundations

// a walled room of any size from its inner rows (padded with air above), for maps bigger than a screen
function room(cols: number, rows: number, bottom: string[], things: ThingDef[] = []): LevelDef['stages'][number] {
  const pad = (r: string) => '#' + r.padEnd(cols - 2, '.').slice(0, cols - 2) + '#'
  const map = ['#'.repeat(cols), ...Array.from({ length: rows - 2 - bottom.length }, () => pad('')), ...bottom.map(pad), '#'.repeat(cols)]
  return { map, things }
}
const levelOf = (...stages: LevelDef['stages']) => compileLevel({ id: 't', name: 't', par: 10, stages })

describe('big maps', () => {
  it('compiles a stage bigger than a screen and walls its edges', () => {
    const L = levelOf(room(80, 40, ['1.2....................................................................RB']))
    const st = L.stages[0]!
    expect([st.cols, st.rows, st.w, st.h]).toEqual([80, 40, 1600, 800])
    expect(st.tiles.length).toBe(80 * 40)
    expect(st.doors[0].x).toBe(72 * TILE)
    expect(tileAt(st, initState(L), 80, 5)).toBe(T_WALL)
    expect(tileAt(st, initState(L), 79, 39)).toBe(T_WALL)
    // far past where a 32-wide world ended
    const r = run(L, initState(L), 400, R)
    expect(r.s.players[0].x).toBeGreaterThan(50 * TILE)
  })

  it('refuses maps smaller than a screen or bigger than four', () => {
    expect(validateLevel({ id: 't', name: 't', par: 1, stages: [room(31, 18, ['12RB'])] }).join()).toMatch(/wide/)
    expect(validateLevel({ id: 't', name: 't', par: 1, stages: [room(32, 73, ['12RB'])] }).join()).toMatch(/rows/)
    expect(validateLevel({ id: 't', name: 't', par: 1, stages: [] }).join()).toMatch(/stages/)
  })

  it('compiles every version 2 glyph and thing', () => {
    const L = levelOf(
      room(40, 20, ['.^.k.C.......', '1.2.T=~OD..RB'], [
        { k: 'lock', x: 20, y: 17, w: 1, h: 2 },
        { k: 'lamp', x: 22, y: 18, secs: 2 },
        { k: 'gswitch', x: 23, y: 18, who: 'other' },
        { k: 'cannon', x: 24, y: 18, aim: 'upright', power: 20, fire: 'lights' },
        { k: 'checkpoint', x: 26, y: 18, zone: { x: 25, y: 10, w: 4, h: 9 } },
        { k: 'dark', x: 30, y: 1, w: 5, h: 5 },
        { k: 'plate', x: 28, y: 1, ch: 'a', ceil: true },
        { k: 'gate', x: 36, y: 16, w: 1, h: 3, open: 'a' },
      ]),
    )
    const st = L.stages[0]!
    expect(st.tiles.slice(18 * 40 + 5, 18 * 40 + 10)).toEqual([6, 7, 8, 9, 10])
    expect(st.crumble).toEqual([18 * 40 + 7])
    expect(st.arrows).toEqual([{ x: 2 * TILE + 10, y: 17 * TILE + 10 }])
    expect(st.keys).toEqual([{ x: 4 * TILE + 10, y: 17 * TILE + 10 }])
    expect(st.checkpoints).toEqual([
      { x: 6 * TILE + 10, y: 18 * TILE, zone: { x: 5 * TILE, y: 15 * TILE, w: 3 * TILE, h: 3 * TILE } },
      { x: 26 * TILE + 10, y: 19 * TILE, zone: { x: 25 * TILE, y: 10 * TILE, w: 4 * TILE, h: 9 * TILE } },
    ])
    expect(st.lamps[0]).toMatchObject({ x: 22 * TILE, ticks: 120 })
    expect(st.gswitches[0]!.who).toBe('other')
    expect(st.cannons[0]!.vx).toBeCloseTo(20 * TILE * Math.SQRT1_2)
    expect(st.cannons[0]!.vy).toBeCloseTo(-20 * TILE * Math.SQRT1_2)
    expect(st.plates[0]).toMatchObject({ y: TILE, h: 4, ceil: true })
    expect(st.darks).toEqual([{ x: 30 * TILE, y: TILE, w: 5 * TILE, h: 5 * TILE }])
    const s = initState(L)
    expect([s.lit, s.keys, s.cp, s.arrows, s.crumble, s.keyTaken, s.locks, s.cannons]).toEqual([true, 0, -1, [0], [0], [false], [false], [0]])
    expect(s.players[0]).toMatchObject({ grav: 1, air: 0, cannon: -1, fly: 0 })
  })

  it('light-only and dark-only blocks follow the lights', () => {
    const L = levelOf({ ...room(32, 18, ['1.2.OD.....RB']), dark: true })
    const st = L.stages[0]!
    const s = initState(L)
    expect(s.lit).toBe(false)
    expect([tileAt(st, s, 5, 16), tileAt(st, s, 6, 16)]).toEqual([0, T_WALL])
    expect([tileAt(st, { ...s, lit: true }, 5, 16), tileAt(st, { ...s, lit: true }, 6, 16)]).toEqual([T_WALL, 0])
  })

  it('wants a key for every lock', () => {
    const def: LevelDef = { id: 't', name: 't', par: 1, stages: [room(32, 18, ['1.2........RB'], [{ k: 'lock', x: 8, y: 15, w: 1, h: 2 }])] }
    expect(validateLevel(def).join()).toMatch(/0 keys for 1 locks/)
  })
})

describe('stages', () => {
  // both one step left of their doors, a gem each on the way in the first stage
  const WIN = ['1rR#...................#2bB']
  const BOTH: Inputs = [IN_RIGHT, IN_RIGHT]

  it('clears a stage, pauses, and begins the next with the clock still running', () => {
    const L = levelOf(room(32, 18, WIN), room(48, 20, ['1.R#...................#2.B']))
    expect(L.gemTotals).toEqual([1, 1])
    let r = run(L, initState(L), 40, BOTH)
    expect(r.s.phase).toBe('clear')
    expect(r.ev).toContainEqual({ k: 'clear', stage: 0 })
    expect(r.ev.some((e) => e.k === 'win')).toBe(false)
    const tick = r.s.tick
    // the pause is STAGE_TICKS long from the moment the doors closed, and the clock stands still through it
    r = run(L, r.s, STAGE_TICKS - r.s.phaseT - 1, [0, 0])
    expect(r.s).toMatchObject({ stage: 0, phase: 'clear', tick })
    r = run(L, r.s, 1, [0, 0])
    expect(r.ev).toEqual([{ k: 'stage', stage: 1 }])
    expect(r.s).toMatchObject({ stage: 1, phase: 'play', banked: [1, 1], cp: -1, saved: null, tick })
    expect(r.s.taken).toEqual([])
    expect(r.s.players[0].x).toBe(L.stages[1]!.spawns[0].x)
    r = run(L, r.s, 40, BOTH)
    expect(r.s.tick).toBeGreaterThan(tick)
    expect(r.s.phase).toBe('won')
    expect(r.ev.at(-1)).toEqual({ k: 'win' })
    expect(scoreOf(L, r.s)).toMatchObject({ gems: [1, 1], total: [1, 1], stars: 3 })
  })

  it('a death in a later stage restarts that stage, not the level', () => {
    const L = levelOf(room(32, 18, WIN), { ...room(32, 18, ['1...........R.....B.......2']), map: room(32, 18, ['1...........R.....B.......2']).map.map((r, i) => (i === 17 ? '####WWWW' + '#'.repeat(24) : r)) })
    let s = run(L, initState(L), 40 + STAGE_TICKS + 2, BOTH).s
    expect(s.stage).toBe(1)
    while (s.phase === 'play') s = step(L, s, R)
    expect(s.phase).toBe('dead')
    const tick = s.tick
    s = run(L, s, DEATH_TICKS, [0, 0]).s
    expect(s).toMatchObject({ stage: 1, phase: 'play', deaths: 1, banked: [1, 1], tick })
    expect(s.events).toEqual([{ k: 'respawn', cp: -1 }])
    expect(s.players[0].x).toBe(L.stages[1]!.spawns[0].x)
  })
})

describe('checkpoints', () => {
  // fire walks right: a gem, the flag at x8 (both must stand in its zone), another gem, then water
  const def = (): LevelDef['stages'][number] => {
    const st = room(32, 18, ['1...r...C2....r............RB'])
    return { ...st, map: st.map.map((r, i) => (i === 17 ? '#'.repeat(20) + 'WWWW' + '#'.repeat(8) : r)) }
  }

  it('is taken when both stand in its zone, and a death comes back to it with the world as it was', () => {
    const L = levelOf(def())
    let r = run(L, initState(L), 30, R)
    expect(r.s.cp).toBe(-1)
    r = run(L, r.s, 40, R)
    expect(r.ev.filter((e) => e.k === 'checkpoint')).toEqual([{ k: 'checkpoint', i: 0, x: 9 * TILE + 10, y: 17 * TILE }])
    expect(r.s.cp).toBe(0)
    const at = r.s.saved!
    expect(at.taken).toEqual([true, false])
    expect(at.cp).toBe(0)
    expect('saved' in at).toBe(false)
    let s = r.s
    while (s.phase === 'play') s = step(L, s, R)
    expect(s.taken).toEqual([true, true])
    const tick = s.tick
    s = run(L, s, DEATH_TICKS, R).s
    expect(s).toMatchObject({ phase: 'play', deaths: 1, cp: 0, tick, taken: [true, false] })
    expect(s.events).toEqual([{ k: 'respawn', cp: 0 }])
    expect(s.players[0].x).toBeCloseTo(at.players[0].x)
    expect(s.saved).toBe(at)
    // the held key isn't a fresh press, and it dies and comes back there again
    while (s.phase === 'play') s = step(L, s, R)
    s = run(L, s, DEATH_TICKS, R).s
    expect(s).toMatchObject({ deaths: 2, cp: 0 })
  })

  it('respawn() goes back at once, and to the start when no flag was taken', () => {
    const L = levelOf(def())
    const moved = run(L, initState(L), 20, R).s
    const back = respawn(L, moved)
    expect(back.players[0].x).toBe(L.stages[0]!.spawns[0].x)
    expect(back).toMatchObject({ deaths: 1, tick: moved.tick, phase: 'play' })
    expect(step(L, moved, R).saved).toBe(null)
  })

  it('needs both players: one alone in the zone takes nothing', () => {
    const st = room(32, 18, ['1...C...............2......RB'])
    const L = levelOf(st)
    expect(run(L, initState(L), 60, R).s.cp).toBe(-1)
  })
})

// ---- ember 2: the mechanics

// the tick (from 0) of the first event of a kind, -1 for none
function when(L: Level, s: GameState, n: number, input: Inputs | ((t: number) => Inputs), k: GameEvent['k']): number {
  for (let t = 0; t < n; t++) {
    s = step(L, s, typeof input === 'function' ? input(t) : input)
    if (s.events.some((e) => e.k === k)) return t
  }
  return -1
}
// the highest the feet get (smallest y) over n ticks
function peak(L: Level, s: GameState, n: number, input: Inputs | ((t: number) => Inputs), p: 0 | 1 = 0): number {
  let top = Infinity
  for (let t = 0; t < n; t++) {
    s = step(L, s, typeof input === 'function' ? input(t) : input)
    top = Math.min(top, feet(s, p))
  }
  return top
}
const FLOOR = 17 * TILE
// a small level from its bottom rows; the doors and frost go at the far right of the last row unless it has them
const mk = (rows: string[], things: ThingDef[] = []) =>
  level(
    rows.map((r, i) => (i === rows.length - 1 && !r.includes('B') ? r.padEnd(26, '.') + 'R.2B' : r)),
    things,
  )

describe('one-way platforms', () => {
  it('a jump goes up through one and lands on top of it', () => {
    const L = mk(['.=', '..', '.1.......................R.2B'])
    const r = run(L, initState(L), 60, (t) => [t < 20 ? IN_UP : 0, 0])
    expect(feet(r.s, 0)).toBe(14 * TILE)
    expect(r.s.players[0].ground).toBe(true)
    // and stays there: there is no dropping through
    const later = run(L, r.s, 120, [IN_DOWN, 0]).s
    expect(feet(later, 0)).toBe(14 * TILE)
    // walking off its end falls back to the floor
    expect(feet(run(L, later, 60, R).s, 0)).toBe(FLOOR)
  })

  it('is never solid sideways, nor from below', () => {
    // one at head height in the way, and a row of them overhead
    const L = mk(['.===', '....', '.1.=.....................R.2B'])
    const r = run(L, initState(L), 40, R)
    expect(r.s.players[0].x).toBeGreaterThan(5 * TILE)
    expect(feet(r.s, 0)).toBe(FLOOR)
  })

  it('a box lands on one, and a player can stand on that box', () => {
    const L = mk(['.1', '.#=='], [{ k: 'box', x: 4, y: 10 }])
    const s = run(L, initState(L), 80, [0, 0]).s
    expect(s.boxes[0]!.y).toBe(16 * TILE - TILE)
    // ember walks out along the platform and pushes it off the far end
    const t = run(L, s, 20, R).s
    expect(feet(t, 0)).toBe(16 * TILE)
    expect(t.boxes[0]!.x).toBeGreaterThan(4 * TILE)
    expect(t.boxes[0]!.y).toBe(15 * TILE)
    expect(run(L, t, 80, R).s.boxes[0]!.y).toBe(FLOOR - TILE)
  })

  it('a state rounded by the network still stands on one', () => {
    const L = mk(['.1', '.='])
    let s = run(L, initState(L), 5, [0, 0]).s
    s = { ...s, players: [{ ...s.players[0], y: s.players[0].y + 0.004 }, s.players[1]] }
    s = run(L, s, 30, [0, 0]).s
    expect(s.players[0].ground).toBe(true)
    expect(feet(s, 0)).toBeCloseTo(16 * TILE, 1)
  })
})

describe('trampolines', () => {
  // ember on a block beside a trampoline
  const PAD = ['.1', '.#T.......................R.2B']
  const TOP = 16 * TILE

  it('stepping onto one bounces at the least speed, about 3.8 tiles', () => {
    const L = mk(PAD)
    const r = run(L, initState(L), 30, R)
    const b = r.ev.find((e) => e.k === 'bounce')
    expect(b).toMatchObject({ k: 'bounce', p: 0, v: TRAMP_MIN_V })
    const s = initState(L)
    s.players[0].x = 3 * TILE + 3
    s.players[0].y = TOP - PLAYER_H
    const top = peak(L, s, 30, [0, 0])
    expect(TOP - top).toBeGreaterThan(3.7 * TILE)
    expect(TOP - top).toBeLessThan(3.9 * TILE)
  })

  it('a longer fall bounces higher, up to the cap, and nobody rests on one', () => {
    const L = mk(PAD)
    const drop = (h: number) => {
      const s = initState(L)
      s.players[0].x = 3 * TILE + 3
      s.players[0].y = TOP - PLAYER_H - h
      s.players[0].ground = false
      const t = when(L, s, 200, [0, 0], 'bounce')
      const at = run(L, s, t + 1, [0, 0])
      const v = (at.ev.find((e) => e.k === 'bounce') as { v: number }).v
      return { v, rise: TOP - peak(L, at.s, 40, [0, 0]) }
    }
    const five = drop(5 * TILE)
    expect(five.rise / TILE).toBeGreaterThan(6)
    expect(five.rise / TILE).toBeLessThan(6.6)
    const far = drop(13 * TILE)
    // at the fastest fall: 620 * 0.9 + 120. only a second bounce reaches the cap
    expect(far.v).toBe(678)
    expect(far.v).toBeLessThan(TRAMP_MAX_V)
    expect(far.rise / TILE).toBeGreaterThan(8)
    expect(far.rise / TILE).toBeLessThan(8.4)
    // left alone on the pad every bounce comes down faster than the last, up to the fastest fall there is
    const s = initState(L)
    s.players[0].x = 3 * TILE + 3
    const r = run(L, s, 600, [0, 0])
    const vs = r.ev.filter((e) => e.k === 'bounce').map((e) => Math.round((e as { v: number }).v))
    expect(vs.slice(0, 5)).toEqual([460, 546, 616, 678, 678])
    expect(vs.length).toBeGreaterThan(8)
    expect(r.ev.filter((e) => e.k === 'land').length).toBe(0)
  })

  it('letting go of jump while rising cuts the bounce', () => {
    const L = mk(PAD)
    const s = initState(L)
    s.players[0].x = 3 * TILE + 3
    s.players[0].y = TOP - PLAYER_H - 40
    s.players[0].ground = false
    const t = when(L, s, 100, [IN_UP, 0], 'bounce')
    const at = run(L, s, t + 1, [IN_UP, 0]).s
    const held = TOP - peak(L, at, 30, [IN_UP, 0])
    const cut = TOP - peak(L, at, 30, (k) => [k < 3 ? IN_UP : 0, 0])
    expect(cut).toBeLessThan(held * 0.6)
  })

  it('boxes do not bounce, and a player on a box on a trampoline stands', () => {
    const L = mk(PAD, [{ k: 'box', x: 3, y: 12 }])
    const r = run(L, initState(L), 120, [0, 0])
    expect(r.s.boxes[0]!.y).toBe(TOP - TILE)
    expect(r.ev.some((e) => e.k === 'bounce')).toBe(false)
  })

  it('bounces up through a one-way and lands on it; a one-way right above shields it', () => {
    const L = mk(['..===', '.....', '.1...', '.#TTT'])
    const s = run(L, initState(L), 120, (t) => [t < 14 ? IN_RIGHT : 0, 0]).s
    expect(s.players[0].ground).toBe(true)
    expect(feet(s, 0)).toBe(13 * TILE)
    const M = mk(['.1..', '.#==', '..TT'])
    const r = run(M, initState(M), 60, (t) => [t < 14 ? IN_RIGHT : 0, 0])
    expect(r.s.players[0].x).toBeGreaterThan(3 * TILE)
    expect(feet(r.s, 0)).toBe(15 * TILE)
    expect(r.ev.some((e) => e.k === 'bounce')).toBe(false)
  })

  it('stops a beam', () => {
    const L = mk(['.1', '....T'], [
      { k: 'emitter', x: 1, y: 16, dir: 'right' },
      { k: 'sensor', x: 9, y: 16, ch: 's' },
    ])
    const s = run(L, initState(L), 3, [0, 0]).s
    expect(s.on).toEqual([])
  })
})

describe('crumbling blocks', () => {
  // a block to stand on, then two crumbling blocks over the floor
  const BRIDGE = ['.1', '.#~~#.....................R.2B']
  const idx = (tx: number, ty: number) => ty * COLS + tx

  it('shakes when stepped on, breaks CRUMBLE_TICKS later whoever is on it, and mends when clear', () => {
    const L = mk(BRIDGE)
    const st = L.stages[0]!
    const walk = (t: number): Inputs => [t < 12 ? IN_RIGHT : 0, 0]
    const t0 = when(L, initState(L), 100, walk, 'shake')
    expect(t0).toBeGreaterThan(0)
    let s = run(L, initState(L), t0 + 1, walk).s
    expect(s.events).toContainEqual({ k: 'shake', tile: idx(3, 16), x: 3 * TILE + 10, y: 16 * TILE + 10 })
    expect(s.crumble[0]).toBe(1)
    const a = run(L, s, CRUMBLE_TICKS - 1, (t) => walk(t + t0 + 1))
    expect(tileAt(st, a.s, 3, 16)).toBe(T_CRUMBLE)
    expect(a.ev.some((e) => e.k === 'break')).toBe(false)
    s = step(L, a.s, [0, 0])
    expect(s.events).toContainEqual({ k: 'break', tile: idx(3, 16), x: 3 * TILE + 10, y: 16 * TILE + 10 })
    expect(tileAt(st, s, 3, 16)).toBe(0)
    // ember stood still on it and is now in the hole: it waits for them
    s = run(L, s, CRUMBLE_GONE_TICKS + 60, [0, 0]).s
    expect(feet(s, 0)).toBe(FLOOR)
    expect(tileAt(st, s, 3, 16)).toBe(0)
    expect(s.crumble[0]).toBe(CRUMBLE_TICKS + CRUMBLE_GONE_TICKS)
    // out of the hole (the second block has long gone too) and it grows back
    const out = run(L, s, 80, (t) => [IN_RIGHT | (t % 30 === 0 ? IN_UP : 0), 0])
    expect(out.ev.some((e) => e.k === 'mend' && e.tile === idx(3, 16))).toBe(true)
  })

  it('is gone for exactly CRUMBLE_GONE_TICKS when nothing is in the way', () => {
    const L = mk(BRIDGE)
    // run straight across: both blocks start, ember is long gone when they break
    const r = run(L, initState(L), CRUMBLE_TICKS + CRUMBLE_GONE_TICKS + 60, (t) => [t < 40 ? IN_RIGHT : 0, 0])
    const at = (k: string, tile: number) => {
      let s = initState(L)
      for (let t = 0; t < 400; t++) {
        s = step(L, s, [t < 40 ? IN_RIGHT : 0, 0])
        if (s.events.some((e) => e.k === k && 'tile' in e && e.tile === tile)) return t
      }
      return -1
    }
    const tile = idx(3, 16)
    expect(at('break', tile) - at('shake', tile)).toBe(CRUMBLE_TICKS)
    expect(at('mend', tile) - at('break', tile)).toBe(CRUMBLE_GONE_TICKS)
    expect(r.s.crumble).toEqual([0, 0])
    expect(r.s.players[0].x).toBeGreaterThan(5 * TILE)
  })

  it('a box does not start one, but falls with it', () => {
    const L = mk(['.1.', '.~~'], [{ k: 'box', x: 3, y: 15 }])
    // ember's block goes from under them; the box's block was never stood on
    let s = run(L, initState(L), CRUMBLE_TICKS + 40, [0, 0]).s
    expect(feet(s, 0)).toBe(FLOOR)
    expect(s.crumble[1]).toBe(0)
    expect(s.boxes[0]!.y).toBe(15 * TILE)
    s = { ...s, crumble: [s.crumble[0]!, 1] }
    s = run(L, s, CRUMBLE_TICKS + 30, [0, 0]).s
    expect(s.boxes[0]!.y).toBe(FLOOR - TILE)
  })
})

describe('double-jump arrows', () => {
  // an arrow two tiles over ember's head
  const ARROW = ['.^', '..', '..', '.1']
  // jump, then press again `at` ticks later
  const hop = (at: number) => (t: number): Inputs => [t < 8 || (t >= at && t < at + 10) ? IN_UP : 0, 0]

  it('is not taken from the ground, is taken in the air, and comes back later', () => {
    const L = mk(['.1^'])
    // walking through it does nothing
    let r = run(L, initState(L), 30, R)
    expect(r.ev.some((e) => e.k === 'arrow')).toBe(false)
    expect(r.s.arrows).toEqual([0])
    // jumping into it takes it
    const M = mk(ARROW)
    const t0 = when(M, initState(M), 60, [IN_UP, 0], 'arrow')
    expect(t0).toBeGreaterThan(0)
    r = run(M, initState(M), t0 + 1, [IN_UP, 0])
    expect(r.s.events).toContainEqual({ k: 'arrow', p: 0, i: 0, x: 2 * TILE + 10, y: 13 * TILE + 10 })
    expect(r.s.players[0].air).toBe(1)
    expect(r.s.arrows).toEqual([ARROW_RESPAWN_TICKS])
    const back = run(M, r.s, ARROW_RESPAWN_TICKS - 1, [0, 0])
    expect(back.s.arrows).toEqual([1])
    const s = step(M, back.s, [0, 0])
    expect(s.arrows).toEqual([0])
    expect(s.events).toContainEqual({ k: 'arrowback', i: 0, x: 2 * TILE + 10, y: 13 * TILE + 10 })
  })

  it('a second press in the air jumps again, about 2.6 tiles, once', () => {
    const L = mk(ARROW)
    // the first hop is let go early, so by itself it stays under 3 tiles
    const plain = (FLOOR - peak(L, initState(L), 80, hop(999))) / TILE
    expect(plain).toBeLessThan(3)
    const r = run(L, initState(L), 80, hop(18))
    expect(r.ev.filter((e) => e.k === 'airjump').length).toBe(1)
    const top = (FLOOR - peak(L, initState(L), 80, hop(18))) / TILE
    expect(top).toBeGreaterThan(plain + 1.5)
    expect(top).toBeLessThan(5.8)
    // from a standstill in the air the arrow's jump is 380² / 2g = 51.6 px
    const still = initState(L)
    Object.assign(still.players[0], { y: 8 * TILE, ground: false, air: 1 })
    const y0 = feet(still, 0)
    expect(y0 - peak(L, still, 40, [IN_UP, 0])).toBeGreaterThan(50)
    expect(y0 - peak(L, still, 40, [IN_UP, 0])).toBeLessThan(52.5)
    // a third press does nothing
    const three = run(L, initState(L), 80, (t) => [hop(18)(t)[0] | (t >= 34 && t < 38 ? IN_UP : 0), 0])
    expect(three.ev.filter((e) => e.k === 'airjump').length).toBe(1)
    expect(r.s.players[0].air).toBe(0)
  })

  it('landing loses it', () => {
    const L = mk(ARROW)
    let s = run(L, initState(L), 70, hop(999)).s
    expect(s.players[0].ground).toBe(true)
    expect(s.players[0].air).toBe(0)
    expect(s.arrows[0]).toBeGreaterThan(0)
    // back on the ground, the next jump is a plain one
    const r = run(L, s, 30, (t) => [t > 2 && t < 6 ? IN_UP : 0, 0])
    expect(r.ev.some((e) => e.k === 'airjump')).toBe(false)
    s = r.s
  })

  it('one arrow, two players: the second waits for it to come back', () => {
    const L = mk(['.^', '..', '..', '.12......................R..B'])
    const both: Inputs = [IN_UP, IN_UP]
    const r = run(L, initState(L), 30, (t) => (t < 8 ? both : [0, 0]))
    expect(r.ev.filter((e) => e.k === 'arrow').map((e) => (e as { p: number }).p)).toEqual([0])
    expect(r.s.players[1].air).toBe(0)
  })

  it('a held arrow survives a trampoline, and its jump works off a bounce', () => {
    const L = mk(['.^', '..', '..', '.1', '.T'])
    const s = initState(L)
    const r = run(L, s, 60, (t) => [t >= 30 && t < 34 ? IN_UP : 0, 0])
    expect(r.ev.map((e) => e.k).filter((k) => k === 'bounce' || k === 'arrow' || k === 'airjump').slice(0, 3)).toEqual(['bounce', 'arrow', 'airjump'])
  })
})

describe('lights', () => {
  const press = (t: number): Inputs => [t === 0 ? IN_DOWN : 0, 0]
  // put a player's feet on the floor of a tile
  const stand = (s: GameState, p: 0 | 1, tx: number, ty: number) => Object.assign(s.players[p], { x: tx * TILE + 3, y: (ty + 1) * TILE - PLAYER_H, vx: 0, vy: 0 })
  const dark = (d: Level): Level => compileLevel({ ...d.def, stages: [{ ...d.def.stages[0]!, dark: true }] })

  it('a lamp toggles the lights on a down press, with an event at the lamp', () => {
    const L = mk(['.1'], [
      { k: 'lamp', x: 2, y: 16 },
      { k: 'gate', x: 10, y: 14, w: 1, h: 3, open: '!lights' },
    ])
    let s = step(L, initState(L), [IN_DOWN, 0])
    expect(s.lit).toBe(false)
    expect(s.events).toContainEqual({ k: 'light', on: false, x: 2 * TILE + 10, y: 16 * TILE + 10 })
    expect(s.on).toEqual([])
    // holding the key is one press; the gate follows the lights
    s = run(L, s, 40, [IN_DOWN, 0]).s
    expect(s.lit).toBe(false)
    expect(s.gates[0]).toBe(1)
    s = step(L, step(L, s, [0, 0]), [IN_DOWN, 0])
    expect(s.lit).toBe(true)
    expect(s.litT).toBe(0)
  })

  it('a lever comes before a lamp in the same tile', () => {
    const L = mk(['.1'], [
      { k: 'lamp', x: 2, y: 16 },
      { k: 'lever', x: 2, y: 16, ch: 'a' },
    ])
    const s = step(L, initState(L), [IN_DOWN, 0])
    expect([s.lit, s.levers[0]]).toEqual([true, true])
  })

  it('a timed lamp holds the other state for its seconds, then falls back; a press restarts it', () => {
    const L = dark(mk(['.1'], [{ k: 'lamp', x: 2, y: 16, secs: 1 }]))
    const st = L.stages[0]!
    let s = step(L, initState(L), [IN_DOWN, 0])
    expect([s.lit, s.litT]).toEqual([true, 60])
    s = run(L, s, 59, [0, 0]).s
    expect([s.lit, s.litT]).toEqual([true, 1])
    s = step(L, s, [0, 0])
    expect([s.lit, s.litT]).toEqual([false, 0])
    expect(s.events).toContainEqual({ k: 'light', on: false, x: st.w / 2, y: st.h / 2 })
    // pressed again half way through, it runs a whole second more and says nothing new
    s = step(L, s, [IN_DOWN, 0])
    s = run(L, s, 30, [0, 0]).s
    const again = step(L, s, [IN_DOWN, 0])
    expect([again.lit, again.litT]).toEqual([true, 60])
    expect(again.events.some((e) => e.k === 'light')).toBe(false)
    // in a lit stage it gives darkness instead
    const M = mk(['.1'], [{ k: 'lamp', x: 2, y: 16, secs: 1 }])
    const m = step(M, initState(M), [IN_DOWN, 0])
    expect([m.lit, m.litT]).toEqual([false, 60])
  })

  it('a body inside a block that turns solid is pushed up onto it', () => {
    const L = mk(['.1...', '....D'], [
      { k: 'lamp', x: 2, y: 15 },
      { k: 'box', x: 6, y: 16 },
    ])
    const s0 = initState(L)
    // frost stands in the gap where the dark-only block will be
    stand(s0, 1, 5, 16)
    let s = run(L, s0, 3, [0, 0]).s
    expect(feet(s, 1)).toBe(FLOOR)
    s = step(L, s, [IN_DOWN, 0])
    expect(s.players[1].alive).toBe(true)
    expect(feet(s, 1)).toBe(16 * TILE)
    s = run(L, s, 10, [0, 0]).s
    expect(s.players[1].ground).toBe(true)
    expect(feet(s, 1)).toBe(16 * TILE)
    // and back on: the block is air again and frost drops to the floor
    s = run(L, s, 30, (t) => [t === 1 ? IN_DOWN : 0, 0]).s
    expect(feet(s, 1)).toBe(FLOOR)
  })

  it('with no room within two tiles the player is crushed', () => {
    const L = mk(['....#', '....D', '.1..D'], [{ k: 'lamp', x: 2, y: 16 }])
    const s0 = initState(L)
    stand(s0, 1, 5, 16)
    const s = step(L, run(L, s0, 3, [0, 0]).s, [IN_DOWN, 0])
    expect(s.players[1].alive).toBe(false)
    expect(s.events).toContainEqual(expect.objectContaining({ k: 'die', p: 1, cause: 'crush' }))
    expect(s.phase).toBe('dead')
    // two tiles of block with air above is survivable: out of the top
    const M = mk(['....D', '.1..D'], [{ k: 'lamp', x: 2, y: 16 }])
    const m0 = initState(M)
    stand(m0, 1, 5, 16)
    const m = step(M, run(M, m0, 3, [0, 0]).s, [IN_DOWN, 0])
    expect(m.players[1].alive).toBe(true)
    expect(feet(m, 1)).toBe(15 * TILE)
  })

  it('lights changing under a box drop it, and a box in a new block is lifted onto it', () => {
    const L = mk(['.1....', '....OD'], [
      { k: 'lamp', x: 2, y: 15 },
      { k: 'box', x: 5, y: 15 },
      { k: 'box', x: 6, y: 16 },
    ])
    let s = run(L, initState(L), 10, [0, 0]).s
    expect(s.boxes.map((b) => b.y)).toEqual([15 * TILE, 16 * TILE])
    s = run(L, s, 40, press).s
    expect(s.boxes.map((b) => b.y)).toEqual([16 * TILE, 15 * TILE])
  })

  it('a timed lamp runs out under a player on a light-only bridge', () => {
    const L = dark(mk(['.1.OOO', '......', '......'], [{ k: 'lamp', x: 2, y: 14, secs: 1 }]))
    let s = step(L, initState(L), [IN_DOWN, 0])
    expect(s.lit).toBe(true)
    stand(s, 0, 5, 13)
    s = run(L, s, 40, [0, 0]).s
    expect(feet(s, 0)).toBe(14 * TILE)
    s = run(L, s, 60, [0, 0]).s
    expect(s.lit).toBe(false)
    expect(feet(s, 0)).toBe(FLOOR)
    expect(s.players[0].alive).toBe(true)
  })

  it('a beam passes a light-only block only in the dark', () => {
    const L = mk(['.1', '....O'], [
      { k: 'lamp', x: 2, y: 15 },
      { k: 'emitter', x: 1, y: 16, dir: 'right' },
      { k: 'sensor', x: 9, y: 16, ch: 's' },
    ])
    let s = run(L, initState(L), 3, [0, 0]).s
    expect(s.on).toEqual([])
    s = run(L, s, 3, press).s
    expect(s.on).toEqual(['s'])
  })
})

describe('keys and locks', () => {
  const LOCK: ThingDef = { k: 'lock', x: 8, y: 14, w: 1, h: 3 }

  it('a lock is a wall, and stops a beam, until a key is brought', () => {
    const M = mk(['.1.k'], [LOCK, { k: 'emitter', x: 1, y: 15, dir: 'right' }, { k: 'sensor', x: 20, y: 15, ch: 's' }])
    // without the key: jump over it and run at the lock
    let s = run(M, initState(M), 120, (t) => [IN_RIGHT | (t > 5 && t < 25 ? IN_UP : 0), 0]).s
    expect(s.keyTaken).toEqual([false])
    expect(s.players[0].x + PLAYER_W).toBe(8 * TILE)
    expect(s.locks).toEqual([false])
    expect(s.on).toEqual([])
    // back for the key, and through
    const r = run(M, s, 200, (t) => [t < 60 ? IN_LEFT : IN_RIGHT, 0])
    expect(r.ev.filter((e) => e.k === 'key' || e.k === 'unlock')).toEqual([
      { k: 'key', p: 0, i: 0, x: 4 * TILE + 10, y: 16 * TILE + 10 },
      { k: 'unlock', i: 0, x: 8 * TILE + 10, y: 15.5 * TILE },
    ])
    s = r.s
    expect([s.keys, s.keyTaken, s.locks]).toEqual([0, [true], [true]])
    expect(s.players[0].x).toBeGreaterThan(9 * TILE)
    expect(s.on).toEqual(['s'])
  })

  it('the team carries keys: one fetches, the other opens', () => {
    const L = mk(['.1.k.................2.....R.B'], [{ k: 'lock', x: 24, y: 14, w: 1, h: 3 }])
    const r = run(L, initState(L), 60, (t) => [t < 30 ? IN_RIGHT : 0, t > 40 ? IN_RIGHT : 0])
    expect(r.ev.filter((e) => e.k === 'key').length).toBe(1)
    expect(r.ev.filter((e) => e.k === 'unlock').length).toBe(1)
    expect(r.s.keys).toBe(0)
    expect(r.s.players[1].x).toBeGreaterThan(24 * TILE)
  })

  it('one key opens one lock; the next lock stays shut', () => {
    // the second key is out of reach overhead
    const L = mk(['.....k', '', '', '', '', '', '.1.k'], [LOCK, { k: 'lock', x: 12, y: 14, w: 1, h: 3 }])
    const r = run(L, initState(L), 160, R)
    expect([r.s.keys, r.s.keyTaken, r.s.locks]).toEqual([0, [false, true], [true, false]])
    expect(r.s.players[0].x + PLAYER_W).toBe(12 * TILE)
  })

  it('a key carried through a portal still opens the lock on the other side', () => {
    const L = mk(['.1.k...#.......'], [
      { k: 'portal', x: 6, y: 16, pair: 'p' },
      { k: 'portal', x: 10, y: 16, pair: 'p' },
      { k: 'lock', x: 14, y: 14, w: 1, h: 3 },
    ])
    const r = run(L, initState(L), 140, R)
    expect(r.ev.map((e) => e.k).filter((k) => k === 'key' || k === 'portal' || k === 'unlock')).toEqual(['key', 'portal', 'unlock'])
    expect(r.s.players[0].x).toBeGreaterThan(15 * TILE)
  })
})

describe('gravity switches', () => {
  // a whole room: rows from the ceiling down, then rows from the floor up, doors bottom right
  const tb = (top: string[], bottom: string[], things: ThingDef[] = []) => {
    const body = [...top, ...Array.from({ length: ROWS - 2 - top.length - bottom.length }, () => ''), ...bottom]
    return level(
      body.map((r, i) => (i === body.length - 1 && !r.includes('B') ? r.padEnd(26, '.') + 'R.2B' : r)),
      things,
    )
  }
  const CEIL = TILE // the underside of the ceiling
  const SW: ThingDef = { k: 'gswitch', x: 2, y: 16 }
  const head = (s: GameState, p: 0 | 1) => s.players[p].y
  // ember flipped and settled on the ceiling
  const up = (L: Level) => run(L, initState(L), 70, (t) => [t === 0 ? IN_DOWN : 0, 0]).s

  it('flips whoever pressed it: they fall up, stand on the ceiling, and a second press brings them down', () => {
    const L = tb([], ['.1'], [SW])
    let s = step(L, initState(L), [IN_DOWN, 0])
    expect(s.events).toEqual(expect.arrayContaining([expect.objectContaining({ k: 'gswitch', i: 0 }), expect.objectContaining({ k: 'flip', p: 0, grav: -1 })]))
    expect(s.players[0].grav).toBe(-1)
    expect(s.players[1].grav).toBe(1)
    const r = run(L, s, 70, [0, 0])
    s = r.s
    expect(head(s, 0)).toBe(CEIL)
    expect(s.players[0].ground).toBe(true)
    expect(r.ev.some((e) => e.k === 'land' && e.p === 0)).toBe(true)
    // walking works as ever
    s = run(L, s, 30, R).s
    expect(head(s, 0)).toBe(CEIL)
    expect(s.players[0].x).toBeGreaterThan(4 * TILE)
    // and back down at the switch (it is out of reach from up here: frost presses nothing, so use the state)
    s = { ...s, players: [{ ...s.players[0], grav: 1, ground: false }, s.players[1]] }
    s = run(L, s, 70, [0, 0]).s
    expect(feet(s, 0)).toBe(FLOOR)
  })

  it('a flipped jump goes down, just as far, and coyote and buffering still work', () => {
    const L = tb([], ['.1'], [SW])
    const s = up(L)
    let low = 0
    let t = s
    const ev: GameEvent[] = []
    for (let k = 0; k < 60; k++) {
      t = step(L, t, [k < 30 ? IN_UP : 0, 0])
      ev.push(...t.events)
      low = Math.max(low, head(t, 0))
    }
    expect(ev.filter((e) => e.k === 'jump').length).toBe(1)
    expect(low - CEIL).toBeGreaterThan(3 * TILE)
    expect(low - CEIL).toBeLessThan(3.4 * TILE)
    expect(head(t, 0)).toBe(CEIL)
    expect(step(L, s, [IN_UP, 0]).players[0].vy).toBeCloseTo(JUMP_V - 1400 / 60)
  })

  it('who: other, fire, frost, both', () => {
    const flips = (who: 'other' | 'fire' | 'frost' | 'both' | 'self', presser: 0 | 1) => {
      const L = tb([], ['.12......................R..B'], [{ k: 'gswitch', x: 2, y: 16, w: 2, who } as ThingDef, { k: 'gswitch', x: 3, y: 16, who }])
      const s = step(L, initState(L), presser === 0 ? [IN_DOWN, 0] : [0, IN_DOWN])
      return [s.players[0].grav, s.players[1].grav]
    }
    expect(flips('self', 1)).toEqual([1, -1])
    expect(flips('other', 0)).toEqual([1, -1])
    expect(flips('other', 1)).toEqual([-1, 1])
    expect(flips('fire', 1)).toEqual([-1, 1])
    expect(flips('frost', 0)).toEqual([1, -1])
    expect(flips('both', 0)).toEqual([-1, -1])
    // both pressing a 'both' switch in the same tick flips each twice
    const L = tb([], ['.12......................R..B'], [{ k: 'gswitch', x: 2, y: 16, who: 'both' }, { k: 'gswitch', x: 3, y: 16, who: 'both' }])
    const s = step(L, initState(L), [IN_DOWN, IN_DOWN])
    expect([s.players[0].grav, s.players[1].grav]).toEqual([1, 1])
  })

  it('pools are walls to a flipped player: frost walks the underside of a lava ceiling', () => {
    // a lava ceiling over the whole room, frost flipped beneath it
    const L = level(['.2.......................R.1B'], [{ k: 'gswitch', x: 2, y: 16 }])
    const lava = compileLevel(remap(L.def, (m) => m.map((r, i) => (i === 1 ? '#' + 'L'.repeat(COLS - 2) + '#' : r))))
    let s = run(lava, initState(lava), 80, (t) => [0, t === 0 ? IN_DOWN : 0]).s
    expect(s.players[1].alive).toBe(true)
    expect(head(s, 1)).toBe(2 * TILE)
    s = run(lava, s, 60, [0, IN_RIGHT]).s
    expect(s.players[1].alive).toBe(true)
    expect(s.players[1].x).toBeGreaterThan(6 * TILE)
    expect(head(s, 1)).toBe(2 * TILE)
  })

  it('a ceiling plate takes a flipped player and nobody else', () => {
    // a low ceiling ember can jump into: the plate hangs from it
    const L = tb([], ['.####', '.....', '.....', '.1...'], [SW, { k: 'plate', x: 2, y: 14, w: 3, ch: 'c', ceil: true }, { k: 'plate', x: 8, y: 16, ch: 'f' }])
    // upright: a jump touches the ceiling and presses nothing
    let r = run(L, initState(L), 40, (t) => [t < 20 ? IN_UP : 0, 0])
    expect(r.ev.some((e) => e.k === 'plate')).toBe(false)
    // flipped: standing on it holds it
    r = run(L, initState(L), 40, (t) => [t === 0 ? IN_DOWN : 0, 0])
    expect(head(r.s, 0)).toBe(14 * TILE)
    expect(r.s.on).toEqual(['c'])
    // and a flipped player jumping down onto a floor plate with their head presses nothing
    const s = initState(L)
    Object.assign(s.players[0], { x: 8 * TILE + 10, y: 16 * TILE - 4, grav: -1, ground: false })
    expect(step(L, s, [0, 0]).on).toEqual([])
    Object.assign(s.players[0], { grav: 1, y: FLOOR - PLAYER_H })
    expect(step(L, s, [0, 0]).on).toEqual(['f'])
  })

  it('a door wants an upright player', () => {
    const L = tb([], ['..#', '...', '.1R.....................2B'], [SW])
    // flipped under the low block right above the door: in the doorway, but upside down
    const s = initState(L)
    Object.assign(s.players[0], { x: 3 * TILE + 3 })
    Object.assign(s.players[1], { x: 26 * TILE + 3 })
    const ok = run(L, s, 5, [0, 0]).s
    expect(ok.phase).toBe('won')
    const f = initState(L)
    Object.assign(f.players[0], { x: 3 * TILE + 3, grav: -1 })
    Object.assign(f.players[1], { x: 26 * TILE + 3 })
    const no = run(L, f, 60, [0, 0]).s
    expect(no.phase).toBe('play')
    expect(no.players[0].inDoor).toBe(false)
  })

  it('a one-way is a floor from above and a ceiling to stand under for the flipped', () => {
    // a row of one-ways three tiles up
    const L = tb([], ['.====', '.....', '.....', '.1...'], [SW])
    const s = run(L, initState(L), 60, (t) => [t === 0 ? IN_DOWN : 0, 0]).s
    // flipped, ember rises and stops on the underside of the slab (its top 6 px)
    expect(head(s, 0)).toBe(13 * TILE + 6)
    expect(s.players[0].ground).toBe(true)
    // a flipped jump from the ceiling goes down through one
    const t = initState(L)
    Object.assign(t.players[0], { y: CEIL, grav: -1 })
    const r = run(L, t, 80, (k) => [k > 2 && k < 8 ? IN_UP : 0, 0])
    expect(r.ev.some((e) => e.k === 'jump')).toBe(true)
    expect(head(r.s, 0)).toBe(CEIL)
    // falling up from under it they stop there; an upright player dropped from above lands on top
    const u = initState(L)
    Object.assign(u.players[0], { y: 5 * TILE, ground: false })
    expect(feet(run(L, u, 80, [0, 0]).s, 0)).toBe(13 * TILE)
  })

  it('a trampoline in the ceiling bounces a flipped player downward', () => {
    const L = tb(['.TTT'], ['.1'], [SW])
    const st = L.stages[0]!
    expect(st.tiles[1 * COLS + 2]).toBe(6)
    const r = run(L, initState(L), 200, (t) => [t === 0 ? IN_DOWN : 0, 0])
    const b = r.ev.find((e) => e.k === 'bounce') as { v: number; y: number }
    expect(b.v).toBeGreaterThan(600)
    // the bounce sends them back down, and they come up again
    expect(r.ev.filter((e) => e.k === 'bounce').length).toBeGreaterThan(1)
    expect(r.ev.some((e) => e.k === 'land')).toBe(false)
  })

  it('a flipped player under a crumbling block starts it, and falls up through the hole', () => {
    const L = tb(['', '.~'], ['.1'], [SW])
    const r = run(L, initState(L), 50, (t) => [t === 0 ? IN_DOWN : 0, 0])
    expect(r.ev.some((e) => e.k === 'shake')).toBe(true)
    expect(head(r.s, 0)).toBe(3 * TILE)
    const later = run(L, r.s, CRUMBLE_TICKS + 30, [0, 0])
    expect(later.ev.some((e) => e.k === 'break')).toBe(true)
    expect(head(later.s, 0)).toBe(CEIL)
  })

  it('an arrow jump while flipped goes down', () => {
    const L = tb([], ['.1'], [SW])
    const s = initState(L)
    Object.assign(s.players[0], { y: 8 * TILE, grav: -1, ground: false, air: 1, vy: 0 })
    const n = step(L, s, [IN_UP, 0])
    expect(n.events).toContainEqual(expect.objectContaining({ k: 'airjump', p: 0 }))
    expect(n.players[0].vy).toBeGreaterThan(350)
    expect(n.players[0].air).toBe(0)
  })

  it('a flipped player rides the underside of a mover, and a mover never crushes them', () => {
    const L = tb([], ['.1'], [SW, { k: 'mover', x: 1, y: 8, w: 6, h: 1, to: { x: 12, y: 8 }, speed: 2 }])
    let s = run(L, initState(L), 40, (t) => [t === 0 ? IN_DOWN : 0, 0]).s
    expect(head(s, 0)).toBe(9 * TILE)
    const off = s.players[0].x - s.movers[0]!.x
    s = run(L, s, 120, [0, 0]).s
    expect(s.movers[0]!.x).toBeGreaterThan(5 * TILE)
    expect(s.players[0].x - s.movers[0]!.x).toBeCloseTo(off, 5)
    // a mover rising under the ceiling carries the player hanging beneath it all the way up
    const M = tb([], ['.1'], [SW, { k: 'lever', x: 20, y: 16, ch: 'go' }, { k: 'mover', x: 2, y: 6, w: 3, h: 1, to: { x: 2, y: 1 }, when: 'go' }])
    let m = up(M)
    expect(head(m, 0)).toBe(7 * TILE)
    m = run(M, { ...m, levers: [true] }, 200, [0, 0]).s
    expect(m.movers[0]!.y).toBe(CEIL)
    expect(head(m, 0)).toBe(2 * TILE)
    // but one rising towards a player already standing on the ceiling waits rather than crush them
    const w = initState(M)
    Object.assign(w.players[0], { y: CEIL, grav: -1 })
    const stuck = run(M, { ...w, levers: [true] }, 200, [0, 0]).s
    expect(head(stuck, 0)).toBe(CEIL)
    expect(stuck.players[0].alive).toBe(true)
    expect(stuck.movers[0]!.y).toBeGreaterThanOrEqual(CEIL + PLAYER_H - 0.01)
  })

  it('a fan blows a flipped player up too, and lights push them out downward', () => {
    const L = tb(['', '.D'], ['.1'], [SW, { k: 'lamp', x: 20, y: 16 }])
    const s = initState(L)
    // flipped ember hanging in the gap where the dark-only block will be, under the ceiling
    Object.assign(s.players[0], { y: CEIL, grav: -1 })
    Object.assign(s.players[1], { x: 20 * TILE + 3 })
    let t = run(L, s, 3, [0, 0]).s
    expect(head(t, 0)).toBe(CEIL)
    t = step(L, t, [0, IN_DOWN])
    expect(t.lit).toBe(false)
    expect(t.players[0].alive).toBe(true)
    expect(head(t, 0)).toBe(3 * TILE)
    expect(run(L, t, 10, [0, 0]).s.players[0].ground).toBe(true)
  })

  it('a checkpoint is taken with a flipped player on the ceiling of its zone, and a death brings them back flipped', () => {
    const L = tb([], ['.12......................R..B'], [SW, { k: 'checkpoint', x: 2, y: 16, zone: { x: 1, y: 1, w: 5, h: 16 } }])
    // the press comes before the flag in the tick, so the flag is first taken with ember on the ceiling
    const s = up(L)
    expect(s.cp).toBe(0)
    expect(s.saved!.players[0].grav).toBe(-1)
    const back = respawn(L, run(L, s, 30, [0, IN_RIGHT]).s)
    expect(back.players[0].grav).toBe(-1)
    expect(head(back, 0)).toBe(CEIL)
    expect(back.deaths).toBe(1)
    expect(run(L, back, 20, [0, 0]).s.players[0].ground).toBe(true)
  })
})
