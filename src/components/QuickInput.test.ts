import { describe, expect, it } from 'vitest'
import { flattenPastedText, spliceText } from './QuickInput'

describe('flattenPastedText', () => {
  it('keeps a single line', () => {
    expect(flattenPastedText('sin(90)')).toBe('sin(90)')
  })

  it('turns line breaks into spaces', () => {
    expect(flattenPastedText('2+2\n3+3')).toBe('2+2 3+3')
    expect(flattenPastedText('a\r\nb\rc')).toBe('a b c')
  })
})

describe('spliceText', () => {
  it('inserts a history answer at the caret', () => {
    expect(spliceText('cos(', '31', 4, 4)).toEqual({ next: 'cos(31', cursor: 6 })
  })

  it('inserts a history expression at the caret', () => {
    expect(spliceText('4*', '2+3', 2, 2)).toEqual({ next: '4*2+3', cursor: 5 })
    expect(spliceText('sin(', '2+3', 4, 4)).toEqual({ next: 'sin(2+3', cursor: 7 })
    expect(spliceText('cos()', 'pi/6', 4, 4)).toEqual({ next: 'cos(pi/6)', cursor: 8 })
  })

  it('replaces a selection', () => {
    expect(spliceText('cos(x)', '31', 4, 5)).toEqual({ next: 'cos(31)', cursor: 6 })
  })
})
