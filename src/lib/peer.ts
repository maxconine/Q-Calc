import { nativeHandler, nativeWindow } from './bridge'
import { DevPeer, Listeners } from './peerDev'
import { isWindowsHost } from './platform'

// a link to one other q calc on the same wi-fi. the mac app's PeerLink does the finding, pairing and
// encryption; the page sees only these events. games and anything else sent between bars sit on top

export type PeerInfo = { id: string; name: string }

export type CloseReason =
  | 'wrong-code'
  | 'busy'
  | 'unreachable'
  | 'lost'
  | 'bye'
  | 'denied'
  | 'network'
  | 'too-many-tries'

export type PeerEvent =
  | { type: 'peers'; peers: PeerInfo[] }
  | { type: 'denied' }
  | { type: 'hosting'; code: string; renewed?: boolean }
  | { type: 'pairing' }
  | { type: 'open'; role: 'host' | 'guest'; peer: string }
  | { type: 'message'; data: string }
  | { type: 'closed'; reason: CloseReason }

export interface PeerTransport {
  // what the lobby says it is; the dev link names itself so nobody mistakes it for the real one
  readonly label: string
  // what the other end is called in the lobby's copy
  readonly peerWord: string
  discover(): void
  stopDiscovery(): void
  host(): void
  join(peerId: string, code: string): void
  send(data: string): void
  // leaves quietly: the other side hears `bye`, this side hears nothing
  close(): void
  subscribe(fn: (e: PeerEvent) => void): () => void
}

const REASONS = new Set<CloseReason>(['wrong-code', 'busy', 'unreachable', 'lost', 'bye', 'denied', 'network', 'too-many-tries'])

export function isPairingCode(s: string): boolean {
  return /^[0-9]{4}$/.test(s)
}

// events come from native code but are still checked; a bad one is dropped
export function parsePeerEvent(raw: unknown): PeerEvent | null {
  if (!raw || typeof raw !== 'object') return null
  const e = raw as Record<string, unknown>
  switch (e.type) {
    case 'peers': {
      if (!Array.isArray(e.peers)) return null
      const peers = e.peers.flatMap((p: unknown): PeerInfo[] => {
        const o = p as Record<string, unknown> | null
        return o && typeof o.id === 'string' && typeof o.name === 'string' ? [{ id: o.id, name: o.name }] : []
      })
      return { type: 'peers', peers }
    }
    case 'denied':
      return { type: 'denied' }
    case 'hosting':
      if (typeof e.code !== 'string' || !isPairingCode(e.code)) return null
      return e.renewed === true ? { type: 'hosting', code: e.code, renewed: true } : { type: 'hosting', code: e.code }
    case 'pairing':
      return { type: 'pairing' }
    case 'open':
      if (e.role !== 'host' && e.role !== 'guest') return null
      return { type: 'open', role: e.role, peer: typeof e.peer === 'string' ? e.peer : '' }
    case 'message':
      return typeof e.data === 'string' ? { type: 'message', data: e.data } : null
    case 'closed':
      return { type: 'closed', reason: REASONS.has(e.reason as CloseReason) ? (e.reason as CloseReason) : 'lost' }
    default:
      return null
  }
}

type PeerWindow = Window & { __qcalcPeer?: (e: unknown) => void }

class NativePeer implements PeerTransport {
  readonly label = 'same Wi-Fi'
  readonly peerWord = 'Mac'
  private listeners = new Listeners()

  constructor() {
    const w = nativeWindow() as PeerWindow | undefined
    if (w) {
      w.__qcalcPeer = (raw) => {
        const e = parsePeerEvent(raw)
        if (e) this.listeners.emit(e)
      }
    }
  }

  private post(op: string, extra: Record<string, unknown> = {}): void {
    nativeHandler()?.postMessage({ type: 'peer', op, ...extra })
  }

  discover(): void {
    this.post('discover')
  }
  stopDiscovery(): void {
    this.post('stopDiscovery')
  }
  host(): void {
    this.post('host')
  }
  join(peerId: string, code: string): void {
    this.post('join', { peer: peerId, code })
  }
  send(data: string): void {
    this.post('send', { data })
  }
  close(): void {
    this.post('close')
  }
  subscribe(fn: (e: PeerEvent) => void): () => void {
    return this.listeners.add(fn)
  }
}

let shared: PeerTransport | null | undefined

// the mac app's link, the dev link in a plain browser, or none (the windows shell has no link yet)
export function peerTransport(): PeerTransport | null {
  if (shared !== undefined) return shared
  if (!hasPeerTransport()) shared = null
  else shared = nativeWindow()?.__QCALC_NATIVE ? new NativePeer() : new DevPeer()
  return shared
}

export function hasPeerTransport(): boolean {
  const w = nativeWindow()
  if (isWindowsHost()) return false
  return w?.__QCALC_NATIVE ? Boolean(nativeHandler()) : typeof BroadcastChannel !== 'undefined'
}

export function closeReasonText(reason: CloseReason): string {
  switch (reason) {
    case 'wrong-code':
      return 'wrong code'
    case 'busy':
      return 'that Mac is already playing'
    case 'unreachable':
      return 'couldn’t reach that Mac'
    case 'lost':
      return 'lost the connection'
    case 'bye':
      return 'your friend left'
    case 'denied':
      return 'Q Calc needs Local Network access · System Settings › Privacy & Security › Local Network'
    case 'network':
      return 'couldn’t start the link'
    case 'too-many-tries':
      return 'stopped hosting after too many wrong codes · host again for a new one'
  }
}
