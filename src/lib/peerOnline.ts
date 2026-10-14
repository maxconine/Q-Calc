import { parsePeerEvent, type CloseReason, type HostOptions, type PeerEvent, type PeerTransport } from './peer'
import { Listeners } from './peerDev'

// the link to another q calc anywhere: both sides open a websocket to the relay (relay/ in this repo). the
// lobby watches the relay's list of rooms for its game; a host opens a room, open or locked with a code, and a
// guest joins one from the list. the relay forwards game messages without reading them

export const RELAY_URL = 'wss://qcalc-relay.maxconine.workers.dev'
export const ONLINE_CODE_LENGTH = 6
const PING_MS = 10_000
// two missed pongs: the connection is gone even if the socket hasn't noticed
const DEAD_MS = 25_000
// a socket still connecting after this long isn't going to
const CONNECT_MS = 10_000
// a dropped link mid-game tries for its seat back this long, a little under the relay's hold (relay/src/seats.js)
const RESUME_MS = 590_000
const RESUME_RETRY_MS = 1_000
const RESUME_RETRY_MAX = 5_000
// what's sent while reconnecting goes once the seat is back
const MAX_OUTBOX = 256
// a close the relay reads as reconnecting, not leaving (it ends the game only on 1000)
const RECONNECTING = 4000
// the room list tries again after this long when it can't reach the relay, up to WATCH_RETRY_MAX
const WATCH_RETRY_MS = 3_000
const WATCH_RETRY_MAX = 30_000

// the relay this build talks to; `npm run dev` pairs tabs instead unless VITE_RELAY_URL points at one
export function relayUrl(): string {
  if (import.meta.env.DEV) return (import.meta.env.VITE_RELAY_URL as string | undefined) ?? ''
  return RELAY_URL
}

type SocketLike = Pick<WebSocket, 'send' | 'close' | 'readyState'> & {
  onmessage: ((e: { data: unknown }) => void) | null
  onclose: (() => void) | null
}

export class OnlinePeer implements PeerTransport {
  readonly label = 'online'
  readonly peerWord = 'computer'
  readonly codeLength = ONLINE_CODE_LENGTH
  private listeners = new Listeners()
  private ws: SocketLike | null = null
  private watcher: SocketLike | null = null
  private watching: string | null = null
  private watchTimer: ReturnType<typeof setTimeout> | undefined
  private watchPing: ReturnType<typeof setInterval> | undefined
  private watchWait = WATCH_RETRY_MS
  // a room list has come down this session, so the relay has them
  private listed = false
  // the list has never come: host and join the way relays before room lists do, by code alone
  private codeOnly = false
  private connectTimer: ReturnType<typeof setTimeout> | undefined
  // the game is on: closing now is `lost`, not `network`
  private paired = false
  // the relay said something, so a close before pairing is the relay's doing, not a failed connection
  private heard = 0
  private timer: ReturnType<typeof setInterval> | undefined
  // the seat to take back if the link drops mid-game, and while reconnecting, when to give up
  private seat: { room: string; token: string } | null = null
  private resumeUntil = 0
  private resumeTimer: ReturnType<typeof setTimeout> | undefined
  private resumeWait = RESUME_RETRY_MS
  private outbox: string[] = []
  private readonly base: string
  private readonly open: (url: string) => SocketLike

  constructor(base: string, open: (url: string) => SocketLike = (url) => new WebSocket(url) as unknown as SocketLike) {
    this.base = base.replace(/\/+$/, '')
    this.open = open
    if (typeof window !== 'undefined' && typeof document !== 'undefined') {
      window.addEventListener('online', () => this.retryNow())
      document.addEventListener('visibilitychange', () => {
        if (document.visibilityState === 'visible') this.retryNow()
      })
      // quitting loses the game itself, so there's nothing to come back to: leave, and the friend hears at once
      window.addEventListener('pagehide', () => {
        if (this.seat) this.close()
      })
    }
  }

  private emit(e: PeerEvent): void {
    this.listeners.emit(e)
  }

  private connect(path: string, closeCode = 1000): void {
    this.drop(closeCode)
    const ws = this.open(this.base + path)
    this.ws = ws
    this.heard = 0
    ws.onmessage = (e) => {
      if (ws === this.ws && typeof e.data === 'string') this.receive(e.data)
    }
    ws.onclose = () => {
      if (ws !== this.ws) return
      if (this.seat) this.lose()
      else this.fail(this.paired ? 'lost' : this.heard ? 'lost' : 'network')
    }
    this.timer = setInterval(() => {
      if (ws !== this.ws) return
      if (this.heard && Date.now() - this.heard > DEAD_MS) {
        if (this.seat) this.lose()
        else this.fail('lost')
      } else if (ws.readyState === 1) ws.send('ping')
    }, PING_MS)
    this.connectTimer = setTimeout(() => {
      if (ws !== this.ws || ws.readyState !== 0) return
      if (this.seat) this.lose()
      else this.fail('network')
    }, CONNECT_MS)
  }

  // the link dropped mid-game: the relay holds the seat, so keep asking for it back until the time's up
  private lose(): void {
    const first = !this.resumeUntil
    this.drop(RECONNECTING)
    if (first) {
      this.resumeUntil = Date.now() + RESUME_MS
      this.resumeWait = RESUME_RETRY_MS
      this.emit({ type: 'away', who: 'self' })
    }
    clearTimeout(this.resumeTimer)
    if (Date.now() >= this.resumeUntil) {
      this.forget()
      this.emit({ type: 'closed', reason: 'lost' })
      return
    }
    this.resumeTimer = setTimeout(() => this.resume(), first ? 0 : this.resumeWait)
    if (!first) this.resumeWait = Math.min(RESUME_RETRY_MAX, this.resumeWait * 2)
  }

  private resume(): void {
    const seat = this.seat
    if (!seat) return
    const q = new URLSearchParams({ token: seat.token })
    this.connect(`/back/${encodeURIComponent(seat.room)}?${q}`, RECONNECTING)
  }

  // the window hid mid-game: the relay holds the seat while nothing here runs, and the game waits for both
  stepAway(): void {
    if (!this.seat || !this.paired) return
    clearTimeout(this.resumeTimer)
    this.resumeTimer = undefined
    this.drop(RECONNECTING)
    this.resumeUntil = Date.now() + RESUME_MS
    this.emit({ type: 'away', who: 'self' })
  }

  // shown again: one try now, however long it's been; the relay says if the game ended meanwhile
  comeBack(): void {
    if (!this.seat || !this.resumeUntil || this.ws) return
    this.resumeUntil = Math.max(this.resumeUntil, Date.now() + RESUME_RETRY_MAX)
    this.resumeWait = RESUME_RETRY_MS
    this.resume()
  }

  // back sooner than the next try: the computer woke or came back online
  retryNow(): void {
    // a stepped-away seat waits for comeBack, not for the network
    if (!this.seat || !this.resumeUntil || !this.resumeTimer || this.ws?.readyState === 1) return
    clearTimeout(this.resumeTimer)
    this.resumeWait = RESUME_RETRY_MS
    this.resume()
  }

  // the game is over for this side: no seat to go back to
  private forget(): void {
    this.seat = null
    this.resumeUntil = 0
    clearTimeout(this.resumeTimer)
    this.resumeTimer = undefined
    this.outbox = []
  }

  private receive(text: string): void {
    this.heard = Date.now()
    if (text === 'pong') return
    if (text[0] === '.') {
      if (this.paired) this.emit({ type: 'message', data: text.slice(1) })
      return
    }
    if (text[0] !== '!') return
    let raw: unknown
    try {
      raw = JSON.parse(text.slice(1))
    } catch {
      return
    }
    const e = parsePeerEvent(raw, this.codeLength)
    if (!e) return
    if (e.type === 'closed') {
      this.forget()
      this.drop()
      this.emit(e)
      return
    }
    if (e.type === 'open') {
      this.paired = true
      if (e.room && e.token) this.seat = { room: e.room, token: e.token }
      if (e.resumed) {
        this.resumeUntil = 0
        clearTimeout(this.resumeTimer)
        this.resumeTimer = undefined
        const out = this.outbox
        this.outbox = []
        for (const data of out) this.send(data)
        this.emit({ type: 'back', who: 'self' })
        return
      }
      this.emit({ ...e, peer: e.peer || 'friend' })
      return
    }
    if (e.type === 'away' || e.type === 'back' || e.type === 'hosting') this.emit(e)
  }

  private fail(reason: CloseReason): void {
    this.forget()
    this.drop()
    this.emit({ type: 'closed', reason })
  }

  // quietly: the relay tells the other side (code 1000 leaves; RECONNECTING keeps the seat)
  private drop(code = 1000): void {
    const ws = this.ws
    this.ws = null
    this.paired = false
    if (this.timer) clearInterval(this.timer)
    this.timer = undefined
    clearTimeout(this.connectTimer)
    this.connectTimer = undefined
    if (ws) {
      ws.onmessage = null
      ws.onclose = null
      try {
        ws.close(code)
      } catch {
        // already closing
      }
    }
  }

  // the room list: its own socket, kept apart from the game's, that the relay pushes the list down
  watch(game: string): void {
    this.unwatch()
    this.watching = game
    this.watchWait = WATCH_RETRY_MS
    this.openWatcher()
  }

  private openWatcher(): void {
    const game = this.watching
    if (!game) return
    const ws = this.open(`${this.base}/rooms/${encodeURIComponent(game)}`)
    this.watcher = ws
    let heard = false
    // a quiet socket can be dropped along the way; the relay answers pings without waking up
    clearInterval(this.watchPing)
    this.watchPing = setInterval(() => {
      if (ws === this.watcher && ws.readyState === 1) ws.send('ping')
    }, PING_MS * 3)
    ws.onmessage = (e) => {
      if (ws !== this.watcher || typeof e.data !== 'string' || e.data[0] !== '!') return
      let raw: unknown
      try {
        raw = JSON.parse(e.data.slice(1))
      } catch {
        return
      }
      const ev = parsePeerEvent(raw, this.codeLength)
      if (ev?.type !== 'rooms') return
      heard = true
      this.listed = true
      this.codeOnly = false
      this.watchWait = WATCH_RETRY_MS
      this.emit(ev)
    }
    ws.onclose = () => {
      if (ws !== this.watcher) return
      this.watcher = null
      clearInterval(this.watchPing)
      // a relay without lists refuses the socket before saying anything; so does one that can't be reached,
      // and either way the code still works
      if (!heard && !this.listed) this.codeOnly = true
      this.emit({ type: 'rooms', rooms: [], offline: true, ...(this.codeOnly ? { codeOnly: true } : {}) })
      this.watchTimer = setTimeout(() => this.openWatcher(), this.watchWait)
      this.watchWait = Math.min(WATCH_RETRY_MAX, this.watchWait * 2)
    }
  }

  unwatch(): void {
    this.watching = null
    clearTimeout(this.watchTimer)
    this.watchTimer = undefined
    clearInterval(this.watchPing)
    this.watchPing = undefined
    const ws = this.watcher
    this.watcher = null
    if (ws) {
      ws.onmessage = null
      ws.onclose = null
      try {
        ws.close(1000)
      } catch {
        // already closing
      }
    }
  }

  host({ game, name, locked }: HostOptions): void {
    this.forget()
    // resume=1: this page can take its seat back after a dropped link (relays that know it hold one)
    if (this.codeOnly) {
      this.connect('/host?resume=1')
      return
    }
    const q = new URLSearchParams({ name, locked: locked ? '1' : '0', resume: '1' })
    this.connect(`/host/${encodeURIComponent(game)}?${q}`)
  }

  join(roomId: string, code: string, name: string): void {
    this.forget()
    this.emit({ type: 'pairing' })
    // no room: the code alone, as relays before room lists take it
    if (!roomId) {
      this.connect(`/join/${encodeURIComponent(code)}?resume=1`)
      return
    }
    const q = new URLSearchParams({ name, resume: '1' })
    if (code) q.set('code', code)
    this.connect(`/join/${encodeURIComponent(roomId)}?${q}`)
  }

  send(data: string): void {
    if (this.paired && this.ws?.readyState === 1) this.ws.send('.' + data)
    else if (this.seat && this.outbox.length < MAX_OUTBOX) this.outbox.push(data)
  }

  close(): void {
    this.forget()
    this.drop()
  }

  subscribe(fn: (e: PeerEvent) => void): () => void {
    return this.listeners.add(fn)
  }
}
