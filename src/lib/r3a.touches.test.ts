import { describe, expect, it } from 'vitest'
import { equalsCommits, lineCopyText } from './touches'

describe('equalsCommits stays quiet on anything that is not plain arithmetic', () => {
  const end = (t: string) => ({ start: t.length, end: t.length })

  it('leaves the symbols that mean a variable, a limit or a series alone', () => {
    for (const t of ['θ', 'θ+1', '∞', 'Σ(1,2,3)', 'lim x→0 x'])
      expect(equalsCommits(t, end(t)), t).toBe(false)
  })

  it('accepts a cube root, since it is as much arithmetic as a square root', () => {
    expect(equalsCommits('∛(8)', end('∛(8)'))).toBe(true)
    expect(equalsCommits('8+∛(8)', end('8+∛(8)'))).toBe(true)
  })

  it('an incomplete expression still passes the character check; validity is the caller’s job', () => {
    // equalsCommits only screens for arithmetic-looking characters; whether it actually
    // evaluates is decided by the = handler itself, not this filter
    expect(equalsCommits('2+', end('2+'))).toBe(true)
    expect(equalsCommits('(2', end('(2'))).toBe(true)
  })

  it('a leading + is still arithmetic', () => {
    expect(equalsCommits('+5', end('+5'))).toBe(true)
  })
})

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
