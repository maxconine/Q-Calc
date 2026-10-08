import { DurableObject } from 'cloudflare:workers'

// the online link for q calc's two-player games. a host gets a 6-digit code; a guest who types it lands in the
// same room, and from then on the room passes each side's messages to the other. nothing is stored and nothing
// is read: frames are forwarded as they are.
//
// wire, both ways, one string per frame:
//   "." + data   a game message, forwarded to the other side untouched
//   "!" + json   a link event from the relay, shaped like the page's PeerEvent ({type: 'hosting' | 'open' | 'closed', ...})
//   "ping"       answered with "pong" without waking the room, so a dead connection is noticed in the lobby

const CODE_DIGITS = 6
// a fresh code that happens to be taken is drawn again, this many times
const HOST_TRIES = 8
// a room nobody joined is closed after this long
const WAIT_MS = 15 * 60 * 1000
// pong's frames are well under 200 bytes; anything this big isn't a game message
const MAX_FRAME = 8192

function newCode(rand = crypto.getRandomValues(new Uint32Array(1))[0]) {
  return String(rand % 10 ** CODE_DIGITS).padStart(CODE_DIGITS, '0')
}

const CODE_PATH = new RegExp(`^/join/([0-9]{${CODE_DIGITS}})$`)

const event = (e) => '!' + JSON.stringify(e)

// a websocket that says why and closes at once; a browser can't read the status of a refused upgrade
function refuse(reason) {
  const [client, server] = Object.values(new WebSocketPair())
  server.accept()
  server.send(event({ type: 'closed', reason }))
  server.close(1000, reason)
  return new Response(null, { status: 101, webSocket: client })
}

async function allowed(limiter, req) {
  if (!limiter) return true
  const { success } = await limiter.limit({ key: req.headers.get('cf-connecting-ip') ?? 'unknown' })
  return success
}

function room(env, code) {
  return env.ROOMS.get(env.ROOMS.idFromName(code))
}

export default {
  async fetch(req, env) {
    const url = new URL(req.url)
    if (url.pathname === '/') return new Response('q calc relay\n')
    if (req.headers.get('Upgrade') !== 'websocket') return new Response('expected a websocket\n', { status: 426 })

    if (url.pathname === '/host') {
      if (!(await allowed(env.HOSTS, req))) return refuse('too-many-tries')
      for (let i = 0; i < HOST_TRIES; i++) {
        const code = newCode()
        const res = await room(env, code).fetch(`https://room/host?code=${code}`, { headers: { Upgrade: 'websocket' } })
        if (res.status !== 409) return res
      }
      return refuse('network')
    }

    const join = CODE_PATH.exec(url.pathname)
    if (join) {
      if (!(await allowed(env.JOINS, req))) return refuse('too-many-tries')
      return room(env, join[1]).fetch('https://room/guest', { headers: { Upgrade: 'websocket' } })
    }
    return new Response('not found\n', { status: 404 })
  },
}

// one code's room: at most a host and a guest. hibernates between messages, so a waiting host costs nothing
export class Room extends DurableObject {
  constructor(ctx, env) {
    super(ctx, env)
    ctx.setWebSocketAutoResponse(new WebSocketRequestResponsePair('ping', 'pong'))
  }

  // the open socket with this tag, skipping one that is on its way out
  live(tag) {
    return this.ctx.getWebSockets(tag).find((ws) => ws.readyState === WebSocket.OPEN) ?? null
  }

  other(ws) {
    const [tag] = this.ctx.getTags(ws)
    return this.live(tag === 'host' ? 'guest' : 'host')
  }

  accept(tag) {
    const [client, server] = Object.values(new WebSocketPair())
    this.ctx.acceptWebSocket(server, [tag])
    return { client, server }
  }

  async fetch(req) {
    const url = new URL(req.url)
    const host = this.live('host')
    if (url.pathname === '/host') {
      if (host) return new Response(null, { status: 409 })
      const { client, server } = this.accept('host')
      server.send(event({ type: 'hosting', code: url.searchParams.get('code') }))
      await this.ctx.storage.setAlarm(Date.now() + WAIT_MS)
      return new Response(null, { status: 101, webSocket: client })
    }
    if (!host) return refuse('wrong-code')
    if (this.live('guest')) return refuse('busy')
    const { client, server } = this.accept('guest')
    await this.ctx.storage.deleteAlarm()
    host.send(event({ type: 'open', role: 'host', peer: '' }))
    server.send(event({ type: 'open', role: 'guest', peer: '' }))
    return new Response(null, { status: 101, webSocket: client })
  }

  webSocketMessage(ws, msg) {
    if (typeof msg !== 'string' || msg.length > MAX_FRAME || msg[0] !== '.') return
    this.other(ws)?.send(msg)
  }

  leave(ws, reason) {
    const other = this.other(ws)
    if (other) {
      other.send(event({ type: 'closed', reason }))
      other.close(1000, reason)
    }
    try {
      ws.close(1000, 'done')
    } catch {
      // already closed
    }
  }

  webSocketClose(ws) {
    this.leave(ws, 'bye')
  }

  webSocketError(ws) {
    this.leave(ws, 'lost')
  }

  // a host nobody joined
  alarm() {
    if (this.live('guest')) return
    for (const ws of this.ctx.getWebSockets('host')) {
      try {
        ws.send(event({ type: 'closed', reason: 'expired' }))
        ws.close(1000, 'expired')
      } catch {
        // already closed
      }
    }
  }
}
