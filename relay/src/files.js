// the plain parts of sending files and clips, kept apart from the workers runtime so they can be tested.
//
// a file is offered first, through the inbox, and nothing is stored unless its recipient accepts it. accepting
// asks the ledger (ledger.js) for room, under hard caps per recipient, per sender, per address and for the
// whole bucket; the ledger then grants the sender one upload of exactly the offered length. the sender's app
// encrypts the file (aes-256-gcm, its key riding only inside the offer message) and uploads it to r2 as a
// multipart upload in PART_BYTES parts, each its own request so every one stays under the free plan's 100 MB
// body limit. the file waits for its one recipient under an id nobody can guess, and goes once it's been
// downloaded, taken back or turned down, or STORED_TTL_MS after it was uploaded.
//
// a clip is clipboard text that rides through the inbox like a message and, like one, is never kept

// the most a file can be, encrypted: 1 GiB of plain bytes and gcm's 16-byte tag per chunk of the app's
export const FILE_MAX = 1024 * 1024 * 1024
export const BYTES_MAX = FILE_MAX + 1024 * 1024
// each part but the last is exactly this long (r2 wants equal parts of at least 5 MiB), so 1 GiB is 21 parts
export const PART_BYTES = 50 * 1024 * 1024
// tries per part, all told, before an upload is given up
export const PART_TRIES = 3

const HOUR_MS = 60 * 60 * 1000
export const DAY_MS = 24 * HOUR_MS
// an offer nobody accepted lapses after this
export const OFFER_TTL_MS = 7 * DAY_MS
// an accepted offer the sender never started uploading gives its room back after this
export const GRANT_TTL_MS = HOUR_MS
// an upload with no part for this long is aborted; one that's taken longer than a day all told is too
export const STALE_UPLOAD_MS = 2 * HOUR_MS
export const UPLOAD_MAX_MS = DAY_MS
// an uploaded file nobody downloaded goes after this
export const STORED_TTL_MS = DAY_MS
// a name this new can't send files, so throwaway names can't
export const NEW_NAME_MS = 10 * 60 * 1000

// the ledger's hard caps. pending is what's granted, being uploaded or waiting to be downloaded
export const CAPS = {
  recipientPending: 2 * 1024 ** 3,
  senderPending: 2 * 1024 ** 3,
  senderDaily: 5 * 1024 ** 3,
  // the whole bucket, which keeps it inside r2's 10 GB free tier
  totalPending: 8 * 1024 ** 3,
  // uploads started per address per hour
  addressHourly: 20,
  // offers a sender can have out at once, accepted or not
  senderOffers: 50,
  // r2 class a operations a day, all told (an upload's start and finish, and each part): about 300,000 a month,
  // under the free tier's million, and few enough durable object requests to leave room for messages and games
  dailyWrites: 10000,
  // r2 class b operations (downloads) a day: about 1.5 million a month, under the free tier's 10 million
  dailyReads: 50000,
}

// every file's object key starts with this, so the bucket's lifecycle rule can name it
export const FILE_PREFIX = 'f/'
// 16 random bytes, as hex
export const FILE_ID = /^[0-9a-f]{32}$/
export const SHA256 = /^[0-9a-f]{64}$/
// an aes-256 key, base64url without padding
export const AES_KEY = /^[A-Za-z0-9_-]{43}$/
// a clip is up to this many bytes of utf-8
export const CLIP_MAX = 64 * 1024
const NAME_MAX = 120
const OCTETS = 'application/octet-stream'

// what an app that only knows text messages shows instead
export const FALLBACK = {
  file: 'sent you a file — update Q Calc to open it',
  clip: 'shared their clipboard — update Q Calc to see it',
}

export function randomHex(bytes = 16) {
  return Array.from(crypto.getRandomValues(new Uint8Array(bytes)), (b) => b.toString(16).padStart(2, '0')).join('')
}

export const fileKey = (id) => FILE_PREFIX + id

// a file's name as the sender's app gave it: its last path part, without control or direction characters,
// path separators, or leading dots, at most NAME_MAX characters with its extension kept; 'file' when nothing's
// left
export function cleanFilename(raw) {
  if (typeof raw !== 'string') return 'file'
  let name = raw.normalize('NFC')
  name = name.split(/[/\\]/).pop()
  // eslint-disable-next-line no-control-regex
  name = name.replace(/[\u0000-\u001f\u007f-\u009f\u200b-\u200f\u2028-\u202e\u2066-\u2069\ufeff:]+/g, '_')
  name = name.replace(/^[\s.]+/, '').replace(/\s+$/, '')
  const chars = Array.from(name)
  if (chars.length > NAME_MAX) {
    const dot = name.lastIndexOf('.')
    const ext = dot > 0 ? Array.from(name.slice(dot)) : []
    name = ext.length > 1 && ext.length <= 16 ? chars.slice(0, NAME_MAX - ext.length).join('').trimEnd() + ext.join('') : chars.slice(0, NAME_MAX).join('').trimEnd()
  }
  return name || 'file'
}

// a media type like image/png, without parameters; application/octet-stream when it isn't one
export function cleanType(raw) {
  if (typeof raw !== 'string') return OCTETS
  const type = raw.split(';')[0].trim().toLowerCase()
  return /^[a-z0-9][a-z0-9!#$&^_.+-]{0,63}\/[a-z0-9][a-z0-9!#$&^_.+-]{0,63}$/.test(type) ? type : OCTETS
}

const count = (n, max) => Number.isSafeInteger(n) && n >= 0 && n <= max

// an offer as the sender's app made it: the facts the recipient sees, or 'too-big', or null when it isn't one.
// size is the plain length, bytes the encrypted length that's uploaded and counted
export function cleanOffer(raw) {
  if (!raw || typeof raw !== 'object') return null
  const { size, bytes, sha256, key } = raw
  if (!Number.isSafeInteger(size) || !Number.isSafeInteger(bytes) || size < 0 || bytes < size) return null
  if (size > FILE_MAX || bytes > BYTES_MAX) return 'too-big'
  if (typeof sha256 !== 'string' || !SHA256.test(sha256) || typeof key !== 'string' || !AES_KEY.test(key)) return null
  return { name: cleanFilename(raw.name), size, bytes, type: cleanType(raw.type), sha256, key }
}

// how many parts an upload of this many bytes takes, and how long part n (from 1) must be
export const partCount = (bytes) => Math.max(1, Math.ceil(bytes / PART_BYTES))
export function partLength(bytes, n) {
  if (!Number.isSafeInteger(n) || n < 1 || n > partCount(bytes)) return null
  return Math.min(PART_BYTES, bytes - (n - 1) * PART_BYTES)
}

// a declared length, or null when it isn't one
export function declaredLength(raw) {
  if (typeof raw !== 'string' || !/^[0-9]{1,12}$/.test(raw)) return null
  const n = Number(raw)
  return count(n, PART_BYTES) ? n : null
}

// a download's start, from a `Range: bytes=N-` header: 0 with none, null when it's one that can't be served
export function rangeStart(raw, bytes) {
  if (raw == null || raw === '') return 0
  const m = /^bytes=([0-9]{1,12})-$/.exec(raw.trim())
  if (!m) return null
  const n = Number(m[1])
  return n < bytes ? n : null
}

// a clip as it was copied, or '' when it's empty or too big
export function cleanClip(raw) {
  if (typeof raw !== 'string' || !raw.length) return ''
  return new TextEncoder().encode(raw).length <= CLIP_MAX ? raw : ''
}

// a content-disposition that names the file without letting it mean anything else
export function disposition(name) {
  const ascii = name.replace(/[^\x20-\x7e]|["\\%]/g, '_')
  return `attachment; filename="${ascii}"; filename*=UTF-8''${encodeURIComponent(name)}`
}

export const hourOf = (now) => Math.floor(now / HOUR_MS)
export const dayOf = (now) => Math.floor(now / DAY_MS)
