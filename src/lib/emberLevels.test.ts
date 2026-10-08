import { describe, expect, it } from 'vitest'
import { compileLevel, initState, step, validateLevel } from './emberEngine'
import { LEVELS } from './emberLevels'
import { reachability, renderReach } from './emberSolver'
import { COLS, ROWS, type Cond, type GameState, type Level, type LevelDef, type ThingDef } from './emberTypes'

// the engine may still be a stub while the levels are written; its tests wait for it
const engineReady = (() => {
  try {
    compileLevel(LEVELS[0]!)
    return true
  } catch (e) {
    return !(e instanceof Error && e.message === 'todo')
  }
})()

const conds = (c: Cond | undefined): string[] => (c === undefined ? [] : Array.isArray(c) ? c : [c]).map((s) => s.replace(/^!/, ''))

function channels(def: LevelDef): { used: Set<string>; driven: Set<string> } {
  const used = new Set<string>()
  const driven = new Set<string>()
  for (const t of def.things) {
    if (t.k === 'plate' || t.k === 'lever' || t.k === 'button' || t.k === 'sensor') driven.add(t.ch)
    if (t.k === 'gate') conds(t.open).forEach((c) => used.add(c))
    if (t.k === 'mover' || t.k === 'fan' || t.k === 'emitter') conds(t.when).forEach((c) => used.add(c))
  }
  return { used, driven }
}

function count(def: LevelDef, ch: string): number {
  return def.map.reduce((n, row) => n + [...row].filter((c) => c === ch).length, 0)
}

describe('ember & frost levels', () => {
  it('has a full set', () => {
    expect(LEVELS.length).toBeGreaterThanOrEqual(12)
  })

  it('has unique kebab-case ids and unique names', () => {
    const ids = LEVELS.map((l) => l.id)
    const names = LEVELS.map((l) => l.name)
    expect(new Set(ids).size).toBe(ids.length)
    expect(new Set(names).size).toBe(names.length)
    for (const id of ids) expect(id, id).toMatch(/^[a-z0-9]+(-[a-z0-9]+)*$/)
  })

  for (const [i, def] of LEVELS.entries()) {
    describe(`${i + 1}. ${def.id}`, () => {
      it('is a bordered 32x18 map with one of each spawn and door', () => {
        expect(def.map.length).toBe(ROWS)
        for (const row of def.map) expect(row.length, row).toBe(COLS)
        expect(def.map[0]).toBe('#'.repeat(COLS))
        expect(def.map[ROWS - 1]).toBe('#'.repeat(COLS))
        for (const row of def.map) expect(row[0] + row[COLS - 1], row).toBe('##')
        for (const ch of ['1', '2', 'R', 'B']) expect(count(def, ch), ch).toBe(1)
        expect(def.par).toBeGreaterThan(0)
        expect(def.name).toMatch(/^[a-z0-9 '-]+$/)
      })

      it('validates and compiles', () => {
        expect(validateLevel(def)).toEqual([])
        if (engineReady) expect(() => compileLevel(def)).not.toThrow()
      })

      it('drives every channel it uses, and uses every channel it drives', () => {
        const { used, driven } = channels(def)
        for (const c of used) expect(driven.has(c), `nothing drives '${c}'`).toBe(true)
        for (const c of driven) expect(used.has(c), `nothing listens to '${c}'`).toBe(true)
      })

      it('pairs every portal', () => {
        const pairs = new Map<string, number>()
        for (const t of def.things) if (t.k === 'portal') pairs.set(t.pair, (pairs.get(t.pair) ?? 0) + 1)
        for (const [p, n] of pairs) expect(n, p).toBe(2)
      })

      it('lets each player reach their door and every gem of their element', () => {
        const r = reachability(def)
        const why = `\n${renderReach(def, r)}\nmissed ${JSON.stringify(r.missedGems)}`
        expect(r.fireDoor, why).toBe(true)
        expect(r.frostDoor, why).toBe(true)
        expect(r.missedGems, why).toEqual([])
        expect(r.gemTotals[0]).toBeGreaterThanOrEqual(2)
        expect(r.gemTotals[1]).toBeGreaterThanOrEqual(2)
      })
    })
  }
})

// a small map for solver tests: rows padded into a closed box
function tiny(rows: string[], things: ThingDef[] = []): LevelDef {
  const w = Math.max(...rows.map((r) => r.length)) + 2
  const map = ['#'.repeat(w), ...rows.map((r) => '#' + r.padEnd(w - 2, '.') + '#'), '#'.repeat(w)]
  return { id: 'tiny', name: 'tiny', par: 1, map, things }
}

describe('ember solver', () => {
  it('climbs a 3-tile wall but not a 4-tile one', () => {
    const three = tiny(['.....', '.....', '..#..', '..#..', '12#RB', '#####'])
    const r3 = reachability(three)
    expect(r3.fireDoor && r3.frostDoor).toBe(true)
    const four = tiny(['.....', '.....', '..#..', '..#..', '..#..', '12#RB', '#####'])
    const r4 = reachability(four)
    expect(r4.fireDoor || r4.frostDoor).toBe(false)
  })

  it('jumps a 3-tile gap but not a 5-tile one', () => {
    const gap = (n: number) => tiny(['.'.repeat(n + 4), '.'.repeat(n + 4), '.'.repeat(n + 4), '12' + '.'.repeat(n) + 'RB', '##' + 'G'.repeat(n) + '##'])
    const r3 = reachability(gap(3))
    expect(r3.fireDoor && r3.frostDoor).toBe(true)
    const r5 = reachability(gap(5))
    expect(r5.fireDoor || r5.frostDoor).toBe(false)
  })

  it('lets fire wade lava and frost wade water, but not the other way', () => {
    // a 2-tile ceiling over a wide pool: no jumping it
    const lava = tiny(['##########', '..........', '12......RB', '##LLLLLL##'])
    const rl = reachability(lava)
    expect(rl.fireDoor).toBe(true)
    expect(rl.frostDoor).toBe(false)
    const water = tiny(['##########', '..........', '12......RB', '##WWWWWW##'])
    const rw = reachability(water)
    expect(rw.fireDoor).toBe(false)
    expect(rw.frostDoor).toBe(true)
  })

  it('melts ice only where fire can reach it', () => {
    // frost is shut in behind ice; fire drops down a hole and melts it from the other side
    const lock = tiny(['.........', '1.R......', '######.##', '.........', '2...B....'], [{ k: 'ice', x: 4, y: 4, w: 1, h: 2 }])
    const r = reachability(lock)
    expect(r.melted).toBe(1)
    expect(r.frostDoor).toBe(true)
    const walled = tiny(['.........', '1.R......', '#########', '.........', '2...B....'], [{ k: 'ice', x: 4, y: 4, w: 1, h: 2 }])
    const rw = reachability(walled)
    expect(rw.melted).toBe(0)
    expect(rw.frostDoor).toBe(false)
  })

  it('freezes thin water only where frost can stand near it', () => {
    const near = tiny(['##########', '..........', '12......RB', '##wwwwww##'])
    const rn = reachability(near)
    expect(rn.frozen).toBe(6)
    expect(rn.fireDoor).toBe(true)
    // frost is boxed in far away, so the water stays liquid and fire can't get over it
    const far = tiny(['.#.........', '2#.........', '##.........', '...........', '.1.......RB', '###wwwwww##'])
    const rf = reachability(far)
    expect(rf.frozen).toBe(0)
    expect(rf.fireDoor).toBe(false)
  })
})

// runs input frames [ticks, fire bits, frost bits] through the real engine
function play(level: Level, script: Array<[number, number, number]>, s: GameState = initState(level)): GameState {
  for (const [ticks, fire, frost] of script) for (let t = 0; t < ticks; t++) s = step(level, s, [fire, frost])
  return s
}

type Script = Array<[ticks: number, fire: number, frost: number]>

// hand-made input sequences that finish levels in the real engine (one player moves at a time, mostly)
const WALKTHROUGHS: Record<string, Script> = {
  'first-light': [
    // ember: over the step, through the lava, over the goo, up the step and shelf, down to the red door
    [8, 2, 0], [20, 6, 0], [20, 2, 0], [40, 2, 0], [2, 2, 0], [25, 6, 0], [10, 2, 0], [5, 0, 0], [12, 6, 0], [10, 0, 0],
    [25, 6, 0], [10, 2, 0], [30, 2, 0], [14, 2, 0], [30, 0, 0],
    // frost: onto the step, the shelf over the lava, over the goo and the steps, to the blue door
    [3, 0, 2], [18, 0, 6], [8, 0, 0], [25, 0, 6], [30, 0, 2], [10, 0, 2], [25, 0, 6], [10, 0, 2], [5, 0, 0], [12, 0, 6],
    [10, 0, 0], [40, 0, 2], [30, 0, 2],
  ],
  'two-lanes': [
    // ember: over the goo, up the stairs, the lava lane, down to the red door
    [6, 2, 0], [20, 6, 0], [10, 2, 0], [20, 6, 0], [10, 2, 0], [20, 6, 0], [10, 2, 0], [20, 6, 0], [60, 2, 0], [40, 2, 0],
    [10, 2, 0], [30, 0, 0],
    // frost: over the goo, up two stairs, the ledge, the lip, the water lane, down to the blue door
    [2, 0, 2], [20, 0, 6], [10, 0, 2], [20, 0, 6], [10, 0, 2], [20, 0, 6], [6, 0, 2], [10, 0, 0], [4, 0, 4], [14, 0, 5],
    [15, 0, 0], [22, 0, 6], [15, 0, 2], [60, 0, 2], [30, 0, 2], [20, 0, 2], [7, 0, 2], [20, 0, 0],
  ],
  'hold-the-door': [
    // ember onto the low plate; frost through the low gate, over the water onto the high plate
    [26, 2, 0], [20, 0, 0], [60, 0, 2], [60, 0, 2], [30, 0, 2], [12, 0, 2], [20, 0, 0],
    // ember up the ledge and shelf, through the high gate and the lava to the red door
    [20, 1, 0], [18, 5, 0], [10, 0, 0], [20, 6, 0], [10, 2, 0], [60, 2, 0], [60, 2, 0], [20, 2, 0], [44, 2, 0], [20, 0, 0],
    // frost off the plate to the blue door
    [33, 0, 2], [20, 0, 0],
  ],
  // the rest are terser: each was found by stepping the real engine and checking positions
  'lockstep': [
    [28, 2, 0], [10, 0, 0], [2, 8, 0], [2, 0, 0], [30, 0, 0], [80, 0, 2], [10, 0, 0], [12, 0, 2], [10, 0, 0],
    [2, 0, 8], [2, 0, 0], [30, 0, 0], [66, 2, 0], [10, 0, 0], [2, 8, 0], [2, 0, 0], [30, 0, 0], [30, 0, 2],
    [20, 0, 6], [12, 0, 2], [10, 0, 0], [2, 0, 8], [2, 0, 0], [30, 0, 0], [30, 2, 0], [20, 6, 0], [12, 2, 0],
    [10, 0, 0], [2, 8, 0], [2, 0, 0], [30, 0, 0], [32, 0, 2], [10, 0, 0], [2, 0, 8], [2, 0, 0], [30, 0, 0],
    [57, 2, 0], [20, 0, 0], [25, 0, 2], [20, 0, 0],
  ],
  'the-lifts': [
    [40, 0, 1], [15, 0, 5], [20, 0, 1], [10, 0, 0], [124, 2, 0], [10, 0, 0], [220, 0, 0], [60, 0, 2], [10, 0, 0],
    [22, 0, 2], [185, 0, 0], [16, 0, 2], [100, 0, 0], [30, 0, 2], [14, 2, 2], [12, 6, 0], [20, 2, 0], [10, 0, 0],
    [2, 0, 8], [2, 0, 0], [250, 0, 0], [20, 5, 0], [10, 1, 0], [20, 0, 0], [26, 1, 2], [17, 1, 0], [20, 0, 0],
  ],
  'crates': [
    [16, 2, 0], [20, 0, 0], [6, 1, 0], [2, 0, 2], [16, 0, 6], [50, 0, 2], [10, 0, 2], [18, 0, 6], [10, 0, 2],
    [5, 0, 0], [18, 0, 6], [8, 0, 0], [18, 0, 6], [8, 0, 0], [18, 0, 6], [8, 0, 0], [18, 0, 6], [8, 0, 0],
    [18, 0, 6], [8, 0, 0], [20, 0, 0], [18, 0, 6], [8, 0, 0], [20, 0, 0], [22, 0, 5], [10, 0, 1], [10, 0, 0],
    [80, 0, 1], [40, 0, 0], [20, 0, 5], [60, 0, 1], [21, 0, 1], [40, 0, 0], [20, 6, 0], [60, 2, 0], [18, 6, 0],
    [8, 0, 0], [18, 6, 0], [8, 0, 0], [18, 6, 0], [8, 0, 0], [18, 6, 0], [8, 0, 0], [18, 6, 0], [8, 0, 0],
    [20, 0, 0], [18, 6, 0], [8, 0, 0], [20, 2, 0], [20, 0, 0], [60, 0, 2], [20, 0, 6], [60, 0, 2], [30, 0, 0],
    [10, 0, 2], [20, 0, 0], [18, 0, 6], [8, 0, 0], [50, 0, 2], [30, 0, 0],
  ],
  'thaw': [
    [98, 2, 0], [60, 1, 2], [30, 1, 2], [40, 0, 2], [15, 0, 0], [18, 5, 0], [10, 0, 0], [20, 6, 0], [10, 2, 0],
    [60, 2, 0], [60, 2, 0], [30, 2, 0], [20, 0, 0], [64, 2, 0], [20, 0, 0], [70, 0, 2], [20, 0, 0],
  ],
  'white-crossing': [
    [60, 2, 2], [20, 0, 0], [24, 0, 2], [22, 0, 6], [10, 0, 0], [12, 0, 2], [3, 0, 2], [10, 0, 0], [24, 2, 0],
    [22, 6, 0], [10, 0, 0], [20, 6, 0], [10, 0, 0], [70, 2, 2], [12, 2, 0], [20, 0, 0], [22, 0, 2], [20, 0, 0],
    [20, 2, 0], [20, 0, 0],
  ],
  'updraft': [
    [116, 2, 0], [10, 0, 0], [2, 8, 0], [2, 0, 0], [20, 0, 0], [30, 0, 2], [24, 0, 6], [8, 0, 2], [60, 0, 0],
    [40, 0, 2], [10, 0, 0], [36, 0, 2], [10, 0, 0], [2, 0, 8], [2, 0, 0], [66, 2, 0], [70, 0, 0], [30, 2, 0],
    [20, 0, 0], [10, 0, 2], [20, 0, 0],
  ],
  'relay': [
    [30, 0, 2], [10, 0, 0], [14, 2, 0], [30, 2, 0], [22, 6, 0], [60, 2, 0], [20, 2, 0], [10, 0, 0], [20, 2, 0],
    [10, 0, 0], [22, 0, 2], [50, 0, 2], [20, 0, 6], [80, 0, 2], [20, 0, 0], [30, 0, 2], [85, 0, 2], [10, 0, 0],
    [50, 2, 0], [20, 0, 0], [103, 1, 0], [20, 0, 0],
  ],
  'break-the-beam': [
    [22, 1, 0], [20, 0, 0], [124, 0, 2], [20, 0, 0], [2, 0, 8], [2, 0, 0], [10, 0, 0], [12, 0, 1], [20, 0, 0],
    [18, 5, 0], [10, 0, 0], [20, 6, 0], [10, 2, 0], [60, 2, 0], [60, 2, 0], [60, 2, 0], [20, 0, 0],
  ],
  'two-mirrors': [
    [4, 1, 0], [20, 5, 0], [10, 0, 0], [20, 5, 0], [10, 0, 0], [20, 5, 0], [10, 0, 0], [12, 1, 0], [22, 5, 0],
    [10, 1, 0], [20, 1, 0], [10, 0, 0], [2, 8, 0], [2, 0, 0], [20, 5, 0], [30, 1, 0], [12, 1, 0], [30, 0, 0],
    [4, 0, 1], [22, 0, 5], [20, 0, 1], [50, 0, 1], [30, 0, 1], [10, 0, 0], [20, 0, 5], [20, 0, 0], [4, 0, 1],
    [10, 0, 0], [2, 0, 8], [2, 0, 0], [20, 0, 0], [40, 1, 0], [78, 0, 1], [20, 0, 0],
  ],
  'cold-storage': [
    [70, 1, 0], [10, 0, 0], [22, 2, 0], [18, 6, 0], [20, 0, 0], [36, 0, 2], [24, 0, 6], [20, 0, 2], [10, 0, 0],
    [220, 0, 0], [91, 2, 0], [40, 0, 0], [20, 6, 0], [20, 2, 0], [10, 0, 0], [30, 0, 2], [18, 0, 6], [8, 0, 0],
    [18, 0, 6], [8, 0, 0], [14, 0, 6], [10, 0, 0], [58, 2, 2], [19, 2, 0], [20, 0, 0],
  ],
  'lantern-works': [
    [22, 0, 1], [10, 0, 0], [2, 0, 8], [2, 0, 0], [20, 2, 0], [20, 6, 0], [24, 2, 0], [2, 2, 0], [40, 0, 0],
    [12, 0, 0], [2, 8, 0], [2, 0, 0], [27, 0, 0], [20, 1, 0], [20, 0, 0], [4, 0, 1], [22, 0, 5], [10, 0, 1],
    [10, 0, 0], [16, 0, 1], [30, 0, 0], [32, 0, 2], [20, 0, 0], [70, 1, 0], [10, 0, 0], [20, 2, 0], [10, 0, 0],
    [2, 8, 0], [2, 0, 0], [10, 0, 0], [60, 0, 2], [10, 0, 0], [70, 2, 0], [30, 0, 0], [44, 2, 1], [68, 0, 1],
    [20, 0, 0],
  ],
  'last-light': [
    [21, 1, 0], [20, 0, 0], [2, 8, 0], [2, 0, 0], [16, 2, 0], [20, 0, 0], [106, 0, 2], [15, 0, 0], [86, 2, 0],
    [40, 0, 0], [38, 0, 2], [50, 0, 0], [24, 0, 2], [10, 0, 0], [22, 6, 0], [10, 0, 0], [44, 2, 2], [30, 2, 0],
    [20, 0, 0],
  ],
}

describe.skipIf(!engineReady)('ember levels in the engine', () => {
  for (const [i, def] of LEVELS.entries()) {
    it(`${i + 1}. ${def.id}: nobody dies standing still`, () => {
      const level = compileLevel(def)
      const s = play(level, [[120, 0, 0]])
      expect(s.phase).toBe('play')
      expect(s.players.map((p) => p.alive)).toEqual([true, true])
      expect(s.deaths).toBe(0)
    })
  }

  // scripted playthroughs: [ticks, fire bits, frost bits] with 1 left, 2 right, 4 jump, 8 down
  for (const [id, script] of Object.entries(WALKTHROUGHS)) {
    it(`${id}: the walkthrough wins`, () => {
      const def = LEVELS.find((l) => l.id === id)
      expect(def, id).toBeDefined()
      const level = compileLevel(def!)
      let s = initState(level)
      for (const [ticks, fire, frost] of script) {
        for (let t = 0; t < ticks && s.phase === 'play'; t++) s = step(level, s, [fire, frost])
      }
      expect(s.deaths).toBe(0)
      expect(s.phase).toBe('won')
    })
  }
})
