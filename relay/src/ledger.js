import { DurableObject } from 'cloudflare:workers'
import { CAPS, dayOf, GRANT_TTL_MS, hourOf, OFFER_TTL_MS, PART_TRIES, partCount, partLength, randomHex, STALE_UPLOAD_MS, STORED_TTL_MS, UPLOAD_MAX_MS } from './files.js'

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
// a removal hands back the record, so the worker can abort its upload or delete its object

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
  // can be accepted again
  async revert(id) {
    const rec = await this.file(id)
    if (rec?.state !== 'granted') return
    await this.reserve(rec, -1)
    await this.add(rec.day, -rec.bytes)
    delete rec.grant
    delete rec.grantExpires
    delete rec.day
    rec.state = 'offered'
    await this.save(rec)
  }

  // its sender starts uploading with the grant, which can't be used again; or comes back to an upload they'd
  // started, and hears which parts are in. ip is a hash of the address, kept for the hour
  async start(id, from, grant, ip) {
    const rec = await this.file(id)
    const now = Date.now()
    if (!rec || rec.from !== from) return { status: 'gone' }
    if (rec.state === 'uploading') {
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
    const hour = `ip:${ip}:${hourOf(now)}`
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
    if (!rec || rec.from !== from || rec.state !== 'uploading' || !rec.uploadId) return { status: 'gone' }
    const want = partLength(rec.bytes, n)
    rec.tries++
    if (want == null || length !== want) return { status: 'over', removed: await this.remove(id) }
    if (rec.tries > partCount(rec.bytes) * PART_TRIES) return { status: 'failed', removed: await this.remove(id) }
    if (!(await this.budget('writes', 1))) return { status: 'relay-full' }
    rec.touched = Date.now()
    await this.save(rec)
    return { status: 'ok', uploadId: rec.uploadId }
  }

  async partDone(id, n, etag) {
    const rec = await this.file(id)
    if (rec?.state !== 'uploading') return false
    rec.parts[n] = etag
    rec.touched = Date.now()
    await this.save(rec)
    return true
  }

  // every part is in: what r2 needs to put them together
  async finish(id, from) {
    const rec = await this.file(id)
    if (!rec || rec.from !== from) return { status: 'gone' }
    if (rec.state === 'stored') return { status: 'done' }
    if (rec.state !== 'uploading' || !rec.uploadId) return { status: 'gone' }
    const parts = []
    const missing = []
    for (let n = 1; n <= partCount(rec.bytes); n++) {
      if (rec.parts[n]) parts.push({ partNumber: n, etag: rec.parts[n] })
      else missing.push(n)
    }
    if (missing.length) return { status: 'incomplete', missing }
    return { status: 'ok', uploadId: rec.uploadId, parts }
  }

  async stored(id) {
    const rec = await this.file(id)
    if (rec?.state !== 'uploading') return null
    const expires = Date.now() + STORED_TTL_MS
    Object.assign(rec, { state: 'stored', expires })
    delete rec.uploadId
    delete rec.parts
    delete rec.tries
    await this.save(rec)
    return { to: rec.to, from: rec.from, expires, bytes: rec.bytes }
  }

  // whether its recipient can download it now
  async readable(id, name) {
    const rec = await this.file(id)
    if (!rec || rec.to !== name) return { status: 'gone' }
    if (rec.state !== 'stored') return { status: 'not-ready' }
    if (rec.expires <= Date.now()) return { status: 'expired', removed: await this.remove(id) }
    if (!(await this.budget('reads', 1))) return { status: 'relay-full' }
    return { status: 'ok', bytes: rec.bytes, from: rec.from }
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
    const hour = hourOf(now)
    const day = dayOf(now)
    for (const [k] of await this.store.list({ prefix: 'ip:' })) if (Number(k.split(':').pop()) < hour) await this.store.delete(k)
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
