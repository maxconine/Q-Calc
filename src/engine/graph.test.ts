import { describe, expect, it } from 'vitest'
import {
  autoYScale,
  buildGraph,
  compileGraphY,
  DEFAULT_GRAPH_DOMAIN,
  evaluateGraphY,
  findCriticalPoints,
  findRoots,
  graphHome,
  graphTicks,
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
      expression: 'x^2',
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
    expect(g!.intent.expression).toBe('x^2')
    const near3 = g!.points.reduce((best, p) =>
      Math.abs(p.x - 3) < Math.abs(best.x - 3) ? p : best,
    )
    expect(near3.y).toBeCloseTo(9, 0)
  })

  it('errors when name lookup misses', () => {
    const g = buildGraph('graph missing')
    expect(g!.error).toBeTruthy()
  })

  it('can omit roots when findRoots is false', () => {
    const g = buildGraph('graph x^2 - 1', { findRoots: false })
    expect(g!.roots).toEqual([])
  })
})

describe('compiled sampler', () => {
  const opts = {
    variables: { k: 3 },
    functions: { f: { params: ['x'], body: 'x^3 - x' } },
  }
  const cases: Array<{ expr: string; angleMode?: 'deg' | 'rad' }> = [
    { expr: 'x^2' },
    { expr: 'x^2-2x' },
    { expr: '2x+1' },
    { expr: 'sin(x)' },
    { expr: 'tan(x)', angleMode: 'rad' },
    { expr: '1/x' },
    { expr: 'sqrt(x)' },
    { expr: 'ln(x)' },
    { expr: 'x!' },
    { expr: 'floor(x)' },
    { expr: 'e^x' },
    { expr: 'k*x' },
    { expr: 'f(x)' },
  ]

  it.each(cases)('matches evaluateGraphY for $expr', ({ expr, angleMode }) => {
    const options = { ...opts, angleMode, domain: [-10, 10] as [number, number], sampleCount: 161 }
    const y = compileGraphY(expr, options)
    let maxErr = 0
    for (const p of sampleGraph(expr, options)) {
      const ref = evaluateGraphY(expr, p.x, options)
      expect(p.y === null, `${expr} at x=${p.x}`).toBe(ref === null)
      expect(y(p.x)).toBe(p.y)
      if (p.y !== null && ref !== null) maxErr = Math.max(maxErr, Math.abs(p.y - ref) / Math.max(1, Math.abs(ref)))
    }
    expect(maxErr).toBeLessThanOrEqual(1e-9)
  })

  it('inlines the body for graph <name>, including a non-x parameter', () => {
    const functions = { f: { params: ['x'], body: 'x^2' }, g: { params: ['t'], body: '2t + 1' } }
    expect(parseGraphIntent('graph f', { functions })?.expression).toBe('x^2')
    expect(parseGraphIntent('graph g', { functions })?.expression).toBe('2x + 1')
    const g = buildGraph('graph g', { functions })
    expect(g!.error).toBeUndefined()
    expect(g!.points[g!.points.length - 1]!.y).toBeCloseTo(21, 9)
  })
})

describe('refined roots and extrema', () => {
  const graph = (expr: string, domain?: [number, number]) => buildGraph(`graph ${expr}`, { domain })!

  it('refines roots by bisection', () => {
    const roots = graph('x^2-2').roots.map((r) => r.x)
    expect(roots).toHaveLength(2)
    expect(roots[1]).toBeCloseTo(Math.SQRT2, 9)
    expect(roots[0]).toBeCloseTo(-Math.SQRT2, 9)
  })

  it('refines extrema off the sample grid', () => {
    const crit = graph('(x-0.333)^2').criticalPoints
    expect(crit).toHaveLength(1)
    expect(crit[0]!.kind).toBe('min')
    expect(crit[0]!.x).toBeCloseTo(0.333, 7)
  })

  it('ignores poles', () => {
    const tan = graph('tan(x)')
    expect(tan.roots.some((r) => Math.abs(Math.abs(r.x) - Math.PI / 2) < 0.1)).toBe(false)
    expect(tan.roots.map((r) => r.x / Math.PI).map(Math.round)).toEqual([-3, -2, -1, 0, 1, 2, 3])
    expect(tan.criticalPoints).toEqual([])
    expect(graph('1/(x-0.3)').roots).toEqual([])
    expect(graph('1/(x-0.3)').criticalPoints).toEqual([])
  })

  it('follows the angle setting, with a degree window for trig in degrees', () => {
    const deg = buildGraph('graph sin(x)', { angleMode: 'deg' })!
    expect(deg.angleUnit).toBe('deg')
    expect(deg.domain).toEqual([-360, 360])
    expect(deg.criticalPoints.map((c) => c.x)).toEqual([-270, -90, 90, 270])
    expect(deg.criticalPoints.every((c) => Math.abs(c.y) === 1)).toBe(true)
    expect(deg.y(30)).toBeCloseTo(0.5, 12)

    const rad = buildGraph('graph sin(x)', { angleMode: 'rad' })!
    expect(rad.angleUnit).toBe('rad')
    expect(rad.domain).toEqual([-10, 10])
    expect(rad.criticalPoints).toHaveLength(6)

    // No trig, no unit — and the ±10 window regardless of the setting.
    const poly = buildGraph('graph x^2', { angleMode: 'deg' })!
    expect(poly.angleUnit).toBeNull()
    expect(poly.domain).toEqual([-10, 10])
    // A user function that hides the trig still counts.
    const viaFn = buildGraph('graph g(x)', { angleMode: 'deg', functions: { g: { params: ['t'], body: 'cos(t)' } } })!
    expect(viaFn.angleUnit).toBe('deg')
  })

  it('collapses runs of zero samples', () => {
    expect(graph('0').roots).toEqual([])
    const floor = graph('floor(x)').roots
    expect(floor).toHaveLength(1)
    expect(floor[0]!.x).toBeCloseTo(0, 9)
  })

  it('keeps large finite values instead of clamping', () => {
    const g = graph('e^x', [-2, 20])
    expect(g.points.every((p) => p.y != null)).toBe(true)
    expect(Math.max(...g.points.map((p) => p.y!)) / Math.exp(20)).toBeCloseTo(1, 12)
  })
})

describe('clean reported values', () => {
  const graph = (expr: string, domain?: [number, number]) => buildGraph(`graph ${expr}`, { domain })!
  const coords = (expr: string) => graph(expr).criticalPoints.map((c) => [c.kind, c.x, c.y])

  it('reports the simplest x the function cannot tell apart from the refined extremum', () => {
    expect(coords('x^2')).toEqual([['min', 0, 0]])
    expect(coords('x^3-3x')).toEqual([['max', -1, 2], ['min', 1, -2]])
    expect(coords('sqrt(9.3-x^2)')).toEqual([['max', 0, Math.sqrt(9.3)]])
    // Irrational extrema keep their precision.
    const s = graph('sin(x)').criticalPoints.find((c) => c.x > 0 && c.x < 2)!
    expect(s.x).toBeCloseTo(Math.PI / 2, 7)
    expect(graph('(x-1/3)^2').criticalPoints[0]!.x).toBeCloseTo(1 / 3, 7)
  })

  it('snaps bisected roots to short decimals only when f agrees', () => {
    expect(graph('x-0.3').roots.map((r) => r.x)).toEqual([0.3])
    expect(graph('sin(x)', [-1, 2.3]).roots.map((r) => r.x)).toEqual([0])
    expect(graph('x^2-2').roots[1]!.x).toBeCloseTo(Math.SQRT2, 14)
  })
})

describe('refined sampling', () => {
  const graph = (expr: string, domain?: [number, number]) => buildGraph(`graph ${expr}`, { domain })!
  const finite = (expr: string, domain?: [number, number]) =>
    graph(expr, domain).points.filter((p): p is { x: number; y: number } => p.y != null)

  it('follows the curve to where its domain ends', () => {
    const pts = finite('sqrt(9.3-x^2)')
    const edge = Math.sqrt(9.3)
    expect(pts[0]!.x).toBeCloseTo(-edge, 8)
    expect(pts[pts.length - 1]!.x).toBeCloseTo(edge, 8)
    expect(Math.min(...pts.map((p) => p.y))).toBeLessThan(1e-3)
  })

  it('breaks the path at jumps instead of drawing risers', () => {
    const pts = graph('floor(x)').points
    for (let i = 1; i < pts.length; i++) {
      const a = pts[i - 1]!.y
      const b = pts[i]!.y
      if (a != null && b != null) expect(b - a).toBe(0)
    }
    expect(pts.filter((p) => p.y == null).length).toBeGreaterThanOrEqual(19)
  })

  it('breaks at every pole of tan, even with both sides in view', () => {
    const g = graph('tan(x)')
    const poles = [-2.5, -1.5, -0.5, 0.5, 1.5, 2.5].map((k) => k * Math.PI)
    for (const pole of poles) {
      const gap = g.points.find((p) => p.y == null && Math.abs(p.x - pole) < 1e-6)
      expect(gap, `pole at ${pole}`).toBeTruthy()
    }
  })

  it('leaves smooth curves alone', () => {
    expect(graph('x').points).toHaveLength(401)
    expect(graph('x^2').points).toHaveLength(401)
    expect(graph('sin(x)').points.length).toBeLessThan(600)
  })
})

describe('auto y-scale', () => {
  it('shows a curve without poles whole and reaches down to the axis', () => {
    const exp = buildGraph('graph e^x')!.yScale
    expect(exp.max).toBeGreaterThan(Math.exp(10))
    expect(exp.min).toBeLessThan(0)
    const cup = buildGraph('graph x^2+1')!.yScale
    expect(cup.min).toBeLessThan(0)
    const high = buildGraph('graph cos(x)+3', { angleMode: 'rad' })!.yScale
    expect(high.min).toBeGreaterThan(1.5)
  })
})

describe('graphHome', () => {
  it('narrows the window around roots and extrema that sit in the middle', () => {
    expect(graphHome('graph x^3-3x')).toEqual([-3, 3])
    expect(graphHome('graph x^3-x')).toEqual([-1.6, 1.6])
  })

  it('keeps the standard window otherwise', () => {
    expect(graphHome('graph x^2')).toEqual([-10, 10])
    expect(graphHome('graph 1/x')).toEqual([-10, 10])
    expect(graphHome('graph sin(x)')).toEqual([-10, 10])
    expect(graphHome('graph sin(x)', { angleMode: 'deg' })).toEqual([-360, 360])
    expect(graphHome('graph nope(x)')).toEqual([-10, 10])
  })
})

describe('graphTicks', () => {
  const labels = (lo: number, hi: number, unit?: 'deg' | 'rad' | null) => graphTicks(lo, hi, unit).map((t) => t.label)

  it('steps 1-2-5 with clean labels and a true minus', () => {
    expect(labels(-10, 10)).toEqual(['\u221210', '\u22125', '0', '5', '10'])
    expect(labels(0.1, 0.7)).toEqual(['0.2', '0.4', '0.6'])
    expect(labels(0, 0.9)).toEqual(['0', '0.2', '0.4', '0.6', '0.8'])
  })

  it('uses multiples of π or of degrees on an angle axis', () => {
    expect(labels(-10, 10, 'rad')).toEqual(['\u22123π', '\u22122π', '\u2212π', '0', 'π', '2π', '3π'])
    expect(labels(-360, 360, 'deg')).toEqual(['\u2212360°', '\u2212180°', '0°', '180°', '360°'])
    // Too narrow or too wide for angle steps: plain decimals.
    expect(labels(0, 1, 'rad')).toEqual(['0', '0.2', '0.4', '0.6', '0.8', '1'])
    expect(graphTicks(-10, 10, 'rad')[4]!.value).toBe(Math.PI)
  })
})
