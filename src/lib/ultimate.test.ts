import { describe, expect, it } from 'vitest'
import {
  canPlay,
  cellIn,
  CELLS,
  decodeUt,
  encodeUt,
  firstFor,
  isOver,
  isUltimateCommand,
  legalCells,
  newGame,
  play,
  type Side,
  type UtState,
} from './ultimate'

// plays [board, spot] pairs in turn from a fresh game, failing on any move the rules refuse
function run(moves: Array<[number, number]>, first: Side = 'host'): UtState {
  let s = newGame(first)
  for (const [b, k] of moves) {
    const next = play(s, cellIn(b, k), s.turn)
    if (!next) throw new Error(`move ${b},${k} refused`)
    s = next
  }
  return s
}

describe('ultimate command', () => {
  it('opens on its names', () => {
    for (const t of ['ultimate', 'Ultimate tic-tac-toe', 'ultimate tic tac toe', 'ultimate ttt', 'uttt', ' ultimate ']) expect(isUltimateCommand(t)).toBe(true)
  })
  it('leaves other text alone', () => {
    for (const t of ['tic tac toe', 'ultimately', 'ultimate 4', '']) expect(isUltimateCommand(t)).toBe(false)
  })
})

describe('ultimate rules', () => {
  it('starts anywhere, then sends you to the board matching the spot', () => {
    const s = newGame()
    expect(legalCells(s)).toHaveLength(CELLS)
    const a = run([[4, 2]])
    expect(a.turn).toBe('guest')
    expect(a.forced).toBe(2)
    expect(canPlay(a, cellIn(5, 0), 'guest')).toBe(false)
    expect(canPlay(a, cellIn(2, 0), 'guest')).toBe(true)
    expect(canPlay(a, cellIn(2, 0), 'host')).toBe(false)
    expect(legalCells(a)).toHaveLength(9)
  })

  it('refuses a taken cell, the wrong turn and a cell out of range', () => {
    const a = run([[0, 0]])
    expect(play(a, cellIn(0, 0), 'guest')).toBeNull()
    expect(play(a, cellIn(0, 1), 'host')).toBeNull()
    expect(play(a, CELLS, 'guest')).toBeNull()
    expect(play(a, -1, 'guest')).toBeNull()
  })

  it('takes a small board with three in a row, and frees a player sent to it', () => {
    // guest takes the middle row of board 0; host's moves there send guest straight back
    const s = run([
      [0, 0],
      [0, 3],
      [3, 0],
      [0, 4],
      [4, 0],
      [0, 5],
    ])
    expect(s.boards[0]).toBe('guest')
    expect(s.forced).toBe(5)
    // sent to board 0, now taken: play anywhere
    const t = play(s, cellIn(5, 0), 'host')!
    expect(t.forced).toBeNull()
    expect(canPlay(t, cellIn(0, 6), 'guest')).toBe(false)
    expect(canPlay(t, cellIn(8, 8), 'guest')).toBe(true)
  })

  it('wins with three small boards in a row and counts a drawn board for nobody', () => {
    let s = newGame()
    // build a position directly: host holds boards 0 and 1 and two of board 2's top row
    s = {
      ...s,
      boards: ['host', 'host', null, 'draw', null, null, null, null, null],
      cells: s.cells.map((_, i) => (i === cellIn(2, 0) || i === cellIn(2, 1) ? 'host' : null)),
      forced: 2,
    }
    const won = play(s, cellIn(2, 2), 'host')!
    expect(won.winner).toBe('host')
    expect(won.line).toEqual([0, 1, 2])
    expect(isOver(won)).toBe(true)
    expect(legalCells(won)).toEqual([])

    const drawnLine = { ...s, boards: ['host', 'draw', null, null, null, null, null, null, null] as UtState['boards'] }
    expect(play(drawnLine, cellIn(2, 2), 'host')!.winner).toBeUndefined()
  })

  it('is a draw once every board is decided with no winner', () => {
    const s = newGame()
    const boards: UtState['boards'] = ['host', 'guest', 'host', 'host', 'guest', 'guest', 'guest', 'host', null]
    const cells = s.cells.map((_, i) => (i >= cellIn(8, 0) && i < cellIn(8, 8) ? (['host', 'guest', 'host', 'host', 'guest', 'guest', 'guest', 'host'] as const)[i - 72]! : null))
    const end = play({ ...s, boards, cells, forced: 8 }, cellIn(8, 8), 'host')!
    expect(end.boards[8]).toBe('draw')
    expect(end.draw).toBe(true)
    expect(end.winner).toBeUndefined()
  })

  it('alternates who goes first', () => {
    expect([0, 1, 2, 3].map(firstFor)).toEqual(['host', 'guest', 'host', 'guest'])
  })
})

describe('ultimate wire', () => {
  it('round-trips its messages', () => {
    for (const m of [{ t: 'ut', v: 1 }, { t: 'ut', v: 1, ok: true }, { t: 'utm', g: 2, n: 5, c: 80 }, { t: 'uta', g: 0 }, { t: 'utk' }, { t: 'bye' }] as const)
      expect(decodeUt(encodeUt(m))).toEqual(m)
  })
  it('spots a friend in another game', () => {
    expect(decodeUt('{"t":"hi","v":3}')).toEqual({ t: 'other', game: 'pong' })
    expect(decodeUt('{"t":"c4","v":1}')).toEqual({ t: 'other', game: 'connect 4' })
  })
  it('drops anything malformed', () => {
    for (const raw of ['', 'nope', '[]', 'null', '{"t":"utm","g":0,"n":0,"c":81}', '{"t":"utm","g":0,"n":0,"c":1.5}', '{"t":"utm","g":-1,"n":0,"c":0}', '{"t":"ut"}', `{"t":"utk","x":"${'a'.repeat(300)}"}`])
      expect(decodeUt(raw)).toBeNull()
  })
})
