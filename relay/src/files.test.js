import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import worker, { Inbox, Ledger } from './index.js'
import { BYTES_MAX, CAPS, cleanSealedClip, cleanSealedOffer, FALLBACK, fileKey, NEW_NAME_MS, OFFER_TTL_MS, PART_BYTES, partCount, partLength, rangeStart, SEALED_CLIP_MAX, SEALED_FILE_MAX, STALE_UPLOAD_MS, STORED_TTL_MS } from './files.js'

// files and clips through the worker, its inboxes and its ledger, against the same kind of stand-in for the
// workers runtime as inbox.test.js, plus a bucket that keeps what's put in it

vi.mock('cloudflare:workers', () => ({
  DurableObject: class {
    constructor(ctx, env) {
      this.ctx = ctx
      this.env = env
    }
  },
}))

const NodeResponse = globalThis.Response

class Sock {
  readyState = 1
  sent = []
  tags = []
  peer = null
  accept() {}
  send(s) {
    if (this.readyState !== 1) throw new Error('closed')
    this.sent.push(s)
  }
  close() {
    this.readyState = 3
  }
}

vi.stubGlobal('WebSocketPair', function () {
  const client = new Sock()
  const server = new Sock()
  client.peer = server
  server.peer = client
  return { 0: client, 1: server }
})
vi.stubGlobal('WebSocketRequestResponsePair', class {})
vi.stubGlobal('WebSocket', { OPEN: 1 })
vi.stubGlobal(
  'Response',
  class {
    constructor(body, init = {}) {
      this.body = body
      this.status = init.status ?? 200
      this.webSocket = init.webSocket ?? null
      this.headers = new Headers(init.headers)
    }
    async json() {
      return JSON.parse(this.body)
    }
  },
)

class Ctx {
  socks = []
  data = new Map()
  storage = {
    get: async (k) => structuredClone(this.data.get(k)),
    put: async (k, v) => void this.data.set(k, structuredClone(v)),
    delete: async (k) => this.data.delete(k),
    deleteAll: async () => this.data.clear(),
    list: async ({ prefix }) => new Map([...this.data].filter(([k]) => k.startsWith(prefix)).map(([k, v]) => [k, structuredClone(v)])),
    setAlarm: async () => {},
    deleteAlarm: async () => {},
  }
  setWebSocketAutoResponse() {}
  acceptWebSocket(ws, tags) {
    ws.tags = tags
    this.socks.push(ws)
  }
  getWebSockets(tag) {
    return this.socks.filter((ws) => ws.readyState !== 3 && ws.tags.includes(tag))
  }
}

async function readAll(body) {
  if (body instanceof Uint8Array) return body
  return new Uint8Array(await new NodeResponse(body).arrayBuffer())
}

// a bucket: objects, and multipart uploads in progress
class Bucket {
  objects = new Map()
  uploads = new Map()
  next = 1
  async get(k, opts) {
    const o = this.objects.get(k)
    if (!o) return null
    const bytes = o.slice(opts?.range?.offset ?? 0)
    return { body: new ReadableStream({ start: (c) => (c.enqueue(bytes), c.close()) }) }
  }
  async delete(k) {
    this.objects.delete(k)
  }
  async createMultipartUpload(k) {
    const id = `u${this.next++}`
    this.uploads.set(id, { k, parts: new Map() })
    return this.resumeMultipartUpload(k, id)
  }
  resumeMultipartUpload(k, id) {
    const bucket = this
    const uploads = this.uploads
    const objects = this.objects
    return {
      uploadId: id,
      async uploadPart(n, body) {
        const bytes = await readAll(body)
        uploads.get(id).parts.set(n, bytes)
        return { partNumber: n, etag: `e${n}-${bytes.length}` }
      },
      async complete(parts) {
        const u = uploads.get(id)
        const chunks = parts.map((p) => u.parts.get(p.partNumber))
        const all = new Uint8Array(chunks.reduce((n, c) => n + c.length, 0))
        let at = 0
        for (const c of chunks) {
          all.set(c, at)
          at += c.length
        }
        objects.set(k, all)
        uploads.delete(id)
        await bucket.onComplete?.()
      },
      async abort() {
        uploads.delete(id)
      },
    }
  }
}

const KEY_A = 'a1'.repeat(32)
const KEY_B = 'b2'.repeat(32)
const KEY_E = 'e5'.repeat(32)
// an x25519 public key (32 bytes, base64url) and a sealed box the relay can't open and doesn't try to
const EPK = 'AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyA'
const BOX = 'c2VhbGVk'.repeat(8)
let nextId = 1
const newId = () => (nextId++).toString(16).padStart(32, '0')

function stub(cls, env, map) {
  return {
    idFromName: (n) => n,
    get(n) {
      if (!map.has(n)) map.set(n, new cls(new Ctx(), env))
      const b = map.get(n)
      return new Proxy(b, { get: (t, k) => (k === 'fetch' ? (url) => t.fetch({ url }) : t[k].bind(t)) })
    },
  }
}

function relay(limits = {}) {
  const boxes = new Map()
  const ledgers = new Map()
  const env = { FILES: new Bucket(), ...limits }
  env.INBOX = stub(Inbox, env, boxes)
  env.LEDGER = stub(Ledger, env, ledgers)
  const waits = []
  const ctx = { waitUntil: (p) => waits.push(p) }
  const headers = (h = {}) => new Headers({ 'cf-connecting-ip': '1.2.3.4', ...h })
  const post = async (path, o) => {
    const text = JSON.stringify(o)
    const res = await worker.fetch({ url: 'https://relay.test' + path, method: 'POST', headers: headers(), text: async () => text }, env, ctx)
    return res.json()
  }
  // a /file request as `name`, holding `key`
  const file = async (method, path, name, key, h = {}, body = null) => {
    const res = await worker.fetch({ url: 'https://relay.test' + path, method, headers: headers({ 'X-QCalc-Name': name, 'X-QCalc-Key': key, ...h }), body }, env, ctx)
    return res
  }
  const connect = async (name, key) => {
    const req = { url: `https://relay.test/inbox/${name}`, method: 'GET', headers: headers({ Upgrade: 'websocket', 'Sec-WebSocket-Protocol': `qcalc-inbox, k${key}` }) }
    return (await worker.fetch(req, env)).webSocket
  }
  const settle = async () => {
    await Promise.all(waits.splice(0))
  }
  return { env, boxes, post, file, connect, settle, ledger: () => ledgers.get('files') }
}

const heard = (ws) => ws.peer.sent.filter((s) => s[0] === '!').map((s) => JSON.parse(s.slice(1)))
const last = (ws) => heard(ws).at(-1)
const status = async (res) => (await res.json()).status
const offer = (o = {}) => ({ id: newId(), epk: EPK, sealed: BOX, bytes: 21, ...o })
const sealedClip = (o = {}) => ({ id: newId(), epk: EPK, sealed: BOX, ...o })

// ana (an older name) and max, both online
async function pair() {
  const r = relay()
  const max = await r.connect('max', KEY_A)
  const ana = await r.connect('ana', KEY_B)
  vi.setSystemTime(NEW_NAME_MS)
  return { r, max, ana }
}

async function offered(r, o = offer()) {
  const res = await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'file', file: o })
  return res.id
}

// an offer accepted and uploaded in one part
async function uploaded(r, ana, bytes = new Uint8Array(21).fill(7)) {
  const id = await offered(r, offer({ bytes: bytes.length }))
  await r.file('POST', `/file/${id}/accept`, 'max', KEY_A)
  const { grant } = last(ana)
  await r.file('POST', `/file/${id}/start`, 'ana', KEY_B, { 'X-QCalc-Grant': grant })
  await r.file('PUT', `/file/${id}/part/1`, 'ana', KEY_B, { 'Content-Length': String(bytes.length) }, bytes)
  await r.file('POST', `/file/${id}/finish`, 'ana', KEY_B)
  return id
}

const saved = { ...CAPS }
beforeEach(() => {
  vi.useFakeTimers()
  vi.setSystemTime(0)
})
afterEach(() => {
  vi.useRealTimers()
  Object.assign(CAPS, saved)
})

describe('the plain rules', () => {
  it('takes a sealed offer only with an id, an ephemeral key, a sealed box and a length', () => {
    const o = offer()
    expect(cleanSealedOffer(o)).toEqual({ id: o.id, epk: EPK, sealed: BOX, bytes: 21 })
    expect(cleanSealedOffer(offer({ bytes: BYTES_MAX + 1 }))).toBe('too-big')
    expect(cleanSealedOffer(offer({ bytes: 2 ** 53 }))).toBe(null)
    expect(cleanSealedOffer(offer({ bytes: 15 }))).toBeNull()
    expect(cleanSealedOffer(offer({ bytes: -1 }))).toBeNull()
    expect(cleanSealedOffer(offer({ bytes: 1.5 }))).toBeNull()
    expect(cleanSealedOffer(offer({ bytes: '21' }))).toBeNull()
    expect(cleanSealedOffer(offer({ id: '../../f/' + '0'.repeat(24) }))).toBeNull()
    expect(cleanSealedOffer(offer({ id: 'A'.repeat(32) }))).toBeNull()
    expect(cleanSealedOffer(offer({ epk: 'short' }))).toBeNull()
    // a key with bits past its 32 bytes isn't one
    expect(cleanSealedOffer(offer({ epk: EPK.slice(0, 42) + 'B' }))).toBeNull()
    expect(cleanSealedOffer(offer({ sealed: 'a+b/'.repeat(16) }))).toBeNull()
    expect(cleanSealedOffer(offer({ sealed: 'Q'.repeat(SEALED_FILE_MAX + 1) }))).toBeNull()
    expect(cleanSealedOffer(offer({ sealed: 'Q'.repeat(8) }))).toBeNull()
  })

  it('takes a sealed clip up to a 64 KB clip’s box, and never a plain one', () => {
    expect(cleanSealedClip(sealedClip({ sealed: 'Q'.repeat(SEALED_CLIP_MAX) }))).not.toBeNull()
    expect(cleanSealedClip(sealedClip({ sealed: 'Q'.repeat(SEALED_CLIP_MAX + 1) }))).toBeNull()
    expect(cleanSealedClip('line one\nline two')).toBeNull()
    expect(cleanSealedClip(null)).toBeNull()
  })

  it('makes an r2 key only from a file id', () => {
    expect(fileKey('ab'.repeat(16))).toBe('f/' + 'ab'.repeat(16))
    for (const bad of ['../x', 'ab'.repeat(16) + '/../y', '', 'AB'.repeat(16), undefined]) expect(() => fileKey(bad)).toThrow()
  })

  it('cuts an upload into equal parts but the last', () => {
    expect(partCount(0)).toBe(1)
    expect(partCount(2 ** 30)).toBe(21)
    expect(partLength(PART_BYTES + 10, 1)).toBe(PART_BYTES)
    expect(partLength(PART_BYTES + 10, 2)).toBe(10)
    expect(partLength(PART_BYTES + 10, 3)).toBeNull()
    expect(partLength(10, 0)).toBeNull()
  })

  it('reads an open-ended range only', () => {
    expect(rangeStart(null, 10)).toBe(0)
    expect(rangeStart('bytes=4-', 10)).toBe(4)
    expect(rangeStart('bytes=10-', 10)).toBeNull()
    expect(rangeStart('bytes=0-3', 10)).toBeNull()
  })

})

describe('clips', () => {
  it('pass through sealed, with a line for apps that only know text', async () => {
    const { r, max } = await pair()
    const clip = sealedClip()
    expect((await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'clip', clip })).status).toBe('delivered')
    expect(last(max)).toEqual({ type: 'message', from: 'ana', text: FALLBACK.clip, kind: 'clip', clip })
    expect(JSON.stringify([...r.boxes.get('max').ctx.data.values()])).not.toContain(BOX)
  })

  it('turn away one that’s plain, too big or empty', async () => {
    const { r } = await pair()
    const send = (clip) => r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'clip', clip })
    expect((await send('line one\nline two')).status).toBe('bad')
    expect((await send(sealedClip({ sealed: 'Q'.repeat(SEALED_CLIP_MAX + 4) }))).status).toBe('bad')
    expect((await send('')).status).toBe('bad')
    expect((await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'nope', text: 'hi' })).status).toBe('bad')
  })
})

describe('offering a file', () => {
  it('passes the sealed offer on under the sender’s id, and stores nothing', async () => {
    const { r, max } = await pair()
    const o = offer()
    const res = await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'file', file: { ...o, name: 'notes.pdf', key: 'A'.repeat(43) } })
    expect(res.status).toBe('delivered')
    expect(res.id).toBe(o.id)
    // only the sealed parts go on: anything plain the sender added is dropped
    expect(last(max)).toEqual({
      type: 'message',
      from: 'ana',
      text: FALLBACK.file,
      kind: 'file',
      file: { id: o.id, epk: EPK, sealed: BOX, bytes: 21, expires: NEW_NAME_MS + OFFER_TTL_MS },
    })
    expect(await r.ledger().totals()).toEqual({ total: 0 })
    // the ledger keeps who, to whom and how long, and nothing sealed
    const kept = JSON.stringify([...r.ledger().ctx.data.values()])
    expect(kept).not.toContain(BOX)
    expect(kept).not.toContain(EPK)
    expect(kept).not.toContain('notes')
    expect(r.env.FILES.objects.size + r.env.FILES.uploads.size).toBe(0)
  })

  it('turns away a plain offer, whatever it says', async () => {
    const { r } = await pair()
    const plain = { name: 'notes.pdf', size: 5, bytes: 21, type: 'application/pdf', sha256: 'c3'.repeat(32), key: 'A'.repeat(43) }
    expect((await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'file', file: plain })).status).toBe('bad')
  })

  it('won’t take an id that’s already out, so nobody can take over another file', async () => {
    const { r } = await pair()
    const o = offer()
    expect((await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'file', file: o })).status).toBe('delivered')
    await r.connect('eve', KEY_E)
    vi.setSystemTime(2 * NEW_NAME_MS)
    expect((await r.post('/send', { from: 'eve', key: KEY_E, to: 'max', kind: 'file', file: o })).status).toBe('bad')
    expect((await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'file', file: o })).status).toBe('bad')
    expect((await r.ledger().file(o.id)).from).toBe('ana')
  })

  it('keeps nothing of an offer nobody heard', async () => {
    const r = relay()
    await r.connect('ana', KEY_B)
    await r.post('/claim', { name: 'max', key: KEY_A })
    vi.setSystemTime(NEW_NAME_MS)
    expect((await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'file', file: offer() })).status).toBe('offline')
    expect([...r.ledger().ctx.data.keys()]).toEqual([])
  })

  it('takes files from a name claimed just now', async () => {
    const r = relay()
    await r.connect('max', KEY_A)
    await r.connect('ana', KEY_B)
    expect((await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'file', file: offer() })).status).not.toBe('new-name')
  })

  it('won’t take files from a name newer than a NEW_NAME_MS var', async () => {
    const r = relay({ NEW_NAME_MS: '600000' })
    await r.connect('max', KEY_A)
    await r.connect('ana', KEY_B)
    vi.setSystemTime(600000 - 1)
    expect((await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'file', file: offer() })).status).toBe('new-name')
  })

  it('turns away a file over 1 GB, or an offer that isn’t one', async () => {
    const { r } = await pair()
    expect((await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'file', file: offer({ bytes: 2 ** 31 }) })).status).toBe('too-big')
    expect((await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'file', file: offer({ epk: undefined }) })).status).toBe('bad')
    expect((await r.post('/send', { from: 'ana', key: KEY_A, to: 'max', kind: 'file', file: offer() })).status).toBe('not-yours')
  })

  it('caps the offers a sender has out', async () => {
    const { r } = await pair()
    CAPS.senderOffers = 2
    await offered(r)
    await offered(r)
    expect((await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'file', file: offer() })).status).toBe('too-many')
  })
})

describe('accepting and uploading', () => {
  it('sends a file end to end, holding its room until it’s downloaded', async () => {
    const { r, max, ana } = await pair()
    const id = await offered(r)
    expect(await status(await r.file('POST', `/file/${id}/accept`, 'ana', KEY_B))).toBe('gone')
    expect(await status(await r.file('POST', `/file/${id}/accept`, 'max', KEY_A))).toBe('ok')
    const accepted = last(ana)
    expect(accepted).toMatchObject({ type: 'file', event: 'accepted', id, to: 'max', bytes: 21, partBytes: PART_BYTES })
    expect(await r.ledger().totals()).toEqual({ total: 21 })
    expect(await status(await r.file('POST', `/file/${id}/accept`, 'max', KEY_A))).toBe('busy')

    // the grant is the sender's, once
    expect(await status(await r.file('POST', `/file/${id}/start`, 'ana', KEY_B, { 'X-QCalc-Grant': 'f'.repeat(32) }))).toBe('gone')
    expect(await status(await r.file('POST', `/file/${id}/start`, 'max', KEY_A, { 'X-QCalc-Grant': accepted.grant }))).toBe('gone')
    const started = await (await r.file('POST', `/file/${id}/start`, 'ana', KEY_B, { 'X-QCalc-Grant': accepted.grant })).json()
    expect(started).toEqual({ status: 'ok', partBytes: PART_BYTES, parts: 1, done: [] })
    expect(await status(await r.file('POST', `/file/${id}/finish`, 'ana', KEY_B))).toBe('incomplete')

    const bytes = new Uint8Array(21).map((_, i) => i)
    expect(await status(await r.file('PUT', `/file/${id}/part/1`, 'ana', KEY_B, { 'Content-Length': '21' }, bytes))).toBe('ok')
    // coming back to it says which parts are in, without the grant
    expect((await (await r.file('POST', `/file/${id}/start`, 'ana', KEY_B)).json()).done).toEqual([1])
    expect(await status(await r.file('POST', `/file/${id}/finish`, 'ana', KEY_B))).toBe('ok')
    expect(last(max)).toEqual({ type: 'file', event: 'ready', id, from: 'ana', expires: NEW_NAME_MS + STORED_TTL_MS })

    // only the recipient can download it
    expect(await status(await r.file('GET', `/file/${id}`, 'ana', KEY_B))).toBe('gone')
    expect(await status(await r.file('GET', `/file/${id}`, 'max', KEY_B))).toBe('not-yours')
    const res = await r.file('GET', `/file/${id}`, 'max', KEY_A)
    expect(res.status).toBe(200)
    expect(res.headers.get('Content-Type')).toBe('application/octet-stream')
    expect(res.headers.get('Content-Disposition')).toMatch(/^attachment;/)
    expect(res.headers.get('X-Content-Type-Options')).toBe('nosniff')
    expect(res.headers.get('Content-Length')).toBe('21')
    expect(Array.from(await readAll(res.body))).toEqual(Array.from(bytes))
    await r.settle()
    // and then it's gone, with its room
    expect(r.env.FILES.objects.size).toBe(0)
    expect(await r.ledger().totals()).toEqual({ total: 0 })
    expect(await status(await r.file('GET', `/file/${id}`, 'max', KEY_A))).toBe('gone')
  })

  it('puts parts together in order, each exactly its length, and aborts an upload that goes over', async () => {
    const { r, max, ana } = await pair()
    const bytes = PART_BYTES + 10
    const id = await offered(r, offer({ bytes }))
    await r.file('POST', `/file/${id}/accept`, 'max', KEY_A)
    await r.file('POST', `/file/${id}/start`, 'ana', KEY_B, { 'X-QCalc-Grant': last(ana).grant })
    expect(await status(await r.file('PUT', `/file/${id}/part/2`, 'ana', KEY_B, { 'Content-Length': '10' }, new Uint8Array(10).fill(2)))).toBe('ok')
    expect(await status(await r.file('PUT', `/file/${id}/part/1`, 'ana', KEY_B, { 'Content-Length': String(PART_BYTES) }, new Uint8Array(PART_BYTES).fill(1)))).toBe('ok')
    expect(await status(await r.file('POST', `/file/${id}/finish`, 'ana', KEY_B))).toBe('ok')
    const stored = [...r.env.FILES.objects.values()][0]
    expect(stored.length).toBe(bytes)
    expect([stored[0], stored[PART_BYTES - 1], stored[PART_BYTES]]).toEqual([1, 1, 2])

    // a part longer than its place, or one past the end, ends the upload
    const id2 = await offered(r)
    await r.file('POST', `/file/${id2}/accept`, 'max', KEY_A)
    await r.file('POST', `/file/${id2}/start`, 'ana', KEY_B, { 'X-QCalc-Grant': last(ana).grant })
    expect(r.env.FILES.uploads.size).toBe(1)
    expect(await status(await r.file('PUT', `/file/${id2}/part/1`, 'ana', KEY_B, { 'Content-Length': '22' }, new Uint8Array(22)))).toBe('over')
    expect(r.env.FILES.uploads.size).toBe(0)
    expect(last(max)).toEqual({ type: 'file', event: 'cancelled', id: id2, from: 'ana' })
    expect(await r.ledger().totals()).toEqual({ total: bytes })
  })

  it('gives up an upload after too many tries', async () => {
    const { r, ana } = await pair()
    const id = await offered(r)
    await r.file('POST', `/file/${id}/accept`, 'max', KEY_A)
    await r.file('POST', `/file/${id}/start`, 'ana', KEY_B, { 'X-QCalc-Grant': last(ana).grant })
    const put = () => r.file('PUT', `/file/${id}/part/1`, 'ana', KEY_B, { 'Content-Length': '21' }, new Uint8Array(21))
    for (let i = 0; i < 3; i++) expect(await status(await put())).toBe('ok')
    expect(await status(await put())).toBe('failed')
    expect(await r.ledger().totals()).toEqual({ total: 0 })
  })

  it('needs the sender online to hear the accept, and gives the room back if not', async () => {
    const r = relay()
    await r.connect('max', KEY_A)
    const ana = await r.connect('ana', KEY_B)
    vi.setSystemTime(NEW_NAME_MS)
    const id = await offered(r)
    ana.peer.readyState = 3
    expect(await status(await r.file('POST', `/file/${id}/accept`, 'max', KEY_A))).toBe('sender-offline')
    expect(await r.ledger().totals()).toEqual({ total: 0 })
    const back = await r.connect('ana', KEY_B)
    expect(await status(await r.file('POST', `/file/${id}/accept`, 'max', KEY_A))).toBe('ok')
    expect(last(back).event).toBe('accepted')
  })

  it('caps what’s pending per recipient, per sender, per day and for the whole bucket', async () => {
    const { r } = await pair()
    const accept = async () => status(await r.file('POST', `/file/${await offered(r)}/accept`, 'max', KEY_A))
    CAPS.totalPending = 30
    expect(await accept()).toBe('ok')
    expect(await accept()).toBe('relay-full')
    Object.assign(CAPS, saved, { recipientPending: 30 })
    expect(await accept()).toBe('recipient-full')
    Object.assign(CAPS, saved, { senderPending: 30 })
    expect(await accept()).toBe('sender-full')
    Object.assign(CAPS, saved, { senderDaily: 40 })
    expect(await accept()).toBe('daily-limit')
  })

  it('caps the r2 operations of a day, for the whole relay', async () => {
    const { r, ana } = await pair()
    CAPS.dailyWrites = 3
    const id = await offered(r)
    await r.file('POST', `/file/${id}/accept`, 'max', KEY_A)
    expect(await status(await r.file('POST', `/file/${id}/start`, 'ana', KEY_B, { 'X-QCalc-Grant': last(ana).grant }))).toBe('ok')
    const put = () => r.file('PUT', `/file/${id}/part/1`, 'ana', KEY_B, { 'Content-Length': '21' }, new Uint8Array(21))
    expect(await status(await put())).toBe('ok')
    expect(await status(await put())).toBe('relay-full')
    vi.setSystemTime(NEW_NAME_MS + 24 * 60 * 60 * 1000)
    CAPS.dailyReads = 0
    expect(await status(await put())).toBe('ok')
    expect(await status(await r.file('POST', `/file/${id}/finish`, 'ana', KEY_B))).toBe('ok')
    expect(await status(await r.file('GET', `/file/${id}`, 'max', KEY_A))).toBe('relay-full')
  })

  it('caps uploads started per address per hour, keeping only a hash of the address', async () => {
    const { r, ana } = await pair()
    CAPS.addressHourly = 1
    const start = async () => {
      const id = await offered(r)
      await r.file('POST', `/file/${id}/accept`, 'max', KEY_A)
      return status(await r.file('POST', `/file/${id}/start`, 'ana', KEY_B, { 'X-QCalc-Grant': last(ana).grant }))
    }
    expect(await start()).toBe('ok')
    expect(await start()).toBe('too-many')
    expect(JSON.stringify([...r.ledger().ctx.data.keys()])).not.toContain('1.2.3.4')
    vi.setSystemTime(NEW_NAME_MS + 60 * 60 * 1000)
    await r.ledger().sweep()
    expect(await start()).toBe('ok')
  })
})

describe('downloading', () => {
  it('picks up where a cut-short download stopped', async () => {
    const { r, ana } = await pair()
    const id = await uploaded(r, ana, new Uint8Array(21).map((_, i) => i))
    expect(await status(await r.file('GET', `/file/${id}`, 'max', KEY_A, { Range: 'bytes=0-3' }))).toBe('bad')
    const res = await r.file('GET', `/file/${id}`, 'max', KEY_A, { Range: 'bytes=15-' })
    expect(res.status).toBe(206)
    expect(res.headers.get('Content-Range')).toBe('bytes 15-20/21')
    expect(Array.from(await readAll(res.body))).toEqual([15, 16, 17, 18, 19, 20])
    // reading to the end counts as downloaded
    await r.settle()
    expect(r.env.FILES.objects.size).toBe(0)
  })

  it('says a file waiting too long has expired, and lets it go', async () => {
    const { r, ana } = await pair()
    const id = await uploaded(r, ana)
    vi.setSystemTime(NEW_NAME_MS + STORED_TTL_MS)
    expect(await status(await r.file('GET', `/file/${id}`, 'max', KEY_A))).toBe('expired')
    expect(r.env.FILES.objects.size).toBe(0)
    expect(await r.ledger().totals()).toEqual({ total: 0 })
  })
})

describe('taking back and turning down', () => {
  it('lets the sender take a file back mid-upload, aborting it, and tells the recipient', async () => {
    const { r, max, ana } = await pair()
    const id = await offered(r)
    await r.file('POST', `/file/${id}/accept`, 'max', KEY_A)
    await r.file('POST', `/file/${id}/start`, 'ana', KEY_B, { 'X-QCalc-Grant': last(ana).grant })
    expect(await status(await r.file('DELETE', `/file/${id}`, 'ana', KEY_B))).toBe('ok')
    expect(r.env.FILES.uploads.size).toBe(0)
    expect(last(max)).toEqual({ type: 'file', event: 'cancelled', id, from: 'ana' })
    expect(await r.ledger().totals()).toEqual({ total: 0 })
  })

  it('lets the recipient turn one down, deleting it, and tells the sender', async () => {
    const { r, ana } = await pair()
    const id = await uploaded(r, ana)
    expect(r.env.FILES.objects.size).toBe(1)
    expect(await status(await r.file('DELETE', `/file/${id}`, 'max', KEY_A))).toBe('ok')
    expect(r.env.FILES.objects.size).toBe(0)
    expect(last(ana)).toEqual({ type: 'file', event: 'declined', id, to: 'max' })
  })

  it('lets nobody else', async () => {
    const { r, ana } = await pair()
    const id = await uploaded(r, ana)
    await r.connect('eve', 'e5'.repeat(32))
    expect(await status(await r.file('DELETE', `/file/${id}`, 'eve', 'e5'.repeat(32)))).toBe('gone')
    expect(r.env.FILES.objects.size).toBe(1)
  })
})

describe('the hourly sweep', () => {
  it('lets lapsed offers, unused grants, stale uploads and expired files go', async () => {
    const { r, max, ana } = await pair()
    const lapsing = await offered(r)
    const stale = await offered(r)
    await r.file('POST', `/file/${stale}/accept`, 'max', KEY_A)
    await r.file('POST', `/file/${stale}/start`, 'ana', KEY_B, { 'X-QCalc-Grant': last(ana).grant })
    const unused = await offered(r)
    await r.file('POST', `/file/${unused}/accept`, 'max', KEY_A)
    const done = await uploaded(r, ana)
    expect(await r.ledger().totals()).toEqual({ total: 63 })

    vi.setSystemTime(NEW_NAME_MS + STALE_UPLOAD_MS)
    await worker.scheduled({}, r.env, { waitUntil: (p) => p })
    await vi.waitFor(() => expect(r.env.FILES.uploads.size).toBe(0))
    expect(heard(max).filter((e) => e.event === 'cancelled').map((e) => e.id)).toEqual([stale])
    // the unused grant is an offer again, and holds nothing
    expect(await r.ledger().totals()).toEqual({ total: 21 })

    vi.setSystemTime(NEW_NAME_MS + STORED_TTL_MS)
    await r.ledger().sweep().then((gone) => Promise.all(gone.map((rec) => r.env.FILES.delete(`f/${rec.id}`))))
    expect(r.env.FILES.objects.size).toBe(0)
    expect(await status(await r.file('GET', `/file/${done}`, 'max', KEY_A))).toBe('gone')

    vi.setSystemTime(NEW_NAME_MS + OFFER_TTL_MS)
    await r.ledger().sweep()
    expect(await status(await r.file('POST', `/file/${lapsing}/accept`, 'max', KEY_A))).toBe('gone')
    expect(await status(await r.file('POST', `/file/${unused}/accept`, 'max', KEY_A))).toBe('gone')
    expect([...r.ledger().ctx.data.keys()]).toEqual([])
  })
})

describe('authorization, guessing, replays, sizes and races', () => {
  // ana sends to max; eve has a name of her own and tries everything on their file
  async function three() {
    const p = await pair()
    await p.r.connect('eve', KEY_E)
    vi.setSystemTime(2 * NEW_NAME_MS)
    return p
  }
  const routes = (id) => [
    ['POST', `/file/${id}/accept`],
    ['POST', `/file/${id}/start`],
    ['PUT', `/file/${id}/part/1`],
    ['POST', `/file/${id}/finish`],
    ['GET', `/file/${id}`],
    ['DELETE', `/file/${id}`],
  ]
  const as = (r, name, key, [method, path], h = {}) =>
    r.file(method, path, name, key, method === 'PUT' ? { 'Content-Length': '21', ...h } : h, method === 'PUT' ? new Uint8Array(21) : null)

  it('lets only the sender start, upload, finish or take back, and only the recipient accept, download or decline', async () => {
    const { r, ana } = await three()
    const id = await offered(r)
    // eve, at every step, on every route: the same answer as a file that doesn't exist
    const eve = async () => {
      for (const route of routes(id)) expect(await status(await as(r, 'eve', KEY_E, route, { 'X-QCalc-Grant': 'f'.repeat(32) }))).toBe('gone')
    }
    await eve()
    expect(await status(await r.file('POST', `/file/${id}/accept`, 'ana', KEY_B))).toBe('gone')
    expect(await status(await r.file('POST', `/file/${id}/accept`, 'max', KEY_A))).toBe('ok')
    const { grant } = last(ana)
    await eve()
    // the recipient can't use the sender's grant, upload, or finish
    expect(await status(await r.file('POST', `/file/${id}/start`, 'max', KEY_A, { 'X-QCalc-Grant': grant }))).toBe('gone')
    expect(await status(await r.file('POST', `/file/${id}/start`, 'ana', KEY_B, { 'X-QCalc-Grant': grant }))).toBe('ok')
    expect(await status(await as(r, 'max', KEY_A, ['PUT', `/file/${id}/part/1`]))).toBe('gone')
    expect(await status(await as(r, 'ana', KEY_B, ['PUT', `/file/${id}/part/1`]))).toBe('ok')
    expect(await status(await r.file('POST', `/file/${id}/finish`, 'max', KEY_A))).toBe('gone')
    expect(await status(await r.file('POST', `/file/${id}/finish`, 'ana', KEY_B))).toBe('ok')
    await eve()
    // the sender can't download it
    expect(await status(await r.file('GET', `/file/${id}`, 'ana', KEY_B))).toBe('gone')
    // a right name with the wrong key is nobody
    expect(await status(await r.file('GET', `/file/${id}`, 'max', KEY_E))).toBe('not-yours')
    expect(r.env.FILES.objects.size).toBe(1)
  })

  it('answers a guessed id exactly as someone else’s, so ids can’t be probed', async () => {
    const { r } = await three()
    const id = await offered(r)
    const guess = 'f'.repeat(32)
    for (const [i, route] of routes(guess).entries()) {
      const theirs = await as(r, 'eve', KEY_E, routes(id)[i])
      const nobody = await as(r, 'eve', KEY_E, route)
      expect([nobody.status, await status(nobody)]).toEqual([theirs.status, await status(theirs)])
    }
    // what isn't an id never reaches the ledger or the bucket: a dotted path is resolved away from /file/
    // altogether, and anything else is gone
    const before = JSON.stringify([...r.ledger().ctx.data])
    for (const path of ['/file/../f/x', '/file/' + 'A'.repeat(32), '/file/' + 'a'.repeat(31), `/file/${id}/part/1/../../x`, `/file/${id}%2f..`, `/file/${id}/part/1%2f..%2f..`]) {
      const res = await r.file('GET', path, 'max', KEY_A)
      expect([404, 426]).toContain(res.status)
    }
    expect(JSON.stringify([...r.ledger().ctx.data])).toBe(before)
    expect(r.env.FILES.objects.size + r.env.FILES.uploads.size).toBe(0)
  })

  it('takes a grant once, and never one that was taken back', async () => {
    const r = relay()
    await r.connect('max', KEY_A)
    let ana = await r.connect('ana', KEY_B)
    vi.setSystemTime(NEW_NAME_MS)
    const id = await offered(r)
    // accepted while ana's away: the grant she never heard of is taken back
    ana.peer.readyState = 3
    expect(await status(await r.file('POST', `/file/${id}/accept`, 'max', KEY_A))).toBe('sender-offline')
    const stale = (await r.ledger().file(id)).grant
    expect(stale).toBeUndefined()
    ana = await r.connect('ana', KEY_B)
    expect(await status(await r.file('POST', `/file/${id}/accept`, 'max', KEY_A))).toBe('ok')
    const { grant } = last(ana)
    // accepting twice doesn't hand out a second grant
    expect(await status(await r.file('POST', `/file/${id}/accept`, 'max', KEY_A))).toBe('busy')
    expect(heard(ana).filter((e) => e.event === 'accepted')).toHaveLength(1)
    expect(await status(await r.file('POST', `/file/${id}/start`, 'ana', KEY_B, { 'X-QCalc-Grant': grant }))).toBe('ok')
    expect((await r.ledger().file(id)).grant).toBeUndefined()
    // once the file's in, the grant can't start it again
    await as(r, 'ana', KEY_B, ['PUT', `/file/${id}/part/1`])
    await r.file('POST', `/file/${id}/finish`, 'ana', KEY_B)
    expect(await status(await r.file('POST', `/file/${id}/start`, 'ana', KEY_B, { 'X-QCalc-Grant': grant }))).toBe('done')
    // and a file taken back is gone for good, grant or not
    await r.file('DELETE', `/file/${id}`, 'ana', KEY_B)
    expect(await status(await r.file('POST', `/file/${id}/start`, 'ana', KEY_B, { 'X-QCalc-Grant': grant }))).toBe('gone')
    expect(await status(await r.file('POST', `/file/${id}/accept`, 'max', KEY_A))).toBe('gone')
  })

  it('turns away parts that are too long, too short, unsized or out of place', async () => {
    const { r, max, ana } = await pair()
    const begin = async (bytes = 21) => {
      const id = await offered(r, offer({ bytes }))
      await r.file('POST', `/file/${id}/accept`, 'max', KEY_A)
      await r.file('POST', `/file/${id}/start`, 'ana', KEY_B, { 'X-QCalc-Grant': last(ana).grant })
      return id
    }
    const put = (id, n, h, body = new Uint8Array(21)) => r.file('PUT', `/file/${id}/part/${n}`, 'ana', KEY_B, h, body)
    let id = await begin()
    // no length, or one that isn't a number of bytes up to a part's, is bad before anything's counted
    for (const len of [undefined, '', '-1', '21.0', '0x15', '1e3', String(PART_BYTES + 1), '9'.repeat(13)]) {
      expect(await status(await put(id, 1, len == null ? {} : { 'Content-Length': len }))).toBe('bad')
    }
    expect((await r.ledger().file(id)).tries).toBe(0)
    // a part number past the end, or 0, ends the upload, as a wrong length does
    expect(await status(await put(id, 99999, { 'Content-Length': '21' }))).toBe('over')
    expect(last(max)).toEqual({ type: 'file', event: 'cancelled', id, from: 'ana' })
    id = await begin()
    expect(await status(await put(id, 0, { 'Content-Length': '21' }))).toBe('over')
    id = await begin()
    expect(await status(await put(id, 1, { 'Content-Length': '20' }, new Uint8Array(20)))).toBe('over')

    // a body shorter or longer than it said: the runtime's FixedLengthStream ends it, nothing is kept, and the
    // part can go again
    globalThis.FixedLengthStream = class {
        constructor(n) {
          let seen = 0
          const t = new TransformStream({
            transform(chunk, c) {
              seen += chunk.length
              if (seen > n) throw new Error('too long')
              c.enqueue(chunk)
            },
            flush() {
              if (seen !== n) throw new Error('too short')
            },
          })
          this.readable = t.readable
          this.writable = t.writable
        }
      }
    try {
      id = await begin()
      const stream = (n) => new NodeResponse(new Uint8Array(n)).body
      expect(await status(await put(id, 1, { 'Content-Length': '21' }, stream(5)))).toBe('failed')
      expect(await status(await put(id, 1, { 'Content-Length': '21' }, stream(40)))).toBe('failed')
      expect(Object.keys((await r.ledger().file(id)).parts)).toEqual([])
      expect(await status(await r.file('POST', `/file/${id}/finish`, 'ana', KEY_B))).toBe('incomplete')
      expect(await status(await put(id, 1, { 'Content-Length': '21' }, stream(21)))).toBe('ok')
      expect(await status(await r.file('POST', `/file/${id}/finish`, 'ana', KEY_B))).toBe('ok')
      expect([...r.env.FILES.objects.values()].at(-1).length).toBe(21)
    } finally {
      delete globalThis.FixedLengthStream
    }
  })

  it('keeps the size math in safe integers', async () => {
    const { r } = await pair()
    const send = (bytes) => r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'file', file: offer({ bytes }) })
    expect((await send(2 ** 53 + 2)).status).toBe('bad')
    expect((await send(Number.MAX_VALUE)).status).toBe('bad')
    expect((await send(BYTES_MAX + 1)).status).toBe('too-big')
    expect((await send(BYTES_MAX)).status).toBe('delivered')
    expect(partCount(BYTES_MAX)).toBe(21)
    expect(partLength(BYTES_MAX, 21)).toBe(BYTES_MAX - 20 * PART_BYTES)
    expect(partLength(BYTES_MAX, 2 ** 53)).toBeNull()
    expect(rangeStart('bytes=9999999999999-', 21)).toBeNull()
    expect(rangeStart('bytes=99999999999-', 21)).toBeNull()
  })

  it('lets a take-back win a race with an accept the sender never heard', async () => {
    const r = relay()
    await r.connect('max', KEY_A)
    const ana = await r.connect('ana', KEY_B)
    vi.setSystemTime(NEW_NAME_MS)
    const id = await offered(r)
    // the grant is made, then the file's taken back before the accept's own revert runs
    const granted = await r.ledger().grant(id, 'max')
    expect(await r.file('DELETE', `/file/${id}`, 'ana', KEY_B).then(status)).toBe('ok')
    await r.ledger().revert(id, granted.grant)
    expect(await r.ledger().totals()).toEqual({ total: 0 })
    expect([...r.ledger().ctx.data.keys()].filter((k) => k.startsWith('file:') || k.startsWith('pend:') || k.startsWith('offers:'))).toEqual([])
    // a revert for an old grant never undoes a newer one
    const id2 = await offered(r)
    const first = await r.ledger().grant(id2, 'max')
    await r.ledger().revert(id2, first.grant)
    const second = await r.ledger().grant(id2, 'max')
    await r.ledger().revert(id2, first.grant)
    expect((await r.ledger().file(id2)).grant).toBe(second.grant)
    expect(ana.peer.readyState).toBe(1)
  })

  it('leaves nothing in the bucket when a file’s taken back mid-part or while it’s being put together', async () => {
    const { r, max, ana } = await pair()
    const id = await offered(r)
    await r.file('POST', `/file/${id}/accept`, 'max', KEY_A)
    await r.file('POST', `/file/${id}/start`, 'ana', KEY_B, { 'X-QCalc-Grant': last(ana).grant })
    await as(r, 'ana', KEY_B, ['PUT', `/file/${id}/part/1`])
    r.env.FILES.onComplete = async () => {
      r.env.FILES.onComplete = null
      await r.file('DELETE', `/file/${id}`, 'max', KEY_A)
    }
    expect(await status(await r.file('POST', `/file/${id}/finish`, 'ana', KEY_B))).toBe('gone')
    expect(r.env.FILES.objects.size).toBe(0)
    expect(heard(max).some((e) => e.event === 'ready')).toBe(false)
    expect(await r.ledger().totals()).toEqual({ total: 0 })

    // a part that lands after its file was turned down isn't kept
    const id2 = await offered(r)
    await r.file('POST', `/file/${id2}/accept`, 'max', KEY_A)
    await r.file('POST', `/file/${id2}/start`, 'ana', KEY_B, { 'X-QCalc-Grant': last(ana).grant })
    const upload = r.env.FILES.resumeMultipartUpload.bind(r.env.FILES)
    r.env.FILES.resumeMultipartUpload = (k, u) => {
      const up = upload(k, u)
      return {
        ...up,
        uploadPart: async (n, body) => {
          await r.file('DELETE', `/file/${id2}`, 'max', KEY_A)
          return up.uploadPart(n, body)
        },
      }
    }
    // r2 refuses a part for an aborted upload ('failed'); if it took it anyway, the ledger doesn't ('gone')
    expect(['failed', 'gone']).toContain(await status(await as(r, 'ana', KEY_B, ['PUT', `/file/${id2}/part/1`])))
    expect(await r.ledger().file(id2)).toBeNull()
    expect(r.env.FILES.uploads.size + r.env.FILES.objects.size).toBe(0)
  })

  it('keeps no address, only a tag under a salt that goes with its hour', async () => {
    const { r, ana } = await pair()
    const start = async () => {
      const id = await offered(r)
      await r.file('POST', `/file/${id}/accept`, 'max', KEY_A)
      return status(await r.file('POST', `/file/${id}/start`, 'ana', KEY_B, { 'X-QCalc-Grant': last(ana).grant }))
    }
    const tags = () => [...r.ledger().ctx.data.keys()].filter((k) => k.startsWith('ip:'))
    expect(await start()).toBe('ok')
    const [first] = tags()
    const all = JSON.stringify([...r.ledger().ctx.data])
    expect(all).not.toContain('1.2.3.4')
    // not the plain hash of the address either, which anyone could work out
    const plain = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode('1.2.3.4'))), (b) => b.toString(16).padStart(2, '0')).join('')
    expect(all).not.toContain(plain)
    // the next hour, the old tag and its salt are gone at the next start, and the new tag doesn't match
    vi.setSystemTime(NEW_NAME_MS + 60 * 60 * 1000)
    expect(await start()).toBe('ok')
    expect(tags()).toHaveLength(1)
    expect(tags()[0].split(':')[1]).not.toBe(first.split(':')[1])
    expect([...r.ledger().ctx.data.keys()].filter((k) => k.startsWith('salt:'))).toHaveLength(1)
  })
})

describe('public keys', () => {
  const PUB = 'ISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0-P0A'
  it('are published by the name’s own key and read back with a lookup', async () => {
    const { r } = await pair()
    expect((await r.post('/pubkey', { name: 'ana', key: KEY_A, pub: PUB })).status).toBe('not-yours')
    expect((await r.post('/pubkey', { name: 'ana', key: KEY_B, pub: 'nope' })).status).toBe('bad')
    expect((await r.post('/pubkey', { name: 'ana', key: KEY_B, pub: PUB.slice(0, 42) + 'B' })).status).toBe('bad')
    expect(await r.post('/lookup', { from: 'max', key: KEY_A, name: 'ana' })).toEqual({ status: 'found', pub: '' })
    expect((await r.post('/pubkey', { name: 'ana', key: KEY_B, pub: PUB })).status).toBe('ok')
    expect(await r.post('/lookup', { from: 'max', key: KEY_A, name: 'ana' })).toEqual({ status: 'found', pub: PUB })
    // nobody's name: no key, and nothing more than a lookup always said
    expect(await r.post('/lookup', { from: 'max', key: KEY_A, name: 'zed' })).toEqual({ status: 'unknown' })
    expect((await r.post('/pubkey', { name: 'zed', key: KEY_E, pub: PUB })).status).toBe('not-yours')
  })

  it('stay with their holder, and go when the name does', async () => {
    const { r } = await pair()
    await r.post('/pubkey', { name: 'ana', key: KEY_B, pub: PUB })
    // a day on, ana's app keeps the name alive, and the key with it
    vi.setSystemTime(NEW_NAME_MS + 2 * 24 * 60 * 60 * 1000)
    await r.post('/claim', { name: 'ana', key: KEY_B })
    expect((await r.post('/lookup', { from: 'max', key: KEY_A, name: 'ana' })).pub).toBe(PUB)
    // let go, the name's someone else's, with no key until they publish one
    await r.post('/release', { name: 'ana', key: KEY_B })
    await r.post('/claim', { name: 'ana', key: KEY_E })
    expect(await r.post('/lookup', { from: 'max', key: KEY_A, name: 'ana' })).toEqual({ status: 'found', pub: '' })
  })
})

describe('logging', () => {
  it('logs nothing on the file routes, so no name, address or file fact is written anywhere', async () => {
    const spies = ['log', 'info', 'warn', 'error', 'debug'].map((m) => vi.spyOn(console, m).mockImplementation(() => {}))
    try {
      const { r, ana } = await pair()
      const id = await uploaded(r, ana)
      await readAll((await r.file('GET', `/file/${id}`, 'max', KEY_A)).body)
      await r.file('DELETE', `/file/${'f'.repeat(32)}`, 'max', KEY_A)
      await r.file('PUT', `/file/${id}/part/1`, 'eve', KEY_E, { 'Content-Length': '1' }, new Uint8Array(1))
      await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'clip', clip: sealedClip() })
      await r.settle()
      for (const spy of spies) expect(spy).not.toHaveBeenCalled()
    } finally {
      for (const spy of spies) spy.mockRestore()
    }
  })
})
