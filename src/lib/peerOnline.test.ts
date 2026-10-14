import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { PeerEvent } from './peer'
import { OnlinePeer } from './peerOnline'

class FakeSocket {
  readyState: 0 | 1 | 2 | 3 = 1
  sent: string[] = []
  closed = false
  onmessage: ((e: { data: unknown }) => void) | null = null
  onclose: (() => void) | null = null
  readonly url: string
  constructor(url: string) {
    this.url = url
  }
  send(s: string) {
    this.sent.push(s)
  }
  closeCode: number | undefined
  close(code?: number) {
    this.closed = true
    this.closeCode = code
    this.readyState = 3
  }
  // from the relay
  hear(s: string) {
    this.onmessage?.({ data: s })
  }
  event(e: object) {
    this.hear('!' + JSON.stringify(e))
  }
  drop() {
    this.readyState = 3
    this.onclose?.()
  }
}

function setup() {
  const sockets: FakeSocket[] = []
  const peer = new OnlinePeer('wss://relay.test/', (url) => {
    const s = new FakeSocket(url)
    sockets.push(s)
    return s
  })
  const events: PeerEvent[] = []
  peer.subscribe((e) => events.push(e))
  return { peer, sockets, events, last: () => sockets[sockets.length - 1]! }
}

beforeEach(() => vi.useFakeTimers())
afterEach(() => vi.useRealTimers())

describe('OnlinePeer', () => {
  it('hosts: opens /host and passes on the code the relay hands out', () => {
    const { peer, events, last } = setup()
    peer.host({ game: 'pong', name: 'teal otter', locked: true })
    expect(last().url).toBe('wss://relay.test/host/pong?name=teal+otter&locked=1&resume=1')
    last().event({ type: 'hosting', code: '042017' })
    expect(events).toEqual([{ type: 'hosting', code: '042017' }])
  })

  it('drops a code of the wrong length', () => {
    const { peer, events, last } = setup()
    peer.host({ game: 'pong', name: 'teal otter', locked: true })
    last().event({ type: 'hosting', code: '0420' })
    expect(events).toEqual([])
  })

  it('hosts an open room with no code', () => {
    const { peer, events, last } = setup()
    peer.host({ game: 'connect4', name: 'amber fox', locked: false })
    expect(last().url).toBe('wss://relay.test/host/connect4?name=amber+fox&locked=0&resume=1')
    last().event({ type: 'hosting', code: '' })
    expect(events).toEqual([{ type: 'hosting', code: '' }])
  })

  it('joins by code and plays: messages go out with a dot and come back without one', () => {
    const { peer, events, last } = setup()
    peer.join('r1', '123456', 'jade owl')
    expect(last().url).toBe('wss://relay.test/join/r1?name=jade+owl&resume=1&code=123456')
    expect(events).toEqual([{ type: 'pairing' }])
    peer.send('too early')
    expect(last().sent).toEqual([])
    last().event({ type: 'open', role: 'guest', peer: '' })
    expect(events.at(-1)).toEqual({ type: 'open', role: 'guest', peer: 'friend' })
    peer.send('{"t":"hi"}')
    expect(last().sent).toEqual(['.{"t":"hi"}'])
    last().hear('.{"t":"s"}')
    expect(events.at(-1)).toEqual({ type: 'message', data: '{"t":"s"}' })
  })

  it('passes on why the relay refused', () => {
    const { peer, events, last } = setup()
    peer.join('r1', '000000', 'jade owl')
    last().event({ type: 'closed', reason: 'wrong-code' })
    expect(events.at(-1)).toEqual({ type: 'closed', reason: 'wrong-code' })
    expect(last().closed).toBe(true)
  })

  it('a connection that never reached the relay is a network failure; one that drops mid-game is lost', () => {
    const a = setup()
    a.peer.host({ game: 'pong', name: 'teal otter', locked: true })
    a.last().drop()
    expect(a.events.at(-1)).toEqual({ type: 'closed', reason: 'network' })

    const b = setup()
    b.peer.join('r1', '', 'jade owl')
    b.last().event({ type: 'open', role: 'guest', peer: '' })
    b.last().drop()
    expect(b.events.at(-1)).toEqual({ type: 'closed', reason: 'lost' })
  })

  it('the other side leaving reads as bye', () => {
    const { peer, events, last } = setup()
    peer.host({ game: 'pong', name: 'teal otter', locked: true })
    last().event({ type: 'open', role: 'host', peer: '' })
    last().event({ type: 'closed', reason: 'bye' })
    expect(events.at(-1)).toEqual({ type: 'closed', reason: 'bye' })
  })

  it('closing is quiet, and a stale socket is ignored after hosting again', () => {
    const { peer, events, sockets, last } = setup()
    peer.host({ game: 'pong', name: 'teal otter', locked: true })
    peer.close()
    expect(sockets[0]!.closed).toBe(true)
    peer.host({ game: 'pong', name: 'teal otter', locked: true })
    sockets[0]!.event({ type: 'hosting', code: '111111' })
    sockets[0]!.onclose?.()
    last().event({ type: 'hosting', code: '222222' })
    expect(events).toEqual([{ type: 'hosting', code: '222222' }])
  })

  it('pings while waiting, and gives up when the relay goes silent', () => {
    const { peer, events, last } = setup()
    peer.host({ game: 'pong', name: 'teal otter', locked: true })
    last().event({ type: 'hosting', code: '042017' })
    vi.advanceTimersByTime(10_000)
    expect(last().sent).toEqual(['ping'])
    last().hear('pong')
    vi.advanceTimersByTime(20_000)
    expect(events.at(-1)).toEqual({ type: 'hosting', code: '042017' })
    vi.advanceTimersByTime(10_000)
    expect(events.at(-1)).toEqual({ type: 'closed', reason: 'lost' })
  })

  it('fails a socket that never finishes connecting', () => {
    const { peer, events, last } = setup()
    peer.host({ game: 'pong', name: 'teal otter', locked: false })
    last().readyState = 0
    vi.advanceTimersByTime(10_000)
    expect(events.at(-1)).toEqual({ type: 'closed', reason: 'network' })
  })

  it('watches a game’s rooms on a socket of its own, and tries again when it drops', () => {
    const { peer, events, sockets, last } = setup()
    peer.watch('pong')
    expect(last().url).toBe('wss://relay.test/rooms/pong')
    last().event({ type: 'rooms', rooms: [{ id: 'a', name: 'teal otter', locked: false }, { id: 'b', name: 'x'.repeat(80), locked: true }, { id: 3 }] })
    expect(events.at(-1)).toEqual({
      type: 'rooms',
      rooms: [
        { id: 'a', name: 'teal otter', locked: false },
        { id: 'b', name: 'x'.repeat(24), locked: true },
      ],
    })
    // a game socket alongside doesn't disturb it
    peer.join('a', '', 'me')
    expect(sockets).toHaveLength(2)
    sockets[0]!.drop()
    expect(events.at(-1)).toEqual({ type: 'rooms', rooms: [], offline: true })
    vi.advanceTimersByTime(3_000)
    expect(sockets).toHaveLength(3)
    expect(last().url).toBe('wss://relay.test/rooms/pong')
    peer.unwatch()
    expect(last().closed).toBe(true)
    vi.advanceTimersByTime(60_000)
    expect(sockets).toHaveLength(3)
  })

  it('falls back to codes when the relay never sends a list (one from before room lists)', () => {
    const { peer, events, last } = setup()
    peer.watch('pong')
    last().drop()
    expect(events.at(-1)).toEqual({ type: 'rooms', rooms: [], offline: true, codeOnly: true })
    peer.host({ game: 'pong', name: 'teal otter', locked: false })
    expect(last().url).toBe('wss://relay.test/host?resume=1')
    peer.join('', '123456', 'teal otter')
    expect(last().url).toBe('wss://relay.test/join/123456?resume=1')
    // the relay is updated later: the list comes, and rooms work again
    vi.advanceTimersByTime(3_000)
    const watcher = last()
    expect(watcher.url).toBe('wss://relay.test/rooms/pong')
    watcher.event({ type: 'rooms', rooms: [] })
    expect(events.at(-1)).toEqual({ type: 'rooms', rooms: [] })
    peer.host({ game: 'pong', name: 'teal otter', locked: false })
    expect(last().url).toBe('wss://relay.test/host/pong?name=teal+otter&locked=0&resume=1')
  })

  it('a list that drops after it came is only offline', () => {
    const { peer, events, last } = setup()
    peer.watch('pong')
    last().event({ type: 'rooms', rooms: [] })
    last().drop()
    expect(events.at(-1)).toEqual({ type: 'rooms', rooms: [], offline: true })
  })

  it('ignores junk from the relay', () => {
    const { peer, events, last } = setup()
    peer.host({ game: 'pong', name: 'teal otter', locked: true })
    for (const junk of ['', '!', '!{', '!{"type":"open","role":"king"}', '?hello', '.before open']) last().hear(junk)
    expect(events).toEqual([])
  })
})

describe('OnlinePeer keeps its seat through a dropped link', () => {
  const TOKEN = '0123456789abcdef0123456789abcdef'

  function playing() {
    const t = setup()
    t.peer.join('room1', '', 'jade owl')
    t.last().event({ type: 'open', role: 'guest', peer: 'teal otter', room: 'room1', token: TOKEN })
    t.events.length = 0
    return t
  }

  it('reconnects to the same seat instead of ending the game', () => {
    const { peer, sockets, events, last } = playing()
    const first = last()
    first.drop()
    expect(events).toEqual([{ type: 'away', who: 'self' }])
    vi.advanceTimersByTime(0)
    expect(sockets).toHaveLength(2)
    expect(last().url).toBe(`wss://relay.test/back/room1?token=${TOKEN}`)
    peer.send('move 3')
    expect(last().sent).toEqual([])
    last().event({ type: 'open', role: 'guest', peer: 'teal otter', room: 'room1', token: TOKEN, resumed: true })
    expect(events).toEqual([
      { type: 'away', who: 'self' },
      { type: 'back', who: 'self' },
    ])
    // what was sent while away goes once the seat is back
    expect(last().sent).toEqual(['.move 3'])
  })

  it('closes a link gone quiet with the code the relay reads as reconnecting', () => {
    const { last } = playing()
    const first = last()
    first.hear('pong')
    vi.advanceTimersByTime(40_000)
    expect(first.closeCode).toBe(4000)
    expect(last()).not.toBe(first)
  })

  it('keeps trying, then gives up once the relay would have let the seat go', () => {
    const { sockets, events, last } = playing()
    last().drop()
    vi.advanceTimersByTime(0)
    for (let i = 0; i < 200 && events.every((e) => e.type !== 'closed'); i++) {
      last().drop()
      vi.advanceTimersByTime(5_000)
    }
    expect(sockets.length).toBeGreaterThan(10)
    expect(events.at(-1)).toEqual({ type: 'closed', reason: 'lost' })
  })

  it('ends when the relay says the friend left meanwhile', () => {
    const { events, last } = playing()
    last().drop()
    vi.advanceTimersByTime(0)
    last().event({ type: 'closed', reason: 'bye' })
    expect(events.at(-1)).toEqual({ type: 'closed', reason: 'bye' })
    const count = events.length
    vi.advanceTimersByTime(60_000)
    expect(events).toHaveLength(count)
  })

  it('passes on the other side stepping away and coming back', () => {
    const { events, last } = playing()
    last().event({ type: 'away' })
    last().event({ type: 'back' })
    expect(events).toEqual([
      { type: 'away', who: 'peer' },
      { type: 'back', who: 'peer' },
    ])
  })

  it('leaving on purpose closes with 1000, so the game ends for the friend', () => {
    const { peer, sockets, last } = playing()
    const first = last()
    peer.close()
    expect(first.closeCode).toBe(1000)
    vi.advanceTimersByTime(60_000)
    expect(sockets).toHaveLength(1)
  })

  it('hiding mid-game steps away: no retries until shown again, then the same seat', () => {
    const { peer, sockets, events, last } = playing()
    const first = last()
    peer.stepAway()
    expect(first.closeCode).toBe(4000)
    expect(events).toEqual([{ type: 'away', who: 'self' }])
    vi.advanceTimersByTime(30 * 60_000)
    expect(sockets).toHaveLength(1)
    peer.comeBack()
    expect(last().url).toBe(`wss://relay.test/back/room1?token=${TOKEN}`)
    last().event({ type: 'open', role: 'guest', peer: 'teal otter', room: 'room1', token: TOKEN, resumed: true })
    expect(events.at(-1)).toEqual({ type: 'back', who: 'self' })
  })

  it('coming back after the game ended says so', () => {
    const { peer, events, last } = playing()
    peer.stepAway()
    vi.advanceTimersByTime(30 * 60_000)
    peer.comeBack()
    last().event({ type: 'closed', reason: 'lost' })
    expect(events.at(-1)).toEqual({ type: 'closed', reason: 'lost' })
  })

  it('stepping away before a game starts does nothing', () => {
    const { peer, sockets, events, last } = setup()
    peer.host({ game: 'pong', name: 'teal otter', locked: false })
    peer.stepAway()
    peer.comeBack()
    expect(sockets).toHaveLength(1)
    expect(last().closed).toBe(false)
    expect(events).toEqual([])
  })

  it('a link that drops before pairing still just fails', () => {
    const { peer, events, last } = setup()
    peer.join('room1', '', 'jade owl')
    last().drop()
    expect(events.at(-1)).toEqual({ type: 'closed', reason: 'network' })
  })
})
