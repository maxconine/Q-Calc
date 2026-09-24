import { describe, expect, it } from 'vitest'
import { buildGraph, graphTicks } from './graph'
import { fillParens, inferParens } from './parens'

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

describe('audit: graph roots', () => {
  it.each<[string, number[]]>([
    ['graph x^2 - 4', [-2, 2]],
    ['graph x', [0]],
    ['graph y = 2x + 1', [-0.5]],
    ['graph f(x) = x^2 - 9', [-3, 3]],
    ['graph x^3 - 3x', [-Math.sqrt(3), 0, Math.sqrt(3)]],
    ['graph e^x - 2', [Math.LN2]],
    ['graph abs(x) - 1', [-1, 1]],
    ['graph ln(x)', [1]],
    ['graph x^2 - 2', [-Math.SQRT2, Math.SQRT2]],
    ['graph (x - 1)(x + 2)(x - 5)', [-2, 1, 5]],
  ])('%s has roots %j', (input, want) => {
    expectXs(roots(input), want)
  })

  it('sin(x) in radians crosses at every multiple of pi in [-10, 10]', () => {
    expectXs(roots('graph sin(x)', { angleMode: 'rad' }), [-3, -2, -1, 0, 1, 2, 3].map((k) => k * Math.PI))
  })

  it('tan(x) crosses at multiples of pi, never at its poles', () => {
    expectXs(roots('graph tan(x)', { angleMode: 'rad' }), [-3, -2, -1, 0, 1, 2, 3].map((k) => k * Math.PI))
  })

  it('sin(x) in degrees crosses at multiples of 180 in [-360, 360]', () => {
    expectXs(roots('graph sin(x)', { angleMode: 'deg' }), [-360, -180, 0, 180, 360], 1e-4)
  })

  it.each(['graph 1/x', 'graph 1/(x - 1)', 'graph x^2 + 1', 'graph x^2 + 0.0001', 'graph e^x', 'graph 1/x^2'])(
    '%s reports no roots',
    (input) => {
      expect(roots(input)).toEqual([])
    },
  )

  // the grid has points at ±4.5e-14; sin() chops |sin x| < 1e-12 to 0 there, so sin(x)/x = 0
  // and a root is reported at -4.5e-14
  it('sin(x)/x has no root at 0 (its limit there is 1)', () => {
    const xs = roots('graph sin(x)/x', { angleMode: 'rad' })
    expect(xs.some((x) => Math.abs(x) < 0.1), xs.join(', ')).toBe(false)
    expectXs(xs, [-3, -2, -1, 1, 2, 3].map((k) => k * Math.PI))
  })
})

describe('audit: graph extrema', () => {
  it('x^2 - 4 has one minimum at (0, -4)', () => {
    const g = buildGraph('graph x^2 - 4')!
    expect(g.criticalPoints).toHaveLength(1)
    expect(g.criticalPoints[0]!.kind).toBe('min')
    expect(g.criticalPoints[0]!.x).toBeCloseTo(0, 6)
    expect(g.criticalPoints[0]!.y).toBeCloseTo(-4, 9)
  })

  it('x^3 - 3x has a max at (-1, 2) and a min at (1, -2)', () => {
    const cps = buildGraph('graph x^3 - 3x')!.criticalPoints
    expect(cps.map((c) => c.kind)).toEqual(['max', 'min'])
    expect(cps[0]!.x).toBeCloseTo(-1, 6)
    expect(cps[0]!.y).toBeCloseTo(2, 9)
    expect(cps[1]!.x).toBeCloseTo(1, 6)
    expect(cps[1]!.y).toBeCloseTo(-2, 9)
  })

  it('abs(x) - 1 has its minimum at the kink', () => {
    const cps = buildGraph('graph abs(x) - 1')!.criticalPoints
    expect(cps).toHaveLength(1)
    expect(cps[0]!.kind).toBe('min')
    expect(cps[0]!.x).toBeCloseTo(0, 6)
  })

  it('a straight line and 1/x have no extrema', () => {
    expect(buildGraph('graph 2x + 1')!.criticalPoints).toEqual([])
    expect(buildGraph('graph 1/x')!.criticalPoints).toEqual([])
  })

  it('sin(x) in degrees peaks at 90 and -270, bottoms at -90 and 270', () => {
    const cps = buildGraph('graph sin(x)', { angleMode: 'deg' })!.criticalPoints
    const max = cps.filter((c) => c.kind === 'max').map((c) => c.x)
    const min = cps.filter((c) => c.kind === 'min').map((c) => c.x)
    expectXs(max, [-270, 90], 1e-3)
    expectXs(min, [-90, 270], 1e-3)
  })
})

describe('audit: graph ticks', () => {
  it.each<[number, number, number[]]>([
    [0, 10, [0, 2, 4, 6, 8, 10]],
    [-10, 10, [-10, -5, 0, 5, 10]],
    [0, 1, [0, 0.2, 0.4, 0.6, 0.8, 1]],
    [-1, 1, [-1, -0.5, 0, 0.5, 1]],
    [0, 100, [0, 20, 40, 60, 80, 100]],
  ])('ticks for [%d, %d]', (lo, hi, want) => {
    expect(graphTicks(lo, hi).map((t) => t.value)).toEqual(want)
  })

  it('a radian axis steps in pi/2 over one turn', () => {
    const ticks = graphTicks(0, 2 * Math.PI, 'rad')
    expect(ticks.map((t) => t.label)).toEqual(['0', 'π/2', 'π', '3π/2', '2π'])
  })

  it('a degree axis labels in degrees', () => {
    const ticks = graphTicks(0, 360, 'deg')
    expect(ticks.map((t) => t.label)).toEqual(['0°', '90°', '180°', '270°', '360°'])
  })

  it('an empty or reversed range has no ticks', () => {
    expect(graphTicks(5, 5)).toEqual([])
    expect(graphTicks(5, 1)).toEqual([])
  })
})

describe('audit: paren filling', () => {
  it.each([
    ['(()', '(())'],
    [')(', '()()'],
    ['sqrt(sqrt(2', 'sqrt(sqrt(2))'],
    ['2+3))*4', '((2+3))*4'],
    ['', ''],
    ['()', '()'],
  ])('%s → %s', (text, want) => {
    expect(fillParens(text)).toBe(want)
  })

  it('counts what is missing on each side', () => {
    const a = inferParens('((1')
    const b = inferParens('1))')
    expect([a.leading === 0, a.trailing]).toEqual([true, 2])
    expect([b.leading, b.trailing]).toEqual([2, 0])
  })
})
