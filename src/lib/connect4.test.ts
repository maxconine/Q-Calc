import { describe, expect, it } from 'vitest'
import {
  C4_VERSION,
  canDrop,
  cellAt,
  CELLS,
  COLS,
  decodeC4,
  drop,
  encodeC4,
  firstFor,
  isConnect4Command,
  isOver,
  landingRow,
  lineThrough,
  newBoard,
  ROWS,
  type C4Msg,
  type C4State,
  type Side,
} from './connect4'
import { C4_SILENCE_MS, Connect4Session, KEEP_MS, type C4View } from './connect4Session'
import { encodePong } from './pong'
import { gameCommand, gameHint } from './games'

// plays columns in turn from the host, failing loudly on an illegal one
function play(cols: number[], s: C4State = newBoard('host')): C4State {
  for (const c of cols) {
    const next = drop(s, c, s.turn)
    if (!next) throw new Error(`column ${c} refused after ${s.moves} moves`)
    s = next
  }
  return s
}

// a full board with no four in a row: columns filled in pairs so colours stack in twos
const DRAW = [0, 1, 0, 1, 0, 1, 1, 0, 1, 0, 1, 0, 2, 3, 2, 3, 2, 3, 3, 2, 3, 2, 3, 2, 4, 5, 4, 5, 4, 5, 5, 4, 5, 4, 5, 4, 6, 6, 6, 6, 6, 6]

describe('isConnect4Command', () => {
  it('takes the usual spellings', () => {
    for (const s of ['connect 4', 'connect4', 'Connect 4', '  connect four ', 'connect-4', 'connectfour', 'CONNECT  4']) {
      expect(isConnect4Command(s), s).toBe(true)
    }
  })

  it('is nothing else', () => {
    for (const s of ['connect', 'connect 5', 'connect 4 now', 'c4', '4', 'pong', 'connect 44']) {
      expect(isConnect4Command(s), s).toBe(false)
    }
  })

  it('sits beside pong in the bar', () => {
    expect(gameCommand('pong')).toBe('pong')
    expect(gameCommand('connect 4')).toBe('connect4')
    expect(gameCommand('1+1')).toBe(null)
    expect(gameHint('connect4', true)).toBe('↵ play connect 4')
    expect(gameHint('connect4', false)).toBe('connect 4 needs Q Calc for Mac')
    expect(gameHint('pong', false)).toBe('pong needs Q Calc for Mac')
  })
})

describe('board', () => {
  it('starts empty with the given side to play', () => {
    const s = newBoard('guest')
    expect(s.board).toHaveLength(COLS * ROWS)
    expect(s.board.every((c) => c === null)).toBe(true)
    expect(s.turn).toBe('guest')
    expect(isOver(s)).toBe(false)
  })

  it('host goes first in game 0, then sides alternate', () => {
    expect([0, 1, 2, 3].map(firstFor)).toEqual(['host', 'guest', 'host', 'guest'])
  })

  it('drops to the lowest free row and passes the turn', () => {
    let s = drop(newBoard('host'), 3, 'host')!
    expect(s.board[cellAt(3, 0)]).toBe('host')
    expect(s.turn).toBe('guest')
    expect(s.last).toBe(cellAt(3, 0))
    s = drop(s, 3, 'guest')!
    expect(s.board[cellAt(3, 1)]).toBe('guest')
    expect(landingRow(s, 3)).toBe(2)
    expect(s.moves).toBe(2)
  })

  it('refuses the wrong side, a full column and columns off the board', () => {
    const s = newBoard('host')
    expect(drop(s, 0, 'guest')).toBe(null)
    expect(drop(s, -1, 'host')).toBe(null)
    expect(drop(s, COLS, 'host')).toBe(null)
    expect(drop(s, 2.5, 'host')).toBe(null)
    expect(drop(s, Number.NaN, 'host')).toBe(null)
    const full = play([0, 0, 0, 0, 0, 0])
    expect(landingRow(full, 0)).toBe(-1)
    expect(canDrop(full, 0, full.turn)).toBe(false)
    expect(drop(full, 0, full.turn)).toBe(null)
    // the input board is never changed
    expect(s.board.every((c) => c === null)).toBe(true)
  })
})

describe('wins', () => {
  it('horizontal', () => {
    const s = play([0, 0, 1, 1, 2, 2, 3])
    expect(s.winner).toBe('host')
    expect(s.line).toEqual([0, 1, 2, 3].map((c) => cellAt(c, 0)))
    expect(isOver(s)).toBe(true)
  })

  it('vertical', () => {
    const s = play([0, 1, 2, 1, 2, 1, 2, 1])
    expect(s.winner).toBe('guest')
    expect(s.line).toEqual([0, 1, 2, 3].map((r) => cellAt(1, r)))
  })

  it('rising diagonal', () => {
    // host builds 0,0 1,1 2,2 3,3
    const s = play([0, 1, 1, 2, 2, 3, 2, 3, 3, 6, 3])
    expect(s.winner).toBe('host')
    expect(s.line).toEqual([cellAt(0, 0), cellAt(1, 1), cellAt(2, 2), cellAt(3, 3)])
  })

  it('falling diagonal', () => {
    // host builds 3,0 2,1 1,2 0,3
    const s = play([3, 2, 2, 1, 1, 0, 1, 0, 0, 6, 0])
    expect(s.winner).toBe('host')
    expect(s.line).toEqual([cellAt(3, 0), cellAt(2, 1), cellAt(1, 2), cellAt(0, 3)])
  })

  it('a disc filling a gap wins, and a run of five counts whole', () => {
    // host: 0 1 _ 3 4 on the bottom row, then 2
    const s = play([0, 0, 1, 1, 3, 3, 4, 4, 2])
    expect(s.winner).toBe('host')
    expect(s.line).toHaveLength(5)
  })

  it('three is not a win, and nothing more drops once the game is won', () => {
    const three = play([0, 0, 1, 1, 2])
    expect(three.winner).toBeUndefined()
    const won = play([0, 0, 1, 1, 2, 2, 3])
    expect(drop(won, 5, 'guest')).toBe(null)
    expect(drop(won, 5, 'host')).toBe(null)
  })

  it('lineThrough is empty on an empty cell', () => {
    expect(lineThrough(newBoard().board, 0, 0)).toEqual([])
  })
})

describe('draw', () => {
  it('a full board with no four is a draw', () => {
    const s = play(DRAW)
    expect(s.moves).toBe(CELLS)
    expect(s.winner).toBeUndefined()
    expect(s.draw).toBe(true)
    expect(isOver(s)).toBe(true)
    for (let c = 0; c < COLS; c++) expect(landingRow(s, c)).toBe(-1)
  })
})

describe('wire', () => {
  it('round-trips every message', () => {
    const msgs: C4Msg[] = [
      { t: 'c4', v: C4_VERSION },
      { t: 'c4', v: C4_VERSION, ok: true },
      { t: 'c4m', g: 2, n: 5, c: 6 },
      { t: 'c4a', g: 3 },
      { t: 'c4k' },
      { t: 'bye' },
    ]
    for (const m of msgs) expect(decodeC4(encodeC4(m))).toEqual(m)
  })

  it('is short', () => {
    expect(encodeC4({ t: 'c4m', g: 0, n: 10, c: 3 }).length).toBeLessThan(40)
  })

  it('knows pong’s hello, and pong ignores connect 4', () => {
    expect(decodeC4(encodePong({ t: 'hi', v: 1 }))).toEqual({ t: 'pong' })
  })

  it('rejects junk', () => {
    const junk = [
      '',
      'nope',
      '{',
      'null',
      '42',
      '"c4m"',
      '[]',
      '[{"t":"c4k"}]',
      '{}',
      '{"t":"who"}',
      '{"t":"c4"}',
      '{"t":"c4","v":"1"}',
      '{"t":"c4m"}',
      '{"t":"c4m","g":0,"n":0}',
      '{"t":"c4m","g":0,"n":0,"c":7}',
      '{"t":"c4m","g":0,"n":0,"c":-1}',
      '{"t":"c4m","g":0,"n":0,"c":1.5}',
      '{"t":"c4m","g":0,"n":0,"c":"3"}',
      '{"t":"c4m","g":0,"n":0,"c":null}',
      '{"t":"c4m","g":-1,"n":0,"c":3}',
      '{"t":"c4m","g":0,"n":42,"c":3}',
      '{"t":"c4m","g":0,"n":1e9,"c":3}',
      '{"t":"c4m","g":0.5,"n":0,"c":3}',
      '{"t":"c4a"}',
      '{"t":"c4a","g":"0"}',
      `{"t":"c4k","pad":"${'x'.repeat(300)}"}`,
    ]
    for (const j of junk) expect(decodeC4(j), j).toBe(null)
  })
})

// two sessions joined by a queue that delivers after `lag` ms
function pair(opts: { lag?: number; guestVersion?: number; drop?: (to: Side, text: string) => boolean } = {}) {
  const lag = opts.lag ?? 5
  let now = 1000
  const queue: Array<{ at: number; to: Side; text: string }> = []
  const route = (to: Side) => (text: string) => {
    if (opts.drop?.(to, text)) return
    queue.push({ at: now + lag, to, text })
  }
  const host = new Connect4Session('host', route('guest'), now)
  const guestSend = route('host')
  const guest = new Connect4Session(
    'guest',
    opts.guestVersion == null ? guestSend : (t) => guestSend(t.replace(`"v":${C4_VERSION}`, `"v":${opts.guestVersion}`)),
    now,
  )
  let hv: C4View = host.tick(now)
  let gv: C4View = guest.tick(now)
  const run = (ms: number, cut = false) => {
    const end = now + ms
    while (now < end) {
      now += 50
      if (cut) queue.length = 0
      for (const m of queue.splice(0).filter((m) => m.at <= now || (queue.push(m), false))) {
        ;(m.to === 'host' ? host : guest).receive(m.text, now)
      }
      hv = host.tick(now)
      gv = guest.tick(now)
    }
  }
  const at = () => now
  // plays cols alternately from whoever's turn it is, letting each move arrive
  const moves = (cols: number[]) => {
    for (const c of cols) {
      const s = host.view().state
      const who = s.turn === 'host' ? host : guest
      expect(who.play(c, now), `column ${c}`).toBe(true)
      run(lag + 100)
    }
  }
  return { host, guest, run, at, moves, views: () => ({ hv, gv }) }
}

describe('Connect4Session', () => {
  it('waits for both sides to say hi before anyone can move', () => {
    const g = pair({ lag: 500 })
    expect(g.host.play(3, g.at())).toBe(false)
    g.run(1200)
    expect(g.views().hv.started).toBe(true)
    expect(g.views().gv.started).toBe(true)
  })

  it('starts even when the first hi was lost', () => {
    let lost = 0
    const g = pair({ drop: (to, text) => to === 'host' && text.includes('"c4"') && lost++ === 0 })
    g.run(1000)
    expect(g.views().hv.started).toBe(true)
    expect(g.views().gv.started).toBe(true)
  })

  it('the host goes first, moves show on both boards, and the guest can’t move out of turn', () => {
    const g = pair()
    g.run(200)
    expect(g.guest.play(0, g.at())).toBe(false)
    expect(g.host.play(3, g.at())).toBe(true)
    // twice in a row is out of turn
    expect(g.host.play(3, g.at())).toBe(false)
    g.run(100)
    expect(g.views().gv.state.board[cellAt(3, 0)]).toBe('host')
    expect(g.views().gv.state.turn).toBe('guest')
    expect(g.guest.play(3, g.at())).toBe(true)
    g.run(100)
    expect(g.views().hv.state.board).toEqual(g.views().gv.state.board)
    expect(g.views().hv.state.turn).toBe('host')
  })

  it('rejects moves from the peer that are out of turn, stale, from another game or into a full column', () => {
    const g = pair()
    g.run(200)
    const before = g.host.view()
    // the guest claiming the first move
    g.host.receive(encodeC4({ t: 'c4m', g: 0, n: 0, c: 2 }), g.at())
    // a move from a game that hasn't happened
    g.host.receive(encodeC4({ t: 'c4m', g: 1, n: 0, c: 2 }), g.at())
    // a move numbered ahead
    g.host.receive(encodeC4({ t: 'c4m', g: 0, n: 3, c: 2 }), g.at())
    g.host.receive('{"t":"c4m","g":0,"n":0,"c":9}', g.at())
    expect(g.host.view().state).toBe(before.state)
    expect(g.host.view().rev).toBe(before.rev)

    g.moves([0, 0, 0, 0, 0, 0])
    const full = g.host.view().state
    expect(full.turn).toBe('host')
    expect(g.host.play(0, g.at())).toBe(false)
    g.host.play(1, g.at())
    // the guest's move 7 into the full column
    g.host.receive(encodeC4({ t: 'c4m', g: 0, n: 7, c: 0 }), g.at())
    expect(g.host.view().state.moves).toBe(7)
    expect(g.host.view().state.turn).toBe('guest')
    // and a replay of an old one
    g.host.receive(encodeC4({ t: 'c4m', g: 0, n: 1, c: 4 }), g.at())
    expect(g.host.view().state.moves).toBe(7)
  })

  it('plays to a win on both sides, counts it, and rematches with the guest going first', () => {
    const g = pair()
    g.run(200)
    g.moves([0, 0, 1, 1, 2, 2, 3])
    const { hv, gv } = g.views()
    expect(hv.state.winner).toBe('host')
    expect(gv.state.winner).toBe('host')
    expect(gv.state.line).toEqual(hv.state.line)
    expect(hv.wins).toEqual([1, 0])
    expect(gv.wins).toEqual([1, 0])

    g.guest.rematch(g.at())
    g.run(100)
    expect(g.views().hv.again).toEqual({ mine: false, theirs: true })
    expect(isOver(g.views().hv.state)).toBe(true)
    g.host.rematch(g.at())
    g.run(100)
    for (const v of [g.views().hv, g.views().gv]) {
      expect(v.state.moves).toBe(0)
      expect(v.state.turn).toBe('guest')
      expect(v.again).toEqual({ mine: false, theirs: false })
      expect(v.wins).toEqual([1, 0])
    }
    expect(g.host.play(3, g.at())).toBe(false)
    expect(g.guest.play(3, g.at())).toBe(true)
    g.run(100)
    expect(g.views().hv.state.board[cellAt(3, 0)]).toBe('guest')
  })

  it('rematches when both ask at once', () => {
    const g = pair({ lag: 200 })
    g.run(800)
    g.moves([0, 0, 1, 1, 2, 2, 3])
    g.run(500)
    g.host.rematch(g.at())
    g.guest.rematch(g.at())
    g.run(500)
    expect(g.views().hv.state.moves).toBe(0)
    expect(g.views().gv.state.moves).toBe(0)
    expect(g.views().hv.state.turn).toBe('guest')
    expect(g.views().gv.state.turn).toBe('guest')
  })

  it('plays to a draw', () => {
    const g = pair()
    g.run(200)
    g.moves(DRAW)
    expect(g.views().hv.state.draw).toBe(true)
    expect(g.views().gv.state.draw).toBe(true)
    expect(g.views().hv.wins).toEqual([0, 0])
  })

  it('rematch does nothing mid game, and an early ask doesn’t count later', () => {
    const g = pair()
    g.run(200)
    g.guest.rematch(g.at())
    g.host.receive(encodeC4({ t: 'c4a', g: 0 }), g.at())
    g.run(100)
    expect(g.views().hv.again.theirs).toBe(false)
    g.moves([0, 0, 1, 1, 2, 2, 3])
    expect(g.views().hv.again).toEqual({ mine: false, theirs: false })
    // an ask for some other game
    g.host.receive(encodeC4({ t: 'c4a', g: 4 }), g.at())
    expect(g.views().hv.again.theirs).toBe(false)
  })

  it('keeps the link warm while nobody moves', () => {
    const g = pair()
    g.run(C4_SILENCE_MS * 3)
    expect(g.views().hv.ended).toBe(null)
    expect(g.views().gv.ended).toBe(null)
    expect(KEEP_MS).toBeLessThan(C4_SILENCE_MS)
  })

  it('ends when the other side goes quiet', () => {
    const g = pair()
    g.run(200)
    g.run(C4_SILENCE_MS + 500, true)
    expect(g.views().hv.ended).toBe('lost the connection')
    expect(g.views().gv.ended).toBe('lost the connection')
  })

  it('ends when the other side says bye', () => {
    const g = pair()
    g.run(200)
    g.host.receive(encodeC4({ t: 'bye' }), g.at())
    expect(g.host.view().ended).toBe('your friend left')
    expect(g.host.play(0, g.at())).toBe(false)
  })

  it('refuses a different version', () => {
    const g = pair({ guestVersion: C4_VERSION + 1 })
    g.run(500)
    expect(g.views().hv.ended).toBe('update both Q Calcs to play each other')
    expect(g.views().gv.ended).toBe('your friend left')
  })

  it('says so when the other side opened pong', () => {
    const sent: string[] = []
    const s = new Connect4Session('guest', (t) => sent.push(t), 0)
    s.receive(encodePong({ t: 'hi', v: 1 }), 10)
    expect(s.view().ended).toMatch(/pong/)
    expect(sent.at(-1)).toBe(encodeC4({ t: 'bye' }))
  })
})
