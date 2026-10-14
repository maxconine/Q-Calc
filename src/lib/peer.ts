import { nativeWindow } from './bridge'
import { DevPeer } from './peerDev'
import { OnlinePeer, relayUrl } from './peerOnline'
import { cleanNickname } from './nickname'
import { isWindowsHost } from './platform'

// a link to one other q calc through the relay (peerOnline), or between tabs under `npm run dev` (peerDev).
// the page sees only these events. games and anything else sent between bars sit on top

// a room in a lobby's list: who's hosting, and whether it takes a code. nothing else about the host is known
export type RoomInfo = { id: string; name: string; locked: boolean }

export type HostOptions = { game: string; name: string; locked: boolean }

export type CloseReason =
  | 'wrong-code'
  | 'busy'
  | 'unreachable'
  | 'lost'
  | 'bye'
  | 'network'
  | 'too-many-tries'
  | 'expired'
  | 'gone'

export type PeerEvent =
  // offline: the list couldn't be reached. codeOnly: it never could, so the relay may be one from before room
  // lists; hosting and joining then go by code alone, which every relay understands
  | { type: 'rooms'; rooms: RoomInfo[]; offline?: boolean; codeOnly?: boolean }
  // code is '' for an open room
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
  readonly codeLength: number
  // lists the rooms open for this game, as `rooms` events, until unwatch
  watch(game: string): void
  unwatch(): void
  host(options: HostOptions): void
  // code is '' for an open room; name is what the host will see
  join(roomId: string, code: string, name: string): void
  send(data: string): void
  // leaves quietly: the other side hears `bye`, this side hears nothing
  close(): void
  subscribe(fn: (e: PeerEvent) => void): () => void
}

const REASONS = new Set<CloseReason>(['wrong-code', 'busy', 'unreachable', 'lost', 'bye', 'network', 'too-many-tries', 'expired', 'gone'])

const MAX_ROOMS = 100

export function isPairingCode(s: string, length: number): boolean {
  return s.length === length && /^[0-9]+$/.test(s)
}

function roomOf(raw: unknown, lockedDefault: boolean): RoomInfo[] {
  const o = raw as Record<string, unknown> | null
  if (!o || typeof o.id !== 'string' || !o.id || o.id.length > 64) return []
  return [{ id: o.id, name: cleanNickname(o.name) || 'friend', locked: typeof o.locked === 'boolean' ? o.locked : lockedDefault }]
}

// events come from the relay but are still checked; a bad one is dropped
export function parsePeerEvent(raw: unknown, codeLength: number): PeerEvent | null {
  if (!raw || typeof raw !== 'object') return null
  const e = raw as Record<string, unknown>
  switch (e.type) {
    case 'rooms':
      if (!Array.isArray(e.rooms)) return null
      return { type: 'rooms', rooms: e.rooms.slice(0, MAX_ROOMS).flatMap((r) => roomOf(r, true)) }
    case 'hosting': {
      const code = e.code ?? ''
      if (typeof code !== 'string' || (code !== '' && !isPairingCode(code, codeLength))) return null
      return e.renewed === true ? { type: 'hosting', code, renewed: true } : { type: 'hosting', code }
    }
    case 'pairing':
      return { type: 'pairing' }
    case 'open':
      if (e.role !== 'host' && e.role !== 'guest') return null
      return { type: 'open', role: e.role, peer: cleanNickname(e.peer) }
    case 'message':
      return typeof e.data === 'string' ? { type: 'message', data: e.data } : null
    case 'closed':
      return { type: 'closed', reason: REASONS.has(e.reason as CloseReason) ? (e.reason as CloseReason) : 'lost' }
    default:
      return null
  }
}

let shared: PeerTransport | null | undefined

// the relay wherever there is one, else the dev link in a plain browser, or none
export function peerTransport(): PeerTransport | null {
  if (shared !== undefined) return shared
  const relay = relayUrl()
  if (!hasPeerTransport()) shared = null
  else if (relay) shared = new OnlinePeer(relay)
  else shared = new DevPeer()
  return shared
}

export function hasPeerTransport(): boolean {
  if (relayUrl() && typeof WebSocket !== 'undefined') return true
  if (isWindowsHost() || nativeWindow()?.__QCALC_NATIVE) return false
  return typeof BroadcastChannel !== 'undefined'
}

// `word` is what the other end is called: a computer online, a tab on the dev link
export function closeReasonText(reason: CloseReason, word = 'computer'): string {
  switch (reason) {
    case 'wrong-code':
      return 'wrong code · try again'
    case 'busy':
      return 'someone else just joined that room'
    case 'gone':
      return 'that room has closed'
    case 'unreachable':
      return `couldn’t reach that ${word}`
    case 'lost':
      return 'lost the connection'
    case 'bye':
      return 'your friend left'
    case 'network':
      return 'couldn’t reach the game server · check your internet'
    case 'too-many-tries':
      return 'too many tries · wait a minute'
    case 'expired':
      return 'nobody joined for 15 minutes · host again'
  }
}
