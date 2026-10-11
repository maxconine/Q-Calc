import { describe, expect, it } from 'vitest'
import { baseView, cameraFor, easeCamera } from './emberCamera'
import { compileLevel, initState } from './emberEngine'
import { CAM_MIN_ZOOM, MAX_COLS, MAX_ROWS, PLAYER_H, PLAYER_W, TILE, VIEW_H, VIEW_W, type Camera, type GameState, type Stage } from './emberTypes'

// an empty walled room of cols x rows tiles, both players and doors on its floor at the left
function room(cols: number, rows: number): { stage: Stage; s: GameState } {
  const mid = '#' + '.'.repeat(cols - 2) + '#'
  const floor = '#12RB' + '.'.repeat(cols - 6) + '#'
  const map = ['#'.repeat(cols), ...Array.from({ length: rows - 3 }, () => mid), floor, '#'.repeat(cols)]
  const level = compileLevel({ id: 'cam', name: 'cam', par: 1, stages: [{ map, things: [] }] })
  return { stage: level.stages[0]!, s: initState(level) }
}

const put = (s: GameState, p: 0 | 1, tx: number, ty: number): void => {
  s.players[p].x = tx * TILE
  s.players[p].y = ty * TILE
}

const holds = (c: Camera, s: GameState) =>
  s.players.every((p) => p.x >= c.x - 1e-6 && p.y >= c.y - 1e-6 && p.x + PLAYER_W <= c.x + c.w + 1e-6 && p.y + PLAYER_H <= c.y + c.h + 1e-6)

const WIDE = 16 / 9

describe('camera', () => {
  it('shows a one-screen stage whole at zoom 1 on a 16:9 canvas', () => {
    const { stage, s } = room(32, 18)
    expect(cameraFor(stage, s, WIDE)).toEqual({ x: 0, y: 0, w: VIEW_W, h: VIEW_H, zoom: 1 })
  })

  it('keeps the canvas aspect, and overhangs a stage smaller than the view evenly', () => {
    const { stage, s } = room(32, 18)
    const tall = cameraFor(stage, s, 1)
    expect(tall.w / tall.h).toBeCloseTo(1)
    expect(tall).toMatchObject({ x: 0, w: VIEW_W, h: VIEW_W, zoom: 1 })
    expect(tall.y).toBeCloseTo(-(VIEW_W - VIEW_H) / 2)
    const wide = cameraFor(stage, s, 3)
    expect(wide.h).toBe(VIEW_H)
    expect(wide.x).toBeCloseTo(-(VIEW_H * 3 - VIEW_W) / 2)
    expect(baseView(Number.NaN)).toEqual({ w: VIEW_W, h: VIEW_H })
  })

  it('follows the players at zoom 1 while they are together, and stays inside the stage', () => {
    const { stage, s } = room(96, 54)
    put(s, 0, 50, 30)
    put(s, 1, 52, 30)
    const c = cameraFor(stage, s, WIDE)
    expect(c.zoom).toBe(1)
    expect(c.x + c.w / 2).toBeCloseTo(51 * TILE + PLAYER_W / 2)
    expect(holds(c, s)).toBe(true)
    put(s, 0, 1, 1)
    put(s, 1, 2, 1)
    expect(cameraFor(stage, s, WIDE)).toMatchObject({ x: 0, y: 0, zoom: 1 })
    put(s, 0, 93, 51)
    put(s, 1, 94, 51)
    const far = cameraFor(stage, s, WIDE)
    expect(far.x + far.w).toBeCloseTo(stage.w)
    expect(far.y + far.h).toBeCloseTo(stage.h)
  })

  it('zooms out as they split up, and always holds both', () => {
    const { stage, s } = room(MAX_COLS, MAX_ROWS)
    let last = 1
    for (const gap of [4, 20, 40, 80, 124]) {
      put(s, 0, 1, 40)
      put(s, 1, 1 + gap, 40)
      const c = cameraFor(stage, s, WIDE)
      expect(holds(c, s), `gap ${gap}`).toBe(true)
      expect(c.zoom).toBeLessThanOrEqual(last)
      expect(c.zoom).toBeGreaterThanOrEqual(CAM_MIN_ZOOM)
      expect(c.w / c.h).toBeCloseTo(WIDE)
      last = c.zoom
    }
    expect(last).toBeLessThan(0.3)
    // opposite corners of the biggest stage, on awkward canvases
    put(s, 0, 1, 1)
    put(s, 1, MAX_COLS - 2, MAX_ROWS - 3)
    for (const aspect of [WIDE, 1, 0.6, 2.4]) expect(holds(cameraFor(stage, s, aspect), s), `aspect ${aspect}`).toBe(true)
  })

  it('is the same for the same state', () => {
    const { stage, s } = room(64, 36)
    put(s, 1, 40, 20)
    expect(cameraFor(stage, s, WIDE)).toEqual(cameraFor(stage, structuredClone(s), WIDE))
  })

  it('eases toward its target and jumps from nothing', () => {
    const to: Camera = { x: 400, y: 100, w: 1280, h: 720, zoom: 0.5 }
    expect(easeCamera(null, to)).toBe(to)
    let c: Camera = { x: 0, y: 0, w: 640, h: 360, zoom: 1 }
    const first = easeCamera(c, to)
    expect(first.x).toBeGreaterThan(0)
    expect(first.x).toBeLessThan(400)
    expect(first.w / first.h).toBeCloseTo(16 / 9)
    for (let i = 0; i < 200; i++) c = easeCamera(c, to)
    expect(c.x).toBeCloseTo(400, 0)
    expect(c.w).toBeCloseTo(1280, 0)
    expect(c.zoom).toBeCloseTo(0.5, 2)
  })
})
