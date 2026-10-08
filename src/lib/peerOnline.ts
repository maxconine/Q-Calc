import { parsePeerEvent, type CloseReason, type PeerEvent, type PeerTransport } from './peer'
import { Listeners } from './peerDev'

// the link to another q calc anywhere: both sides open a websocket to the relay (relay/ in this repo), the host
// gets a code and the guest types it. the relay forwards game messages without reading them

export const RELAY_URL = 'wss://qcalc-relay.maxconine.workers.dev'
export const ONLINE_CODE_LENGTH = 6
const PING_MS = 10_000
// two missed pongs: the connection is gone even if the socket hasn't noticed
const DEAD_MS = 25_000

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
  readonly discovers = false
  readonly codeLength = ONLINE_CODE_LENGTH
  private listeners = new Listeners()
  private ws: SocketLike | null = null
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

  // there is nothing to look for: the guest types the code
  discover(): void {}
  stopDiscovery(): void {}

  host(): void {
    this.connect('/host')
  }

  join(_peerId: string, code: string): void {
    this.emit({ type: 'pairing' })
    this.connect(`/join/${encodeURIComponent(code)}`)
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
