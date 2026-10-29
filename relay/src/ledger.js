import { DurableObject } from 'cloudflare:workers'
import { CAPS, dayOf, FILE_ID, GRANT_TTL_MS, hourOf, OFFER_TTL_MS, PART_TRIES, partCount, partLength, randomHex, STALE_UPLOAD_MS, STORED_TTL_MS, streamPartBytes, UPLOAD_MAX_MS } from './files.js'

// the one ledger of every file offered, accepted, being uploaded or waiting to be downloaded, and of the bytes
// they hold against the hard caps in files.js. it keeps no file's contents, name or key: only who sent it to
// whom, how long it is, and where it's got to. an offer holds no room; accepting it reserves its full length
// against the recipient's, the sender's and the bucket's caps, and the room comes back when the file is
// downloaded, taken back, turned down or expired. a sender's day of uploads isn't given back.
//
//   offered    the recipient heard of it; nothing stored                       lapses after OFFER_TTL_MS
//   granted    the recipient accepted; the sender may start, once (`grant`)     back to offered after GRANT_TTL_MS
//   uploading  parts are arriving (`uploadId`, `parts`)                         aborted when stale
//   stored     waiting for its recipient                                        deleted after STORED_TTL_MS
//
// a streamed upload (`stream`, its part length) has no multipart upload: each part is an object of its own, and
// its recipient may read each one (`partReadable`, `partRead`) while the rest are still coming. it goes once every
// part has been read and the upload's finished, or, read as one file by an app that doesn't stream, as any other.
//
// a removal hands back the record, so the worker can abort its upload or delete its object (or parts).
//
// an address is never kept: an upload's start counts against a tag, sha-256 over a random salt drawn for that
// hour and the address, and both the salt and the tags go once the hour's over, so a tag can't be traced back
// to an address afterwards, even by trying every one. each method runs start to finish without another request
// in between (a durable object's input gate holds them while it only awaits its own storage), so an accept and
// a take-back can't interleave inside the ledger; the worker's steps around it are checked for it below

async function sha256Hex(text) {
  const d = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text))
  return Array.from(new Uint8Array(d), (b) => b.toString(16).padStart(2, '0')).join('')
}

// the states that hold room against the caps
function holds(rec) {
  return rec.state === 'granted' || rec.state === 'uploading' || rec.state === 'stored'
}

export class Ledger extends DurableObject {
  get store() {
    return this.ctx.storage
  }

  async add(key, n) {
    const v = ((await this.store.get(key)) ?? 0) + n
    if (v > 0) await this.store.put(key, v)
    else await this.store.delete(key)
    return v
  }

  // counts n of a day's r2 operations of one class, false when the day's cap is reached
  async budget(kind, n) {
    const key = `${kind}:${dayOf(Date.now())}`
    const cap = kind === 'writes' ? CAPS.dailyWrites : CAPS.dailyReads
    if (((await this.store.get(key)) ?? 0) + n > cap) return false
    await this.add(key, n)
    return true
  }

  async file(id) {
    return (await this.store.get(`file:${id}`)) ?? null
  }

  async save(rec) {
    await this.store.put(`file:${rec.id}`, rec)
  }

  async reserve(rec, sign) {
    await this.add(`pend:to:${rec.to}`, sign * rec.bytes)
    await this.add(`pend:from:${rec.from}`, sign * rec.bytes)
    await this.add('total', sign * rec.bytes)
  }

  async remove(id, name = null) {
    const rec = await this.file(id)
    if (!rec || (name != null && name !== rec.from && name !== rec.to)) return null
    if (holds(rec)) await this.reserve(rec, -1)
    await this.add(`offers:${rec.from}`, -1)
    await this.store.delete(`file:${id}`)
    return rec
  }

  // a sender offers a file; it holds no room until it's accepted
  async offer({ id, from, to, bytes }) {
    // the sender's app drew the id; one that's already out is turned away, never shared or replaced
    if (!FILE_ID.test(id ?? '') || (await this.file(id))) return { status: 'bad' }
    if (bytes > Math.min(CAPS.recipientPending, CAPS.senderPending, CAPS.senderDaily, CAPS.totalPending)) return { status: 'too-big' }
    if (((await this.store.get(`offers:${from}`)) ?? 0) >= CAPS.senderOffers) return { status: 'too-many' }
    const now = Date.now()
    await this.add(`offers:${from}`, 1)
    await this.save({ id, from, to, bytes, state: 'offered', created: now, expires: now + OFFER_TTL_MS })
    return { status: 'ok', expires: now + OFFER_TTL_MS }
  }

  // its recipient accepts: room for it under every cap, and a grant for its sender to upload it once
  async grant(id, name) {
    const rec = await this.file(id)
    const now = Date.now()
    if (!rec || rec.to !== name) return { status: 'gone' }
    if (rec.state !== 'offered') return { status: 'busy' }
    if (rec.expires <= now) {
      await this.remove(id)
      return { status: 'expired' }
    }
    const day = `day:${rec.from}:${dayOf(now)}`
    const has = async (k) => (await this.store.get(k)) ?? 0
    if ((await has(`pend:to:${rec.to}`)) + rec.bytes > CAPS.recipientPending) return { status: 'recipient-full' }
    if ((await has(`pend:from:${rec.from}`)) + rec.bytes > CAPS.senderPending) return { status: 'sender-full' }
    if ((await has(day)) + rec.bytes > CAPS.senderDaily) return { status: 'daily-limit' }
    if ((await has('total')) + rec.bytes > CAPS.totalPending) return { status: 'relay-full' }
    await this.reserve(rec, 1)
    await this.add(day, rec.bytes)
    Object.assign(rec, { state: 'granted', grant: randomHex(), grantExpires: now + GRANT_TTL_MS, day })
    await this.save(rec)
    return { status: 'ok', from: rec.from, grant: rec.grant, bytes: rec.bytes }
  }

  // a grant its sender never heard of, or never used: the room and the day's bytes come back, and the offer
  // can be accepted again. with `grant`, only that grant is taken back, so a late revert never undoes a newer one
  async revert(id, grant = null) {
    const rec = await this.file(id)
    if (rec?.state !== 'granted' || (grant != null && rec.grant !== grant)) return
    await this.reserve(rec, -1)
    await this.add(rec.day, -rec.bytes)
    delete rec.grant
    delete rec.grantExpires
    delete rec.day
    rec.state = 'offered'
    await this.save(rec)
  }

  // its sender starts uploading with the grant, which can't be used again; or comes back to an upload they'd
  // started, and hears which parts are in. `address` is counted under the hour's salted tag and never kept
  async start(id, from, grant, address) {
    const rec = await this.file(id)
    const now = Date.now()
    if (!rec || rec.from !== from) return { status: 'gone' }
    if (rec.state === 'uploading') {
      if (rec.stream) return { status: 'ok', stream: rec.stream, parts: Object.keys(rec.parts).map(Number), bytes: rec.bytes }
      if (!rec.uploadId) return { status: 'busy' }
      return { status: 'ok', uploadId: rec.uploadId, parts: Object.keys(rec.parts).map(Number), bytes: rec.bytes }
    }
    if (rec.state === 'stored') return { status: 'done' }
    if (rec.state !== 'granted') return { status: 'not-accepted' }
    if (rec.grant !== grant) return { status: 'gone' }
    if (rec.grantExpires <= now) {
      await this.revert(id)
      return { status: 'expired' }
    }
    await this.prune(now)
    const hour = `ip:${await this.tag(address, now)}:${hourOf(now)}`
    if (((await this.store.get(hour)) ?? 0) >= CAPS.addressHourly) return { status: 'too-many' }
    // its start and its finish
    if (!(await this.budget('writes', 2))) return { status: 'relay-full' }
    await this.add(hour, 1)
    delete rec.grant
    delete rec.grantExpires
    Object.assign(rec, { state: 'uploading', started: now, touched: now, parts: {}, tries: 0, uploadId: null })
    await this.save(rec)
    return { status: 'ok', uploadId: null, parts: [], bytes: rec.bytes }
  }

  // its sender starts a streamed upload with the grant (the first part carries it): each part is an object of its
  // own that its recipient can read as soon as it's in, each streamPartBytes long but the last. coming back to a
  // streamed upload, without the grant, is `start`'s
  async startStream(id, from, grant, address) {
    const rec = await this.file(id)
    const now = Date.now()
    if (!rec || rec.from !== from) return { status: 'gone' }
    if (rec.state === 'uploading' && rec.stream) return { status: 'ok', started: false }
    if (rec.state === 'uploading') return { status: 'busy' }
    if (rec.state === 'stored') return { status: 'done' }
    if (rec.state !== 'granted') return { status: 'not-accepted' }
    if (rec.grant !== grant) return { status: 'gone' }
    if (rec.grantExpires <= now) {
      await this.revert(id)
      return { status: 'expired' }
    }
    await this.prune(now)
    const hour = `ip:${await this.tag(address, now)}:${hourOf(now)}`
    if (((await this.store.get(hour)) ?? 0) >= CAPS.addressHourly) return { status: 'too-many' }
    await this.add(hour, 1)
    delete rec.grant
    delete rec.grantExpires
    // no multipart upload to make or put together: each part is one write, counted as it comes
    Object.assign(rec, { state: 'uploading', started: now, touched: now, parts: {}, tries: 0, stream: streamPartBytes(rec.bytes), read: {} })
    await this.save(rec)
    return { status: 'ok', started: true }
  }

  // the hour's salt, drawn the first time it's needed
  async tag(address, now) {
    const key = `salt:${hourOf(now)}`
    let salt = await this.store.get(key)
    if (!salt) {
      salt = randomHex(32)
      await this.store.put(key, salt)
    }
    return sha256Hex(`${salt}:${address}`)
  }

  // address tags and salts from hours that are over go at once, not only at the hourly sweep
  async prune(now) {
    const hour = hourOf(now)
    for (const prefix of ['ip:', 'salt:']) {
      for (const [k] of await this.store.list({ prefix })) if (Number(k.split(':').pop()) < hour) await this.store.delete(k)
    }
  }

  async started(id, uploadId) {
    const rec = await this.file(id)
    if (rec?.state !== 'uploading') return false
    rec.uploadId = uploadId
    await this.save(rec)
    return true
  }

  // a part on its way: it must be exactly the length its place in the file has, so no byte goes past what was
  // granted. one that isn't, or too many tries, ends the upload (the record comes back as `removed`)
  async part(id, from, n, length) {
    const rec = await this.file(id)
    if (!rec || rec.from !== from || rec.state !== 'uploading' || !(rec.uploadId || rec.stream)) return { status: 'gone' }
    const size = rec.stream || undefined
    const want = partLength(rec.bytes, n, size)
    rec.tries++
    if (want == null || length !== want) return { status: 'over', removed: await this.remove(id) }
    if (rec.tries > partCount(rec.bytes, size) * PART_TRIES) return { status: 'failed', removed: await this.remove(id) }
    if (!(await this.budget('writes', 1))) return { status: 'relay-full' }
    rec.touched = Date.now()
    await this.save(rec)
    return { status: 'ok', uploadId: rec.uploadId, stream: rec.stream ?? 0, to: rec.to }
  }

  async partDone(id, n, etag) {
    const rec = await this.file(id)
    if (rec?.state !== 'uploading') return false
    rec.parts[n] = etag
    rec.touched = Date.now()
    await this.save(rec)
    return true
  }

  // every part is in: what r2 needs to put them together (a streamed file's parts stay as they are)
  async finish(id, from) {
    const rec = await this.file(id)
    if (!rec || rec.from !== from) return { status: 'gone' }
    if (rec.state === 'stored') return { status: 'done' }
    if (rec.state !== 'uploading' || !(rec.uploadId || rec.stream)) return { status: 'gone' }
    const parts = []
    const missing = []
    for (let n = 1; n <= partCount(rec.bytes, rec.stream || undefined); n++) {
      if (rec.parts[n]) parts.push({ partNumber: n, etag: rec.parts[n] })
      else missing.push(n)
    }
    if (missing.length) return { status: 'incomplete', missing }
    return { status: 'ok', uploadId: rec.uploadId, parts, stream: rec.stream ?? 0 }
  }

  // uploaded. a streamed file whose recipient has already read every part goes at once (`gone` comes back true)
  async stored(id) {
    const rec = await this.file(id)
    if (rec?.state !== 'uploading') return null
    const expires = Date.now() + STORED_TTL_MS
    Object.assign(rec, { state: 'stored', expires })
    delete rec.uploadId
    delete rec.tries
    if (rec.stream && this.allRead(rec)) {
      await this.remove(id)
      return { to: rec.to, from: rec.from, expires, bytes: rec.bytes, stream: rec.stream, gone: true }
    }
    if (!rec.stream) delete rec.parts
    await this.save(rec)
    return { to: rec.to, from: rec.from, expires, bytes: rec.bytes, stream: rec.stream ?? 0 }
  }

  allRead(rec) {
    const n = partCount(rec.bytes, rec.stream)
    return Object.keys(rec.read ?? {}).length >= n
  }

  // whether its recipient can download it now, as one file (a streamed one is read part by part from r2, a read
  // each)
  async readable(id, name) {
    const rec = await this.file(id)
    if (!rec || rec.to !== name) return { status: 'gone' }
    if (rec.state !== 'stored') return { status: 'not-ready' }
    if (rec.expires <= Date.now()) return { status: 'expired', removed: await this.remove(id) }
    if (!(await this.budget('reads', rec.stream ? partCount(rec.bytes, rec.stream) : 1))) return { status: 'relay-full' }
    return { status: 'ok', bytes: rec.bytes, from: rec.from, stream: rec.stream ?? 0 }
  }

  // whether its recipient can read part n of a streamed file now: `not-ready` until it's in, `whole` for a file
  // that isn't streamed (its sender's app uploads it in one, to be read once it's `ready`)
  async partReadable(id, name, n) {
    const rec = await this.file(id)
    if (!rec || rec.to !== name) return { status: 'gone' }
    if (rec.state === 'offered') return { status: 'not-accepted' }
    if (rec.state === 'granted') return { status: 'not-ready' }
    if (!rec.stream) return { status: 'whole' }
    const length = partLength(rec.bytes, n, rec.stream)
    if (length == null) return { status: 'bad' }
    if (!rec.parts[n]) return { status: 'not-ready' }
    if (rec.state === 'stored' && rec.expires <= Date.now()) return { status: 'expired', removed: await this.remove(id) }
    if (!(await this.budget('reads', 1))) return { status: 'relay-full' }
    return { status: 'ok', length, from: rec.from }
  }

  // part n has been read to its end. once every part has been and the upload's finished, the file goes (the
  // record comes back as `removed`, for the worker to delete its parts)
  async partRead(id, n) {
    const rec = await this.file(id)
    if (!rec?.stream) return {}
    rec.read[n] = 1
    if (rec.state === 'stored' && this.allRead(rec)) return { removed: await this.remove(id) }
    await this.save(rec)
    return {}
  }

  // the hourly sweep: lapsed offers, unused grants, stale uploads and expired files, and old counters. hands
  // back what it removed
  async sweep() {
    const now = Date.now()
    const removed = []
    for (const [, rec] of await this.store.list({ prefix: 'file:' })) {
      if (rec.state === 'granted' && rec.grantExpires <= now) await this.revert(rec.id)
      const stale = rec.state === 'uploading' && (rec.touched + STALE_UPLOAD_MS <= now || rec.started + UPLOAD_MAX_MS <= now)
      if (stale || ((rec.state === 'offered' || rec.state === 'stored') && rec.expires <= now)) removed.push(await this.remove(rec.id))
    }
    const day = dayOf(now)
    await this.prune(now)
    for (const prefix of ['day:', 'writes:', 'reads:']) {
      for (const [k] of await this.store.list({ prefix })) if (Number(k.split(':').pop()) < day) await this.store.delete(k)
    }
    return removed
  }

  // what's held, for the tests and for a look from the dashboard
  async totals() {
    return { total: (await this.store.get('total')) ?? 0 }
  }
}
