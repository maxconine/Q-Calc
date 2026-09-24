import { describe, expect, it } from 'vitest'
import { evaluateSheet } from './evaluate'

const last = (lines: string[]) => evaluateSheet(lines, { angleMode: 'rad' }).at(-1)!.display

describe('letters of defined variables multiply', () => {
  it.each([
    [['x=12.01', 'y=1.008', '(xy)/(x+y)'], '0.929949300968'],
    [['x=12.01', 'y=1.008', 'xy/(x+y)'], '0.929949300968'],
    [['x=2', 'y=3', 'z=4', 'xyz'], '24'],
    [['a=2', 'b=3', 'ab+ba'], '12'],
    [['x=2', 'y=3', '2xy^2'], '36'],
    [['x=2', 'y=3', '6/xy'], last(['x=2', 'y=3', '6/x y'])],
    [['x=2', 'y=3', 'f(t)=t+1', 'f(xy)'], '7'],
    [['f(a,b)=ab', 'f(2,5)'], '10'],
    [['x=2', 'y=3', 'w=xy', 'w'], '6'],
    [['d=2', 'm=5', 'dm'], '1 dm'],
  ])('%j → %s', (lines, shown) => {
    expect(last(lines)).toBe(shown)
  })

  it('leaves a word alone when a letter is undefined or the word is a name', () => {
    expect(last(['x=2', 'xy'])).toBe('')
    expect(last(['x=2', 'y=3', 'xy=10', 'xy'])).toBe('10')
    expect(last(['p=2', 'i=3', 'pi'])).toMatch(/^3\.14/)
    expect(last(['a=1', 'n=2', 's=3', '5', 'ans'])).toBe('5')
  })
})
