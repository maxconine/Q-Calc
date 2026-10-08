import { describe, expect, it } from 'vitest'
import { joinDots } from './dots'

const squash = (s: string) => joinDots(s).replace(/\s+/g, ' ').trim()

describe('joinDots', () => {
  it.each([
    ['\\dot\\theta', 'thetadot'],
    ['\\ddot{x}', 'xddot'],
    ['e^{k\\theta}\\,\\dot\\theta', 'e^{k\\theta}\\, thetadot'],
    ['dot(theta)', 'thetadot'],
    ['dot.double(x)', 'xddot'],
    ['θ̇', 'thetadot'],
    ['θ²θ̇', 'θ² thetadot'],
    ['ẋ sin θ', 'xdot sin θ'],
    ['ẍ', 'xddot'],
  ])('reads %s as %s', (typed, want) => {
    expect(squash(typed)).toBe(want)
  })

  it.each(['2 dot 3', 'x dot y', '5 Å', 'café', '5 Ångström in m', 'naïve', 'dot([1, 2], [3, 4])'])('leaves %s alone', (typed) => {
    expect(joinDots(typed)).toBe(typed)
  })
})
