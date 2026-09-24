import { describe, expect, it } from 'vitest'
import { lineCopyText } from './touches'

describe('lineCopyText with an unusual solve outcome', () => {
  it('a message-only solve reads as just the message, no "x =" prefix', () => {
    const solve = { variable: 'x', outcome: 'none' }
    expect(lineCopyText('x^2 = -1', { display: 'no real solution', solve }, 'approx')).toBe('x^2 = -1, no real solution')
  })

  it('an identity-style outcome still just appends its message', () => {
    const solve = { variable: 'x', outcome: 'all' }
    expect(lineCopyText('x = x', { display: 'true for all x', solve }, 'approx')).toBe('x = x, true for all x')
  })
})

describe('lineCopyText leaves an already-stated answer alone even with extra spacing', () => {
  it('does not double up when the right-hand side already matches after trimming', () => {
    expect(lineCopyText('x =  5 ', { display: '5' }, 'approx')).toBe('x =  5')
  })
})
