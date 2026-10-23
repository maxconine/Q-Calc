// the plain parts of messaging, kept apart from the workers runtime so they can be tested. a name is held by
// the hash of a key its app made and keeps; the relay never sees the key at rest, never stores a message, and
// lets a name nobody's used in LAPSE_MS go

// 2–20 lowercase letters, digits, - or _, starting with a letter or digit; the same rule as the page's cleanName
export const NAME = /^[a-z0-9][a-z0-9_-]{1,19}$/
// 32 random bytes, as hex
export const KEY = /^[0-9a-f]{64}$/
export const TEXT_MAX = 500
// a name not seen for this long lapses, and anyone can take it
export const LAPSE_MS = 30 * 24 * 60 * 60 * 1000
// how often a held name's last-seen time is written, so a busy name doesn't write on every use
export const SEEN_EVERY_MS = 24 * 60 * 60 * 1000
// what an inbox socket offers alongside its key, and what the relay answers with
export const INBOX_PROTOCOL = 'qcalc-inbox'
// the most inbox sockets one name keeps; a new one closes the oldest
export const MAX_INBOX_SOCKETS = 3

// a name as typed, lowercased; '' when it can't be one
export function cleanHandle(raw) {
  if (typeof raw !== 'string') return ''
  const name = raw.trim().toLowerCase()
  return NAME.test(name) ? name : ''
}

// one line of at most TEXT_MAX, control characters as single spaces; '' when nothing's left
export function cleanText(raw) {
  if (typeof raw !== 'string') return ''
  // eslint-disable-next-line no-control-regex
  return raw.normalize('NFC').replace(/[\u0000-\u001f\u007f-\u009f\u200e\u200f\u2028-\u202e\u2066-\u2069]+/g, ' ').replace(/ {2,}/g, ' ').trim().slice(0, TEXT_MAX).trim()
}

// the key an inbox socket offers as a protocol (`k<hex>`), since a browser can't set a header; null when none
export function keyFromProtocols(header) {
  if (typeof header !== 'string') return null
  const offered = header.split(',').map((p) => p.trim())
  if (!offered.includes(INBOX_PROTOCOL)) return null
  const k = offered.find((p) => p[0] === 'k' && KEY.test(p.slice(1)))
  return k ? k.slice(1) : null
}

export async function hashKey(key) {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(key))
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, '0')).join('')
}

export function lapsed(claim, now) {
  return !claim || now - claim.seen > LAPSE_MS
}

// what holding a name with this key does: 'ok' and whether to write the claim, or 'taken'
export function holdName(claim, hash, now) {
  if (!lapsed(claim, now) && claim.hash !== hash) return { ok: false }
  const fresh = lapsed(claim, now) || now - claim.seen > SEEN_EVERY_MS
  return { ok: true, write: fresh ? { hash, seen: now } : null }
}
