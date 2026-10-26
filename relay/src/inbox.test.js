import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import worker, { Inbox } from './index.js'
import { LAPSE_MS } from './names.js'

// messaging through the worker and its inboxes, against the same kind of stand-in for the workers runtime as
// index.test.js: sockets that keep what's sent to them, a storage map, and stubs that call straight in

vi.mock('cloudflare:workers', () => ({
  DurableObject: class {
    constructor(ctx, env) {
      this.ctx = ctx
      this.env = env
    }
  },
}))

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
  alarm = null
  storage = {
    get: async (k) => structuredClone(this.data.get(k)),
    put: async (k, v) => void this.data.set(k, structuredClone(v)),
    delete: async (k) => this.data.delete(k),
    deleteAll: async () => this.data.clear(),
    setAlarm: async (t) => void (this.alarm = t),
    deleteAlarm: async () => void (this.alarm = null),
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

// a limiter that lets `limit` through per key
function limiter(limit) {
  const counts = new Map()
  return {
    limit: async ({ key }) => {
      counts.set(key, (counts.get(key) ?? 0) + 1)
      return { success: counts.get(key) <= limit }
    },
  }
}

const KEY_A = 'a1'.repeat(32)
const KEY_B = 'b2'.repeat(32)

function relay(limits = {}) {
  const boxes = new Map()
  const env = {
    INBOX: {
      idFromName: (n) => n,
      get(n) {
        if (!boxes.has(n)) boxes.set(n, new Inbox(new Ctx(), env))
        const b = boxes.get(n)
        return new Proxy(b, {
          get: (t, k) => (k === 'fetch' ? (url) => t.fetch({ url }) : t[k].bind(t)),
        })
      },
    },
    ...limits,
  }
  const headers = (h = {}) => new Headers({ 'cf-connecting-ip': '1.2.3.4', ...h })
  const post = async (path, o) => {
    const text = typeof o === 'string' ? o : JSON.stringify(o)
    const res = await worker.fetch({ url: 'https://relay.test' + path, method: 'POST', headers: headers(), text: async () => text }, env)
    return (await res.json()).status
  }
  const connect = async (name, key) => {
    const req = {
      url: `https://relay.test/inbox/${name}`,
      method: 'GET',
      headers: headers({ Upgrade: 'websocket', 'Sec-WebSocket-Protocol': `qcalc-inbox, k${key}` }),
    }
    const res = await worker.fetch(req, env)
    return { res, ws: res.webSocket }
  }
  return { env, boxes, post, connect, box: (n) => boxes.get(n) }
}

const heard = (ws) => ws.peer.sent.filter((s) => s[0] === '!').map((s) => JSON.parse(s.slice(1)))
const last = (ws) => heard(ws).at(-1)

beforeEach(() => {
  vi.useFakeTimers()
  vi.setSystemTime(0)
})
afterEach(() => {
  vi.useRealTimers()
})

describe('claiming a name', () => {
  it('holds a name for the first key and turns away any other', async () => {
    const r = relay()
    expect(await r.post('/claim', { name: 'max', key: KEY_A })).toBe('ok')
    expect(await r.post('/claim', { name: 'Max', key: KEY_A })).toBe('ok')
    expect(await r.post('/claim', { name: 'max', key: KEY_B })).toBe('taken')
    // only a hash of the key is kept, and nothing about who asked
    const stored = JSON.stringify([...r.box('max').ctx.data.values()])
    expect(stored).not.toContain(KEY_A)
    expect(stored).not.toContain('1.2.3.4')
  })

  it('lets a name go when its holder releases it, and not when someone else asks', async () => {
    const r = relay()
    await r.post('/claim', { name: 'max', key: KEY_A })
    expect(await r.post('/release', { name: 'max', key: KEY_B })).toBe('ok')
    expect(await r.post('/claim', { name: 'max', key: KEY_B })).toBe('taken')
    await r.post('/release', { name: 'max', key: KEY_A })
    expect(r.box('max').ctx.data.size).toBe(0)
    expect(await r.post('/claim', { name: 'max', key: KEY_B })).toBe('ok')
  })

  it('lets a name nobody has used lapse, but not one whose app is still connected', async () => {
    const r = relay()
    await r.post('/claim', { name: 'max', key: KEY_A })
    await r.post('/claim', { name: 'ana', key: KEY_A })
    await r.connect('ana', KEY_A)
    expect(r.box('max').ctx.alarm).toBe(LAPSE_MS)
    vi.setSystemTime(LAPSE_MS + 1)
    await r.box('max').alarm()
    await r.box('ana').alarm()
    expect(r.box('max').ctx.data.size).toBe(0)
    expect(r.box('ana').ctx.data.get('claim')).toMatchObject({ seen: LAPSE_MS + 1 })
    expect(await r.post('/claim', { name: 'max', key: KEY_B })).toBe('ok')
    expect(await r.post('/claim', { name: 'ana', key: KEY_B })).toBe('taken')
  })

  it('turns away names and keys that can’t be, and bodies that aren’t json', async () => {
    const r = relay()
    expect(await r.post('/claim', { name: 'm', key: KEY_A })).toBe('bad')
    expect(await r.post('/claim', { name: 'max', key: 'short' })).toBe('bad')
    expect(await r.post('/claim', 'not json')).toBe('bad')
    expect(await r.post('/claim', 'x'.repeat(5000))).toBe('bad')
  })

  it('counts claims per address', async () => {
    const r = relay({ NAMES: limiter(2) })
    expect(await r.post('/claim', { name: 'aa', key: KEY_A })).toBe('ok')
    expect(await r.post('/claim', { name: 'bb', key: KEY_A })).toBe('ok')
    expect(await r.post('/claim', { name: 'cc', key: KEY_A })).toBe('too-many')
  })
})

describe('the inbox socket', () => {
  it('claims the name, answers with the protocol, and says it’s ready', async () => {
    const r = relay()
    const { res, ws } = await r.connect('max', KEY_A)
    expect(res.headers.get('Sec-WebSocket-Protocol')).toBe('qcalc-inbox')
    expect(last(ws)).toEqual({ type: 'inbox', name: 'max' })
    expect(await r.post('/claim', { name: 'max', key: KEY_B })).toBe('taken')
  })

  it('says a name held by another key is taken', async () => {
    const r = relay()
    await r.post('/claim', { name: 'max', key: KEY_A })
    const { res, ws } = await r.connect('max', KEY_B)
    expect(res.headers.get('Sec-WebSocket-Protocol')).toBe('qcalc-inbox')
    expect(last(ws)).toEqual({ type: 'closed', reason: 'taken' })
  })

  it('needs a key', async () => {
    const r = relay()
    const req = {
      url: 'https://relay.test/inbox/max',
      method: 'GET',
      headers: new Headers({ Upgrade: 'websocket', 'Sec-WebSocket-Protocol': 'qcalc-inbox' }),
    }
    const res = await worker.fetch(req, r.env)
    expect(last(res.webSocket)).toEqual({ type: 'closed', reason: 'gone' })
    expect(r.boxes.size).toBe(0)
  })

  it('keeps a few sockets per name, closing the oldest', async () => {
    const r = relay()
    const socks = []
    for (let i = 0; i < 4; i++) socks.push((await r.connect('max', KEY_A)).ws)
    expect(r.box('max').open()).toHaveLength(3)
    expect(socks[0].peer.readyState).toBe(3)
  })

  it('closes when its holder lets the name go', async () => {
    const r = relay()
    const { ws } = await r.connect('max', KEY_A)
    await r.post('/release', { name: 'max', key: KEY_A })
    expect(last(ws)).toEqual({ type: 'closed', reason: 'released' })
  })
})

describe('sending', () => {
  it('hands a message to an open inbox and keeps nothing of it', async () => {
    const r = relay()
    await r.post('/claim', { name: 'ana', key: KEY_B })
    const { ws } = await r.connect('max', KEY_A)
    expect(await r.post('/send', { from: 'ana', key: KEY_B, to: 'Max', text: ' hello\n there ' })).toBe('delivered')
    expect(last(ws)).toEqual({ type: 'message', from: 'ana', text: 'hello there' })
    expect(JSON.stringify([...r.box('max').ctx.data.values()])).not.toContain('hello')
  })

  it('turns a message away when the name’s inbox is closed, or nobody holds the name', async () => {
    const r = relay()
    await r.post('/claim', { name: 'ana', key: KEY_B })
    await r.post('/claim', { name: 'max', key: KEY_A })
    expect(await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', text: 'hi' })).toBe('offline')
    expect(await r.post('/send', { from: 'ana', key: KEY_B, to: 'nobody', text: 'hi' })).toBe('unknown')
    expect(r.box('max').ctx.data.get('queue')).toBeUndefined()
  })

  it('needs the sender’s own key for their name', async () => {
    const r = relay()
    await r.post('/claim', { name: 'ana', key: KEY_B })
    const { ws } = await r.connect('max', KEY_A)
    expect(await r.post('/send', { from: 'ana', key: KEY_A, to: 'max', text: 'it’s ana, honest' })).toBe('not-yours')
    expect(heard(ws).filter((e) => e.type === 'message')).toEqual([])
  })

  it('claims a free sender name on the way, as a claim would', async () => {
    const r = relay()
    await r.connect('max', KEY_A)
    expect(await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', text: 'hi' })).toBe('delivered')
    expect(await r.post('/claim', { name: 'ana', key: KEY_A })).toBe('taken')
  })

  it('counts sends per address and per sender', async () => {
    const r = relay({ MESSAGES: limiter(100), SENDERS: limiter(2) })
    await r.connect('max', KEY_A)
    const send = () => r.post('/send', { from: 'ana', key: KEY_B, to: 'max', text: 'hi' })
    expect(await send()).toBe('delivered')
    expect(await send()).toBe('delivered')
    expect(await send()).toBe('too-many')
  })

  it('needs something to say', async () => {
    const r = relay()
    expect(await r.post('/send', { from: 'ana', key: KEY_B, to: 'max', text: ' \n ' })).toBe('bad')
  })

  it('answers a preflight, and every reply can be read from a page', async () => {
    const r = relay()
    const pre = await worker.fetch({ url: 'https://relay.test/send', method: 'OPTIONS', headers: new Headers() }, r.env)
    expect(pre.status).toBe(204)
    expect(pre.headers.get('Access-Control-Allow-Origin')).toBe('*')
    const res = await worker.fetch(
      { url: 'https://relay.test/claim', method: 'POST', headers: new Headers(), text: async () => JSON.stringify({ name: 'max', key: KEY_A }) },
      r.env,
    )
    expect(res.headers.get('Access-Control-Allow-Origin')).toBe('*')
  })
})

describe('looking a name up', () => {
  it('says whether anyone holds a name, as a send would, without claiming it or keeping it alive', async () => {
    const r = relay()
    await r.post('/claim', { name: 'max', key: KEY_A })
    expect(await r.post('/lookup', { from: 'ana', key: KEY_B, name: 'Max' })).toBe('found')
    expect(await r.post('/lookup', { from: 'ana', key: KEY_B, name: 'nobody' })).toBe('unknown')
    expect(r.box('nobody').ctx.data.size).toBe(0)
    const seen = r.box('max').ctx.data.get('claim').seen
    vi.setSystemTime(LAPSE_MS / 2)
    await r.post('/lookup', { from: 'ana', key: KEY_B, name: 'max' })
    expect(r.box('max').ctx.data.get('claim').seen).toBe(seen)
  })

  it('says the same whether or not the name has an open inbox, so nothing about who is reading shows', async () => {
    const r = relay()
    await r.post('/claim', { name: 'max', key: KEY_A })
    const before = await r.post('/lookup', { from: 'ana', key: KEY_B, name: 'max' })
    await r.connect('max', KEY_A)
    expect(await r.post('/lookup', { from: 'ana', key: KEY_B, name: 'max' })).toBe(before)
  })

  it('says a lapsed name is unknown', async () => {
    const r = relay()
    await r.post('/claim', { name: 'max', key: KEY_A })
    vi.setSystemTime(LAPSE_MS + 1)
    expect(await r.post('/lookup', { from: 'ana', key: KEY_B, name: 'max' })).toBe('unknown')
  })

  it('needs the asker’s own name and key', async () => {
    const r = relay()
    await r.post('/claim', { name: 'ana', key: KEY_B })
    expect(await r.post('/lookup', { from: 'ana', key: KEY_A, name: 'ana' })).toBe('not-yours')
    expect(await r.post('/lookup', { key: KEY_A, name: 'ana' })).toBe('bad')
    expect(await r.post('/lookup', { from: 'ana', key: KEY_B, name: 'a' })).toBe('bad')
  })

  it('counts lookups per address and per asker', async () => {
    const r = relay({ LOOKUPS: limiter(2) })
    const look = () => r.post('/lookup', { from: 'ana', key: KEY_B, name: 'max' })
    expect(await look()).toBe('unknown')
    expect(await look()).toBe('unknown')
    expect(await look()).toBe('too-many')
  })
})
