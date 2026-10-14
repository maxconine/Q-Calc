import { describe, expect, it } from 'vitest'
import { cleanName, ipKey, MAX_ROOMS, newRoomId, ROOM_ID, ROOM_TTL_MS, RoomList, SHOWN_ROOMS } from './directory.js'

describe('room list', () => {
  it('shows only id, nickname and lock, newest first', () => {
    const list = new RoomList()
    list.add({ id: 'a', name: 'teal otter', locked: false, code: '123456', ip: '1.2.3.4' }, 1000)
    list.add({ id: 'b', name: 'amber fox', locked: true }, 2000)
    expect(list.shown(3000)).toEqual([
      { id: 'b', name: 'amber fox', locked: true },
      { id: 'a', name: 'teal otter', locked: false },
    ])
    expect(JSON.stringify(list.dump())).not.toMatch(/123456|1\.2\.3\.4/)
  })

  it('drops rooms that are joined, closed or expired', () => {
    const list = new RoomList()
    list.add({ id: 'a', name: 'a', locked: false }, 0)
    list.add({ id: 'b', name: 'b', locked: false }, 1000)
    expect(list.remove('a')).toBe(true)
    expect(list.remove('a')).toBe(false)
    expect(list.nextExpiry()).toBe(1000 + ROOM_TTL_MS)
    expect(list.shown(1000 + ROOM_TTL_MS + 1)).toEqual([])
    expect(list.prune(1000 + ROOM_TTL_MS + 1)).toBe(true)
    expect(list.size).toBe(0)
    expect(list.nextExpiry()).toBeNull()
  })

  it('keeps at most so many rooms, losing the oldest, and shows fewer', () => {
    const list = new RoomList()
    for (let i = 0; i < MAX_ROOMS + 5; i++) list.add({ id: `r${i}`, name: 'x', locked: false }, i)
    expect(list.size).toBe(MAX_ROOMS)
    expect(list.dump()[0].id).toBe('r5')
    expect(list.shown(MAX_ROOMS + 5)).toHaveLength(SHOWN_ROOMS)
  })

  it('comes back from storage, dropping anything malformed', () => {
    const list = new RoomList([{ id: 'a', name: '<x>', locked: true, at: 5 }, { id: 3 }, null])
    expect(list.shown(10)).toEqual([{ id: 'a', name: 'x', locked: true }])
  })
})

describe('relay helpers', () => {
  it('cleans nicknames the way the page does', () => {
    expect(cleanName('  teal   otter ')).toBe('teal otter')
    expect(cleanName('')).toBe('friend')
    expect(cleanName(null)).toBe('friend')
    expect(cleanName('a'.repeat(50))).toHaveLength(24)
  })

  it('makes room ids that pass its own check', () => {
    expect(newRoomId()).toMatch(ROOM_ID)
    expect(newRoomId(new Uint8Array(12))).toBe('aaaaaaaaaaaa')
  })

  it('counts ipv6 by its /64', () => {
    expect(ipKey('203.0.113.9')).toBe('203.0.113.9')
    expect(ipKey('2001:db8:aa:bb:1:2:3:4')).toBe('2001:db8:aa:bb::/64')
    expect(ipKey('2001:0db8:00aa:00bb::9')).toBe('2001:db8:aa:bb::/64')
    expect(ipKey('2001:db8::1')).toBe('2001:db8:0:0::/64')
    expect(ipKey('::ffff:198.51.100.7')).toBe('198.51.100.7')
    expect(ipKey(null)).toBe('unknown')
  })
})
