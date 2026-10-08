import { describe, expect, it } from 'vitest'
import { COURT_H, decodePong, encodePong, PADDLE_H, SILENCE_MS, STATE_HZ, WIN_SCORE } from './pong'
import { PongSession, type SessionView } from './pongSession'

// two sessions joined by a queue that delivers after `lag` ms, stepped at 60fps
function pair(
  opts: { lag?: number; guestVersion?: number; dropFirst?: number; fps?: number; drop?: (to: 'host' | 'guest', text: string) => boolean } = {},
) {
  const lag = opts.lag ?? 5
  let now = 1000
  let dropped = 0
  const queue: Array<{ at: number; to: 'host' | 'guest'; text: string }> = []
  const sent: Array<{ at: number; to: 'host' | 'guest'; text: string }> = []
  const route = (to: 'host' | 'guest') => (text: string) => {
    sent.push({ at: now, to, text })
    if (dropped < (opts.dropFirst ?? 0)) {
      dropped++
      return
    }
    if (opts.drop?.(to, text)) return
    queue.push({ at: now + lag, to, text })
  }
  // a fixed sequence: serves go out at an angle, so still paddles miss
  let r = 0
  const rand = () => [0.9, 0.1, 0.8, 0.2][r++ % 4]!
  const host = new PongSession('host', route('guest'), now, rand)
  const guestSend = route('host')
  const guest = new PongSession(
    'guest',
    opts.guestVersion == null ? guestSend : (t) => guestSend(t.replace(/"v":1/, `"v":${opts.guestVersion}`)),
    now,
    rand,
  )
  let hv: SessionView = host.tick(now)
  let gv: SessionView = guest.tick(now)
  const run = (ms: number, cut = false) => {
    const end = now + ms
    while (now < end) {
      now += 1000 / (opts.fps ?? 60)
      if (!cut) {
        for (const m of queue.splice(0).filter((m) => m.at <= now || (queue.push(m), false))) {
          ;(m.to === 'host' ? host : guest).receive(m.text, now)
        }
      } else queue.length = 0
      hv = host.tick(now)
      gv = guest.tick(now)
    }
  }
  return { host, guest, run, sent, views: () => ({ hv, gv }) }
}

describe('PongSession', () => {
  it('waits for both courts, then serves', () => {
    const g = pair({ dropFirst: 2 })
    expect(g.views().hv.started).toBe(false)
    g.run(1000)
    // the first hellos were lost; the resent ones got through
    expect(g.views().hv.started).toBe(true)
    expect(g.views().gv.started).toBe(true)
    g.run(1000)
    expect(g.views().hv.snap?.phase).toBe('play')
  })

  it('the guest sees the host ball, a little behind, and the host sees the guest paddle', () => {
    const g = pair()
    g.run(1500)
    const { hv, gv } = g.views()
    expect(gv.snap?.phase).toBe('play')
    // 80ms behind, the ball has moved less far on the guest's screen, but in the same direction
    const dx = Math.abs(hv.snap!.ball.x - gv.snap!.ball.x)
    expect(dx).toBeGreaterThan(5)
    expect(dx).toBeLessThan(60)
    g.guest.key('up', true)
    g.run(1000)
    expect(g.views().gv.snap?.paddles[1]).toBe(PADDLE_H / 2)
    expect(g.views().hv.snap?.paddles[1]).toBe(PADDLE_H / 2)
  })

  it('plays to the win, then rematches when both ask', () => {
    const g = pair()
    // both paddles out of the way, so every serve scores
    g.host.key('up', true)
    g.guest.key('down', true)
    g.run(60_000)
    const { hv, gv } = g.views()
    expect(hv.snap?.phase).toBe('over')
    expect(Math.max(...hv.snap!.score)).toBe(WIN_SCORE)
    expect(gv.snap?.phase).toBe('over')
    expect(gv.snap?.winner).toBe(hv.snap?.winner)

    g.guest.rematch()
    g.run(200)
    expect(g.views().hv.again).toEqual({ mine: false, theirs: true })
    expect(g.views().hv.snap?.phase).toBe('over')
    g.host.rematch()
    g.run(300)
    expect(g.views().hv.snap?.score).toEqual([0, 0])
    expect(g.views().gv.snap?.score).toEqual([0, 0])
    expect(g.views().gv.again).toEqual({ mine: false, theirs: false })
  })

  it('rematch does nothing mid game', () => {
    const g = pair()
    g.run(1500)
    g.guest.rematch()
    g.run(200)
    expect(g.views().hv.again.theirs).toBe(false)
  })

  it('ends when the other side goes quiet', () => {
    const g = pair()
    g.run(1500)
    g.run(SILENCE_MS + 500, true)
    expect(g.views().hv.ended).toBe('lost the connection')
    expect(g.views().gv.ended).toBe('lost the connection')
  })

  it('ends when the other side says bye', () => {
    const g = pair()
    g.run(1500)
    g.host.receive(encodePong({ t: 'bye' }), 3000)
    g.run(100)
    expect(g.views().hv.ended).toBe('your friend left')
  })

  it('refuses a different version', () => {
    const g = pair({ guestVersion: 2 })
    g.run(500)
    expect(g.views().hv.ended).toBe('update both Q Calcs to play each other')
    expect(g.views().gv.ended).toBe('your friend left')
  })

  it('starts when the guest court came up first and missed nothing but its own hi', () => {
    // the guest's page mounted first: its hi went to a host page that wasn't listening yet
    let guestHis = 0
    const g = pair({ drop: (to, text) => to === 'host' && text.includes('"hi"') && guestHis++ === 0 })
    g.run(2000)
    expect(g.views().hv.started).toBe(true)
    expect(g.views().hv.snap?.phase).toBe('play')
  })

  it('stops saying hi once both sides have heard each other', () => {
    const g = pair({ dropFirst: 1 })
    g.run(3000)
    const lateHis = g.sent.filter((m) => m.at > 2500 && m.text.includes('"hi"'))
    expect(lateHis).toEqual([])
  })

  it.each([60, 120, 144])('sends state about STATE_HZ times a second at %i fps', (fps) => {
    const g = pair({ fps })
    g.run(3000)
    const states = g.sent.filter((m) => m.to === 'guest' && m.at > 2000 && m.at <= 3000 && m.text.startsWith('{"t":"s"'))
    expect(states.length).toBeGreaterThanOrEqual(STATE_HZ - 2)
    expect(states.length).toBeLessThanOrEqual(STATE_HZ + 2)
  })

  it('a rematch ask from mid game does not carry over to the end', () => {
    const g = pair()
    g.run(1500)
    g.host.receive(encodePong({ t: 'again' }), 2500)
    g.host.key('up', true)
    g.guest.key('down', true)
    g.run(60_000)
    expect(g.views().hv.snap?.phase).toBe('over')
    expect(g.views().hv.again.theirs).toBe(false)
    g.host.rematch()
    g.run(200)
    expect(g.views().hv.snap?.phase).toBe('over')
  })

  it('the guest paddle message stays on the court', () => {
    const sent: string[] = []
    const s = new PongSession('guest', (t) => sent.push(t), 0)
    s.key('down', true)
    for (let t = 16; t < 2000; t += 16) s.tick(t)
    const last = decodePong(sent[sent.length - 1]!)
    expect(last).toEqual({ t: 'p', y: COURT_H - PADDLE_H / 2 })
    expect(encodePong({ t: 'p', y: 1 })).toBe('{"t":"p","y":1}')
  })
})
