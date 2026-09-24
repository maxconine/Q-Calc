import { describe, expect, it } from 'vitest'
import { evaluateSheet } from './evaluate'
import { buildGraph } from './graph'

const shown = (...lines: string[]) => evaluateSheet(lines, { angleMode: 'deg' }).at(-1)!.display

describe('a function glued to its letter', () => {
  it.each([
    [['logx=2'], '100'],
    [['lnx=1'], '2.71828182846'],
    [['sinx=0.5'], '30, 150'],
    [['cosy=0'], '90, 270'],
    [['sqrtx=3'], '9'],
    [['cbrtx=2'], '8'],
    [['absx=3'], '±3'],
    [['sinθ=0.5'], '30, 150'],
    [['x=100', 'logx'], '2'],
    [['x=100', '2logx'], '4'],
    [['x=2.5', 'floorx'], '2'],
    [['a=30', 'sina'], '0.5'],
    [['t=0', 'cost'], '1'],
  ])('%j → %s', (lines, answer) => {
    expect(shown(...lines)).toBe(answer)
  })

  it('leaves words alone', () => {
    expect(shown('cost=5', 'cost')).toBe('5')
    expect(shown('logb=2')).toBe('2')
    expect(shown('logs')).toBe('')
  })

  it('graphs the same curve', () => {
    expect(buildGraph('graph sinx')?.points).toEqual(buildGraph('graph sin x')?.points)
  })
})
