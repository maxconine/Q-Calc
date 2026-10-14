import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import worker, { Room } from './index.js'

// the worker and its rooms, run against a small stand-in for the workers runtime: sockets that keep what's sent
// to them, a storage map, and stubs that call straight into the objects

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
  att = null
  peer = null
  accept() {}
  send(s) {
    if (this.readyState !== 1) throw new Error('closed')
    this.sent.push(s)
  }
  close() {
    this.readyState = 3
  }
  serializeAttachment(v) {
    this.att = structuredClone(v)
  }
  deserializeAttachment() {
    return structuredClone(this.att)
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
      this.status = init.status ?? 200
      this.webSocket = init.webSocket ?? null
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
    delete: async (k) => (Array.isArray(k) ? k.forEach((x) => this.data.delete(x)) : this.data.delete(k)),
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
  getTags(ws) {
    return ws.tags
  }
}

function relay() {
  const rooms = new Map()
  const listed = new Map()
  const env = {
    ROOMS: {
      idFromName: (n) => n,
      get(n) {
        if (!rooms.has(n)) rooms.set(n, new Room(new Ctx(), env))
        const r = rooms.get(n)
        return new Proxy(r, {
          get: (t, k) =>
            k === 'fetch' ? (url, init) => t.fetch({ url, headers: new Headers(init?.headers) }) : t[k].bind(t),
        })
      },
    },
    DIRECTORY: {
      idFromName: (g) => g,
      get: (g) => ({
        add: async ({ id }) => void listed.set(id, g),
        remove: async (id) => void listed.delete(id),
      }),
    },
  }
  const open = async (path) => {
    const req = { url: 'https://relay.test' + path, headers: new Headers({ Upgrade: 'websocket' }) }
    const res = await worker.fetch(req, env)
    return res.webSocket
  }
  // a code's room holds a claim on it
  const claimed = (code) => rooms.get(code)?.ctx.data.get('claim') ?? null
  const hostSock = (id) => rooms.get(`room:${id}`).ctx.getWebSockets('host')[0]
  return { env, rooms, listed, open, claimed, hostSock, room: (name) => rooms.get(name) }
}

// what a client socket heard from the relay
const heard = (ws) => ws.peer.sent.filter((s) => s[0] === '!').map((s) => JSON.parse(s.slice(1)))
const last = (ws) => heard(ws).at(-1)

// the codes the relay draws next, in order
const realRandom = crypto.getRandomValues.bind(crypto)
function codes(...list) {
  vi.spyOn(crypto, 'getRandomValues').mockImplementation((arr) => {
    if (arr instanceof Uint32Array && list.length) {
      arr[0] = Number(list.shift())
      return arr
    }
    return realRandom(arr)
  })
}

beforeEach(() => {
  vi.useFakeTimers()
  vi.setSystemTime(0)
})
afterEach(() => {
  vi.useRealTimers()
  vi.restoreAllMocks()
})

async function hostLocked(r, code, name = 'amber fox') {
  codes(code)
  const ws = await r.open(`/host/pong?name=${encodeURIComponent(name)}&locked=1`)
  const id = [...r.listed.keys()].at(-1)
  return { ws, id }
}

describe('joining by a code alone', () => {
  it('reaches a locked listed room, which then leaves the list and the code index', async () => {
    const r = relay()
    const { ws: host, id } = await hostLocked(r, '111111')
    expect(last(host)).toEqual({ type: 'hosting', code: '111111' })
    expect(r.claimed('111111')).toMatchObject({ id, code: '111111' })

    const guest = await r.open('/join/111111?name=jade+owl')
    expect(last(guest)).toEqual({ type: 'open', role: 'guest', peer: 'amber fox' })
    expect(last(host)).toEqual({ type: 'open', role: 'host', peer: 'jade owl' })
    expect(r.listed.size).toBe(0)
    expect(r.claimed('111111')).toBeNull()

    // the join let the code go, so it reaches nothing now; a released app sends no name
    const late = await r.open('/join/111111')
    expect(last(late)).toEqual({ type: 'closed', reason: 'wrong-code' })
  })

  it('turns away a code nobody holds, and an open room holds none', async () => {
    const r = relay()
    const ws = await r.open('/join/222222?name=jade+owl')
    expect(last(ws)).toEqual({ type: 'closed', reason: 'wrong-code' })

    codes('333333')
    const host = await r.open('/host/pong?name=teal+otter&locked=0')
    expect(last(host)).toEqual({ type: 'hosting', code: '' })
    expect(r.claimed('333333')).toBeNull()
  })

  it('counts wrong codes through the list, and the fifth draws a new code that works alone', async () => {
    const r = relay()
    const { ws: host, id } = await hostLocked(r, '111111')
    codes('444444')
    for (let i = 0; i < 5; i++) {
      const ws = await r.open(`/join/${id}?name=x&code=000000`)
      expect(last(ws)).toEqual({ type: 'closed', reason: 'wrong-code' })
    }
    expect(last(host)).toEqual({ type: 'hosting', code: '444444', renewed: true })
    expect(r.claimed('111111')).toBeNull()
    expect(r.claimed('444444')).toMatchObject({ id })

    expect(last(await r.open('/join/111111?name=x'))).toEqual({ type: 'closed', reason: 'wrong-code' })
    const guest = await r.open('/join/444444?name=jade+owl')
    expect(last(guest)).toEqual({ type: 'open', role: 'guest', peer: 'amber fox' })
  })

  it('lets the code go when the host leaves', async () => {
    const r = relay()
    const { id } = await hostLocked(r, '111111')
    const sock = r.hostSock(id)
    sock.close()
    await r.room(`room:${id}`).webSocketClose(sock)
    expect(r.claimed('111111')).toBeNull()
    expect(r.listed.size).toBe(0)
    expect(last(await r.open('/join/111111'))).toEqual({ type: 'closed', reason: 'wrong-code' })
  })

  it('lets the code go when the room expires, and a claim left behind lapses by itself', async () => {
    const r = relay()
    const { ws: host, id } = await hostLocked(r, '111111')
    vi.setSystemTime(15 * 60 * 1000 + 1)
    await r.room(`room:${id}`).alarm()
    expect(last(host)).toEqual({ type: 'closed', reason: 'expired' })
    expect(r.claimed('111111')).toBeNull()

    // a room that couldn't let its code go: the claim stops counting at its time, and its alarm clears it
    await r.env.ROOMS.get('555555').claim('gonegonegone', '555555', 20 * 60 * 1000)
    vi.setSystemTime(20 * 60 * 1000)
    expect(last(await r.open('/join/555555'))).toEqual({ type: 'closed', reason: 'wrong-code' })
    await r.env.ROOMS.get('555555').claim('gonegonegone', '555555', 30 * 60 * 1000)
    vi.setSystemTime(30 * 60 * 1000)
    await r.room('555555').alarm()
    expect(r.claimed('555555')).toBeNull()
  })

  it('never hands out a code a code-only room or another listed room has', async () => {
    const r = relay()
    codes('666666')
    const old = await r.open('/host')
    expect(last(old)).toEqual({ type: 'hosting', code: '666666' })

    codes('666666', '777777')
    const a = await r.open('/host/pong?name=a&locked=1')
    expect(last(a)).toEqual({ type: 'hosting', code: '777777' })

    codes('777777', '888888')
    const b = await r.open('/host/connect4?name=b&locked=1')
    expect(last(b)).toEqual({ type: 'hosting', code: '888888' })

    codes('888888', '999999')
    const later = await r.open('/host')
    expect(last(later)).toEqual({ type: 'hosting', code: '999999' })

    // each code still reaches its own room
    expect(last(await r.open('/join/777777?name=x'))).toMatchObject({ type: 'open', peer: 'a' })
    expect(last(await r.open('/join/888888?name=x'))).toMatchObject({ type: 'open', peer: 'b' })
  })

  it('still pairs the code-only rooms of earlier releases as before', async () => {
    const r = relay()
    codes('123456')
    const host = await r.open('/host')
    expect(last(host)).toEqual({ type: 'hosting', code: '123456' })
    const guest = await r.open('/join/123456')
    expect(last(guest)).toEqual({ type: 'open', role: 'guest', peer: '' })
    expect(last(host)).toEqual({ type: 'open', role: 'host', peer: '' })
    expect(last(await r.open('/join/123456?name=x'))).toEqual({ type: 'closed', reason: 'busy' })
    expect(r.claimed('123456')).toBeNull()
  })
})

describe('codes and seats together', () => {
  // a socket the relay closed or dropped, as the runtime would report it
  async function dropped(r, name, sock, code = 4000) {
    sock.close()
    await r.room(name).webSocketClose(sock, code)
  }
  const guestSock = (r, id) => r.room(`room:${id}`).ctx.getWebSockets('guest')[0]

  it('keeps a waiting locked room’s claim through hibernation, and its code joins with a seat', async () => {
    const r = relay()
    codes('111111')
    const host = await r.open('/host/pong?name=amber+fox&locked=1&resume=1')
    const id = [...r.listed.keys()].at(-1)
    // hibernation: the room object is rebuilt from its storage and sockets
    const ctx = r.room(`room:${id}`).ctx
    r.rooms.set(`room:${id}`, new Room(ctx, r.env))
    expect(r.claimed('111111')).toMatchObject({ id })

    const guest = await r.open('/join/111111?name=jade+owl&resume=1')
    const open = last(guest)
    expect(open).toMatchObject({ type: 'open', role: 'guest', peer: 'amber fox', room: id })
    expect(open.token).toMatch(/^[0-9a-f]{32}$/)
    expect(last(host)).toMatchObject({ type: 'open', role: 'host', peer: 'jade owl', room: id })
    expect(r.claimed('111111')).toBeNull()
  })

  it('a dropped link mid-game comes back through its seat, never as a new guest or a wrong code', async () => {
    const r = relay()
    codes('111111')
    const host = await r.open('/host/pong?name=amber+fox&locked=1&resume=1')
    const id = [...r.listed.keys()].at(-1)
    const guest = await r.open('/join/111111?name=jade+owl&resume=1')
    const hostToken = last(host).token
    const guestToken = last(guest).token

    // the guest's link drops: the host waits, the seat is held, and nobody else takes it, right code or wrong.
    // none of those counts against the code, so it isn't redrawn
    await dropped(r, `room:${id}`, guestSock(r, id))
    expect(last(host)).toEqual({ type: 'away' })
    for (let i = 0; i < 6; i++) {
      expect(last(await r.open(`/join/${id}?name=x&code=000000`))).toEqual({ type: 'closed', reason: 'busy' })
    }
    expect(last(await r.open(`/join/${id}?name=x&code=111111&resume=1`))).toEqual({ type: 'closed', reason: 'busy' })
    expect(r.hostSock(id).deserializeAttachment().wrong).toBe(0)
    expect(heard(host).filter((e) => e.type === 'hosting')).toEqual([{ type: 'hosting', code: '111111' }])
    // the join let the code go, so typed alone it reaches nothing
    expect(last(await r.open('/join/111111?name=x&resume=1'))).toEqual({ type: 'closed', reason: 'wrong-code' })
    const guestBack = await r.open(`/back/${id}?token=${guestToken}`)
    expect(heard(guestBack)[0]).toMatchObject({ type: 'open', role: 'guest', peer: 'amber fox', room: id, resumed: true })
    expect(last(host)).toEqual({ type: 'back' })

    // now the host's link drops and comes back
    await dropped(r, `room:${id}`, r.hostSock(id))
    expect(last(guestBack)).toEqual({ type: 'away' })
    expect(last(await r.open(`/join/${id}?name=x&code=111111`))).toEqual({ type: 'closed', reason: 'gone' })
    // a token sent to the code's room is no join, and takes nothing there
    expect(last(await r.open(`/back/111111?token=${hostToken}`))).toEqual({ type: 'closed', reason: 'gone' })
    expect(r.claimed('111111')).toBeNull()
    const back = await r.open(`/back/${id}?token=${hostToken}`)
    expect(heard(back)[0]).toMatchObject({ type: 'open', role: 'host', peer: 'jade owl', room: id, resumed: true })
    expect(last(guestBack)).toEqual({ type: 'back' })

    // the game ends on purpose: the other side hears bye, and the seats go
    await dropped(r, `room:${id}`, guestSock(r, id), 1000)
    expect(last(back)).toEqual({ type: 'closed', reason: 'bye' })
    expect(last(await r.open(`/back/${id}?token=${hostToken}`))).toEqual({ type: 'closed', reason: 'bye' })
    // the code is free for a new room straight away
    codes('111111')
    const next = await r.open('/host/pong?name=b&locked=1&resume=1')
    expect(last(next)).toEqual({ type: 'hosting', code: '111111' })
  })

  it('lets the code go when a waiting host’s link drops, since a seat comes only after pairing', async () => {
    const r = relay()
    codes('111111')
    const host = await r.open('/host/pong?name=amber+fox&locked=1&resume=1')
    const id = [...r.listed.keys()].at(-1)
    await dropped(r, `room:${id}`, r.hostSock(id))
    expect(r.claimed('111111')).toBeNull()
    expect(r.listed.size).toBe(0)
    expect(heard(host).at(-1)).toEqual({ type: 'hosting', code: '111111' })
    expect(last(await r.open('/join/111111?name=x&resume=1'))).toEqual({ type: 'closed', reason: 'wrong-code' })
  })

  it('lets the code go when a held seat runs out', async () => {
    const r = relay()
    codes('111111')
    const host = await r.open('/host/pong?name=amber+fox&locked=1&resume=1')
    const id = [...r.listed.keys()].at(-1)
    await r.open('/join/111111?name=jade+owl&resume=1')
    await dropped(r, `room:${id}`, guestSock(r, id))
    vi.setSystemTime(10 * 60 * 1000)
    await r.room(`room:${id}`).alarm()
    expect(last(host)).toEqual({ type: 'closed', reason: 'lost' })
    expect(r.claimed('111111')).toBeNull()
    expect(r.room(`room:${id}`).ctx.data.get('seats')).toBeUndefined()
  })

  it('never claims the code of a code-only game whose player is away, which still comes back by its code', async () => {
    const r = relay()
    codes('123456')
    const host = await r.open('/host?resume=1')
    const guest = await r.open('/join/123456?resume=1')
    expect(last(guest)).toMatchObject({ type: 'open', role: 'guest', room: '123456' })
    const token = last(host).token
    await dropped(r, '123456', r.room('123456').ctx.getWebSockets('host')[0])

    codes('123456', '654321')
    const listed = await r.open('/host/pong?name=a&locked=1&resume=1')
    expect(last(listed)).toEqual({ type: 'hosting', code: '654321' })
    expect(r.claimed('123456')).toBeNull()
    // a stranger typing the away game's code takes nothing
    expect(last(await r.open('/join/123456?name=x&resume=1'))).toEqual({ type: 'closed', reason: 'wrong-code' })

    const back = await r.open(`/back/123456?token=${token}`)
    expect(heard(back)[0]).toMatchObject({ type: 'open', role: 'host', resumed: true })
    expect(last(guest)).toEqual({ type: 'back' })
    // and with the host back, a stranger finds it busy rather than a seat to take
    expect(last(await r.open('/join/123456?name=x&resume=1'))).toEqual({ type: 'closed', reason: 'busy' })
  })
})
