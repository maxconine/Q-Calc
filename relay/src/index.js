import { DurableObject } from 'cloudflare:workers'
import { cleanName, GAMES, ipKey, newRoomId, ROOM_ID, RoomList } from './directory.js'
import { enqueue, GRACE_MS, LEFT, newSeats, otherSide, overdue, seatOf, SIDES } from './seats.js'

// the online link for q calc's two-player games. each game has a list of open rooms that the lobby watches;
// a host opens a room, open or locked with a 6-digit code, and a guest picks it from the list (typing the code
// for a locked one). from then on the room passes each side's messages to the other. nothing is read: frames
// are forwarded as they are, and a list shows only a room's id, the host's nickname and whether it's locked.
//
// wire, both ways, one string per frame:
//   "." + data   a game message, forwarded to the other side untouched
//   "!" + json   a link event from the relay, shaped like the page's PeerEvent
//                ({type: 'rooms' | 'hosting' | 'open' | 'away' | 'back' | 'closed', ...})
//   "ping"       answered with "pong" without waking anything, so a dead connection is noticed
//
// routes, all websockets:
//   /rooms/<game>                          the game's open rooms, sent now and again on every change
//   /host/<game>?name=…&locked=1|0         opens a room; hears hosting {code}, code '' when open
//   /join/<room>?name=…[&code=123456]      joins a room from the list
//   /host, /join/<code>                    the code-only rooms of earlier releases, kept so they still pair
//   /back/<room or code>?token=…          takes back a seat after a dropped link (seats.js)
//
// a page that can take a seat back asks with resume=1; only when both sides did is a seat held. then `open`
// carries the room and the side's token. a close other than 1000 (a lost link, a quit app)
// holds the seat for a few minutes: the other side hears `away`, then `back`, or `closed` once time's up

const CODE_DIGITS = 6
// a fresh code or room id that happens to be taken is drawn again, this many times
const HOST_TRIES = 8
// a room nobody joined is closed after this long
const WAIT_MS = 15 * 60 * 1000
// pong's frames are well under 200 bytes; anything this big isn't a game message
const MAX_FRAME = 8192
// wrong codes a locked room takes before it draws a new one, so a code can't be worn down by many addresses
const TRIES_PER_CODE = 5

function newCode(rand = crypto.getRandomValues(new Uint32Array(1))[0]) {
  return String(rand % 10 ** CODE_DIGITS).padStart(CODE_DIGITS, '0')
}

const LEGACY_JOIN = new RegExp(`^/join/([0-9]{${CODE_DIGITS}})$`)
const CODE = new RegExp(`^[0-9]{${CODE_DIGITS}}$`)
const ROUTE = /^\/(rooms|host|join|back)\/([a-z0-9]+)$/

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
  const { success } = await limiter.limit({ key: ipKey(req.headers.get('cf-connecting-ip')) })
  return success
}

function room(env, name) {
  return env.ROOMS.get(env.ROOMS.idFromName(name))
}

function directory(env, game) {
  return env.DIRECTORY.get(env.DIRECTORY.idFromName(game))
}

const upgrade = { headers: { Upgrade: 'websocket' } }

export default {
  async fetch(req, env) {
    const url = new URL(req.url)
    if (url.pathname === '/') return new Response('q calc relay\n')
    if (req.headers.get('Upgrade') !== 'websocket') return new Response('expected a websocket\n', { status: 426 })

    // before the room routes: a 6-digit code is not a room id
    const legacy = LEGACY_JOIN.exec(url.pathname)
    if (legacy) {
      if (!(await allowed(env.JOINS, req))) return refuse('too-many-tries')
      const resume = url.searchParams.get('resume') === '1' ? '1' : '0'
      return room(env, legacy[1]).fetch(`https://room/guest?key=${legacy[1]}&resume=${resume}`, upgrade)
    }

    const route = ROUTE.exec(url.pathname)
    if (route && route[1] === 'rooms') {
      if (!GAMES.has(route[2])) return new Response('not found\n', { status: 404 })
      if (!(await allowed(env.LISTS, req))) return refuse('too-many-tries')
      return directory(env, route[2]).fetch('https://directory/watch', upgrade)
    }

    if (route && route[1] === 'host') {
      const game = route[2]
      if (!GAMES.has(game)) return new Response('not found\n', { status: 404 })
      if (!(await allowed(env.HOSTS, req))) return refuse('too-many-tries')
      const locked = url.searchParams.get('locked') === '1'
      const name = cleanName(url.searchParams.get('name'))
      for (let i = 0; i < HOST_TRIES; i++) {
        const id = newRoomId()
        const q = new URLSearchParams({ id, game, name, locked: locked ? '1' : '0', code: locked ? newCode() : '', resume: url.searchParams.get('resume') === '1' ? '1' : '0' })
        const res = await room(env, `room:${id}`).fetch(`https://room/open?${q}`, upgrade)
        if (res.status !== 409) return res
      }
      return refuse('network')
    }

    if (route && route[1] === 'join') {
      const id = route[2]
      const code = url.searchParams.get('code') ?? ''
      if (!ROOM_ID.test(id) || (code && !CODE.test(code))) return refuse('gone')
      if (!(await allowed(env.JOINS, req))) return refuse('too-many-tries')
      const q = new URLSearchParams({ name: cleanName(url.searchParams.get('name')), code, resume: url.searchParams.get('resume') === '1' ? '1' : '0' })
      return room(env, `room:${id}`).fetch(`https://room/enter?${q}`, upgrade)
    }

    if (route && route[1] === 'back') {
      const key = route[2]
      const token = url.searchParams.get('token') ?? ''
      if (!(ROOM_ID.test(key) || CODE.test(key)) || !/^[0-9a-f]{32}$/.test(token)) return refuse('gone')
      if (!(await allowed(env.JOINS, req))) return refuse('too-many-tries')
      return room(env, ROOM_ID.test(key) ? `room:${key}` : key).fetch(`https://room/back?token=${token}`, upgrade)
    }

    if (url.pathname === '/host') {
      if (!(await allowed(env.HOSTS, req))) return refuse('too-many-tries')
      for (let i = 0; i < HOST_TRIES; i++) {
        const code = newCode()
        const resume = url.searchParams.get('resume') === '1' ? '1' : '0'
        const res = await room(env, code).fetch(`https://room/host?code=${code}&resume=${resume}`, upgrade)
        if (res.status !== 409) return res
      }
      return refuse('network')
    }

    return new Response('not found\n', { status: 404 })
  },
}

// one room: at most a host and a guest. hibernates between messages, so a waiting host costs nothing. a listed
// room keeps what it is (its id, game, nickname and code) on the host's socket, which outlasts hibernation
export class Room extends DurableObject {
  constructor(ctx, env) {
    super(ctx, env)
    ctx.setWebSocketAutoResponse(new WebSocketRequestResponsePair('ping', 'pong'))
    // both read from storage when first needed, since a room that hibernated forgets them
    this.seatsCache = undefined
    this.queue = null
    this.queueTimer = null
  }

  // the paired players' seats, or null before pairing and after the game ends
  async seats() {
    if (this.seatsCache === undefined) this.seatsCache = (await this.ctx.storage.get('seats')) ?? null
    return this.seatsCache
  }

  async saveSeats(seats) {
    this.seatsCache = seats
    await this.ctx.storage.put('seats', seats)
  }

  async loadQueue() {
    if (!this.queue) this.queue = (await this.ctx.storage.get('queue')) ?? { host: [], guest: [] }
    return this.queue
  }

  // written at most once a second, so a live game's frames don't each cost a write
  saveQueueSoon() {
    if (this.queueTimer) return
    this.queueTimer = setTimeout(() => {
      this.queueTimer = null
      if (this.queue) this.ctx.storage.put('queue', this.queue)
    }, 1000)
  }

  // seats only when both pages can take one back; an earlier release's game ends the moment a link drops
  async pair(host, guest, key, hostName, guestName, resume) {
    if (!resume || !key) {
      host.send(event({ type: 'open', role: 'host', peer: guestName }))
      guest.send(event({ type: 'open', role: 'guest', peer: hostName }))
      return
    }
    const seats = newSeats(key, hostName, guestName)
    await this.saveSeats(seats)
    host.send(event({ type: 'open', role: 'host', peer: guestName, room: key, token: seats.host.token }))
    guest.send(event({ type: 'open', role: 'guest', peer: hostName, room: key, token: seats.guest.token }))
  }

  // the game is over: the seats go, and a player coming back hears why
  async end(reason) {
    clearTimeout(this.queueTimer)
    this.queueTimer = null
    this.seatsCache = null
    this.queue = null
    await this.ctx.storage.delete(['seats', 'queue'])
    await this.ctx.storage.put('ended', reason)
    // the alarm then clears the room's storage
    await this.ctx.storage.setAlarm(Date.now() + GRACE_MS)
  }

  // the open socket with this tag, skipping one that is on its way out
  live(tag) {
    return this.ctx.getWebSockets(tag).find((ws) => ws.readyState === WebSocket.OPEN) ?? null
  }

  other(ws) {
    const [tag] = this.ctx.getTags(ws)
    return this.live(tag === 'host' ? 'guest' : 'host')
  }

  accept(tag, meta) {
    const [client, server] = Object.values(new WebSocketPair())
    this.ctx.acceptWebSocket(server, [tag])
    if (meta) server.serializeAttachment(meta)
    return { client, server }
  }

  // the listed room's facts, from its host's socket; null for a code-only room
  meta(host) {
    return host?.deserializeAttachment() ?? null
  }

  async unlist(meta) {
    if (!meta?.id) return
    try {
      await directory(this.env, meta.game).remove(meta.id)
    } catch {
      // the list drops it when it expires anyway
    }
  }

  async fetch(req) {
    const url = new URL(req.url)
    const p = url.searchParams
    const host = this.live('host')
    switch (url.pathname) {
      case '/open': {
        if (host || (await this.seats())) return new Response(null, { status: 409 })
        const meta = { id: p.get('id'), game: p.get('game'), name: p.get('name'), locked: p.get('locked') === '1', code: p.get('code') ?? '', wrong: 0, resume: p.get('resume') === '1' }
        const { client, server } = this.accept('host', meta)
        server.send(event({ type: 'hosting', code: meta.code }))
        await this.ctx.storage.delete('ended')
        await this.ctx.storage.setAlarm(Date.now() + WAIT_MS)
        try {
          await directory(this.env, meta.game).add({ id: meta.id, name: meta.name, locked: meta.locked })
        } catch {
          // unlisted, the room still works for whoever has its id; the host sees nobody come
        }
        return new Response(null, { status: 101, webSocket: client })
      }
      case '/enter': {
        const meta = this.meta(host)
        if (!host || !meta?.id) return refuse('gone')
        if (this.live('guest')) return refuse('busy')
        if (meta.locked && p.get('code') !== meta.code) {
          meta.wrong++
          if (meta.wrong >= TRIES_PER_CODE) {
            meta.wrong = 0
            meta.code = newCode()
            host.send(event({ type: 'hosting', code: meta.code, renewed: true }))
          }
          host.serializeAttachment(meta)
          return refuse('wrong-code')
        }
        const name = p.get('name') || 'friend'
        const { client, server } = this.accept('guest', { name })
        await this.ctx.storage.deleteAlarm()
        await this.unlist(meta)
        await this.pair(host, server, meta.id, meta.name, name, meta.resume && p.get('resume') === '1')
        return new Response(null, { status: 101, webSocket: client })
      }
      // the code-only rooms of earlier releases
      case '/host': {
        if (host || (await this.seats())) return new Response(null, { status: 409 })
        // a code-only room's host carries no meta, only whether it can take its seat back
        const { client, server } = this.accept('host', { resume: p.get('resume') === '1' })
        server.send(event({ type: 'hosting', code: p.get('code') }))
        await this.ctx.storage.delete('ended')
        await this.ctx.storage.setAlarm(Date.now() + WAIT_MS)
        return new Response(null, { status: 101, webSocket: client })
      }
      case '/guest': {
        if (!host || this.meta(host)?.id) return refuse('wrong-code')
        if (this.live('guest')) return refuse('busy')
        const { client, server } = this.accept('guest')
        await this.ctx.storage.deleteAlarm()
        await this.pair(host, server, p.get('key') ?? '', '', '', Boolean(this.meta(host)?.resume) && p.get('resume') === '1')
        return new Response(null, { status: 101, webSocket: client })
      }
      case '/back': {
        const seats = await this.seats()
        if (!seats) return refuse((await this.ctx.storage.get('ended')) ?? 'gone')
        const side = seatOf(seats, p.get('token'))
        if (!side) return refuse('gone')
        // the old link may not have noticed it's dead yet: this one takes over, and its closing ends nothing
        const old = this.live(side)
        if (old) {
          old.serializeAttachment({ replaced: true })
          try {
            old.close(1000, 'replaced')
          } catch {
            // already closing
          }
        }
        const { client, server } = this.accept(side, { seat: side })
        delete seats.away[side]
        await this.saveSeats(seats)
        const them = otherSide(side)
        server.send(event({ type: 'open', role: side, peer: seats[them].name, room: seats.key, token: seats[side].token, resumed: true }))
        const queue = await this.loadQueue()
        for (const msg of queue[side]) server.send(msg)
        queue[side] = []
        await this.ctx.storage.put('queue', queue)
        const other = this.live(them)
        if (other) other.send(event({ type: 'back' }))
        else server.send(event({ type: 'away' }))
        if (seats.away[them] == null) await this.ctx.storage.deleteAlarm()
        return new Response(null, { status: 101, webSocket: client })
      }
      default:
        return new Response('not found\n', { status: 404 })
    }
  }

  async webSocketMessage(ws, msg) {
    if (typeof msg !== 'string' || msg.length > MAX_FRAME || msg[0] !== '.') return
    const other = this.other(ws)
    if (other) {
      other.send(msg)
      return
    }
    // kept for a player who's away, to hand over when they're back
    const seats = await this.seats()
    const [tag] = this.ctx.getTags(ws)
    const them = otherSide(tag)
    if (seats?.away[them] == null) return
    enqueue((await this.loadQueue())[them], msg)
    this.saveQueueSoon()
  }

  // a link that dropped mid-game: the seat is held, and the other side told
  async drop(ws) {
    const seats = await this.seats()
    const [tag] = this.ctx.getTags(ws)
    if (!seats) return this.leave(ws, 'lost')
    // a newer link already holds this seat
    if (this.live(tag)) return
    seats.away[tag] = Date.now()
    await this.saveSeats(seats)
    this.other(ws)?.send(event({ type: 'away' }))
    const { next } = overdue(seats, Date.now())
    if (next != null) await this.ctx.storage.setAlarm(next)
    try {
      ws.close(1000, 'done')
    } catch {
      // already closed
    }
  }

  async leave(ws, reason) {
    const [tag] = this.ctx.getTags(ws)
    if (await this.seats()) await this.end(reason)
    // a host leaving before anyone joined takes its room off the list
    if (tag === 'host' && !this.live('guest')) await this.unlist(this.meta(ws))
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

  async webSocketClose(ws, code) {
    if (ws.deserializeAttachment()?.replaced) return
    if (code === LEFT) await this.leave(ws, 'bye')
    else await this.drop(ws)
  }

  async webSocketError(ws) {
    if (ws.deserializeAttachment()?.replaced) return
    await this.drop(ws)
  }

  // a held seat whose time is up, a host nobody joined, or an ended game's leftovers
  async alarm() {
    const seats = await this.seats()
    if (seats) {
      const { gone, next } = overdue(seats, Date.now())
      if (!gone.length) {
        if (next != null) await this.ctx.storage.setAlarm(next)
        return
      }
      await this.end('lost')
      for (const side of SIDES) {
        const ws = this.live(side)
        if (!ws) continue
        try {
          ws.send(event({ type: 'closed', reason: 'lost' }))
          ws.close(1000, 'lost')
        } catch {
          // already closed
        }
      }
      return
    }
    // an ended game with nobody left: its leftovers go
    if (!this.live('host') && !this.live('guest')) {
      await this.ctx.storage.deleteAll()
      return
    }
    if (this.live('guest')) return
    for (const ws of this.ctx.getWebSockets('host')) {
      await this.unlist(this.meta(ws))
      try {
        ws.send(event({ type: 'closed', reason: 'expired' }))
        ws.close(1000, 'expired')
      } catch {
        // already closed
      }
    }
  }
}

// one game's list of open rooms, and the lobbies watching it. the watchers' sockets hibernate, so an idle
// lobby costs nothing; each change sends the list to every watcher
export class Directory extends DurableObject {
  constructor(ctx, env) {
    super(ctx, env)
    ctx.setWebSocketAutoResponse(new WebSocketRequestResponsePair('ping', 'pong'))
    this.list = null
  }

  async load() {
    if (!this.list) this.list = new RoomList((await this.ctx.storage.get('rooms')) ?? [])
    return this.list
  }

  async changed() {
    const list = await this.load()
    await this.ctx.storage.put('rooms', list.dump())
    const next = list.nextExpiry()
    if (next == null) await this.ctx.storage.deleteAlarm()
    else await this.ctx.storage.setAlarm(next)
    const text = event({ type: 'rooms', rooms: list.shown(Date.now()) })
    for (const ws of this.ctx.getWebSockets('watch')) {
      try {
        ws.send(text)
      } catch {
        // on its way out
      }
    }
  }

  async fetch() {
    const list = await this.load()
    const [client, server] = Object.values(new WebSocketPair())
    this.ctx.acceptWebSocket(server, ['watch'])
    server.send(event({ type: 'rooms', rooms: list.shown(Date.now()) }))
    return new Response(null, { status: 101, webSocket: client })
  }

  // from a room, over rpc
  async add(entry) {
    const list = await this.load()
    list.add(entry, Date.now())
    await this.changed()
  }

  async remove(id) {
    const list = await this.load()
    if (list.remove(id)) await this.changed()
  }

  // watchers only listen
  webSocketMessage() {}

  webSocketClose(ws) {
    try {
      ws.close(1000, 'done')
    } catch {
      // already closed
    }
  }

  async alarm() {
    const list = await this.load()
    if (list.prune(Date.now())) await this.changed()
    else {
      const next = list.nextExpiry()
      if (next != null) await this.ctx.storage.setAlarm(next)
    }
  }
}
