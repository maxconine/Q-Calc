import { describe, expect, it } from 'vitest'
import {
  autoYScale,
  buildGraph,
  DEFAULT_GRAPH_DOMAIN,
  findCriticalPoints,
  findRoots,
  isGraphCommand,
  parseGraphIntent,
  sampleGraph,
} from './graph'

describe('parseGraphIntent', () => {
  it('parses bare expression, y=, and inline function def', () => {
    expect(parseGraphIntent('graph x^3')).toEqual({
      expression: 'x^3',
      label: 'x^3',
    })
    expect(parseGraphIntent('graph y = x^3')).toEqual({
      expression: 'x^3',
      label: 'y = x^3',
    })
    expect(parseGraphIntent('graph f(x) = x^3')).toEqual({
      expression: 'x^3',
      label: 'f(x)',
      functionDef: { name: 'f', params: ['x'], body: 'x^3' },
    })
  })

  it('looks up a defined function by name', () => {
    const intent = parseGraphIntent('graph f', {
      functions: { f: { params: ['x'], body: 'x^2' } },
    })
    expect(intent).toEqual({
      expression: 'f(x)',
      label: 'f(x)',
    })
  })

  it('rewrites non-x parameter bodies for name lookup and inline defs', () => {
    expect(
      parseGraphIntent('graph g', {
        functions: { g: { params: ['t'], body: 't^2 + 1' } },
      }),
    ).toEqual({
      expression: 'x^2 + 1',
      label: 'g(t)',
    })
    expect(parseGraphIntent('graph h(t) = 2*t')).toMatchObject({
      expression: '2*x',
      functionDef: { name: 'h', params: ['t'], body: '2*t' },
    })
  })

  it('returns null for non-graph input and empty graph', () => {
    expect(parseGraphIntent('x^3')).toBeNull()
    expect(parseGraphIntent('graph')).toBeNull()
    expect(parseGraphIntent('graph   ')).toBeNull()
  })

  it('isGraphCommand detects the prefix', () => {
    expect(isGraphCommand('graph x')).toBe(true)
    expect(isGraphCommand('GRAPH')).toBe(true)
    expect(isGraphCommand('  graph')).toBe(true)
    expect(isGraphCommand('x^2')).toBe(false)
  })
})

describe('sampleGraph and autoYScale', () => {
  it('samples over the default domain', () => {
    const points = sampleGraph('x^2')
    expect(points.length).toBeGreaterThan(10)
    expect(points[0]!.x).toBeCloseTo(DEFAULT_GRAPH_DOMAIN[0], 10)
    expect(points[points.length - 1]!.x).toBeCloseTo(DEFAULT_GRAPH_DOMAIN[1], 10)
    const at0 = points.find((p) => Math.abs(p.x) < 1e-9)
    expect(at0?.y).toBeCloseTo(0, 5)
    const near2 = points.reduce((best, p) =>
      Math.abs(p.x - 2) < Math.abs(best.x - 2) ? p : best,
    )
    expect(near2.y).toBeCloseTo(4, 1)
  })

  it('inserts nulls for non-finite samples (asymptotes)', () => {
    const points = sampleGraph('1/x', { domain: [-2, 2], sampleCount: 21 })
    const at0 = points.find((p) => Math.abs(p.x) < 1e-12)
    expect(at0?.y).toBeNull()
    expect(points.some((p) => p.y != null)).toBe(true)
  })

  it('clamps auto y-scale so asymptotes do not explode the range', () => {
    const points = sampleGraph('1/x', { domain: [-5, 5], sampleCount: 201 })
    const scale = autoYScale(points)
    expect(Math.abs(scale.max)).toBeLessThan(1e5)
    expect(Math.abs(scale.min)).toBeLessThan(1e5)
    expect(scale.max).toBeGreaterThan(scale.min)
  })

  it('uses variables and functions from options', () => {
    const points = sampleGraph('k*x', {
      domain: [0, 1],
      sampleCount: 3,
      variables: { k: 3 },
    })
    expect(points[2]!.y).toBeCloseTo(3, 5)

    const viaFn = sampleGraph('f(x)', {
      domain: [0, 2],
      sampleCount: 3,
      functions: { f: { params: ['x'], body: 'x^3' } },
    })
    expect(viaFn[2]!.y).toBeCloseTo(8, 5)
  })
})

describe('critical points and roots', () => {
  it('finds a minimum for x^2 near the origin', () => {
    const points = sampleGraph('x^2', { domain: [-5, 5], sampleCount: 201 })
    const crit = findCriticalPoints(points)
    expect(crit.length).toBeGreaterThanOrEqual(1)
    const near0 = crit.find((c) => Math.abs(c.x) < 0.2)
    expect(near0).toBeTruthy()
    expect(near0!.kind).toBe('min')
    expect(near0!.y).toBeCloseTo(0, 1)
  })

  it('finds a maximum for -x^2', () => {
    const points = sampleGraph('-x^2', { domain: [-3, 3], sampleCount: 101 })
    const max = findCriticalPoints(points).find((c) => c.kind === 'max')
    expect(max).toBeTruthy()
    expect(Math.abs(max!.x)).toBeLessThan(0.2)
  })

  it('finds roots of x^2 - 1', () => {
    const points = sampleGraph('x^2 - 1', { domain: [-3, 3], sampleCount: 301 })
    const roots = findRoots(points)
    expect(roots.length).toBeGreaterThanOrEqual(2)
    const xs = roots.map((r) => r.x).sort((a, b) => a - b)
    expect(xs[0]).toBeCloseTo(-1, 1)
    expect(xs[xs.length - 1]).toBeCloseTo(1, 1)
  })
})

describe('buildGraph', () => {
  it('returns null for non-graph input', () => {
    expect(buildGraph('2+2')).toBeNull()
  })

  it('builds a full result for graph x^3', () => {
    const g = buildGraph('graph x^3')
    expect(g).toBeTruthy()
    expect(g!.intent.expression).toBe('x^3')
    expect(g!.domain).toEqual([-10, 10])
    expect(g!.points.length).toBeGreaterThan(10)
    expect(g!.error).toBeUndefined()
    expect(g!.yScale.max).toBeGreaterThan(g!.yScale.min)
  })

  it('returns functionDef for inline graph f(x)=… and can sample it', () => {
    const g = buildGraph('graph f(x) = x^3')
    expect(g!.intent.functionDef).toEqual({ name: 'f', params: ['x'], body: 'x^3' })
    expect(g!.intent.expression).toBe('x^3')
    const near2 = g!.points.reduce((best, p) =>
      Math.abs(p.x - 2) < Math.abs(best.x - 2) ? p : best,
    )
    expect(near2.y).toBeCloseTo(8, 0)
  })

  it('resolves graph f after the function is defined in options', () => {
    const g = buildGraph('graph f', {
      functions: { f: { params: ['x'], body: 'x^2' } },
    })
    expect(g!.error).toBeUndefined()
    expect(g!.intent.expression).toBe('f(x)')
    const near3 = g!.points.reduce((best, p) =>
      Math.abs(p.x - 3) < Math.abs(best.x - 3) ? p : best,
    )
    expect(near3.y).toBeCloseTo(9, 0)
  })

  it('errors when name lookup misses', () => {
    const g = buildGraph('graph missing')
    // Bare name with no function is treated as expression "missing" → eval fails
    expect(g!.error).toBeTruthy()
  })

  it('can omit roots when findRoots is false', () => {
    const g = buildGraph('graph x^2 - 1', { findRoots: false })
    expect(g!.roots).toEqual([])
  })
})
