import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { completionFor } from './completion'
import {
  allowSender,
  arrival,
  blockSender,
  cleanName,
  forgetSender,
  InboxLink,
  isMessageCommand,
  lookupName,
  messageContacts,
  messageHint,
  messageNameSpan,
  newKey,
  RECENT_MAX,
  withRecent,
  parseInboxFrame,
  parseMessage,
  sanitizeMessaging,
  sendMessage,
  sendStatusText,
  type InboxEvent,
  type Messaging,
} from './messaging'

const KEY = 'ab'.repeat(32)
const me: Messaging = { name: 'ana', key: KEY, accepted: ['max'], blocked: ['spam'], recent: [] }

describe('messaging names and lists', () => {
  it('takes the same names the relay does, lowercased', () => {
    expect(cleanName(' Max ')).toBe('max')
    expect(cleanName('m')).toBe('')
    expect(cleanName('max conine')).toBe('')
    expect(cleanName('-x')).toBe('')
  })

  it('keeps only what can be a name, a key and a list, and never a name on both lists', () => {
    expect(sanitizeMessaging({ name: 'Ana', key: KEY, accepted: ['Max', 'max', 'x', 'bob'], blocked: ['bob'] })).toEqual({
      name: 'ana',
      key: KEY,
      accepted: ['max'],
      blocked: ['bob'],
      recent: [],
    })
    expect(sanitizeMessaging({ key: 'nope' })).toEqual({ name: '', key: '', accepted: [], blocked: [], recent: [] })
    expect(sanitizeMessaging(null).name).toBe('')
  })

  it('makes a fresh 32-byte key', () => {
    expect(newKey()).toMatch(/^[0-9a-f]{64}$/)
    expect(newKey()).not.toBe(newKey())
  })

  it('shows an accepted name, asks about someone new, and drops a blocked name', () => {
    expect(arrival(me, 'max')).toBe('show')
    expect(arrival(me, 'zoe')).toBe('ask')
    expect(arrival(me, 'spam')).toBe('drop')
  })

  it('moves a name between the lists', () => {
    expect(allowSender(me, 'spam')).toMatchObject({ accepted: ['max', 'spam'], blocked: [] })
    expect(blockSender(me, 'max')).toMatchObject({ accepted: [], blocked: ['max', 'spam'] })
    expect(forgetSender(me, 'max')).toMatchObject({ accepted: [], blocked: ['spam'] })
  })
})

describe('the message command', () => {
  it('reads who to and what to say', () => {
    expect(isMessageCommand('message max hello')).toBe(true)
    expect(isMessageCommand('message')).toBe(true)
    expect(isMessageCommand('messages')).toBe(false)
    expect(isMessageCommand('2 + message')).toBe(false)
    expect(parseMessage('message Max hello there  ')).toEqual({ to: 'Max', name: 'max', text: 'hello there' })
    expect(parseMessage('message max')).toEqual({ to: 'max', name: 'max', text: '' })
    expect(parseMessage('message')).toEqual({ to: '', name: '', text: '' })
  })

  it('says what’s missing as it’s typed', () => {
    const hint = (text: string, m = me, relay = true) => messageHint(parseMessage(text)!, m, relay)
    expect(hint('message max hi', me, false)).toBe('messages need the online relay')
    expect(hint('message max hi', { ...me, name: '' })).toMatch(/set your name in settings/)
    expect(hint('message')).toBe('message <name> <what to say>')
    expect(hint('message m hi')).toMatch(/2–20 letters/)
    expect(hint('message max')).toBe('then what to say to max')
    expect(hint('message max hi')).toBe('↵ sends to max')
  })

  it('says a name is nobody’s only when the relay said so', () => {
    const line = parseMessage('message zed hi')!
    expect(messageHint(line, me, true, false)).toBe('username not found')
    expect(messageHint(parseMessage('message zed')!, me, true, false)).toBe('username not found')
    expect(messageHint(line, me, true, null)).toBe('↵ sends to zed')
    expect(messageHint(line, me, true, true)).toBe('↵ sends to zed')
    expect(messageHint(parseMessage('message z hi')!, me, true, false)).toMatch(/2–20 letters/)
  })

  it('finds the name in the line to paint it', () => {
    expect(messageNameSpan('message Max hi')).toEqual({ start: 8, end: 11 })
    expect(messageNameSpan('  message  max')).toEqual({ start: 11, end: 14 })
    expect(messageNameSpan('message ')).toBeNull()
  })

  it('keeps who you sent to, the latest first, and completes them before the other accepted names', () => {
    let m = withRecent(withRecent(withRecent(me, 'Zoe'), 'bob'), 'zoe')
    expect(m.recent).toEqual(['zoe', 'bob'])
    expect(withRecent(m, 'x')).toBe(m)
    expect(messageContacts({ ...m, accepted: ['bob', 'max'] })).toEqual(['zoe', 'bob', 'max'])
    for (let i = 0; i < 20; i++) m = withRecent(m, `n${i}`)
    expect(m.recent).toHaveLength(RECENT_MAX)
    expect(m.recent[0]).toBe('n19')
    expect(sanitizeMessaging({ recent: ['Zoe', 'zoe', 'x', 'bob', 3] }).recent).toEqual(['zoe', 'bob'])
    expect(completionFor('message b', { contacts: messageContacts({ ...m, recent: ['bea', 'bob'], accepted: ['bo'] }) })).toBe('ea')
  })

  it('says how a send went without saying whether you’re accepted', () => {
    expect(sendStatusText('delivered', 'max')).toBe('sent to max')
    expect(sendStatusText('offline', 'max')).toMatch(/max isn’t on q calc right now · not sent/)
    expect(sendStatusText('unknown', 'max')).toMatch(/nobody’s called max/)
    expect(sendStatusText('network', 'max')).toMatch(/not sent/)
  })

  it('completes the command and the accepted names, and never what’s said', () => {
    const names = { contacts: ['max', 'maya'] }
    expect(completionFor('mes', names)).toBe('sage')
    expect(completionFor('message', names)).toBe('')
    expect(completionFor('message ma', names)).toBe('x')
    expect(completionFor('message may', names)).toBe('a')
    expect(completionFor('message zo', names)).toBe('')
    expect(completionFor('message max let’s go sq', names)).toBe('')
    expect(completionFor('message max 5 kilo', names)).toBe('')
    // a calculation still completes as before
    expect(completionFor('sq')).toBe('rt')
  })
})

describe('sending', () => {
  it('posts the sender’s name and key as plain text, and reads the relay’s status', async () => {
    const post = vi.fn(async () => ({ json: async () => ({ status: 'delivered' }) }))
    expect(await sendMessage(me, 'max', 'hi', post, 'https://relay.test')).toBe('delivered')
    expect(post).toHaveBeenCalledWith('https://relay.test/send', {
      method: 'POST',
      headers: { 'Content-Type': 'text/plain' },
      body: JSON.stringify({ from: 'ana', key: KEY, to: 'max', text: 'hi' }),
    })
  })

  it('says network when the relay can’t be reached or answers nonsense', async () => {
    const down = async () => {
      throw new Error('offline')
    }
    expect(await sendMessage(me, 'max', 'hi', down, 'https://relay.test')).toBe('network')
    expect(await sendMessage(me, 'max', 'hi', async () => ({ json: async () => ({ status: 'what' }) }), 'https://relay.test')).toBe('network')
    expect(await sendMessage(me, 'max', 'hi', down, '')).toBe('network')
  })

  it('looks a name up with the sender’s own name and key, and says nothing it wasn’t told', async () => {
    const answer = (status: unknown) => vi.fn(async () => ({ json: async () => ({ status }) }))
    const found = answer('found')
    expect(await lookupName(me, 'Max', found, 'https://relay.test')).toBe(true)
    expect(found).toHaveBeenCalledWith('https://relay.test/lookup', {
      method: 'POST',
      headers: { 'Content-Type': 'text/plain' },
      body: JSON.stringify({ from: 'ana', key: KEY, name: 'max' }),
    })
    expect(await lookupName(me, 'zed', answer('unknown'), 'https://relay.test')).toBe(false)
    for (const status of ['too-many', 'not-yours', 'bad', undefined]) {
      expect(await lookupName(me, 'zed', answer(status), 'https://relay.test')).toBeNull()
    }
    const down = vi.fn(async () => {
      throw new Error('offline')
    })
    expect(await lookupName(me, 'zed', down, 'https://relay.test')).toBeNull()
    const unasked = answer('found')
    expect(await lookupName({ ...me, key: '' }, 'zed', unasked, 'https://relay.test')).toBeNull()
    expect(await lookupName(me, 'z', unasked, 'https://relay.test')).toBeNull()
    expect(unasked).not.toHaveBeenCalled()
  })
})

class FakeSocket {
  readyState: 0 | 1 | 2 | 3 = 1
  sent: string[] = []
  closed = false
  onmessage: ((e: { data: unknown }) => void) | null = null
  onclose: (() => void) | null = null
  readonly url: string
  readonly protocols: string[]
  constructor(url: string, protocols: string[]) {
    this.url = url
    this.protocols = protocols
  }
  send(s: string) {
    this.sent.push(s)
  }
  close() {
    this.closed = true
  }
  hear(e: Record<string, unknown>) {
    this.onmessage?.({ data: '!' + JSON.stringify(e) })
  }
  drop() {
    this.readyState = 3
    this.onclose?.()
  }
}

describe('the inbox link', () => {
  beforeEach(() => vi.useFakeTimers())
  afterEach(() => vi.useRealTimers())

  const link = () => {
    const socks: FakeSocket[] = []
    const events: InboxEvent[] = []
    const l = new InboxLink(
      (e) => events.push(e),
      (url, protocols) => {
        const s = new FakeSocket(url, protocols)
        socks.push(s)
        return s
      },
      'wss://relay.test/',
    )
    return { l, socks, events }
  }

  it('opens the name’s inbox with its key as a protocol, never in the url', () => {
    const { l, socks, events } = link()
    l.start('ana', KEY)
    expect(socks[0]!.url).toBe('wss://relay.test/inbox/ana')
    expect(socks[0]!.protocols).toEqual(['qcalc-inbox', `k${KEY}`])
    socks[0]!.hear({ type: 'inbox', name: 'ana' })
    socks[0]!.hear({ type: 'message', from: 'Max', text: 'hello' })
    expect(events).toEqual([
      { type: 'state', state: 'ready' },
      { type: 'message', from: 'max', text: 'hello' },
    ])
  })

  it('tries again after a drop, waiting longer each time', () => {
    const { l, socks } = link()
    l.start('ana', KEY)
    socks[0]!.drop()
    vi.advanceTimersByTime(3000)
    expect(socks).toHaveLength(2)
    socks[1]!.drop()
    vi.advanceTimersByTime(3000)
    expect(socks).toHaveLength(2)
    vi.advanceTimersByTime(3000)
    expect(socks).toHaveLength(3)
  })

  it('stops for good when the name is taken', () => {
    const { l, socks, events } = link()
    l.start('ana', KEY)
    socks[0]!.hear({ type: 'closed', reason: 'taken' })
    socks[0]!.drop()
    vi.advanceTimersByTime(120_000)
    expect(socks).toHaveLength(1)
    expect(events).toEqual([{ type: 'state', state: 'taken' }])
  })

  it('closes when stopped, and does nothing without a name or a relay', () => {
    const { l, socks } = link()
    l.start('ana', KEY)
    l.stop()
    expect(socks[0]!.closed).toBe(true)
    l.start('', KEY)
    expect(socks).toHaveLength(1)
    const off = new InboxLink(() => {}, () => {
      throw new Error('opened')
    }, '')
    off.start('ana', KEY)
  })

  it('takes controls and direction overrides out of a message', () => {
    const frame = parseInboxFrame('!' + JSON.stringify({ type: 'message', from: 'max', text: 'hi\u0085\u202eana: x' }))
    expect(frame).toEqual({ type: 'message', from: 'max', text: 'hi ana: x' })
  })

  it('ignores frames that aren’t its own', () => {
    expect(parseInboxFrame('.game')).toBeNull()
    expect(parseInboxFrame('!{bad')).toBeNull()
    expect(parseInboxFrame('!' + JSON.stringify({ type: 'message', from: 'x', text: 'hi' }))).toBeNull()
    expect(parseInboxFrame('!' + JSON.stringify({ type: 'message', from: 'max', text: '' }))).toBeNull()
  })
})
