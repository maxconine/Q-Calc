import { describe, expect, it } from 'vitest'
import { emptyProgress, finishedCount, firstUnfinished, improved, parseProgress, PROGRESS_VERSION, recordResult, sanitizeProgress } from './emberProgress'
import { LEVELS as SHIPPED } from './emberLevels'
import type { Score } from './emberTypes'

const score = (secs: number, stars: number, fire = 0, frost = 0): Score => ({ secs, stars, gems: [fire, frost], total: [2, 2] })
const LEVELS = [{ id: 'a' }, { id: 'b' }, { id: 'c' }]

describe('recordResult', () => {
  it('records a first finish', () => {
    const p = recordResult(emptyProgress(), 'a', score(12.5, 2, 1, 1), 3)
    expect(p.levels.a).toEqual({ completed: true, stars: 2, secs: 12.5, gems: 2, deaths: 3 })
    expect(p.v).toBe(PROGRESS_VERSION)
  })

  it('keeps the best of each field separately', () => {
    let p = recordResult(emptyProgress(), 'a', score(20, 3, 2, 2), 4)
    p = recordResult(p, 'a', score(9, 1, 0, 1), 7)
    expect(p.levels.a).toEqual({ completed: true, stars: 3, secs: 9, gems: 4, deaths: 4 })
    p = recordResult(p, 'a', score(30, 2, 1, 0), 0)
    expect(p.levels.a).toEqual({ completed: true, stars: 3, secs: 9, gems: 4, deaths: 0 })
  })

  it('does not mutate and leaves other levels alone', () => {
    const p0 = recordResult(emptyProgress(), 'b', score(5, 1), 0)
    const p1 = recordResult(p0, 'a', score(5, 1), 0)
    expect(p0.levels.a).toBeUndefined()
    expect(p1.levels.b).toEqual(p0.levels.b)
  })

  it('clamps silly scores', () => {
    const p = recordResult(emptyProgress(), 'a', score(Number.NaN, 9, -3, 2.7), -2)
    expect(p.levels.a).toEqual({ completed: true, stars: 3, secs: null, gems: 2, deaths: null })
  })

  it('a finish with no death count keeps the one it had, and fills one in when it comes', () => {
    let p = recordResult(emptyProgress(), 'a', score(10, 1))
    expect(p.levels.a!.deaths).toBeNull()
    p = recordResult(p, 'a', score(10, 1), 5)
    expect(p.levels.a!.deaths).toBe(5)
    p = recordResult(p, 'a', score(10, 1))
    expect(p.levels.a!.deaths).toBe(5)
    p = recordResult(p, 'a', score(10, 1), 2.9)
    expect(p.levels.a!.deaths).toBe(2)
  })
})

describe('improved', () => {
  it('says nothing on a first finish', () => {
    expect(improved(undefined, score(5, 3, 2, 2), 0)).toEqual({ secs: false, stars: false, gems: false, deaths: false })
  })

  it('names what a later finish beat', () => {
    const old = recordResult(emptyProgress(), 'a', score(20, 2, 1, 1), 3).levels.a
    expect(improved(old, score(19, 2, 1, 1), 3)).toEqual({ secs: true, stars: false, gems: false, deaths: false })
    expect(improved(old, score(25, 3, 2, 2), 1)).toEqual({ secs: false, stars: true, gems: true, deaths: true })
    expect(improved(old, score(20, 2, 1, 1), 3)).toEqual({ secs: false, stars: false, gems: false, deaths: false })
  })

  it('has no death record to beat in a version 1 best', () => {
    const old = sanitizeProgress({ levels: { 'first-light': { completed: true, stars: 2, secs: 20, gems: 2 } } }).levels['first-light']
    expect(improved(old, score(30, 1), 0).deaths).toBe(false)
  })
})

describe('sanitizeProgress', () => {
  it('tolerates junk', () => {
    for (const raw of [null, 3, 'x', [], { levels: 'no' }, { levels: [1, 2] }, { v: 2, levels: null }, { v: 'two', levels: 7 }])
      expect(sanitizeProgress(raw)).toEqual(emptyProgress())
    expect(parseProgress('{not json')).toEqual(emptyProgress())
    expect(parseProgress(null)).toEqual(emptyProgress())
    expect(parseProgress('null')).toEqual(emptyProgress())
  })

  it('keeps good levels and cleans fields', () => {
    const p = sanitizeProgress({
      v: 2,
      muted: true,
      extra: 1,
      levels: {
        a: { completed: true, stars: 7, secs: 11.2, gems: '4', deaths: 2.6, junk: true },
        b: { completed: false, stars: 3, secs: 4 },
        c: { completed: true, stars: -1, secs: -5, gems: 3.9, deaths: -1 },
        d: 'nope',
        e: { completed: true, stars: 1, secs: 9, gems: 0, deaths: 0 },
        ['x'.repeat(65)]: { completed: true, stars: 1, secs: 9, gems: 0, deaths: 0 },
      },
    })
    expect(p).toEqual({
      v: 2,
      muted: true,
      levels: {
        a: { completed: true, stars: 3, secs: 11.2, gems: 0, deaths: 2 },
        c: { completed: true, stars: 0, secs: null, gems: 3, deaths: null },
        e: { completed: true, stars: 1, secs: 9, gems: 0, deaths: 0 },
      },
    })
  })

  it('round-trips through JSON', () => {
    const p = { ...recordResult(emptyProgress(), 'a', score(8, 2, 1, 0), 1), muted: true }
    expect(parseProgress(JSON.stringify(p))).toEqual(p)
  })

  it('reads a file from a newer version as far as it can', () => {
    const p = sanitizeProgress({ v: 9, muted: true, levels: { a: { completed: true, stars: 2, secs: 5, gems: 1, deaths: 4, medals: ['x'] } }, more: {} })
    expect(p).toEqual({ v: 2, muted: true, levels: { a: { completed: true, stars: 2, secs: 5, gems: 1, deaths: 4 } } })
  })

  it('does not load a prototype key as a level', () => {
    const p = parseProgress('{"v":2,"levels":{"__proto__":{"completed":true,"stars":3,"secs":1,"gems":9}}}')
    expect(p.levels).toEqual({})
    expect(Object.getPrototypeOf(p.levels)).toBe(Object.prototype)
    expect((p.levels as Record<string, unknown>).completed).toBeUndefined()
    expect(firstUnfinished(p, LEVELS)).toBe(0)
  })
})

// what version 1 wrote: no `v`, no deaths, and the ids of the fifteen one-screen levels
const V1 = JSON.stringify({
  muted: true,
  levels: {
    'first-light': { completed: true, stars: 3, secs: 21.4, gems: 4 },
    crates: { completed: true, stars: 2, secs: 30.1, gems: 3 },
    'hold-the-door': { completed: true, stars: 3, secs: 28, gems: 4 },
    thaw: { completed: true, stars: 1, secs: 55.5, gems: 0 },
    'last-light': { completed: true, stars: 3, secs: 40, gems: 6 },
  },
})

describe('version 1 progress', () => {
  it('keeps the mute switch and first-light, the one map that is the same', () => {
    const p = parseProgress(V1)
    expect(p).toEqual({
      v: 2,
      muted: true,
      levels: { 'first-light': { completed: true, stars: 3, secs: 21.4, gems: 4, deaths: null } },
    })
  })

  it('never shows an old best against another level', () => {
    const p = parseProgress(V1)
    expect(SHIPPED[0]!.id).toBe('first-light')
    for (const l of SHIPPED.slice(1)) expect(p.levels[l.id], l.id).toBeUndefined()
    expect(finishedCount(p, SHIPPED)).toBe(1)
    expect(firstUnfinished(p, SHIPPED)).toBe(1)
  })

  it('is version 2 once saved, and reads back the same', () => {
    const p = parseProgress(V1)
    const again = parseProgress(JSON.stringify(p))
    expect(again).toEqual(p)
    const next = recordResult(again, 'first-light', score(25, 3, 2, 2), 0)
    expect(next.levels['first-light']).toEqual({ completed: true, stars: 3, secs: 21.4, gems: 4, deaths: 0 })
  })

  it('a version 1 file with nothing finished is just empty', () => {
    expect(parseProgress('{"levels":{},"muted":false}')).toEqual(emptyProgress())
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

describe('finishedCount', () => {
  it('counts only levels that still exist', () => {
    let p = recordResult(emptyProgress(), 'a', score(5, 1))
    p = recordResult(p, 'gone', score(5, 1))
    expect(finishedCount(p, LEVELS)).toBe(1)
    expect(finishedCount(emptyProgress(), LEVELS)).toBe(0)
  })
})
