import { nativeHandler, nativeWindow, type NativeWindow } from './bridge'
import { relayUrl } from './peerOnline'
import { isWindowsHost } from './platform'

// messages between q calc users, through the relay (relay/src/index.js, Inbox). a name set in settings is held
// on the relay by a key this app made and keeps; without one, nothing is received. a message from someone new
// shows only as a request, and only accepted names' messages show at all. the relay keeps no message: one to a
// name whose app isn't connected is turned away, and the sender's bar says so.
//
// the mac app keeps the name, the key, the lists and the inbox itself, so messages arrive while the bar is put
// away, and hands the page a copy as __QCALC_MESSAGING. everywhere else they live in local storage, and the
// bar holds the inbox (InboxLink) while it runs

export const NAME_RULE = /^[a-z0-9][a-z0-9_-]{1,19}$/
export const TEXT_MAX = 500
const KEY = 'qcalc-messaging'
const CHANGED = 'qcalc-messaging'
const PING_MS = 30_000
const RETRY_MS = 3_000
const RETRY_MAX = 60_000
export const INBOX_PROTOCOL = 'qcalc-inbox'

export type Messaging = { name: string; key: string; accepted: string[]; blocked: string[] }

type MessagingWindow = NativeWindow & { __QCALC_MESSAGING?: unknown; __qcalcMessaging?: (m: unknown) => void }

// a name as typed, lowercased; '' when it can't be one
export function cleanName(raw: unknown): string {
  if (typeof raw !== 'string') return ''
  const name = raw.trim().toLowerCase()
  return NAME_RULE.test(name) ? name : ''
}

const names = (raw: unknown): string[] =>
  Array.isArray(raw) ? [...new Set(raw.map(cleanName).filter(Boolean))].sort() : []

export function sanitizeMessaging(raw: unknown): Messaging {
  const o = raw && typeof raw === 'object' ? (raw as Record<string, unknown>) : {}
  const key = typeof o.key === 'string' && /^[0-9a-f]{64}$/.test(o.key) ? o.key : ''
  const blocked = names(o.blocked)
  return { name: cleanName(o.name), key, accepted: names(o.accepted).filter((n) => !blocked.includes(n)), blocked }
}

export function newKey(bytes = crypto.getRandomValues(new Uint8Array(32))): string {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('')
}

// the mac app holds the inbox and the lists, and its own settings window edits them
export function nativeMessaging(): boolean {
  return Boolean(nativeHandler()) && !isWindowsHost()
}

export function loadMessaging(): Messaging {
  if (nativeMessaging()) return sanitizeMessaging((nativeWindow() as MessagingWindow | undefined)?.__QCALC_MESSAGING)
  try {
    const raw = localStorage.getItem(KEY)
    return sanitizeMessaging(raw ? JSON.parse(raw) : null)
  } catch {
    return sanitizeMessaging(null)
  }
}

// a key is made the first time one's needed and kept with the name, so the name stays this app's
export function saveMessaging(m: Messaging): void {
  const next = sanitizeMessaging(m)
  if (nativeMessaging()) {
    nativeHandler()?.postMessage({ type: 'messaging', accepted: next.accepted, blocked: next.blocked })
    return
  }
  try {
    localStorage.setItem(KEY, JSON.stringify(next))
  } catch {
    // private window: nothing kept
  }
  window.dispatchEvent(new Event(CHANGED))
}

// another window's change (the settings window's, through storage), this window's, or the mac app's
export function onMessagingChange(fn: (m: Messaging) => void): () => void {
  const w = nativeWindow() as MessagingWindow | undefined
  if (!w) return () => {}
  const changed = () => fn(loadMessaging())
  const onStorage = (e: StorageEvent) => {
    if (e.key === KEY) changed()
  }
  w.__qcalcMessaging = (m) => {
    w.__QCALC_MESSAGING = m
    changed()
  }
  w.addEventListener('storage', onStorage)
  w.addEventListener(CHANGED, changed)
  return () => {
    w.removeEventListener('storage', onStorage)
    w.removeEventListener(CHANGED, changed)
  }
}

export type Arrival = 'show' | 'ask' | 'drop'

// an accepted name's message shows; a blocked name's goes unseen; anyone else's is a request
export function arrival(m: Messaging, from: string): Arrival {
  if (m.blocked.includes(from)) return 'drop'
  return m.accepted.includes(from) ? 'show' : 'ask'
}

export const allowSender = (m: Messaging, who: string): Messaging => ({
  ...m,
  accepted: [...m.accepted.filter((n) => n !== who), who].sort(),
  blocked: m.blocked.filter((n) => n !== who),
})

export const blockSender = (m: Messaging, who: string): Messaging => ({
  ...m,
  accepted: m.accepted.filter((n) => n !== who),
  blocked: [...m.blocked.filter((n) => n !== who), who].sort(),
})

export const forgetSender = (m: Messaging, who: string): Messaging => ({
  ...m,
  accepted: m.accepted.filter((n) => n !== who),
  blocked: m.blocked.filter((n) => n !== who),
})

// the bar's `message <name> <text>`
export function isMessageCommand(text: string): boolean {
  return /^\s*message(?:\s|$)/i.test(text)
}

export type MessageLine = { to: string; name: string; text: string }

// `to` as typed, `name` cleaned ('' when it can't be a name), the rest as the message
export function parseMessage(text: string): MessageLine | null {
  const m = /^\s*message(?:\s+(\S+))?(?:\s+([\s\S]*))?$/i.exec(text)
  if (!m) return null
  const to = m[1] ?? ''
  return { to, name: cleanName(to), text: (m[2] ?? '').trim().slice(0, TEXT_MAX) }
}

// what the bar says while a message is typed
export function messageHint(line: MessageLine, m: Messaging, relay = Boolean(relayUrl())): string {
  if (!relay) return 'messages need the online relay'
  if (!m.name || !m.key) return 'set your name in settings to send messages · ↵ opens settings'
  if (!line.to) return 'message <name> <what to say>'
  if (!line.name) return 'a name is 2–20 letters, digits, - or _'
  if (!line.text) return `then what to say to ${line.name}`
  return `↵ sends to ${line.name}`
}

export type SendStatus = 'delivered' | 'offline' | 'unknown' | 'not-yours' | 'too-many' | 'bad' | 'network'

// what the bar says once a message has gone, or hasn't. the sender isn't told whether they're accepted, so
// blocking someone is never given away
export function sendStatusText(status: SendStatus, to: string): string {
  switch (status) {
    case 'delivered':
      return `sent to ${to}`
    case 'offline':
      return `${to} isn’t on q calc right now · not sent`
    case 'unknown':
      return `nobody’s called ${to} · not sent`
    case 'not-yours':
      return 'your name’s been taken · pick another in settings'
    case 'too-many':
      return 'that’s a lot of messages · try again in a minute'
    case 'bad':
      return 'that message couldn’t be sent'
    default:
      return 'couldn’t reach the relay · not sent'
  }
}

export function relayHttp(base = relayUrl()): string {
  return base.replace(/^ws/, 'http').replace(/\/+$/, '')
}

type Post = (url: string, init: { method: string; headers: Record<string, string>; body: string }) => Promise<{ json: () => Promise<unknown> }>

// text/plain, so a page sends it without a preflight
async function call(path: string, body: Record<string, unknown>, post: Post, base: string): Promise<string> {
  if (!base) return 'network'
  try {
    const res = await post(base + path, { method: 'POST', headers: { 'Content-Type': 'text/plain' }, body: JSON.stringify(body) })
    const o = (await res.json()) as { status?: unknown }
    return typeof o?.status === 'string' ? o.status : 'network'
  } catch {
    return 'network'
  }
}

const SEND_STATUSES = new Set<string>(['delivered', 'offline', 'unknown', 'not-yours', 'too-many', 'bad'])

export async function sendMessage(m: Messaging, to: string, text: string, post: Post = fetch, base = relayHttp()): Promise<SendStatus> {
  const status = await call('/send', { from: m.name, key: m.key, to, text }, post, base)
  return SEND_STATUSES.has(status) ? (status as SendStatus) : 'network'
}

export type ClaimStatus = 'ok' | 'taken' | 'too-many' | 'bad' | 'network'

export async function claimName(name: string, key: string, post: Post = fetch, base = relayHttp()): Promise<ClaimStatus> {
  const status = await call('/claim', { name, key }, post, base)
  return status === 'ok' || status === 'taken' || status === 'too-many' || status === 'bad' ? status : 'network'
}

export async function releaseName(name: string, key: string, post: Post = fetch, base = relayHttp()): Promise<void> {
  await call('/release', { name, key }, post, base)
}

export type InboxEvent =
  | { type: 'message'; from: string; text: string }
  | { type: 'state'; state: 'ready' | 'taken' | 'offline' }

type SocketLike = Pick<WebSocket, 'send' | 'close' | 'readyState'> & {
  onmessage: ((e: { data: unknown }) => void) | null
  onclose: (() => void) | null
}

// controls and direction overrides go here too, whatever the relay did, so a line can't fake another sender's
export function cleanMessageText(raw: unknown): string {
  if (typeof raw !== 'string') return ''
  // eslint-disable-next-line no-control-regex
  return raw.replace(/[\u0000-\u001f\u007f-\u009f\u200e\u200f\u2028-\u202e\u2066-\u2069]+/g, ' ').slice(0, TEXT_MAX)
}

export function parseInboxFrame(data: unknown): InboxEvent | null {
  if (typeof data !== 'string' || data[0] !== '!') return null
  let o: Record<string, unknown>
  try {
    o = JSON.parse(data.slice(1)) as Record<string, unknown>
  } catch {
    return null
  }
  if (o?.type === 'inbox') return { type: 'state', state: 'ready' }
  if (o?.type === 'closed' && (o.reason === 'taken' || o.reason === 'released')) return { type: 'state', state: 'taken' }
  if (o?.type === 'message') {
    const from = cleanName(o.from)
    const text = cleanMessageText(o.text)
    return from && text ? { type: 'message', from, text } : null
  }
  return null
}

// the inbox for hosts without a native one: a socket to the relay while the page runs, tried again with a
// growing wait when it drops. a name that's taken stops it until the name changes
export class InboxLink {
  private ws: SocketLike | null = null
  private ping: ReturnType<typeof setInterval> | undefined
  private retry: ReturnType<typeof setTimeout> | undefined
  private wait = RETRY_MS
  private who: { name: string; key: string } | null = null
  private readonly onEvent: (e: InboxEvent) => void
  private readonly open: (url: string, protocols: string[]) => SocketLike
  private readonly base: string

  constructor(
    onEvent: (e: InboxEvent) => void,
    open: (url: string, protocols: string[]) => SocketLike = (url, protocols) => new WebSocket(url, protocols) as unknown as SocketLike,
    base = relayUrl(),
  ) {
    this.onEvent = onEvent
    this.open = open
    this.base = base.replace(/\/+$/, '')
  }

  start(name: string, key: string): void {
    if (this.who?.name === name && this.who.key === key) return
    this.stop()
    if (!name || !key || !this.base) return
    this.who = { name, key }
    this.wait = RETRY_MS
    this.connect()
  }

  private connect(): void {
    const who = this.who
    if (!who) return
    const ws = this.open(`${this.base}/inbox/${encodeURIComponent(who.name)}`, [INBOX_PROTOCOL, `k${who.key}`])
    this.ws = ws
    let taken = false
    ws.onmessage = (e) => {
      if (ws !== this.ws) return
      const ev = parseInboxFrame(e.data)
      if (!ev) return
      if (ev.type === 'state' && ev.state === 'ready') this.wait = RETRY_MS
      if (ev.type === 'state' && ev.state === 'taken') taken = true
      this.onEvent(ev)
    }
    ws.onclose = () => {
      if (ws !== this.ws) return
      this.ws = null
      clearInterval(this.ping)
      if (taken) {
        this.who = null
        return
      }
      this.onEvent({ type: 'state', state: 'offline' })
      this.retry = setTimeout(() => this.connect(), this.wait)
      this.wait = Math.min(RETRY_MAX, this.wait * 2)
    }
    // a quiet socket can be dropped along the way; the relay answers pings without waking up
    this.ping = setInterval(() => {
      if (ws === this.ws && ws.readyState === 1) ws.send('ping')
    }, PING_MS)
  }

  // back sooner than the next try: the computer woke or came back online
  retryNow(): void {
    if (!this.who || this.ws || !this.retry) return
    clearTimeout(this.retry)
    this.wait = RETRY_MS
    this.connect()
  }

  stop(): void {
    this.who = null
    clearTimeout(this.retry)
    this.retry = undefined
    clearInterval(this.ping)
    const ws = this.ws
    this.ws = null
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
}
