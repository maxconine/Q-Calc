import { describe, expect, it } from 'vitest'
import { lineCopyText } from './touches'

describe('⌘⇧C line copy', () => {
  it('writes the line and its answer', () => {
    expect(lineCopyText('12*7', { display: '84' }, 'approx')).toBe('12*7 = 84')
    expect(lineCopyText(' 5 ft to cm ', { display: '152.4 cm' }, 'approx')).toBe('5 ft to cm = 152.4 cm')
  })

  it('shows both sides of a dual answer', () => {
    expect(lineCopyText('√8', { display: '2.82843', exact: '2√2' }, 'approx')).toBe('√8 = 2√2 ≈ 2.82843')
  })

  it('follows the answer form when there is one side', () => {
    expect(lineCopyText('1/4', { display: '0.25', exact: '0.25' }, 'exact')).toBe('1/4 = 0.25')
  })

  it('names the letter a solve found', () => {
    const solve = { variable: 'x', outcome: 'roots' }
    expect(lineCopyText('x^2 = 2', { display: '±1.41421', exact: '±√2', solve }, 'approx')).toBe('x^2 = 2, x = ±√2 ≈ ±1.41421')
    expect(lineCopyText('x^2 = -1', { display: 'no real solution', solve: { variable: 'x', outcome: 'none' } }, 'approx')).toBe(
      'x^2 = -1, no real solution',
    )
  })

  it("doesn't repeat an answer the line already says", () => {
    expect(lineCopyText('x = 5', { display: '5' }, 'approx')).toBe('x = 5')
    expect(lineCopyText('x = 3+4', { display: '7' }, 'approx')).toBe('x = 3+4 = 7')
  })

  it('is just the answer without a line, and empty without an answer', () => {
    expect(lineCopyText('', { display: '3' }, 'approx')).toBe('3')
    expect(lineCopyText('2+', { display: '' }, 'approx')).toBe('')
  })
})
