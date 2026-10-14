// the plain parts of keeping a game's seats through a dropped link, kept apart from the workers runtime so they
// can be tested. once two players pair, each gets a token for their seat. a link that drops (a closed laptop,
// a quit app, a network change) leaves the seat held for GRACE_MS, and the token takes it back. the other side
// hears `away` and `back`, and what it sends meanwhile is kept for the one who's away

// how long a dropped player's seat is held; hiding q calc mid-game counts as dropping, so it's generous
export const GRACE_MS = 10 * 60 * 1000
// what's kept for a player who's away, newest last; a turn game sends a handful, and a live one pauses
export const MAX_QUEUED = 256
// a close the player chose ends the game at once; any other (a lost link, a quit app) holds the seat
export const LEFT = 1000

export const SIDES = ['host', 'guest']

export const otherSide = (side) => (side === 'host' ? 'guest' : 'host')

export function newToken(bytes = crypto.getRandomValues(new Uint8Array(16))) {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('')
}

export function newSeats(key, hostName, guestName, token = newToken) {
  return { key, host: { token: token(), name: hostName }, guest: { token: token(), name: guestName }, away: {} }
}

// whose seat a token is, or null
export function seatOf(seats, token) {
  if (!seats || typeof token !== 'string' || !token) return null
  return SIDES.find((side) => seats[side]?.token === token) ?? null
}

// the sides that have been away too long, and when the next one will have been
export function overdue(seats, now) {
  const gone = []
  let next = null
  for (const side of SIDES) {
    const since = seats.away[side]
    if (since == null) continue
    if (now - since >= GRACE_MS) gone.push(side)
    else next = Math.min(next ?? Infinity, since + GRACE_MS)
  }
  return { gone, next }
}

export function enqueue(queue, msg) {
  queue.push(msg)
  if (queue.length > MAX_QUEUED) queue.splice(0, queue.length - MAX_QUEUED)
  return queue
}
