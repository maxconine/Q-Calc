import { describe, expect, it } from 'vitest'
import { isSysCommand, solveLive, sysCommand } from './system'

describe('opening a system', () => {
  it('reads the count glued, spaced, or spelled out', () => {
    expect(sysCommand('sys3')).toEqual({ count: 3 })
    expect(sysCommand('sys 3')).toEqual({ count: 3 })
    expect(sysCommand('SYS2')).toEqual({ count: 2 })
    expect(sysCommand('system 4')).toEqual({ count: 4 })
    expect(sysCommand('sys9')).toEqual({ hint: 'up to 5 equations' })
    expect(sysCommand('sys')).toHaveProperty('hint')
  })

  it('leaves other words alone', () => {
    expect(isSysCommand('sys3')).toBe(true)
    expect(isSysCommand('systematic')).toBe(false)
    expect(isSysCommand('sysx')).toBe(false)
    expect(isSysCommand('sys3x')).toBe(false)
  })
})

describe('solving before every field is filled', () => {
  it('shows what the typed equations already fix', () => {
    expect(solveLive(['x + y + z = 6', 'z = 1', ''])?.display).toBe('z = 1')
    expect(solveLive(['x + y = 5', 'x - y = 1', ''])?.display).toBe('x = 3, y = 2')
  })

  it('stays blank while nothing is fixed yet', () => {
    expect(solveLive(['x + y = 5', '', ''])).toBeNull()
  })

  it('reads lowercase names as unknowns when there are enough equations', () => {
    expect(solveLive(['F = m*a', 'm = 2', 'a = 3'])?.display).toBe('F = 6, m = 2, a = 3')
  })
})

describe('units', () => {
  it('carries a unit through a linear system', () => {
    expect(solveLive(['x + y = 10 m', 'x - y = 2 m'])?.display).toBe('x = 6 m, y = 4 m')
  })

  it('keeps the unit that was typed', () => {
    expect(solveLive(['x + y = 10 km', 'x - y = 2 km'])?.display).toBe('x = 6 km, y = 4 km')
  })

  it('mixes units of one dimension', () => {
    expect(solveLive(['x + y = 1 km', 'x - y = 200 m'])?.display).toBe('x = 0.6 km, y = 0.4 km')
  })

  it('derives a unit from a product', () => {
    expect(solveLive(['F = m*a', 'm = 2 kg', 'a = 3 m/s^2'])?.display).toBe('F = 6 N, m = 2 kg, a = 3 m/s²')
  })

  it('reads a variable that shares a unit symbol as the variable', () => {
    expect(solveLive(['m + n = 5 kg', 'm - n = 1 kg'])?.display).toBe('m = 3 kg, n = 2 kg')
  })

  it('solves distance over time', () => {
    expect(solveLive(['d = v*t', 'v = 20 m/s', 't = 3 s'])?.display).toBe('d = 60 m, v = 20 m/s, t = 3 s')
  })

  it('says when units disagree', () => {
    expect(solveLive(['x + y = 10 m', 'x - y = 2 kg'])).toEqual({ display: "units don't match", message: true })
  })

  it('solves the typed lines with units before the rest are filled', () => {
    expect(solveLive(['x + y = 10 m', 'x - y = 2 m', ''])?.display).toBe('x = 6 m, y = 4 m')
  })

  it('leaves unitless systems as they were', () => {
    expect(solveLive(['2x + 3y = 12', 'x - y = 1'])?.display).toBe('x = 3, y = 2')
  })
})
