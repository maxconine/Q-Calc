import { afterEach, describe, expect, it } from 'vitest'
import { closeReasonText, isPairingCode, parsePeerEvent, type PeerEvent } from './peer'
import { DevPeer } from './peerDev'

describe('isPairingCode', () => {
  it('is exactly that many ascii digits', () => {
    expect(isPairingCode('0420', 4)).toBe(true)
    expect(isPairingCode('420', 4)).toBe(false)
    expect(isPairingCode('04201', 4)).toBe(false)
    expect(isPairingCode('04a0', 4)).toBe(false)
    expect(isPairingCode('٠٤٢٠', 4)).toBe(false)
  })
})

describe('parsePeerEvent', () => {
  const parse = (raw: unknown) => parsePeerEvent(raw, 4)

  it('reads each event the relay sends', () => {
    expect(parse({ type: 'rooms', rooms: [{ id: 'a', name: 'teal otter' }, { id: 1 }, null] })).toEqual({
      type: 'rooms',
      rooms: [{ id: 'a', name: 'teal otter', locked: true }],
    })
    expect(parse({ type: 'rooms', rooms: [{ id: 'r', name: '', locked: false }] })).toEqual({
      type: 'rooms',
      rooms: [{ id: 'r', name: 'friend', locked: false }],
    })
    expect(parse({ type: 'hosting', code: '' })).toEqual({ type: 'hosting', code: '' })
    expect(parse({ type: 'hosting', code: '0007' })).toEqual({ type: 'hosting', code: '0007' })
    expect(parse({ type: 'hosting', code: '0007', renewed: true })).toEqual({ type: 'hosting', code: '0007', renewed: true })
    expect(parse({ type: 'open', role: 'guest', peer: 'teal otter' })).toEqual({ type: 'open', role: 'guest', peer: 'teal otter' })
    expect(parse({ type: 'message', data: '{}' })).toEqual({ type: 'message', data: '{}' })
    expect(parse({ type: 'closed', reason: 'wrong-code' })).toEqual({ type: 'closed', reason: 'wrong-code' })
    expect(parse({ type: 'closed', reason: 'weird' })).toEqual({ type: 'closed', reason: 'lost' })
    expect(parse({ type: 'pairing' })).toEqual({ type: 'pairing' })
  })

  it('drops anything malformed', () => {
    for (const raw of [null, 'x', 3, {}, { type: 'hosting', code: '12' }, { type: 'open', role: 'boss' }, { type: 'message', data: 5 }, { type: 'rooms' }]) {
      expect(parse(raw)).toBeNull()
    }
  })

  it('has words for every close', () => {
    for (const r of ['wrong-code', 'busy', 'unreachable', 'lost', 'bye', 'network', 'gone', 'expired', 'too-many-tries'] as const) {
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

  it('lists a locked room, refuses a wrong code, then pairs and carries messages', async () => {
    const host = make('left')
    const guest = make('right')
    host.p.host({ game: 'pong', name: 'teal otter', locked: true })
    await until(() => Boolean(last(host.events, 'hosting')))
    const code = last(host.events, 'hosting')!.code
    expect(code).toMatch(/^[0-9]{4}$/)
    guest.p.watch('pong')
    await until(() => (last(guest.events, 'rooms')?.rooms.length ?? 0) > 0)
    const found = last(guest.events, 'rooms')!.rooms[0]!
    expect(found).toMatchObject({ name: 'teal otter', locked: true })

    guest.p.join(found.id, code === '0000' ? '0001' : '0000', 'jade owl')
    await until(() => Boolean(last(guest.events, 'closed')))
    expect(last(guest.events, 'closed')?.reason).toBe('wrong-code')

    guest.p.join(found.id, code, 'jade owl')
    await until(() => Boolean(last(guest.events, 'open')) && Boolean(last(host.events, 'open')))
    expect(last(host.events, 'open')).toEqual({ type: 'open', role: 'host', peer: 'jade owl' })
    expect(last(guest.events, 'open')).toEqual({ type: 'open', role: 'guest', peer: 'teal otter' })

    guest.p.send('ping')
    host.p.send('pong')
    await until(() => Boolean(last(host.events, 'message')) && Boolean(last(guest.events, 'message')))
    expect(last(host.events, 'message')?.data).toBe('ping')
    expect(last(guest.events, 'message')?.data).toBe('pong')

    guest.p.close()
    await until(() => Boolean(last(host.events, 'closed')))
    expect(last(host.events, 'closed')?.reason).toBe('bye')
  })

  it('joins an open room without a code, and lists only the game it watches', async () => {
    const pong = make('a')
    const c4 = make('b')
    const guest = make('c')
    pong.p.host({ game: 'pong', name: 'amber fox', locked: false })
    c4.p.host({ game: 'connect4', name: 'plum newt', locked: false })
    await until(() => last(pong.events, 'hosting')?.code === '')
    guest.p.watch('pong')
    await until(() => (last(guest.events, 'rooms')?.rooms.length ?? 0) > 0)
    await new Promise((r) => setTimeout(r, 30))
    expect(last(guest.events, 'rooms')!.rooms).toEqual([{ id: pong.p.id, name: 'amber fox', locked: false }])
    guest.p.join(pong.p.id, '', 'jade owl')
    await until(() => Boolean(last(guest.events, 'open')))
  })

  it('shows a new code after three wrong ones, and turns away a second guest', async () => {
    const host = make('host')
    const a = make('a')
    const b = make('b')
    host.p.host({ game: 'pong', name: 'host', locked: true })
    await until(() => Boolean(last(host.events, 'hosting')))
    const first = last(host.events, 'hosting')!.code
    const wrong = first === '0000' ? '0001' : '0000'
    for (let i = 0; i < 3; i++) {
      a.events.length = 0
      a.p.join(host.p.id, wrong, 'a')
      await until(() => Boolean(last(a.events, 'closed')))
    }
    await until(() => last(host.events, 'hosting')?.renewed === true)
    const code = last(host.events, 'hosting')!.code
    a.p.join(host.p.id, code, 'a')
    await until(() => Boolean(last(a.events, 'open')))
    b.p.join(host.p.id, code, 'b')
    await until(() => Boolean(last(b.events, 'closed')))
    expect(last(b.events, 'closed')?.reason).toBe('busy')
  })
})
