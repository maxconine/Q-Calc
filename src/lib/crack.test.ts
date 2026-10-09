import { describe, expect, it } from 'vitest'
import {
  BASE_SPEED,
  CRACK_HINT,
  CLEAR_MS,
  FAIL_MS,
  HIT_WINDOW,
  MAX_AHEAD,
  MAX_SPEED,
  MIN_GAP,
  isCrackCommand,
  overDot,
  parseBest,
  press,
  speedFor,
  startLevel,
  tick,
  type CrackState,
} from './crack'
import { GAMES, gameCommand, gameHint } from './games'

const TAU = Math.PI * 2
const fixed = (v: number) => () => v

// angular distance between two angles, either way round
const gap = (a: number, b: number) => {
  const d = Math.abs(a - b) % TAU
  return Math.min(d, TAU - d)
}

// runs the needle forward in small frames until it is `ahead` short of the dot (or past it, if negative)
function runTo(s: CrackState, ahead: number, rand = fixed(0.5)): CrackState {
  const ms = ((s.ahead - ahead) / speedFor(s.level)) * 1000
  const steps = Math.ceil(ms / 10)
  let cur = s
  for (let i = 1; i <= steps; i++) {
    const r = tick(cur, s.at + (ms * i) / steps, rand)
    cur = r.state
    if (r.event === 'miss') break
  }
  return cur
}

const playing = (level = 1, rand = fixed(0.5)) => press(startLevel(level, 0, rand), 0, rand).state

describe('isCrackCommand', () => {
  it('takes crack and its longer names, in any case and spacing', () => {
    for (const t of ['crack', 'CRACK', ' crack ', 'crack the safe', 'crack  the   safe', 'safecracker', 'safe cracking'])
      expect(isCrackCommand(t)).toBe(true)
  })
  it('leaves near misses to the calculator', () => {
    for (const t of ['crac', 'crack2', 'cracks', 'crack safe', 'safe', '', 'crack the'])
      expect(isCrackCommand(t)).toBe(false)
  })
})

describe('startLevel', () => {
  it('needs as many hits as the level, and waits for a press', () => {
    const s = startLevel(4, 100, fixed(0.3))
    expect(s.left).toBe(4)
    expect(s.phase).toBe('ready')
    const after = tick(s, 5000, fixed(0.3)).state
    expect(after.angle).toBe(s.angle)
  })

  it('never puts the dot within the minimum gap of the needle, either side', () => {
    for (const r of [0, 0.001, 0.25, 0.5, 0.999, 0.9999999]) {
      for (const dir of [1, -1] as const) {
        const s = startLevel(1, 0, fixed(r), 1.2, dir)
        expect(s.ahead).toBeGreaterThanOrEqual(MIN_GAP)
        expect(s.ahead).toBeLessThanOrEqual(MAX_AHEAD)
        expect(gap(s.dot, s.angle)).toBeGreaterThanOrEqual(MIN_GAP - 1e-9)
      }
    }
  })

  it('puts the dot ahead of the needle in the way it turns', () => {
    const cw = startLevel(1, 0, fixed(0), 0, 1)
    const ccw = startLevel(1, 0, fixed(0), 0, -1)
    expect(cw.dot).toBeCloseTo(MIN_GAP)
    expect(ccw.dot).toBeCloseTo(TAU - MIN_GAP)
  })
})

describe('hit window', () => {
  it('counts a press just inside the window on either side of the dot', () => {
    for (const ahead of [HIT_WINDOW * 0.95, 0, -HIT_WINDOW * 0.95]) {
      const s = runTo(playing(3), ahead)
      expect(overDot(s)).toBe(true)
      const r = press(s, s.at, fixed(0.5))
      expect(r.event).toBe('hit')
      expect(r.state.left).toBe(2)
    }
  })

  it('fails a press just outside the window', () => {
    const s = runTo(playing(3), HIT_WINDOW * 1.1)
    const r = press(s, s.at, fixed(0.5))
    expect(r.event).toBe('miss')
    expect(r.state.phase).toBe('fail')
    expect(r.state.left).toBe(3)
  })

  it('fails a press made far from the dot', () => {
    const r = press(playing(2), 10, fixed(0.5))
    expect(r.event).toBe('miss')
  })
})

describe('a hit', () => {
  it('reverses the needle and places a new dot ahead of it the other way', () => {
    const s = runTo(playing(3), 0)
    expect(s.dir).toBe(1)
    const r = press(s, s.at, fixed(0.2))
    expect(r.state.dir).toBe(-1)
    expect(r.state.ahead).toBeGreaterThanOrEqual(MIN_GAP)
    expect(gap(r.state.dot, r.state.angle)).toBeGreaterThanOrEqual(MIN_GAP - 1e-9)
    // and the needle now heads backwards
    const later = tick(r.state, r.state.at + 40, fixed(0.2)).state
    const moved = later.angle - r.state.angle
    expect(Math.sign(Math.abs(moved) > Math.PI ? -moved : moved)).toBe(-1)
  })
})

describe('passing the dot', () => {
  it('is a miss once the needle is entirely past it, and not before', () => {
    const s = playing(2)
    const before = runTo(s, -HIT_WINDOW * 0.9)
    expect(before.phase).toBe('play')
    const r = tick(before, before.at + 60, fixed(0.5))
    expect(r.event).toBe('miss')
    expect(r.state.phase).toBe('fail')
  })

  it('does not skip over the dot when a frame comes late', () => {
    const s = runTo(playing(1), HIT_WINDOW * 1.5)
    const r = tick(s, s.at + 2000, fixed(0.5))
    expect(r.event).toBeNull()
    expect(r.state.ahead).toBeGreaterThan(-HIT_WINDOW)
  })

  it('retries the same level after the shake', () => {
    const failed = press(playing(3), 10, fixed(0.5)).state
    expect(press(failed, 20, fixed(0.5)).event).toBeNull()
    expect(tick(failed, 10 + FAIL_MS - 1, fixed(0.5)).state.phase).toBe('fail')
    const back = tick(failed, 10 + FAIL_MS, fixed(0.5)).state
    expect(back.phase).toBe('ready')
    expect(back.level).toBe(3)
    expect(back.left).toBe(3)
  })
})

describe('levels', () => {
  it('clears after n hits and opens level n + 1', () => {
    let s = playing(3)
    for (let i = 0; i < 3; i++) {
      s = runTo(s, 0)
      const r = press(s, s.at, fixed(0.5))
      expect(r.event).toBe(i < 2 ? 'hit' : 'clear')
      s = r.state
    }
    expect(s.phase).toBe('clear')
    expect(s.left).toBe(0)
    expect(tick(s, s.since + CLEAR_MS - 1, fixed(0.5)).state.phase).toBe('clear')
    const next = tick(s, s.since + CLEAR_MS, fixed(0.5)).state
    expect(next.phase).toBe('ready')
    expect(next.level).toBe(4)
    expect(next.left).toBe(4)
  })

  it('speeds up a little each level, up to a ceiling', () => {
    expect(speedFor(1)).toBe(BASE_SPEED)
    for (let n = 1; n < 60; n++) expect(speedFor(n + 1)).toBeGreaterThanOrEqual(speedFor(n))
    expect(speedFor(2) - speedFor(1)).toBeLessThan(0.2)
    expect(speedFor(1000)).toBe(MAX_SPEED)
  })

  it('reads a saved best level, or starts at 1', () => {
    expect(parseBest('7')).toBe(7)
    for (const raw of [null, '', 'x', '0', '-2', '2.5']) expect(parseBest(raw)).toBe(1)
  })
})

describe('in the games list', () => {
  it('opens from the bar without a peer link', () => {
    expect(gameCommand('crack')).toBe('crack')
    expect(GAMES.some((g) => g.kind === 'crack')).toBe(true)
    expect(gameHint('crack', false)).toBe(CRACK_HINT)
  })
})
