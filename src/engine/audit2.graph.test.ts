import { describe, expect, it } from 'vitest'
import { buildGraph } from './graph'

function roots(input: string, opts = {}): number[] {
  const g = buildGraph(input, opts)
  expect(g, input).not.toBeNull()
  expect(g!.error, input).toBeUndefined()
  return g!.roots.map((r) => r.x)
}

function expectXs(got: number[], want: number[], tol = 1e-6): void {
  expect(got.length, `got ${got.join(', ')} want ${want.join(', ')}`).toBe(want.length)
  const sorted = [...got].sort((a, b) => a - b)
  want.forEach((w, i) => expect(Math.abs(sorted[i]! - w), `root ${w} (got ${sorted[i]})`).toBeLessThan(tol))
}

describe('audit2: graphs of functions with a restricted domain still find the right root', () => {
  // sqrt(x-3) is undefined for x<3; the sampler must skip that half of the domain without
  // reporting a spurious root or extremum there
  it('sqrt(x - 3) has one root at x = 3 and no extrema', () => {
    const g = buildGraph('graph sqrt(x - 3)')!
    expectXs(g.roots.map((r) => r.x), [3])
    expect(g.criticalPoints).toEqual([])
  })

  it('ln(x - 5) has one root at x = 6 (shifted off the y-axis)', () => {
    expectXs(roots('graph ln(x - 5)'), [6])
  })

  it('acos(x) is only defined on [-1, 1] and has its root at x = 1', () => {
    expectXs(roots('graph acos(x)'), [1])
  })

  it('sqrt(4 - x^2) (a semicircle) has roots at the two ends of its domain and a max at the top', () => {
    const g = buildGraph('graph sqrt(4 - x^2)')!
    expectXs(g.roots.map((r) => r.x), [-2, 2])
    expect(g.criticalPoints).toHaveLength(1)
    expect(g.criticalPoints[0]!.kind).toBe('max')
    expect(g.criticalPoints[0]!.x).toBeCloseTo(0, 6)
    expect(g.criticalPoints[0]!.y).toBeCloseTo(2, 6)
  })
})

describe('audit2: vertical asymptotes never masquerade as roots or extrema', () => {
  it('1/(x - 2) has a pole at x = 2 but no root and no extremum anywhere', () => {
    const g = buildGraph('graph 1/(x - 2)')!
    expect(g.roots).toEqual([])
    expect(g.criticalPoints).toEqual([])
  })

  it('1/(x^2 - 1) has poles at ±1, no roots, and a genuine local max at (0, -1) between them', () => {
    const g = buildGraph('graph 1/(x^2 - 1)')!
    expect(g.roots).toEqual([])
    expect(g.criticalPoints).toHaveLength(1)
    expect(g.criticalPoints[0]!.kind).toBe('max')
    expect(g.criticalPoints[0]!.x).toBeCloseTo(0, 6)
    expect(g.criticalPoints[0]!.y).toBeCloseTo(-1, 9)
  })

  it('1/x^3 is monotonic on each branch: no roots, no extrema', () => {
    const g = buildGraph('graph 1/x^3')!
    expect(g.roots).toEqual([])
    expect(g.criticalPoints).toEqual([])
  })
})

describe('audit2: a cubic with three distinct real roots and two extrema', () => {
  it('x^3 - 6x^2 + 11x - 6 factors as (x-1)(x-2)(x-3)', () => {
    expectXs(roots('graph x^3 - 6x^2 + 11x - 6'), [1, 2, 3])
  })

  it('has a local max at x = 2 - sqrt(3)/3 and a local min at x = 2 + sqrt(3)/3', () => {
    // f'(x) = 3x^2 - 12x + 11 = 0 at x = (12 ± sqrt(12))/6 = 2 ± sqrt(3)/3
    const g = buildGraph('graph x^3 - 6x^2 + 11x - 6')!
    expect(g.criticalPoints).toHaveLength(2)
    const [maxPt, minPt] = g.criticalPoints
    expect(maxPt!.kind).toBe('max')
    expect(maxPt!.x).toBeCloseTo(2 - Math.sqrt(3) / 3, 6)
    expect(maxPt!.y).toBeCloseTo(0.38490017945974486, 9)
    expect(minPt!.kind).toBe('min')
    expect(minPt!.x).toBeCloseTo(2 + Math.sqrt(3) / 3, 6)
    expect(minPt!.y).toBeCloseTo(-0.3849001794597511, 9)
  })
})

describe('audit2: plain quadratics, roots and vertex worked out by hand', () => {
  it('x^2 - 4x + 3 factors as (x-1)(x-3): roots at 1 and 3, vertex (2, -1)', () => {
    const g = buildGraph('graph x^2 - 4x + 3')!
    expectXs(g.roots.map((r) => r.x), [1, 3])
    expect(g.criticalPoints).toHaveLength(1)
    expect(g.criticalPoints[0]!.kind).toBe('min')
    expect(g.criticalPoints[0]!.x).toBeCloseTo(2, 6)
    expect(g.criticalPoints[0]!.y).toBeCloseTo(-1, 9)
  })

  it('-x^2 + 4 opens downward: roots at ±2, vertex (0, 4)', () => {
    const g = buildGraph('graph -x^2 + 4')!
    expectXs(g.roots.map((r) => r.x), [-2, 2])
    expect(g.criticalPoints).toHaveLength(1)
    expect(g.criticalPoints[0]!.kind).toBe('max')
    expect(g.criticalPoints[0]!.x).toBeCloseTo(0, 6)
    expect(g.criticalPoints[0]!.y).toBeCloseTo(4, 9)
  })
})
