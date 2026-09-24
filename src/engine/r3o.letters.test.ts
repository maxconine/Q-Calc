import { describe, expect, it } from 'vitest'
import { evaluateSheet } from './evaluate'

const last = (lines: string[]) => evaluateSheet(lines, { angleMode: 'rad' }).at(-1)!.display

describe('letter splitting leaves units and differentials whole', () => {
  it.each([
    [['m=2', 's=3', '5 ms'], '5 ms'],
    [['m=2', 'A=3', '5 mA'], '5 mA'],
    [['k=2', 'V=3', '2 kV'], '2 kV'],
    [['n=2', 's=3', '4 ns'], '4 ns'],
    [['d=2', 'x=3', '∫0..1 x^2 dx'], '0.333333333333'],
    [['d=2', 't=3', '∫_0^1 t^2 dt'], '0.333333333333'],
    [['d=2', 'x=5', '∫(x^2 dx, 0, 1)'], '0.333333333333'],
    [['d=2', 'x=3', 'd/dx x^2'], '2x'],
  ])('%j → %s', (lines, shown) => {
    expect(last(lines)).toBe(shown)
  })

  it('still splits outside calculus and when the case is not a unit symbol', () => {
    expect(last(['d=2', 'x=3', 'dx'])).toBe('6')
    expect(last(['m=2', 'a=3', 'ma'])).toBe('6')
  })
})

describe('letter splitting leaves keywords whole', () => {
  it.each([
    [['a=2', 't=3', 'd/dx x^2 at 3'], '6'],
    [['f=1', 'o=2', 'r=3', 'solve y^2=4 for y'], '±2'],
    [['f=1', 'r=3', 'o=2', 'm=4', '∫ x^2 from 0 to 1'], '0.333333333333'],
    [['l=1', 'i=2', 'm=3', 'lim x→0 sin(x)/x'], '1'],
    [['o=1', 'v=2', 'e2=1', 'r=3', 'sum k over k=1..3'], '6'],
  ])('%j → %s', (lines, shown) => {
    expect(last(lines)).toBe(shown)
  })
})

it('a word run into a decimal point stays unread', () => {
  expect(last(['x=2', 'y=3', 'xy.5'])).toBe('')
})
