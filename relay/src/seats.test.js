import { describe, expect, it } from 'vitest'
import { enqueue, GRACE_MS, MAX_QUEUED, newSeats, newToken, otherSide, overdue, seatOf } from './seats.js'

describe('seats', () => {
  it('gives each side its own token', () => {
    const seats = newSeats('abc', 'teal otter', 'amber fox')
    expect(seats.host.token).toMatch(/^[0-9a-f]{32}$/)
    expect(seats.host.token).not.toBe(seats.guest.token)
    expect(seatOf(seats, seats.guest.token)).toBe('guest')
    expect(seatOf(seats, seats.host.token)).toBe('host')
  })

  it('takes no token but a seat’s own', () => {
    const seats = newSeats('abc', 'a', 'b')
    expect(seatOf(seats, '')).toBeNull()
    expect(seatOf(seats, 'nope')).toBeNull()
    expect(seatOf(null, seats.host.token)).toBeNull()
  })

  it('makes tokens from the bytes given', () => {
    expect(newToken(new Uint8Array([0, 15, 255]))).toBe('000fff')
  })

  it('knows the other side', () => {
    expect(otherSide('host')).toBe('guest')
    expect(otherSide('guest')).toBe('host')
  })

  it('holds a seat for the grace time and no longer', () => {
    const seats = newSeats('abc', 'a', 'b')
    seats.away.guest = 1000
    expect(overdue(seats, 1000 + GRACE_MS - 1)).toEqual({ gone: [], next: 1000 + GRACE_MS })
    expect(overdue(seats, 1000 + GRACE_MS).gone).toEqual(['guest'])
    expect(overdue(newSeats('abc', 'a', 'b'), 5)).toEqual({ gone: [], next: null })
  })

  it('keeps only the newest messages for someone away', () => {
    const q = []
    for (let i = 0; i < MAX_QUEUED + 10; i++) enqueue(q, `.${i}`)
    expect(q).toHaveLength(MAX_QUEUED)
    expect(q[0]).toBe('.10')
    expect(q.at(-1)).toBe(`.${MAX_QUEUED + 9}`)
  })
})
