import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import worker, { Inbox, Ledger } from './index.js'
import { CAPS, cleanClip, cleanFilename, cleanOffer, cleanType, CLIP_MAX, FALLBACK, NEW_NAME_MS, OFFER_TTL_MS, PART_BYTES, partCount, partLength, rangeStart, STALE_UPLOAD_MS, STORED_TTL_MS } from './files.js'

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
      },
      async abort() {
        uploads.delete(id)
      },
    }
  }
}

const KEY_A = 'a1'.repeat(32)
const KEY_B = 'b2'.repeat(32)
const AES = 'A'.repeat(43)
const SHA = 'c3'.repeat(32)

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
const offer = (o = {}) => ({ name: 'notes.pdf', size: 5, bytes: 21, type: 'application/pdf', sha256: SHA, key: AES, ...o })

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
  const id = await offered(r, offer({ bytes: bytes.length, size: Math.min(5, bytes.length) }))
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
  it('keeps a file name to its last part, without controls, direction marks or leading dots', () => {
    expect(cleanFilename('../../etc/passwd')).toBe('passwd')
    expect(cleanFilename('C:\\Users\\me\\a.txt')).toBe('a.txt')
    expect(cleanFilename('.hidden')).toBe('hidden')
    expect(cleanFilename('evil\u202Etxt.exe')).toBe('evil_txt.exe')
    expect(cleanFilename('a\nb:c')).toBe('a_b_c')
    expect(cleanFilename('...')).toBe('file')
    expect(cleanFilename(42)).toBe('file')
    const long = cleanFilename('x'.repeat(300) + '.tar.gz')
    expect(Array.from(long)).toHaveLength(120)
    expect(long.endsWith('.gz')).toBe(true)
  })

  it('keeps a media type only when it is one', () => {
    expect(cleanType('Image/PNG; charset=x')).toBe('image/png')
    expect(cleanType('text/html<script>')).toBe('application/octet-stream')
    expect(cleanType(undefined)).toBe('application/octet-stream')
  })

  it('takes an offer only with its facts, its hash and its key', () => {
    expect(cleanOffer(offer())).toEqual({ name: 'notes.pdf', size: 5, bytes: 21, type: 'application/pdf', sha256: SHA, key: AES })
    expect(cleanOffer(offer({ size: 2 ** 30 + 1, bytes: 2 ** 30 + 17 }))).toBe('too-big')
    expect(cleanOffer(offer({ sha256: 'nope' }))).toBeNull()
    expect(cleanOffer(offer({ key: 'short' }))).toBeNull()
    expect(cleanOffer(offer({ bytes: 4 }))).toBeNull()
    expect(cleanOffer(offer({ size: 1.5 }))).toBeNull()
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

  it('keeps a clip as it is, up to 64 KB of utf-8', () => {
    expect(cleanClip(' a\n\tb ')).toBe(' a\n\tb ')
    expect(cleanClip('é'.repeat(CLIP_MAX / 2))).toBe('é'.repeat(CLIP_MAX / 2))
    expect(cleanClip('é'.repeat(CLIP_MAX / 2 + 1))).toBe('')
    expect(cleanClip('')).toBe('')
  })
})

describe('clips', () => {
  it('pass through as they are, with a line for apps that only know text', async () => {
    const { r, max } = await pair()
    const clip = 'line one\n\tline two  '
    expect((await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'clip', clip })).status).toBe('delivered')
    expect(last(max)).toEqual({ type: 'message', from: 'ana', text: FALLBACK.clip, kind: 'clip', clip })
    expect(JSON.stringify([...r.boxes.get('max').ctx.data.values()])).not.toContain('line one')
  })

  it('turn away one that’s too big or empty', async () => {
    const { r } = await pair()
    expect((await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'clip', clip: 'x'.repeat(CLIP_MAX + 1) })).status).toBe('bad')
    expect((await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'clip', clip: '' })).status).toBe('bad')
    expect((await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'nope', text: 'hi' })).status).toBe('bad')
  })
})

describe('offering a file', () => {
  it('tells the recipient with the file’s facts and key, and stores nothing', async () => {
    const { r, max } = await pair()
    const res = await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'file', file: offer({ name: '../x/notes.pdf' }) })
    expect(res.status).toBe('delivered')
    expect(res.id).toMatch(/^[0-9a-f]{32}$/)
    expect(last(max)).toEqual({
      type: 'message',
      from: 'ana',
      text: FALLBACK.file,
      kind: 'file',
      file: { id: res.id, name: 'notes.pdf', size: 5, bytes: 21, type: 'application/pdf', sha256: SHA, key: AES, expires: NEW_NAME_MS + OFFER_TTL_MS },
    })
    expect(await r.ledger().totals()).toEqual({ total: 0 })
    // the ledger never holds the key or the name
    const kept = JSON.stringify([...r.ledger().ctx.data.values()])
    expect(kept).not.toContain(AES)
    expect(kept).not.toContain('notes')
    expect(r.env.FILES.objects.size + r.env.FILES.uploads.size).toBe(0)
  })

  it('keeps nothing of an offer nobody heard', async () => {
    const r = relay()
    await r.connect('ana', KEY_B)
    await r.post('/claim', { name: 'max', key: KEY_A })
    vi.setSystemTime(NEW_NAME_MS)
    expect((await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'file', file: offer() })).status).toBe('offline')
    expect([...r.ledger().ctx.data.keys()]).toEqual([])
  })

  it('won’t take files from a name claimed in the last 10 minutes', async () => {
    const r = relay()
    await r.connect('max', KEY_A)
    await r.connect('ana', KEY_B)
    vi.setSystemTime(NEW_NAME_MS - 1)
    expect((await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'file', file: offer() })).status).toBe('new-name')
  })

  it('turns away a file over 1 GB, or an offer that isn’t one', async () => {
    const { r } = await pair()
    expect((await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'file', file: offer({ size: 2 ** 30 + 1, bytes: 2 ** 31 }) })).status).toBe('too-big')
    expect((await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', kind: 'file', file: offer({ key: undefined }) })).status).toBe('bad')
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
    const id = await offered(r, offer({ size: 1, bytes }))
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
