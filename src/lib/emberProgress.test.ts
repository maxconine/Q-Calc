import { describe, expect, it } from 'vitest'
import { emptyProgress, parseProgress, recordResult, sanitizeProgress, firstUnfinished } from './emberProgress'
import type { Score } from './emberTypes'

const score = (secs: number, stars: number, fire = 0, frost = 0): Score => ({ secs, stars, gems: [fire, frost], total: [2, 2] })
const LEVELS = [{ id: 'a' }, { id: 'b' }, { id: 'c' }]

describe('recordResult', () => {
  it('records a first finish', () => {
    const p = recordResult(emptyProgress(), 'a', score(12.5, 2, 1, 1))
    expect(p.levels.a).toEqual({ completed: true, stars: 2, secs: 12.5, gems: 2 })
  })

  it('keeps the best of each field separately', () => {
    let p = recordResult(emptyProgress(), 'a', score(20, 3, 2, 2))
    p = recordResult(p, 'a', score(9, 1, 0, 1))
    expect(p.levels.a).toEqual({ completed: true, stars: 3, secs: 9, gems: 4 })
    p = recordResult(p, 'a', score(30, 2, 1, 0))
    expect(p.levels.a).toEqual({ completed: true, stars: 3, secs: 9, gems: 4 })
  })

  it('does not mutate and leaves other levels alone', () => {
    const p0 = recordResult(emptyProgress(), 'b', score(5, 1))
    const p1 = recordResult(p0, 'a', score(5, 1))
    expect(p0.levels.a).toBeUndefined()
    expect(p1.levels.b).toEqual(p0.levels.b)
  })

  it('clamps silly scores', () => {
    const p = recordResult(emptyProgress(), 'a', score(Number.NaN, 9, -3, 2.7))
    expect(p.levels.a).toEqual({ completed: true, stars: 3, secs: null, gems: 2 })
  })
})

describe('sanitizeProgress', () => {
  it('tolerates junk', () => {
    for (const raw of [null, 3, 'x', [], { levels: 'no' }, { levels: [1, 2] }]) expect(sanitizeProgress(raw)).toEqual(emptyProgress())
    expect(parseProgress('{not json')).toEqual(emptyProgress())
    expect(parseProgress(null)).toEqual(emptyProgress())
  })

  it('keeps good levels and cleans fields', () => {
    const p = sanitizeProgress({
      muted: true,
      extra: 1,
      levels: {
        a: { completed: true, stars: 7, secs: 11.2, gems: '4', junk: true },
        b: { completed: false, stars: 3, secs: 4 },
        c: { completed: true, stars: -1, secs: -5, gems: 3.9 },
        d: 'nope',
      },
    })
    expect(p).toEqual({
      muted: true,
      levels: {
        a: { completed: true, stars: 3, secs: 11.2, gems: 0 },
        c: { completed: true, stars: 0, secs: null, gems: 3 },
      },
    })
  })

  it('round-trips through JSON', () => {
    const p = { ...recordResult(emptyProgress(), 'a', score(8, 2, 1, 0)), muted: true }
    expect(parseProgress(JSON.stringify(p))).toEqual(p)
  })
})

describe('firstUnfinished', () => {
  it('starts on the first level, and on nothing when there are none', () => {
    expect(firstUnfinished(emptyProgress(), LEVELS)).toBe(0)
    expect(firstUnfinished(emptyProgress(), [])).toBe(0)
  })

  it('skips finished levels, wherever they are', () => {
    let p = recordResult(emptyProgress(), 'a', score(5, 1))
    expect(firstUnfinished(p, LEVELS)).toBe(1)
    p = recordResult(p, 'c', score(5, 1))
    expect(firstUnfinished(p, LEVELS)).toBe(1)
    p = recordResult(p, 'b', score(5, 1))
    expect(firstUnfinished(p, LEVELS)).toBe(0)
  })
})
