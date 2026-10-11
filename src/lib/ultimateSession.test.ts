import { describe, expect, it } from 'vitest'
import { cellIn, encodeUt } from './ultimate'
import { BOT_THINK_MS, UltimateSession, UT_SILENCE_MS } from './ultimateSession'

// two sessions wired to each other, with messages delivered when flush is called
function pair() {
  const toGuest: string[] = []
  const toHost: string[] = []
  const host = new UltimateSession('host', (t) => toGuest.push(t), 0)
  const guest = new UltimateSession('guest', (t) => toHost.push(t), 0)
  const flush = (now: number) => {
    while (toGuest.length || toHost.length) {
      for (const t of toGuest.splice(0)) guest.receive(t, now)
      for (const t of toHost.splice(0)) host.receive(t, now)
    }
  }
  return { host, guest, flush }
}

describe('ultimate session', () => {
  it('says hi both ways, then each move reaches the other board', () => {
    const { host, guest, flush } = pair()
    expect(host.view().started).toBe(false)
    flush(1)
    expect(host.view().started && guest.view().started).toBe(true)
    expect(guest.play(cellIn(0, 0), 2)).toBe(false)
    expect(host.play(cellIn(4, 2), 2)).toBe(true)
    flush(3)
    expect(guest.view().state.cells[cellIn(4, 2)]).toBe('host')
    expect(guest.view().state.forced).toBe(2)
    expect(guest.play(cellIn(2, 4), 4)).toBe(true)
    flush(5)
    expect(host.view().state.moves).toBe(2)
    expect(host.view().state.forced).toBe(4)
  })

  it('ignores a move that is stale, out of turn or made up', () => {
    const { host, flush } = pair()
    flush(1)
    // the host moves first, so a first move from the guest is out of turn
    host.receive(encodeUt({ t: 'utm', g: 0, n: 0, c: 5 }), 2)
    expect(host.view().state.moves).toBe(0)
    host.receive(encodeUt({ t: 'utm', g: 0, n: 3, c: 5 }), 2)
    host.receive(encodeUt({ t: 'utm', g: 1, n: 0, c: 5 }), 2)
    expect(host.view().state.moves).toBe(0)
  })

  it('ends when a friend opened another game, or went quiet', () => {
    const a = new UltimateSession('host', () => {}, 0)
    a.receive('{"t":"c4","v":1}', 1)
    expect(a.view().ended).toMatch(/connect 4/)
    const b = new UltimateSession('host', () => {}, 0)
    expect(b.tick(UT_SILENCE_MS + 1).ended).toBe('lost the connection')
  })

  it('solo: the computer answers after its pause, and a rematch is instant', () => {
    const s = new UltimateSession('guest', () => {}, 0, 'easy')
    expect(s.role).toBe('host')
    expect(s.play(cellIn(4, 4), 0)).toBe(true)
    expect(s.tick(BOT_THINK_MS - 1).state.moves).toBe(1)
    const v = s.tick(BOT_THINK_MS)
    expect(v.state.moves).toBe(2)
    expect(v.state.cells.slice(36, 45).filter(Boolean)).toHaveLength(2)
    expect(v.thinking).toBe(false)
  })
})
