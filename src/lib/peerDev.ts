import type { CloseReason, HostOptions, PeerEvent, PeerTransport, RoomInfo } from './peer'

// the peer link for a plain browser: tabs of the same browser list each other's rooms and pair over a
// BroadcastChannel, so the lobby and the games can be played and checked without two computers. the code is
// only compared here, nothing is encrypted; it is a stand-in for the relay's room list, not a network link

export class Listeners {
  private fns = new Set<(e: PeerEvent) => void>()

  add(fn: (e: PeerEvent) => void): () => void {
    this.fns.add(fn)
    return () => {
      this.fns.delete(fn)
    }
  }

  emit(e: PeerEvent): void {
    for (const fn of [...this.fns]) fn(e)
  }
}

type Wire =
  | { k: 'where'; game: string }
  | { k: 'here'; id: string; game: string; name: string; locked: boolean }
  | { k: 'gone'; id: string }
  | { k: 'join'; from: string; name: string; to: string; code: string }
  | { k: 'ok'; from: string; to: string; name: string }
  | { k: 'nope'; to: string; reason: CloseReason }
  | { k: 'data'; from: string; to: string; data: string }
  | { k: 'beat'; from: string; to: string }
  | { k: 'bye'; from: string; to: string }

export const DEV_CHANNEL = 'qcalc-peer-dev'
const TRIES_PER_CODE = 3
const BEAT_MS = 1000
const HOST_SEEN_MS = 3500
const LOST_MS = 4500
const JOIN_WAIT_MS = 3000

function randomId(): string {
  return Math.random().toString(36).slice(2, 10)
}

function newCode(): string {
  return String(Math.floor(Math.random() * 10000)).padStart(4, '0')
}

type Hosted = { game: string; name: string; locked: boolean }

export class DevPeer implements PeerTransport {
  readonly label = 'dev link · tabs in this browser'
  readonly peerWord = 'tab'
  readonly codeLength = 4
  readonly id = randomId()
  readonly name: string
  private channel: BroadcastChannel
  private listeners = new Listeners()
  private hosting: Hosted | null = null
  private code = ''
  private failures = 0
  private watching: string | null = null
  private hosts = new Map<string, { name: string; locked: boolean; seen: number }>()
  private peer: string | null = null
  private pending: string | null = null
  private joinTimer: ReturnType<typeof setTimeout> | undefined
  private heard = 0
  private timer: ReturnType<typeof setInterval> | undefined

  constructor(channelName = DEV_CHANNEL, name?: string) {
    this.name = name ?? `tab ${this.id.slice(0, 4)}`
    this.channel = new BroadcastChannel(channelName)
    this.channel.onmessage = (e: MessageEvent) => this.receive(e.data as Wire)
    if (typeof window !== 'undefined') window.addEventListener('pagehide', () => this.close())
  }

  private post(m: Wire): void {
    this.channel.postMessage(m)
  }

  private emit(e: PeerEvent): void {
    this.listeners.emit(e)
  }

  private announce(): void {
    const h = this.hosting
    if (h && !this.peer) this.post({ k: 'here', id: this.id, game: h.game, name: h.name, locked: h.locked })
  }

  private tick = () => {
    const now = Date.now()
    this.announce()
    if (this.watching) {
      let changed = false
      for (const [id, h] of this.hosts) {
        if (now - h.seen > HOST_SEEN_MS) {
          this.hosts.delete(id)
          changed = true
        }
      }
      if (changed) this.publish()
    }
    if (this.peer) {
      this.post({ k: 'beat', from: this.id, to: this.peer })
      if (now - this.heard > LOST_MS) this.fail('lost')
    }
    this.idle()
  }

  private wake(): void {
    if (!this.timer) this.timer = setInterval(this.tick, BEAT_MS)
  }

  private idle(): void {
    if (this.timer && !this.hosting && !this.watching && !this.peer) {
      clearInterval(this.timer)
      this.timer = undefined
    }
  }

  private publish(): void {
    const rooms: RoomInfo[] = [...this.hosts]
      .map(([id, h]) => ({ id, name: h.name, locked: h.locked }))
      .sort((a, b) => a.name.localeCompare(b.name))
    this.emit({ type: 'rooms', rooms })
  }

  private receive(m: Wire): void {
    if (!m || typeof m !== 'object') return
    switch (m.k) {
      case 'where':
        if (this.hosting?.game === m.game) this.announce()
        return
      case 'here':
        if (!this.watching || m.game !== this.watching || m.id === this.id) return
        this.hosts.set(m.id, { name: m.name, locked: m.locked, seen: Date.now() })
        this.publish()
        return
      case 'gone':
        if (this.hosts.delete(m.id) && this.watching) this.publish()
        return
      case 'join':
        if (m.to !== this.id || !this.hosting) return
        if (this.peer) {
          this.post({ k: 'nope', to: m.from, reason: 'busy' })
          return
        }
        if (this.hosting.locked && m.code !== this.code) {
          this.post({ k: 'nope', to: m.from, reason: 'wrong-code' })
          if (++this.failures >= TRIES_PER_CODE) {
            this.failures = 0
            this.code = newCode()
            this.emit({ type: 'hosting', code: this.code, renewed: true })
          }
          return
        }
        this.peer = m.from
        this.heard = Date.now()
        this.post({ k: 'gone', id: this.id })
        this.post({ k: 'ok', from: this.id, to: m.from, name: this.hosting.name })
        this.emit({ type: 'open', role: 'host', peer: m.name })
        return
      case 'ok':
        if (m.to !== this.id || this.pending !== m.from) return
        this.endJoin()
        this.peer = m.from
        this.heard = Date.now()
        this.wake()
        this.emit({ type: 'open', role: 'guest', peer: m.name })
        return
      case 'nope':
        if (m.to !== this.id || !this.pending) return
        this.endJoin()
        this.emit({ type: 'closed', reason: m.reason })
        return
      case 'data':
        if (m.to !== this.id || m.from !== this.peer) return
        this.heard = Date.now()
        this.emit({ type: 'message', data: m.data })
        return
      case 'beat':
        if (m.to === this.id && m.from === this.peer) this.heard = Date.now()
        return
      case 'bye':
        if (m.to === this.id && m.from === this.peer) this.fail('bye')
    }
  }

  private endJoin(): void {
    this.pending = null
    clearTimeout(this.joinTimer)
    this.joinTimer = undefined
  }

  private fail(reason: CloseReason): void {
    this.reset()
    this.emit({ type: 'closed', reason })
  }

  private reset(): void {
    if (this.hosting && !this.peer) this.post({ k: 'gone', id: this.id })
    this.hosting = null
    this.peer = null
    this.endJoin()
    this.idle()
  }

  watch(game: string): void {
    this.watching = game
    this.hosts.clear()
    this.publish()
    this.wake()
    this.post({ k: 'where', game })
  }

  unwatch(): void {
    this.watching = null
    this.hosts.clear()
    this.idle()
  }

  host({ game, name, locked }: HostOptions): void {
    this.reset()
    this.hosting = { game, name: name || this.name, locked }
    this.code = locked ? newCode() : ''
    this.failures = 0
    this.wake()
    this.announce()
    // after the caller has subscribed, like the native link's listener coming up
    queueMicrotask(() => {
      if (this.hosting) this.emit({ type: 'hosting', code: this.code })
    })
  }

  join(peerId: string, code: string, name: string): void {
    this.peer = null
    this.endJoin()
    this.pending = peerId
    this.emit({ type: 'pairing' })
    this.post({ k: 'join', from: this.id, name: name || this.name, to: peerId, code })
    this.joinTimer = setTimeout(() => {
      if (this.pending !== peerId) return
      this.endJoin()
      this.emit({ type: 'closed', reason: 'unreachable' })
    }, JOIN_WAIT_MS)
  }

  send(data: string): void {
    if (this.peer) this.post({ k: 'data', from: this.id, to: this.peer, data })
  }

  close(): void {
    // a join still waiting tells the host too, in case it already said yes
    const other = this.peer ?? this.pending
    if (other) this.post({ k: 'bye', from: this.id, to: other })
    this.reset()
  }

  subscribe(fn: (e: PeerEvent) => void): () => void {
    return this.listeners.add(fn)
  }

  // tests only
  dispose(): void {
    this.close()
    this.unwatch()
    this.channel.close()
  }
}
