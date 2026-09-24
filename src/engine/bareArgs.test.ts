import { describe, expect, it } from 'vitest'
import { evaluateSheet } from './evaluate'
import { buildGraph } from './graph'

const last = (lines: string[]) => evaluateSheet(lines, { angleMode: 'deg' }).at(-1)!.display

describe('a function reads a name without brackets', () => {
  it.each([
    [['x = 30', 'sin x'], '0.5'],
    [['x = 30', 'sin x + 1'], '1.5'],
    [['x = 30', '2 sin x'], '1'],
    [['x = 30', 'sin 2x'], '0.866025403784'],
    [['x = 30', 'sin x cos x'], '0.433012701892'],
    [['x = 30', 'sin x/2'], '0.25'],
    [['x = 100', 'log x'], '2'],
    [['x = 4', 'sqrt x'], '2'],
    [['theta = 30', 'sin θ'], '0.5'],
    [['sin x = 0.5'], '30, 150'],
  ])('%j → %s', (lines, shown) => {
    expect(last(lines)).toBe(shown)
  })

  it('never reads sin 2x as sin(2)·x', () => {
    expect(last(['x = 30', 'sin 2x'])).toBe(last(['x = 30', 'sin(2x)']))
  })

  it('leaves sin x y blank, since it could be sin(xy)', () => {
    expect(last(['x = 30', 'y = 2', 'sin x y'])).toBe('')
  })

  it('keeps √2x as √2·x, as the bar draws it', () => {
    expect(last(['x = 3', '√2x'])).toBe(last(['x = 3', '√(2)x']))
  })

  it('graphs sin x like sin(x)', () => {
    expect(buildGraph('graph sin x')?.points).toEqual(buildGraph('graph sin(x)')?.points)
  })
})
