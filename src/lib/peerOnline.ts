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
  private readonly base: string
  private readonly open: (url: string) => SocketLike

  constructor(base: string, open: (url: string) => SocketLike = (url) => new WebSocket(url) as unknown as SocketLike) {
    this.base = base.replace(/\/+$/, '')
    this.open = open
  }

  private emit(e: PeerEvent): void {
    this.listeners.emit(e)
  }

  private connect(path: string): void {
    this.drop()
    const ws = this.open(this.base + path)
    this.ws = ws
    this.heard = 0
    ws.onmessage = (e) => {
      if (ws === this.ws && typeof e.data === 'string') this.receive(e.data)
    }
    ws.onclose = () => {
      if (ws !== this.ws) return
      this.fail(this.paired ? 'lost' : this.heard ? 'lost' : 'network')
    }
    this.timer = setInterval(() => {
      if (ws !== this.ws) return
      if (this.heard && Date.now() - this.heard > DEAD_MS) this.fail('lost')
      else if (ws.readyState === 1) ws.send('ping')
    }, PING_MS)
    this.connectTimer = setTimeout(() => {
      if (ws === this.ws && ws.readyState === 0) this.fail('network')
    }, CONNECT_MS)
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
      this.drop()
      this.emit(e)
      return
    }
    if (e.type === 'open') {
      this.paired = true
      this.emit({ ...e, peer: e.peer || 'friend' })
      return
    }
    if (e.type === 'hosting') this.emit(e)
  }

  private fail(reason: CloseReason): void {
    this.drop()
    this.emit({ type: 'closed', reason })
  }

  // quietly: the relay tells the other side
  private drop(): void {
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
        ws.close(1000)
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
    if (this.codeOnly) {
      this.connect('/host')
      return
    }
    const q = new URLSearchParams({ name, locked: locked ? '1' : '0' })
    this.connect(`/host/${encodeURIComponent(game)}?${q}`)
  }

  join(roomId: string, code: string, name: string): void {
    this.emit({ type: 'pairing' })
    // no room: the code alone, as relays before room lists take it
    if (!roomId) {
      this.connect(`/join/${encodeURIComponent(code)}`)
      return
    }
    const q = new URLSearchParams({ name })
    if (code) q.set('code', code)
    this.connect(`/join/${encodeURIComponent(roomId)}?${q}`)
  }

  send(data: string): void {
    if (this.paired && this.ws?.readyState === 1) this.ws.send('.' + data)
  }

  close(): void {
    this.drop()
  }

  subscribe(fn: (e: PeerEvent) => void): () => void {
    return this.listeners.add(fn)
  }
}
