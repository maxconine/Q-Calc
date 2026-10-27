// the plain parts of sending files and clips, kept apart from the workers runtime so they can be tested.
//
// both are end-to-end encrypted by the apps (native/PRIVACY.md): the relay sees a file's encrypted length, who
// sent it to whom and when, and never its name, type, hash, key or contents; a clip it sees only as a sealed box.
//
// a file is offered first, through the inbox, and nothing is stored unless its recipient accepts it. accepting
// asks the ledger (ledger.js) for room, under hard caps per recipient, per sender, per address and for the
// whole bucket; the ledger then grants the sender one upload of exactly the offered length. the sender's app
// encrypts the file (aes-256-gcm, its key sealed for the recipient alone inside the offer) and uploads it to r2 as a
// multipart upload in PART_BYTES parts, each its own request so every one stays under the free plan's 100 MB
// body limit. the file waits for its one recipient under an id the sender drew (128 random bits), and goes once it's been
// downloaded, taken back or turned down, or STORED_TTL_MS after it was uploaded.
//
// a clip is sealed clipboard text that rides through the inbox like a message and, like one, is never kept

// the most a file can be, encrypted: 1 GiB of plain bytes and gcm's 16-byte tag per chunk of the app's
export const FILE_MAX = 1024 * 1024 * 1024
// (1024 chunks of 1 MiB, each with its tag: exactly what the app sends for a 1 GiB file)
export const BYTES_MAX = FILE_MAX + 16 * 1024
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
// an x25519 public key, 32 bytes as base64url without padding (its last character carries two zero bits)
export const PUBLIC_KEY = /^[A-Za-z0-9_-]{42}[AEIMQUYcgkosw048]$/
const BOX = /^[A-Za-z0-9_-]+$/
// a clip is up to this many bytes of utf-8
export const CLIP_MAX = 64 * 1024
// a sealed box: a 12-byte nonce, at least a byte, and a 16-byte tag; a file's facts are well under 2 KB, and a
// clip's box is at most its 64 KB with the nonce and tag, all in base64url
const SEALED_MIN = 40
export const SEALED_FILE_MAX = 4096
export const SEALED_CLIP_MAX = Math.ceil((CLIP_MAX + 28) / 3) * 4
// an empty file, encrypted, is one 16-byte tag
export const BYTES_MIN = 16

// what an app that only knows text messages shows instead
export const FALLBACK = {
  file: 'sent you a file — update Q Calc to open it',
  clip: 'shared their clipboard — update Q Calc to see it',
}

export function randomHex(bytes = 16) {
  return Array.from(crypto.getRandomValues(new Uint8Array(bytes)), (b) => b.toString(16).padStart(2, '0')).join('')
}

// the r2 key for a file: only ever an id of 32 hex characters under FILE_PREFIX, whatever reaches here, so no
// path or key can be smuggled into the bucket
export function fileKey(id) {
  if (typeof id !== 'string' || !FILE_ID.test(id)) throw new Error('not a file id')
  return FILE_PREFIX + id
}

const count = (n, max) => Number.isSafeInteger(n) && n >= 0 && n <= max

// a sealed share's parts, as the sender's app made them: an id it drew (16 random bytes as hex), its ephemeral
// x25519 public key and the sealed box (base64url). the relay can't open the box and doesn't try; it only checks
// that each part is the shape and size it should be
function cleanSealed(raw, max) {
  if (!raw || typeof raw !== 'object') return null
  const { id, epk, sealed } = raw
  if (typeof id !== 'string' || !FILE_ID.test(id)) return null
  if (typeof epk !== 'string' || !PUBLIC_KEY.test(epk)) return null
  if (typeof sealed !== 'string' || sealed.length < SEALED_MIN || sealed.length > max || !BOX.test(sealed)) return null
  return { id, epk, sealed }
}

// a file's offer: its id, ephemeral key and sealed facts, and the encrypted length the relay has to hold. 'too-big'
// past a 1 GiB file's length, null when it isn't one. the file's name, type, size, hash and key are all inside
// the sealed box, so none of them ever reaches the relay
export function cleanSealedOffer(raw) {
  if (!raw || typeof raw !== 'object') return null
  const { bytes } = raw
  if (!Number.isSafeInteger(bytes) || bytes < BYTES_MIN) return null
  if (bytes > BYTES_MAX) return 'too-big'
  const sealed = cleanSealed(raw, SEALED_FILE_MAX)
  return sealed ? { ...sealed, bytes } : null
}

// a clip: its id, ephemeral key and the sealed clipboard text, which is never more than a clip's 64 KB
export function cleanSealedClip(raw) {
  return cleanSealed(raw, SEALED_CLIP_MAX)
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

// a content-disposition that names the file without letting it mean anything else
export function disposition(name) {
  const ascii = name.replace(/[^\x20-\x7e]|["\\%]/g, '_')
  return `attachment; filename="${ascii}"; filename*=UTF-8''${encodeURIComponent(name)}`
}

export const hourOf = (now) => Math.floor(now / HOUR_MS)
export const dayOf = (now) => Math.floor(now / DAY_MS)
