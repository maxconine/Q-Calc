import { afterEach, describe, expect, it } from 'vitest'
import { closeReasonText, isPairingCode, parsePeerEvent, type PeerEvent } from './peer'
import { DevPeer } from './peerDev'

describe('isPairingCode', () => {
  it('is exactly four ascii digits', () => {
    expect(isPairingCode('0420')).toBe(true)
    expect(isPairingCode('420')).toBe(false)
    expect(isPairingCode('04201')).toBe(false)
    expect(isPairingCode('04a0')).toBe(false)
    expect(isPairingCode('٠٤٢٠')).toBe(false)
  })
})

describe('parsePeerEvent', () => {
  it('reads each event the mac app sends', () => {
    expect(parsePeerEvent({ type: 'peers', peers: [{ id: 'a', name: 'A' }, { id: 1 }, null] })).toEqual({
      type: 'peers',
      peers: [{ id: 'a', name: 'A' }],
    })
    expect(parsePeerEvent({ type: 'hosting', code: '0007' })).toEqual({ type: 'hosting', code: '0007' })
    expect(parsePeerEvent({ type: 'hosting', code: '0007', renewed: true })).toEqual({ type: 'hosting', code: '0007', renewed: true })
    expect(parsePeerEvent({ type: 'open', role: 'guest', peer: 'Mac' })).toEqual({ type: 'open', role: 'guest', peer: 'Mac' })
    expect(parsePeerEvent({ type: 'message', data: '{}' })).toEqual({ type: 'message', data: '{}' })
    expect(parsePeerEvent({ type: 'closed', reason: 'wrong-code' })).toEqual({ type: 'closed', reason: 'wrong-code' })
    expect(parsePeerEvent({ type: 'closed', reason: 'weird' })).toEqual({ type: 'closed', reason: 'lost' })
    expect(parsePeerEvent({ type: 'denied' })).toEqual({ type: 'denied' })
    expect(parsePeerEvent({ type: 'pairing' })).toEqual({ type: 'pairing' })
  })

  it('drops anything malformed', () => {
    for (const raw of [null, 'x', 3, {}, { type: 'hosting', code: '12' }, { type: 'open', role: 'boss' }, { type: 'message', data: 5 }, { type: 'peers' }]) {
      expect(parsePeerEvent(raw)).toBeNull()
    }
  })

  it('has words for every close', () => {
    for (const r of ['wrong-code', 'busy', 'unreachable', 'lost', 'bye', 'denied', 'network'] as const) {
      expect(closeReasonText(r)).toBeTruthy()
    }
  })
})

describe('DevPeer', () => {
  const made: DevPeer[] = []
  const make = (name: string) => {
    const p = new DevPeer('qcalc-peer-test', name)
    made.push(p)
    const events: PeerEvent[] = []
    p.subscribe((e) => events.push(e))
    return { p, events }
  }
  const until = async (ok: () => boolean) => {
    for (let i = 0; i < 200 && !ok(); i++) await new Promise((r) => setTimeout(r, 5))
    expect(ok()).toBe(true)
  }
  const last = <T extends PeerEvent['type']>(events: PeerEvent[], type: T) =>
    [...events].reverse().find((e) => e.type === type) as Extract<PeerEvent, { type: T }> | undefined

  afterEach(() => {
    for (const p of made.splice(0)) p.dispose()
  })

  it('finds a host, refuses a wrong code, then pairs and carries messages', async () => {
    const host = make('left')
    const guest = make('right')
    host.p.host()
    await until(() => Boolean(last(host.events, 'hosting')))
    const code = last(host.events, 'hosting')!.code
    guest.p.discover()
    await until(() => (last(guest.events, 'peers')?.peers.length ?? 0) > 0)
    const found = last(guest.events, 'peers')!.peers[0]!
    expect(found.name).toBe('left')

    guest.p.join(found.id, code === '0000' ? '0001' : '0000')
    await until(() => Boolean(last(guest.events, 'closed')))
    expect(last(guest.events, 'closed')?.reason).toBe('wrong-code')

    guest.p.join(found.id, code)
    await until(() => Boolean(last(guest.events, 'open')) && Boolean(last(host.events, 'open')))
    expect(last(host.events, 'open')).toEqual({ type: 'open', role: 'host', peer: 'right' })
    expect(last(guest.events, 'open')).toEqual({ type: 'open', role: 'guest', peer: 'left' })

    guest.p.send('ping')
    host.p.send('pong')
    await until(() => Boolean(last(host.events, 'message')) && Boolean(last(guest.events, 'message')))
    expect(last(host.events, 'message')?.data).toBe('ping')
    expect(last(guest.events, 'message')?.data).toBe('pong')

    guest.p.close()
    await until(() => Boolean(last(host.events, 'closed')))
    expect(last(host.events, 'closed')?.reason).toBe('bye')
  })

  it('shows a new code after three wrong ones, and turns away a second guest', async () => {
    const host = make('host')
    const a = make('a')
    const b = make('b')
    host.p.host()
    await until(() => Boolean(last(host.events, 'hosting')))
    const first = last(host.events, 'hosting')!.code
    const wrong = first === '0000' ? '0001' : '0000'
    for (let i = 0; i < 3; i++) {
      a.events.length = 0
      a.p.join(host.p.id, wrong)
      await until(() => Boolean(last(a.events, 'closed')))
    }
    await until(() => last(host.events, 'hosting')?.renewed === true)
    const code = last(host.events, 'hosting')!.code
    a.p.join(host.p.id, code)
    await until(() => Boolean(last(a.events, 'open')))
    b.p.join(host.p.id, code)
    await until(() => Boolean(last(b.events, 'closed')))
    expect(last(b.events, 'closed')?.reason).toBe('busy')
  })
})
