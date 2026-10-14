// the plain parts of the relay's room lists, kept apart from the workers runtime so they can be tested.
// a list holds, per room, only what the lobby shows: a random room id, the host's nickname and whether the room
// takes a code. never an address, and never the code

// the games with an online lobby
export const GAMES = new Set(['pong', 'connect4', 'ultimate', 'ember'])

// a room nobody joined leaves the list after this long
export const ROOM_TTL_MS = 15 * 60 * 1000
// the most rooms one game's list keeps, and the most it shows; past that the oldest go
export const MAX_ROOMS = 500
export const SHOWN_ROOMS = 50
export const NAME_MAX = 24

const ID_CHARS = 'abcdefghijkmnpqrstuvwxyz23456789'
export const ROOM_ID = /^[a-z0-9]{12}$/

export function newRoomId(bytes = crypto.getRandomValues(new Uint8Array(12))) {
  return Array.from(bytes, (b) => ID_CHARS[b % ID_CHARS.length]).join('')
}

// the same rule as the page's cleanNickname; anything that cleans to nothing is "friend"
export function cleanName(raw) {
  if (typeof raw !== 'string') return 'friend'
  const name = raw
    .normalize('NFC')
    .replace(/[^\p{L}\p{N} '’.-]/gu, '')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, NAME_MAX)
    .trim()
  return name || 'friend'
}

// what a rate limit counts by: an ipv4 address, or an ipv6 address's /64, which one user can rotate through
export function ipKey(ip) {
  if (typeof ip !== 'string' || !ip) return 'unknown'
  if (!ip.includes(':')) return ip
  // an ipv4 address written as ipv6 (::ffff:1.2.3.4) is that ipv4 address
  const v4 = /:(\d+\.\d+\.\d+\.\d+)$/.exec(ip)
  if (v4) return v4[1]
  const [head, tail] = ip.split('::')
  const left = head ? head.split(':') : []
  const right = tail != null && tail !== '' ? tail.split(':') : []
  const groups = tail != null ? [...left, ...Array(Math.max(0, 8 - left.length - right.length)).fill('0'), ...right] : left
  return groups
    .slice(0, 4)
    .map((g) => (g || '0').toLowerCase().replace(/^0+(?=.)/, ''))
    .join(':') + '::/64'
}

// one game's open rooms, newest first
export class RoomList {
  constructor(entries = []) {
    this.rooms = new Map()
    for (const e of entries) {
      if (e && typeof e.id === 'string' && typeof e.at === 'number') {
        this.rooms.set(e.id, { id: e.id, name: cleanName(e.name), locked: e.locked === true, at: e.at })
      }
    }
  }

  add({ id, name, locked }, now) {
    this.rooms.delete(id)
    this.rooms.set(id, { id, name: cleanName(name), locked: locked === true, at: now })
    while (this.rooms.size > MAX_ROOMS) this.rooms.delete(this.rooms.keys().next().value)
  }

  remove(id) {
    return this.rooms.delete(id)
  }

  // drops rooms past their time; true when any went
  prune(now) {
    let gone = false
    for (const [id, r] of this.rooms) {
      if (now - r.at > ROOM_TTL_MS) {
        this.rooms.delete(id)
        gone = true
      }
    }
    return gone
  }

  // what a watcher sees
  shown(now) {
    return [...this.rooms.values()]
      .filter((r) => now - r.at <= ROOM_TTL_MS)
      .reverse()
      .slice(0, SHOWN_ROOMS)
      .map(({ id, name, locked }) => ({ id, name, locked }))
  }

  // the oldest room's expiry, for the next alarm
  nextExpiry() {
    let soonest = null
    for (const r of this.rooms.values()) soonest = soonest == null ? r.at : Math.min(soonest, r.at)
    return soonest == null ? null : soonest + ROOM_TTL_MS
  }

  get size() {
    return this.rooms.size
  }

  dump() {
    return [...this.rooms.values()]
  }
}
