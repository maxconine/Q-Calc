import { describe, expect, it } from 'vitest'
import { buildGraph, graphHome, marchingSquares, parseGraphIntent } from './graph'

const drawn = (input: string, domain?: [number, number]) =>
  buildGraph(input, domain ? { domain } : {})!.points.filter((p) => p.y != null)
const xs = (input: string) => {
  const pts = drawn(input)
  return [Math.min(...pts.map((p) => p.x)), Math.max(...pts.map((p) => p.x))]
}

describe('graph bounds', () => {
  it('peels braces off the curve, in every way of writing a range', () => {
    for (const input of [
      'graph y = x ^ 2 {2 < y < 5}',
      'graph y = x^2 {2<y<5}',
      'graph y = x^2 {5 > y > 2}',
      'graph y = x^2 {2 <= y <= 5}',
      'graph y = x^2 {2 ≤ y ≤ 5}',
      'graph y = x^2, {2 < y < 5}',
    ]) {
      const intent = parseGraphIntent(input)!
      expect(intent.expression.replace(/\s/g, '')).toBe('x^2')
      expect(intent.bounds).toEqual({ x: [], y: [{ lo: 2, hi: 5 }] })
    }
    expect(parseGraphIntent('graph y = x^2 {x > 0}')!.bounds).toEqual({ x: [{ lo: 0, hi: Infinity }], y: [] })
    expect(parseGraphIntent('graph y = x^2 {3 > x}')!.bounds).toEqual({ x: [{ lo: -Infinity, hi: 3 }], y: [] })
    expect(parseGraphIntent('graph y = x^2 {-pi < x < pi}')!.bounds!.x[0]!.hi).toBeCloseTo(Math.PI)
  })

  it('reproduces the report: y = x ^ 2 {2 < y < 5} plots only y in [2, 5]', () => {
    const g = buildGraph('graph y = x ^ 2 {2 < y < 5}')!
    expect(g.error).toBeUndefined()
    const ys = g.points.flatMap((p) => (p.y == null ? [] : [p.y]))
    expect(Math.min(...ys)).toBeCloseTo(2, 6)
    expect(Math.max(...ys)).toBeCloseTo(5, 6)
    // two pieces, nothing between -√2 and √2
    expect(g.points.some((p) => p.y != null && Math.abs(p.x) < 1.4)).toBe(false)
    expect(graphHome('graph y = x ^ 2 {2 < y < 5}')).toEqual([-3, 3])
  })

  it('cuts x exactly at the bounds and frames them', () => {
    expect(xs('graph y = x^2 {0 < x < 3}')).toEqual([0, 3])
    expect(graphHome('graph y = x^2 {0 < x < 3}')).toEqual([-0.4, 3.4])
    expect(xs('graph f(t) = t^2 {0 < t < 3}')).toEqual([0, 3])
  })

  it('unions several ranges, in separate braces or one', () => {
    for (const input of ['graph y = x^2 {-3 < x < -1} {1 < x < 3}', 'graph y = x^2 {-3<x<-1, 1<x<3}']) {
      const pts = drawn(input)
      expect(xs(input)).toEqual([-3, 3])
      expect(pts.some((p) => Math.abs(p.x) < 1)).toBe(false)
      expect(pts.some((p) => p.x === -1) && pts.some((p) => p.x === 1)).toBe(true)
    }
  })

  it('intersects x and y bounds', () => {
    const pts = drawn('graph y = x^2 {x > 0} {y < 4}')
    expect(Math.min(...pts.map((p) => p.x))).toBe(0)
    expect(Math.max(...pts.map((p) => p.x))).toBeCloseTo(2, 6)
    expect(pts.every((p) => p.y! <= 4)).toBe(true)
  })

  it('keeps the curve while a bound is still being typed', () => {
    expect(drawn('graph y = x^2 {2 <').length).toBeGreaterThan(100)
    expect(parseGraphIntent('graph y = x^2 {2 < y < 5')!.bounds).toEqual({ x: [], y: [{ lo: 2, hi: 5 }] })
  })

  it('says so when braces are not bounds or nothing is left', () => {
    expect(buildGraph('graph y = x^2 {2 < q < 3}')!.error).toMatch(/bounds like/)
    expect(buildGraph('graph y = x^2 {20 < x < 30}', { domain: [-10, 10] })!.error).toMatch(/within the bounds/)
  })
})

describe('contour plots', () => {
  it('reads functions of x and y as level curves', () => {
    expect(parseGraphIntent('graph f(x,y) = x^2 + y^2')!.contour).toBe('levels')
    expect(parseGraphIntent('graph z = x^2 + y^2')).toMatchObject({ expression: 'x^2 + y^2', contour: 'levels' })
    expect(parseGraphIntent('graph x^2 + y^2')!.contour).toBe('levels')
    expect(parseGraphIntent('graph x^2 + y^2 = 4')).toMatchObject({ expression: '(x^2 + y^2) - (4)', contour: 'zero' })
    expect(parseGraphIntent('graph g(u, v) = u - v')!.expression).toBe('x - y')
    expect(parseGraphIntent('graph h(y, x) = y - 2x')!.expression).toBe('x - 2y')
    // y stays a number once it's a variable, and y = is still a curve
    expect(parseGraphIntent('graph x + y', { variables: { y: 2 } })!.contour).toBeUndefined()
    expect(parseGraphIntent('graph y = x^2')!.contour).toBeUndefined()
  })

  it('draws labelled round levels at one scale for x and y', () => {
    const g = buildGraph('graph f(x,y) = x^2 + y^2')!
    expect(g.error).toBeUndefined()
    expect(g.contours!.map((c) => c.level)).toEqual([20, 40, 60, 80, 100])
    expect(g.yScale.max - g.yScale.min).toBeCloseTo((20 * 160) / 600)
    expect(g.contours!.filter((c) => c.at).length).toBeGreaterThanOrEqual(2)
    // every point on the 20 line is √20 from the origin
    const s = g.contours![0]!.segments
    for (let k = 0; k < s.length; k += 2) expect(Math.hypot(s[k]!, s[k + 1]!)).toBeCloseTo(Math.sqrt(20), 1)
    expect(g.z!(1, 2)).toBe(5)
  })

  it('draws an implicit curve as its zero level', () => {
    const g = buildGraph('graph x^2 + y^2 = 4')!
    expect(g.contours!.map((c) => c.level)).toEqual([0])
    const s = g.contours![0]!.segments
    expect(s.length).toBeGreaterThan(40)
    for (let k = 0; k < s.length; k += 2) expect(Math.hypot(s[k]!, s[k + 1]!)).toBeCloseTo(2, 1)
  })

  it('clips level curves to the bounds', () => {
    const g = buildGraph('graph z = x^2 + y^2 {0 < x < 5} {-1 < y < 1}', { domain: [-10, 10] })!
    const s = g.contours!.flatMap((c) => c.segments)
    expect(s.length).toBeGreaterThan(0)
    for (let k = 0; k < s.length; k += 2) {
      expect(s[k]!).toBeGreaterThanOrEqual(-1e-9)
      expect(s[k]!).toBeLessThanOrEqual(5 + 1e-9)
      expect(Math.abs(s[k + 1]!)).toBeLessThanOrEqual(1 + 1e-9)
    }
    expect(g.z!(-1, 0)).toBeNull()
    expect(graphHome('graph z = x^2 + y^2 {0 < x < 5}')).toEqual([-0.5, 5.5])
  })

  it('splits a saddle cell by its centre', () => {
    // corners 1, 0, 1, 0 around a centre of 0.5: above 0.4 the high corners join
    const grid = [1, 0, 0, 1]
    expect(marchingSquares(grid, 1, 1, 0.4, 0, 0, 1, 1)).toHaveLength(8)
    expect(marchingSquares([0, 0, 0, 0], 1, 1, 0.5, 0, 0, 1, 1)).toEqual([])
  })

  it('is quick enough to resample on every pan', () => {
    const t = performance.now()
    for (let i = 0; i < 5; i++) buildGraph('graph z = sin(x) * cos(y)', { domain: [-10 + i, 10 + i] })
    expect((performance.now() - t) / 5).toBeLessThan(60)
  })
})
