import { describe, expect, it, vi } from 'vitest'
import { cameraFor } from '../lib/emberCamera'
import { compileLevel, initState } from '../lib/emberEngine'
import { CRUMBLE_TICKS, GLOW_EMBER, GLOW_FROST, PLAYER_H, PLAYER_W, TILE } from '../lib/emberTypes'
import type { GameEvent, GameState, LevelDef, StageDef } from '../lib/emberTypes'
import { drawThumb, EmberRenderer } from './emberDraw'
import { chunkRange, darknessAt, darkRects, inView, lightHoles, mipFor, MIPS, toCanvas, toWorld, viewFor, visibleTiles } from './emberFx'

// a canvas that swallows every call: enough to run a whole frame in node and see that nothing throws
function fakeCanvas(w: number, h: number): HTMLCanvasElement {
  const noop = () => {}
  const grad = { addColorStop: noop }
  const ctx = new Proxy({} as Record<string, unknown>, {
    get(t, k) {
      if (k in t) return t[k as string]
      if (k === 'createLinearGradient' || k === 'createRadialGradient') return () => grad
      if (k === 'measureText') return () => ({ width: 20 })
      return noop
    },
    set(t, k, v) {
      t[k as string] = v
      return true
    },
  })
  return { width: w, height: h, getContext: () => ctx, getBoundingClientRect: () => ({ width: w, height: h }) } as unknown as HTMLCanvasElement
}

function stage(cols: number, rows: number, put: Record<string, string>, extra: Partial<StageDef> = {}): StageDef {
  const map: string[][] = []
  for (let y = 0; y < rows; y++) map.push(Array.from({ length: cols }, (_, x) => (x === 0 || y === 0 || x === cols - 1 || y === rows - 1 ? '#' : '.')))
  for (const [at, glyphs] of Object.entries(put)) {
    const [x, y] = at.split(',').map(Number)
    for (let i = 0; i < glyphs.length; i++) map[y][x + i] = glyphs[i]
  }
  return { map: map.map((r) => r.join('')), things: [], ...extra }
}

const everything: LevelDef = {
  id: 'scratch',
  name: 'scratch',
  par: 60,
  stages: [
    stage(
      64,
      36,
      { '2,34': '1.2', '8,34': 'R.B', '12,34': 'r.b', '16,34': 'TT.==.~~.OO.DD', '32,34': '^.k.C', '40,34': 'LLWWGGww' },
      {
        dark: true,
        things: [
          { k: 'lamp', x: 4, y: 30, secs: 4 },
          { k: 'gswitch', x: 6, y: 30, who: 'other' },
          { k: 'gswitch', x: 8, y: 30, who: 'both' },
          { k: 'lock', x: 50, y: 30, w: 1, h: 2 },
          { k: 'cannon', x: 54, y: 30, aim: 'upright' },
          { k: 'cannon', x: 56, y: 30, aim: 'left', fire: 'a' },
          { k: 'plate', x: 20, y: 20, ch: 'a', ceil: true },
          { k: 'button', x: 24, y: 20, ch: 'b', secs: 2, ceil: true },
          { k: 'dark', x: 2, y: 2, w: 20, h: 10 },
          { k: 'checkpoint', x: 44, y: 20 },
        ],
      },
    ),
    stage(32, 18, { '2,16': '1.2', '8,16': 'R.B' }),
  ],
}

const ALL_EVENTS: GameEvent[] = [
  { k: 'jump', p: 0, x: 50, y: 680 },
  { k: 'land', p: 1, x: 50, y: 680 },
  { k: 'die', p: 0, x: 50, y: 680, cause: 'crush' },
  { k: 'door', p: 0, x: 50, y: 680 },
  { k: 'airjump', p: 1, x: 50, y: 680 },
  { k: 'bounce', p: 0, x: 330, y: 680, v: 600 },
  { k: 'gem', p: 0, x: 50, y: 680, el: 'fire' },
  { k: 'arrow', p: 0, i: 0, x: 650, y: 690 },
  { k: 'key', p: 1, i: 0, x: 690, y: 690 },
  { k: 'load', p: 0, i: 0, x: 1090, y: 610 },
  { k: 'fire', p: 0, i: 0, x: 1090, y: 610 },
  { k: 'flip', p: 1, x: 50, y: 680, grav: -1 },
  { k: 'gswitch', i: 0, x: 130, y: 610 },
  { k: 'arrowback', i: 0, x: 650, y: 690 },
  { k: 'unlock', i: 0, x: 1010, y: 620 },
  { k: 'checkpoint', i: 0, x: 730, y: 700 },
  { k: 'light', on: true, x: 90, y: 610 },
  { k: 'light', on: false, x: 90, y: 610 },
  { k: 'shake', tile: 34 * 64 + 22, x: 450, y: 690 },
  { k: 'break', tile: 34 * 64 + 22, x: 450, y: 690 },
  { k: 'mend', tile: 34 * 64 + 22, x: 450, y: 690 },
  { k: 'clear', stage: 0 },
  { k: 'stage', stage: 1 },
  { k: 'respawn', cp: 0 },
  { k: 'win' },
]

describe('the camera on a canvas', () => {
  it('maps world to canvas and back on whole pixels', () => {
    const v = viewFor({ x: 100.3, y: 40.7, w: 640, h: 360, zoom: 1 }, 1280)
    expect(v.k).toBe(2)
    expect(Number.isInteger(v.ox) && Number.isInteger(v.oy)).toBe(true)
    const [cx, cy] = toCanvas(v, 420, 220)
    const [wx, wy] = toWorld(v, cx, cy)
    expect(wx).toBeCloseTo(420)
    expect(wy).toBeCloseTo(220)
  })

  it('a one-screen stage on a 16:9 canvas is the whole stage from the corner', () => {
    const level = compileLevel({ ...everything, stages: [everything.stages[1]] })
    const cam = cameraFor(level.stages[0], initState(level), 16 / 9)
    expect(viewFor(cam, 1280)).toEqual({ k: 2, ox: 0, oy: 0 })
  })

  it('culls tiles and rects to the camera', () => {
    const cam = { x: 400, y: 200, w: 640, h: 360, zoom: 1 }
    expect(visibleTiles(cam, { cols: 128, rows: 72 })).toEqual({ x0: 20, y0: 10, x1: 52, y1: 28 })
    expect(visibleTiles({ ...cam, x: -50, y: -50 }, { cols: 32, rows: 18 }, 1)).toEqual({ x0: 0, y0: 0, x1: 31, y1: 17 })
    expect(inView(cam, 380, 220, 10, 10)).toBe(false)
    expect(inView(cam, 380, 220, 10, 10, 24)).toBe(true)
    expect(inView(cam, 1100, 300, 10, 10, 24)).toBe(false)
  })

  it('bakes the static layer at the nearest zoom and keeps the one it has between two', () => {
    expect(MIPS[mipFor(1)]).toBe(1)
    expect(MIPS[mipFor(0.5)]).toBe(0.5)
    expect(MIPS[mipFor(0.25)]).toBe(0.25)
    const mid = Math.sqrt(MIPS[0] * MIPS[1]) * 0.99
    expect(mipFor(mid)).toBe(1)
    expect(mipFor(mid, 0)).toBe(0)
    // the widest view of the biggest stage on a 1920 canvas is a handful of chunks, not hundreds
    const r = chunkRange({ x: 0, y: 0, w: 2560, h: 1440, zoom: 0.25 }, 0.75)
    expect((r.x1 - r.x0) * (r.y1 - r.y0)).toBeLessThanOrEqual(12)
  })
})

describe('the dark', () => {
  const level = compileLevel(everything)
  const st = level.stages[0]
  const s = initState(level)

  it('is black away from every light and clear at each player, ember further out than frost', () => {
    const whole = [{ x: -1e5, y: -1e5, w: 3e5, h: 3e5 }]
    const holes = lightHoles(st, s)
    const [e, f] = s.players
    const mid = (p: { x: number; y: number }) => [p.x + PLAYER_W / 2, p.y + PLAYER_H / 2] as const
    expect(darknessAt(holes, whole, ...mid(e))).toBe(0)
    expect(darknessAt(holes, whole, ...mid(f))).toBe(0)
    expect(darknessAt(holes, whole, 30 * TILE, 5 * TILE)).toBe(1)
    expect(holes[0].r).toBe(GLOW_EMBER)
    expect(holes[1].r).toBe(GLOW_FROST)
    // the lamp and the doors light themselves a little; a taken gem stops
    const lamp = st.lamps[0]
    expect(darknessAt(holes, whole, lamp.x + 10, lamp.y + 10)).toBeLessThan(0.3)
    const n = holes.length
    expect(lightHoles(st, { ...s, taken: s.taken.map(() => true) }).length).toBe(n - st.gems.length)
  })

  it('stays inside the dark patches when a stage has them', () => {
    expect(darkRects(st)).toBe(st.darks)
    expect(darknessAt([], darkRects(st), 5 * TILE, 5 * TILE)).toBe(1)
    expect(darknessAt([], darkRects(st), 40 * TILE, 20 * TILE)).toBe(0)
    expect(darknessAt([], darkRects(level.stages[1]), -500, 9000)).toBe(1)
  })
})

describe('EmberRenderer', () => {
  it('draws every state of every thing, and a thumbnail, without throwing', () => {
    vi.stubGlobal('document', { createElement: () => fakeCanvas(64, 64) })
    try {
      const level = compileLevel(everything)
      const st = level.stages[0]
      const base = initState(level)
      const r = new EmberRenderer(fakeCanvas(1920, 1080))
      // nothing going on, lights off
      expect(() => r.draw(st, null, base, 0, [], 1000, { me: null, reduceMotion: false })).not.toThrow()
      // everything going on at once
      const busy: GameState = JSON.parse(JSON.stringify(base))
      busy.lit = true
      busy.litT = 30
      busy.keys = 2
      busy.keyTaken = busy.keyTaken.map(() => true)
      busy.arrows = busy.arrows.map(() => 60)
      busy.crumble = busy.crumble.map((_, i) => (i ? CRUMBLE_TICKS + 40 : 12))
      busy.cannons = busy.cannons.map(() => 10)
      busy.cp = 0
      busy.players[0].cannon = 0
      busy.players[1].grav = -1
      busy.players[1].air = 1
      busy.players[1].fly = 5
      busy.players[1].x = 60 * TILE
      busy.players[1].y = 3 * TILE
      for (const reduceMotion of [false, true])
        for (const phase of ['play', 'dead', 'clear', 'won'] as const) {
          expect(() => r.draw(st, base, { ...busy, phase, phaseT: 40 }, 0.5, ALL_EVENTS, 1016, { me: 1, reduceMotion })).not.toThrow()
        }
      // the next stage, a tall thin canvas, and a state off the wire with its arrays short
      const next = { ...initState(level), stage: 1 }
      expect(() => r.draw(level.stages[1], next, next, 0, [], 1033, { me: null, reduceMotion: false })).not.toThrow()
      const tall = new EmberRenderer(fakeCanvas(400, 900))
      const bare = { ...busy, crumble: [], arrows: [], cannons: [], locks: [], keyTaken: [], taken: [], thin: [], plates: undefined, sensors: undefined }
      expect(() => tall.draw(st, null, bare, 0, [], 1000, { me: 0, reduceMotion: false })).not.toThrow()
      expect(() => drawThumb(fakeCanvas(144, 81), st, { stages: 2 })).not.toThrow()
      expect(() => drawThumb(fakeCanvas(144, 81), level.stages[1])).not.toThrow()
    } finally {
      vi.unstubAllGlobals()
    }
  })
})
