import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { PeerEvent } from './peer'
import { OnlinePeer } from './peerOnline'

class FakeSocket {
  readyState = 1
  sent: string[] = []
  closed = false
  onmessage: ((e: { data: unknown }) => void) | null = null
  onclose: (() => void) | null = null
  constructor(readonly url: string) {}
  send(s: string) {
    this.sent.push(s)
  }
  close() {
    this.closed = true
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
    peer.host()
    expect(last().url).toBe('wss://relay.test/host')
    last().event({ type: 'hosting', code: '042017' })
    expect(events).toEqual([{ type: 'hosting', code: '042017' }])
  })

  it('drops a code of the wrong length', () => {
    const { peer, events, last } = setup()
    peer.host()
    last().event({ type: 'hosting', code: '0420' })
    expect(events).toEqual([])
  })

  it('joins by code and plays: messages go out with a dot and come back without one', () => {
    const { peer, events, last } = setup()
    peer.join('', '123456')
    expect(last().url).toBe('wss://relay.test/join/123456')
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
    peer.join('', '000000')
    last().event({ type: 'closed', reason: 'wrong-code' })
    expect(events.at(-1)).toEqual({ type: 'closed', reason: 'wrong-code' })
    expect(last().closed).toBe(true)
  })

  it('a connection that never reached the relay is a network failure; one that drops mid-game is lost', () => {
    const a = setup()
    a.peer.host()
    a.last().drop()
    expect(a.events.at(-1)).toEqual({ type: 'closed', reason: 'network' })

    const b = setup()
    b.peer.join('', '123456')
    b.last().event({ type: 'open', role: 'guest', peer: '' })
    b.last().drop()
    expect(b.events.at(-1)).toEqual({ type: 'closed', reason: 'lost' })
  })

  it('the other side leaving reads as bye', () => {
    const { peer, events, last } = setup()
    peer.host()
    last().event({ type: 'open', role: 'host', peer: '' })
    last().event({ type: 'closed', reason: 'bye' })
    expect(events.at(-1)).toEqual({ type: 'closed', reason: 'bye' })
  })

  it('closing is quiet, and a stale socket is ignored after hosting again', () => {
    const { peer, events, sockets, last } = setup()
    peer.host()
    peer.close()
    expect(sockets[0]!.closed).toBe(true)
    peer.host()
    sockets[0]!.event({ type: 'hosting', code: '111111' })
    sockets[0]!.onclose?.()
    last().event({ type: 'hosting', code: '222222' })
    expect(events).toEqual([{ type: 'hosting', code: '222222' }])
  })

  it('pings while waiting, and gives up when the relay goes silent', () => {
    const { peer, events, last } = setup()
    peer.host()
    last().event({ type: 'hosting', code: '042017' })
    vi.advanceTimersByTime(10_000)
    expect(last().sent).toEqual(['ping'])
    last().hear('pong')
    vi.advanceTimersByTime(20_000)
    expect(events.at(-1)).toEqual({ type: 'hosting', code: '042017' })
    vi.advanceTimersByTime(10_000)
    expect(events.at(-1)).toEqual({ type: 'closed', reason: 'lost' })
  })

  it('ignores junk from the relay', () => {
    const { peer, events, last } = setup()
    peer.host()
    for (const junk of ['', '!', '!{', '!{"type":"open","role":"king"}', '?hello', '.before open']) last().hear(junk)
    expect(events).toEqual([])
  })
})
