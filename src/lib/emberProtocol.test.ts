import { describe, expect, it, vi } from 'vitest'
import { compileLevel, initState, stageOf, startStage, step } from './emberEngine'
import { channelsOf, decodeEmber, encodeEmber, eventFits, fastOf, joinState, MAX_TEXT, slowOf, type StateMsg } from './emberProtocol'
import { CRUMBLE_TICKS, FREEZE_TICKS, type GameEvent, type GameState, type LevelDef, type ThingDef } from './emberTypes'

// the decoder only asks the level list how many levels there are
vi.mock('./emberLevels', () => ({ LEVELS: [{}, {}, {}] }))

// a stage of cols x rows, walled, with glyphs dropped at [x, y] and a floor to stand on
function stageDef(cols: number, rows: number, cells: Array<[number, number, string]>, things: ThingDef[] = [], dark = false) {
  const grid = Array.from({ length: rows }, (_, y) => Array.from({ length: cols }, (_, x): string => (x === 0 || y === 0 || x === cols - 1 || y === rows - 1 ? '#' : '.')))
  for (const [x, y, c] of cells) grid[y]![x] = c
  return { map: grid.map((r) => r.join('')), things, dark }
}

// the biggest stage there is, with every kind of thing in it: 60 gems, 40 crumbling tiles, 30 thin tiles
function bigDef(): LevelDef {
  const cells: Array<[number, number, string]> = [
    [2, 70, '1'],
    [4, 70, '2'],
    [120, 70, 'R'],
    [122, 70, 'B'],
    [8, 70, 'C'],
    [60, 70, 'C'],
    [30, 66, 'k'],
    [32, 66, 'k'],
  ]
  for (let i = 0; i < 60; i++) cells.push([10 + i, 60, i % 2 ? 'r' : 'b'])
  for (let i = 0; i < 40; i++) cells.push([10 + i, 50, '~'])
  for (let i = 0; i < 30; i++) cells.push([10 + i, 40, 'w'])
  for (let i = 0; i < 4; i++) cells.push([20 + 3 * i, 30, '^'])
  for (let i = 0; i < 6; i++) cells.push([70 + i, 50, 'T'], [80 + i, 50, '='], [90 + i, 50, 'O'], [100 + i, 50, 'D'])
  const things: ThingDef[] = [
    { k: 'plate', x: 12, y: 70, ch: 'a' },
    { k: 'plate', x: 16, y: 20, ch: 'roof', ceil: true },
    { k: 'lever', x: 20, y: 70, ch: 'b' },
    { k: 'lever', x: 22, y: 70, ch: 'c', on: true },
    { k: 'button', x: 24, y: 70, ch: 'd', secs: 3 },
    { k: 'button', x: 26, y: 70, ch: 'e', secs: 2 },
    { k: 'gate', x: 40, y: 66, w: 1, h: 4, open: 'a' },
    { k: 'gate', x: 44, y: 66, w: 1, h: 4, open: ['b', '!lights'] },
    { k: 'gate', x: 48, y: 66, w: 1, h: 4, open: 'd' },
    { k: 'mover', x: 50, y: 60, w: 3, h: 1, to: { x: 60, y: 60 } },
    { k: 'mover', x: 50, y: 56, w: 3, h: 1, to: { x: 50, y: 46 }, when: 'c' },
    { k: 'mover', x: 70, y: 56, w: 3, h: 1, to: { x: 76, y: 52 }, when: 'e' },
    { k: 'box', x: 64, y: 70 },
    { k: 'box', x: 66, y: 70 },
    { k: 'box', x: 68, y: 70 },
    { k: 'box', x: 70, y: 70 },
    { k: 'fan', x: 74, y: 70, w: 2, h: 6, when: 'a' },
    { k: 'ice', x: 78, y: 68, w: 2, h: 3 },
    { k: 'ice', x: 82, y: 68, w: 2, h: 3 },
    { k: 'emitter', x: 86, y: 68, dir: 'right' },
    { k: 'mirror', x: 90, y: 68, slant: '/' },
    { k: 'mirror', x: 90, y: 64, slant: '\\' },
    { k: 'sensor', x: 96, y: 64, ch: 'seen' },
    { k: 'portal', x: 100, y: 70, pair: 'p' },
    { k: 'portal', x: 110, y: 70, pair: 'p' },
    { k: 'sign', x: 5, y: 66, text: 'hello' },
    { k: 'checkpoint', x: 90, y: 70, zone: { x: 86, y: 66, w: 8, h: 5 } },
    { k: 'lamp', x: 14, y: 70, secs: 4 },
    { k: 'gswitch', x: 16, y: 70, who: 'other' },
    { k: 'lock', x: 104, y: 66, w: 1, h: 4 },
    { k: 'lock', x: 106, y: 66, w: 1, h: 4 },
    { k: 'cannon', x: 112, y: 70, aim: 'upleft', fire: 'a' },
    { k: 'cannon', x: 114, y: 70, aim: 'up' },
    { k: 'dark', x: 10, y: 10, w: 20, h: 20 },
  ]
  return {
    id: 't-big',
    name: 'big',
    par: 100,
    stages: [stageDef(32, 18, [[2, 16, '1'], [4, 16, '2'], [20, 16, 'R'], [22, 16, 'B']]), stageDef(128, 72, cells, things, true)],
  }
}

const level = compileLevel(bigDef())
const big = level.stages[1]!

// a busy moment on the big stage: every state field away from its default
function busy(): GameState {
  const s = startStage(level, 1, { tick: 4321, deaths: 7, banked: [3, 2] })
  s.phaseT = 512
  s.players[0] = { ...s.players[0], x: 1234.56, y: 987.65, vx: -149.9, vy: 233.3, ground: false, face: -1, coyote: 3, buffer: 2, prevIn: 6, portalCd: 11, gems: 4, air: 1, fly: 20, cannonCd: 9 }
  s.players[1] = { ...s.players[1], x: 2200.1, y: 1400, grav: -1, inDoor: true, gems: 9, cannon: 1 }
  s.taken = s.taken.map((_, i) => i % 3 === 0)
  s.levers = [true, false]
  s.buttons = [0, 77]
  s.gates = [0.3333, 1, 0]
  s.movers[0] = { x: 1111.11, y: 1200, t: 0.5555, dir: -1 }
  s.movers[2] = { x: 1400, y: 1120, t: 0, dir: 1 }
  s.boxes[1] = { x: 1333.33, y: 1380.5, vx: 70, vy: -12.5 }
  s.boxes[3] = { x: 1400, y: 1300, vx: 0, vy: 310 }
  s.ice = [0.4222, 1]
  s.thin = s.thin.map((_, i) => (i >= 10 && i < 15 ? FREEZE_TICKS - i : 0))
  s.mirrors = ['\\', '\\']
  s.lit = !s.lit
  s.litT = 150
  s.arrows = [0, 120, 0, 7]
  s.crumble = s.crumble.map((_, i) => (i >= 5 && i < 17 ? CRUMBLE_TICKS + i : 0))
  s.keys = 1
  s.keyTaken = [true, true]
  s.locks = [false, true]
  s.cannons = [0, 25]
  s.cp = 2
  s.on = ['b', 'd', 'seen']
  return s
}

const stateMsg = (s: GameState, events: GameEvent[] = [], key = true): StateMsg => ({
  t: 's',
  a: 3,
  f: 5000,
  fi: 5,
  sl: [2, 4990],
  r: 12,
  ...(key ? { k: slowOf(1, stageOf(level, s), s) } : {}),
  g: fastOf(stageOf(level, s), s),
  e: events,
})

// through the wire and back into a state
function across(s: GameState, lvl = level): GameState | null {
  const m = decodeEmber(encodeEmber(stateMsg(s)))
  return m?.t === 's' && m.k ? joinState(lvl, m.k, m.g) : null
}

// what the wire is allowed to lose: saved, plates, sensors, and sub-pixel digits
function rounded(s: GameState): GameState {
  const { saved: _s, plates: _p, sensors: _n, ...rest } = s
  return JSON.parse(JSON.stringify(rest, (_k, v: unknown) => (typeof v === 'number' ? Math.round(v * 10) / 10 : v))) as GameState
}

describe('the wire: state', () => {
  it('carries every field of a busy state', () => {
    const s = busy()
    const back = across(s)
    expect(back).not.toBe(null)
    expect(rounded(back!)).toEqual(rounded({ ...s, events: [] }))
    // positions keep two decimals, the rest is exact
    expect(back!.players[0].x).toBe(1234.56)
    expect(back!.movers[0]).toEqual({ x: 1111.11, y: 1200, t: 0.5555, dir: -1 })
    expect(back!.crumble).toEqual(s.crumble)
    expect(back!.thin).toEqual(s.thin)
    expect(back!.on).toEqual(['b', 'd', 'seen'])
  })

  it('carries a fresh state of every stage, and every phase', () => {
    for (const i of [0, 1]) {
      const s = startStage(level, i)
      expect(rounded(across(s)!)).toEqual(rounded(s))
    }
    for (const phase of ['play', 'dead', 'clear', 'won'] as const) expect(across({ ...busy(), phase })!.phase).toBe(phase)
  })

  it('never sends saved, plates or sensors', () => {
    const s = busy()
    s.saved = JSON.parse(JSON.stringify({ ...s, events: undefined })) as GameState
    const text = encodeEmber(stateMsg(s))
    expect(text).not.toMatch(/saved|plates|sensors/)
    const back = across(s)!
    expect('saved' in back).toBe(false)
    expect(back.plates).toBeUndefined()
    // and what comes back still steps
    expect(() => step(level, back, [0, 0])).not.toThrow()
  })

  it('leaves out what is at rest', () => {
    const s = startStage(level, 0)
    const m = stateMsg(s)
    expect(Object.keys(m.g).sort()).toEqual(['T', 'p', 'pt'])
    expect(m.g.p[0]).toHaveLength(5)
    expect(m.k).toEqual({ l: 1 })
    // a mover on its path goes as its progress alone
    const b = startStage(level, 1)
    expect(fastOf(big, b).mv).toEqual([[0], [1], [0]])
    expect(fastOf(big, b).bx!.every((x) => x.length === 2)).toBe(true)
  })

  it('the slow part reads the same until something slow changes', () => {
    const a = initState(level)
    let b = a
    for (let i = 0; i < 30; i++) b = step(level, b, [2, 1])
    expect(b.players[0].x).not.toBe(a.players[0].x)
    expect(JSON.stringify(slowOf(0, level.stages[0]!, b))).toBe(JSON.stringify(slowOf(0, level.stages[0]!, a)))
    expect(JSON.stringify(slowOf(0, level.stages[0]!, { ...b, deaths: 1 }))).not.toBe(JSON.stringify(slowOf(0, level.stages[0]!, b)))
    // decoding rebuilds it key for key
    const k = slowOf(1, big, busy())
    const m = decodeEmber(encodeEmber(stateMsg(busy())))
    expect(m?.t === 's' && JSON.stringify(m.k)).toBe(JSON.stringify(k))
  })

  it('names channels by the stage', () => {
    expect(channelsOf(big)).toEqual(['a', 'b', 'c', 'd', 'e', 'roof', 'seen'])
    const s = busy()
    s.on = ['b', 'zz']
    expect(across(s)!.on).toEqual(['b', 'zz'])
  })

  it('a busy frame on the biggest stage is small', () => {
    const s = busy()
    const events: GameEvent[] = [
      { k: 'jump', p: 0, x: 1234.5, y: 900 },
      { k: 'shake', tile: 6410, x: 210, y: 1010 },
      { k: 'gem', p: 1, x: 300, y: 1210, el: 'frost' },
    ]
    const full = encodeEmber(stateMsg(s, events))
    const fast = encodeEmber(stateMsg(s, events, false))
    expect(full.length).toBeLessThan(1200)
    expect(fast.length).toBeLessThan(1000)
    expect(full.length).toBeLessThan(MAX_TEXT)
    // a quiet moment on the same stage
    const quiet = startStage(level, 1, { tick: 4321, deaths: 7, banked: [3, 2] })
    expect(encodeEmber(stateMsg(quiet, [], false)).length).toBeLessThan(260)
  })

  it('refuses a state that does not fit its stage', () => {
    const m = decodeEmber(encodeEmber(stateMsg(busy()))) as StateMsg
    const k = m.k!
    expect(joinState(level, k, m.g)).not.toBe(null)
    // the same parts against the small stage, or a stage that isn't there
    expect(joinState(level, { ...k, st: undefined }, m.g)).toBe(null)
    expect(joinState(level, { ...k, st: 2 }, m.g)).toBe(null)
    expect(joinState(level, { ...k, cp: 3 }, m.g)).toBe(null)
    expect(joinState(level, { ...k, gm: 'ffffffffffffffff' }, m.g)).toBe(null)
    expect(joinState(level, { ...k, lv: '4' }, m.g)).toBe(null)
    expect(joinState(level, { ...k, on: 'ff' }, m.g)).toBe(null)
    expect(joinState(level, k, { ...m.g, cr: [40, 3] })).toBe(null)
    expect(joinState(level, k, { ...m.g, cn: [2, 3] })).toBe(null)
    expect(joinState(level, k, { ...m.g, bt: [1, 2, 3] })).toBe(null)
    expect(joinState(level, k, { ...m.g, mv: [[0]] })).toBe(null)
    expect(joinState(level, k, { ...m.g, bx: undefined })).toBe(null)
    const inCannon = [...m.g.p[0]]
    inCannon[10] = 3
    expect(joinState(level, k, { ...m.g, p: [inCannon, m.g.p[1]] })).toBe(null)
  })
})

describe('the wire: events', () => {
  const all: GameEvent[] = [
    { k: 'jump', p: 0, x: 10, y: 20 },
    { k: 'land', p: 1, x: 10, y: 20 },
    { k: 'die', p: 1, x: 10, y: 20, cause: 'lava' },
    { k: 'die', p: 0, x: 10, y: 20, cause: 'crush' },
    { k: 'door', p: 0, x: 10, y: 20 },
    { k: 'portal', p: 1, x: 10, y: 20 },
    { k: 'push', p: 0, x: 10, y: 20 },
    { k: 'airjump', p: 1, x: 10, y: 20 },
    { k: 'bounce', p: 0, x: 10, y: 20, v: 640 },
    { k: 'gem', p: 1, x: 10, y: 20, el: 'frost' },
    { k: 'arrow', p: 0, i: 3, x: 10, y: 20 },
    { k: 'key', p: 1, i: 1, x: 10, y: 20 },
    { k: 'load', p: 0, i: 1, x: 10, y: 20 },
    { k: 'fire', p: 1, i: 0, x: 10, y: 20 },
    { k: 'flip', p: 1, x: 10, y: 20, grav: -1 },
    { k: 'lever', i: 1, x: 10, y: 20 },
    { k: 'mirror', i: 1, x: 10, y: 20 },
    { k: 'gswitch', i: 0, x: 10, y: 20 },
    { k: 'arrowback', i: 2, x: 10, y: 20 },
    { k: 'unlock', i: 1, x: 10, y: 20 },
    { k: 'checkpoint', i: 2, x: 10, y: 20 },
    { k: 'plate', i: 1, on: true, x: 10, y: 20 },
    { k: 'button', i: 0, on: false, x: 10, y: 20 },
    { k: 'sensor', i: 0, on: true, x: 10, y: 20 },
    { k: 'light', on: false, x: 1280, y: 720 },
    { k: 'melt', i: 1, x: 10, y: 20 },
    { k: 'freeze', tile: 5130, x: 10, y: 20 },
    { k: 'thaw', tile: 5131, x: 10, y: 20 },
    { k: 'shake', tile: 6410, x: 10, y: 20 },
    { k: 'break', tile: 6411, x: 10, y: 20 },
    { k: 'mend', tile: 9215, x: 10, y: 20 },
    { k: 'clear', stage: 0 },
    { k: 'stage', stage: 1 },
    { k: 'respawn', cp: -1 },
    { k: 'respawn', cp: 2 },
    { k: 'win' },
  ]

  it('carries every kind of event, in both batches', () => {
    const m = decodeEmber(encodeEmber({ ...stateMsg(busy(), all), q: [4998, all] })) as StateMsg
    expect(m.e).toEqual(all)
    expect(m.q).toEqual([4998, all])
    for (const e of all) expect(eventFits(level, big, e)).toBe(true)
  })

  it('knows an event that points outside its stage', () => {
    const small = level.stages[0]!
    expect(eventFits(level, small, { k: 'lever', i: 0, x: 0, y: 0 })).toBe(false)
    expect(eventFits(level, small, { k: 'checkpoint', i: 0, x: 0, y: 0 })).toBe(false)
    expect(eventFits(level, small, { k: 'respawn', cp: 0 })).toBe(false)
    expect(eventFits(level, small, { k: 'respawn', cp: -1 })).toBe(true)
    expect(eventFits(level, small, { k: 'shake', tile: 576, x: 0, y: 0 })).toBe(false)
    expect(eventFits(level, small, { k: 'stage', stage: 2 })).toBe(false)
    expect(eventFits(level, big, { k: 'fire', p: 0, i: 2, x: 0, y: 0 })).toBe(false)
  })
})

describe('the wire: strictness', () => {
  const good = encodeEmber(stateMsg(busy(), [{ k: 'jump', p: 0, x: 1, y: 2 }]))
  const edit = (f: (o: Record<string, any>) => void) => {
    const o = JSON.parse(good) as Record<string, any>
    f(o)
    return JSON.stringify(o)
  }

  it('reads what it wrote', () => {
    expect(decodeEmber(good)?.t).toBe('s')
    expect(decodeEmber(edit(() => {}))).toEqual(decodeEmber(good))
  })

  it.each<[string, (o: Record<string, any>) => void]>([
    ['an unknown key', (o) => (o.zz = 1)],
    ['an unknown fast key', (o) => (o.g.saved = {})],
    ['an unknown slow key', (o) => (o.k.plates = [])],
    ['no fast part', (o) => delete o.g],
    ['a level that is not there', (o) => (o.k.l = 3)],
    ['a slow part without its level', (o) => delete o.k.l],
    ['a negative attempt', (o) => (o.a = -1)],
    ['a fractional frame', (o) => (o.f = 1.5)],
    ['keys out of range', (o) => (o.fi = 16)],
    ['a missing slow number', (o) => delete o.r],
    ['a phase that is not one', (o) => (o.g.ph = 4)],
    ['a negative clock', (o) => (o.g.T = -1)],
    ['three players', (o) => o.g.p.push(o.g.p[0])],
    ['a short player', (o) => (o.g.p[0] = [1, 2, 3, 4])],
    ['a long player', (o) => (o.g.p[0] = Array.from({ length: 14 }, () => 0))],
    ['a player far outside any stage', (o) => (o.g.p[0][0] = 1e6)],
    ['a player at null', (o) => (o.g.p[1][1] = null)],
    ['a speed out of range', (o) => (o.g.p[0][3] = -99999)],
    ['player flags out of range', (o) => (o.g.p[0][4] = 16)],
    ['two arrows held', (o) => (o.g.p[0][9] = 2)],
    ['a gate past open', (o) => (o.g.gt[0] = 1.2)],
    ['a mover going nowhere', (o) => (o.g.mv[0] = [0.5, 0])],
    ['a mover with half a position', (o) => (o.g.mv[0] = [0.5, 1, 3])],
    ['a box with one number', (o) => (o.g.bx[0] = [5])],
    ['sparse timers of odd length', (o) => o.g.cr.push(39)],
    ['sparse timers out of order', (o) => (o.g.cr = [7, 3, 5, 3])],
    ['a sparse timer at zero', (o) => (o.g.th = [3, 0])],
    ['a timer far too long', (o) => (o.g.ar = [1, 1e7])],
    ['too many buttons', (o) => (o.g.bt = Array.from({ length: 600 }, () => 1))],
    ['a stage out of range', (o) => (o.k.st = 64)],
    ['a mask that is not hex', (o) => (o.k.gm = 'xyz')],
    ['a mask that is a number', (o) => (o.k.lv = 5)],
    ['a mask far too long', (o) => (o.k.gm = 'f'.repeat(200))],
    ['lights neither on nor off', (o) => (o.k.dk = 2)],
    ['gravity bits out of range', (o) => (o.k.gv = 4)],
    ['negative deaths', (o) => (o.k.d = -2)],
    ['banked as one number', (o) => (o.k.bk = [1])],
    ['a channel that is not a string', (o) => (o.k.ox = [5])],
    ['an event of no kind', (o) => (o.e = [[99, 0, 1, 2]])],
    ['an event with too little', (o) => (o.e = [[0, 0, 1]])],
    ['an event with a string', (o) => (o.e = [[0, 0, 'x', 2]])],
    ['an event for a third player', (o) => (o.e = [[0, 2, 1, 2]])],
    ['too many events', (o) => (o.e = Array.from({ length: 65 }, () => [0, 0, 1, 2]))],
    ['an earlier batch from the future', (o) => (o.q = [9999, []])],
    ['an earlier batch that is junk', (o) => (o.q = [10, [[99]]])],
    ['slack as one number', (o) => (o.sl = [1])],
  ])('refuses %s', (_name, f) => {
    expect(decodeEmber(edit(f))).toBe(null)
  })

  it('refuses an oversized frame, and anything that is not a message', () => {
    // more running timers than any stage could have, though still under the size limit
    const many = edit((o) => (o.g.th = Array.from({ length: 2400 }, (_, i) => (i % 2 ? 150 : i))))
    expect(many.length).toBeLessThan(MAX_TEXT)
    expect(decodeEmber(many)).toBe(null)
    expect(decodeEmber(good + ' '.repeat(MAX_TEXT))).toBe(null)
    for (const junk of ['', 'null', '[]', '{}', '"s"', '{"t":"s"}', '{"t":"zz"}', '1e999', '{"t":"k","a":1,"f":-1,"i":[]}', '{"t":"k","a":1,"f":1,"i":[],"r":-1}', '{"t":"pick","l":99}', '{"t":"go","l":1.5}', '{"t":"note","w":"hi"}'])
      expect(decodeEmber(junk)).toBe(null)
  })

  it('survives a thousand garbled frames', () => {
    let seed = 11
    const rand = () => ((seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648)
    for (let i = 0; i < 1000; i++) {
      const at = Math.floor(rand() * good.length)
      const pick = rand()
      const text =
        pick < 0.3
          ? good.slice(0, at)
          : pick < 0.6
            ? good.slice(0, at) + String.fromCharCode(32 + Math.floor(rand() * 90)) + good.slice(at + 1)
            : good.replace(/-?\d+(\.\d+)?/g, (n) => (rand() < 0.05 ? ['1e999', '-1', '9999999', '0.5', '"x"', 'null', '[]', '{}'][Math.floor(rand() * 8)]! : n))
      let m: ReturnType<typeof decodeEmber> = null
      expect(() => (m = decodeEmber(text))).not.toThrow()
      const got = m as ReturnType<typeof decodeEmber>
      // whatever does decode either fits the level or is turned away, and what fits can be stepped
      if (got?.t !== 's' || !got.k) continue
      let s: GameState | null = null
      expect(() => (s = joinState(level, got.k!, got.g))).not.toThrow()
      if (s) expect(() => step(level, s!, [0, 0])).not.toThrow()
    }
  })
})

describe('the wire: the small messages', () => {
  it('keeps key messages tiny', () => {
    expect(encodeEmber({ t: 'k', a: 3, f: 120, i: [[118, 2]] })).toBe('{"t":"k","a":3,"f":120,"i":[[118,2]]}')
    expect(decodeEmber('{"t":"k","a":3,"f":120,"i":[[118,2]],"r":7}')).toEqual({ t: 'k', a: 3, f: 120, i: [[118, 2]], r: 7 })
  })

  it('reads a hello from any version, so it can say so', () => {
    // version 1's hi carried u; a bare hi is pong's (hello.ts)
    expect(decodeEmber('{"t":"hi","v":1,"u":3}')).toEqual({ t: 'hi', v: 1, w: 0 })
    expect(decodeEmber('{"t":"hi","v":2,"w":1,"ok":true}')).toEqual({ t: 'hi', v: 2, w: 1, ok: true })
    expect(decodeEmber('{"t":"hi"}')).toBe(null)
  })

  it('round-trips the rest', () => {
    for (const m of [{ t: 'bye' }, { t: 'sel', l: 2 }, { t: 'pick', l: 0 }, { t: 'go', l: 1 }, { t: 'retry' }, { t: 'restart' }, { t: 'next' }, { t: 'menu' }, { t: 'note', w: 'restart' }] as const)
      expect(decodeEmber(encodeEmber(m))).toEqual(m)
  })
})
