import { useEffect, useRef, useState } from 'react'
import { plainKey, swallow } from '../lib/gameKeys'
import { closeReasonText, type PeerTransport } from '../lib/peer'
import type { Side } from '../lib/pong'
import { boardOf, boardOpen, canPlay, cellIn, CELLS, isOver, SIZE, spotOf, type UtState } from '../lib/ultimate'
import { UT_SKILLS, type UtSkill } from '../lib/ultimateBot'
import { UltimateSession, type UtView } from '../lib/ultimateSession'
import { GameLobby, type GameProps } from './GameLobby'

export function UltimatePanel({ transport, onClose }: { transport: PeerTransport; onClose: () => void }) {
  return (
    <div className="pong" aria-label="Ultimate tic-tac-toe">
      <GameLobby
        transport={transport}
        name="ultimate"
        blurb="win three small boards in a row."
        onClose={onClose}
        game={(p) => <UltimateGame {...p} />}
        solo={{
          levels: UT_SKILLS,
          play: (level, onEnd) => <UltimateGame transport={null} role="host" peer="computer" solo={level as UtSkill} onEnd={onEnd} />,
        }}
      />
    </div>
  )
}

// geometry in svg units: nine 3×3 boards with a gap between them
const CELL = 28
const BOARD = 3 * CELL
const GAP = 10
const W = 3 * BOARD + 2 * GAP
const bx = (b: number) => (b % 3) * (BOARD + GAP)
const by = (b: number) => Math.floor(b / 3) * (BOARD + GAP)
const cx = (cell: number) => bx(boardOf(cell)) + (spotOf(cell) % 3) * CELL
const cy = (cell: number) => by(boardOf(cell)) + Math.floor(spotOf(cell) / 3) * CELL

// the 9×9 grid the arrows move over, and back to a cell
const gridRow = (cell: number) => Math.floor(boardOf(cell) / 3) * 3 + Math.floor(spotOf(cell) / 3)
const gridCol = (cell: number) => (boardOf(cell) % 3) * 3 + (spotOf(cell) % 3)
const atGrid = (row: number, col: number) => cellIn(Math.floor(row / 3) * 3 + Math.floor(col / 3), (row % 3) * 3 + (col % 3))

const KEEP_TICK_MS = 200
const CENTRE = 4

// the host plays x and the guest o, whoever goes first
const markOf = (who: Side) => (who === 'host' ? 'x' : 'o')

type Actions = { play: (cell: number) => void; rematch: () => void }

// solo: against the computer, at that level; there's no link then
function UltimateGame({
  transport,
  role,
  peer,
  onEnd,
  solo = null,
}: Omit<GameProps, 'transport'> & { transport: GameProps['transport'] | null; solo?: UtSkill | null }) {
  const [view, setView] = useState<UtView | null>(null)
  const [cursor, setCursor] = useState(cellIn(CENTRE, CENTRE))
  // free to play anywhere: the board 1–9 picked, so the next digit picks a cell in it
  const [picked, setPicked] = useState<number | null>(null)
  // aiming: the mouse is over the board, or a cell was just picked with the keys; the cursor only shows then
  const [aiming, setAiming] = useState(false)
  const cursorRef = useRef(cursor)
  cursorRef.current = cursor
  const pickedRef = useRef(picked)
  pickedRef.current = picked
  const onEndRef = useRef(onEnd)
  onEndRef.current = onEnd
  const act = useRef<Actions>({ play: () => {}, rematch: () => {} })

  useEffect(() => {
    const session = new UltimateSession(role, (text) => transport?.send(text), performance.now(), solo)
    let shown = -1
    let moved = 0
    let done = false
    const finish = (note?: string) => {
      if (done) return
      done = true
      onEndRef.current(note)
    }
    const sync = () => {
      const v = session.view()
      if (v.ended != null) return finish(v.ended || undefined)
      if (v.rev === shown) return
      shown = v.rev
      setView(v)
      // each move: the board pick is spent, and the cursor jumps to the board the next move is sent to
      if (v.state.moves !== moved) {
        moved = v.state.moves
        setPicked(null)
        const forced = v.state.forced
        if (forced != null) setCursor((c) => cellIn(forced, spotOf(c)))
      }
    }
    act.current = {
      play: (c) => {
        session.play(c, performance.now())
        sync()
      },
      rematch: () => {
        session.rematch(performance.now())
        sync()
      },
    }

    const off = transport?.subscribe((e) => {
      if (e.type === 'message') {
        session.receive(e.data, performance.now())
        sync()
      } else if (e.type === 'closed') finish(closeReasonText(e.reason, transport.peerWord))
    })
    const timer = window.setInterval(() => {
      session.tick(performance.now())
      sync()
    }, KEEP_TICK_MS)

    const onKey = (e: KeyboardEvent) => {
      if (!plainKey(e) || e.key === 'Escape') return
      const state = session.view().state
      const n = /^[1-9]$/.test(e.key) ? Number(e.key) - 1 : -1
      const step: Record<string, [number, number]> = { ArrowUp: [-1, 0], ArrowDown: [1, 0], ArrowLeft: [0, -1], ArrowRight: [0, 1] }
      if (step[e.key]) {
        const [dr, dc] = step[e.key]!
        const c = cursorRef.current
        setCursor(atGrid((gridRow(c) + dr + SIZE) % SIZE, (gridCol(c) + dc + SIZE) % SIZE))
        setAiming(true)
      } else if (n >= 0) {
        const zone = state.forced ?? pickedRef.current
        if (zone != null) setCursor(cellIn(zone, n))
        else if (boardOpen(state, n)) {
          setPicked(n)
          setCursor(cellIn(n, CENTRE))
        }
        setAiming(true)
      } else if (e.key === 'Backspace') {
        setPicked(null)
      } else if (e.key === 'Enter' || e.key === ' ') {
        if (!e.repeat) {
          if (isOver(state)) act.current.rematch()
          else act.current.play(cursorRef.current)
        }
      } else if (e.key.length !== 1) return
      swallow(e)
    }
    window.addEventListener('keydown', onKey, true)
    sync()

    return () => {
      off?.()
      window.clearInterval(timer)
      window.removeEventListener('keydown', onKey, true)
      act.current = { play: () => {}, rematch: () => {} }
    }
  }, [transport, role, solo])

  if (!view) return null
  const { state, started, again, wins } = view
  const over = isOver(state)
  const myTurn = started && !over && state.turn === role
  const theirName = peer
  const left = role === 'host' ? 'you' : theirName
  const right = role === 'guest' ? 'you' : theirName

  let status: string
  if (!started) status = `waiting for ${theirName}…`
  else if (over) {
    const who = state.winner === role ? 'you win' : state.winner ? `${theirName} wins` : 'draw'
    status = solo
      ? `${who} · ↵ play again`
      : `${who} · ${again.mine ? 'waiting for a rematch…' : again.theirs ? `${theirName} wants a rematch · ↵` : '↵ rematch'}`
  } else if (view.thinking) status = state.moves === 0 ? 'the computer goes first…' : 'the computer is thinking…'
  else if (state.moves === 0) status = myTurn ? `you go first · ${markOf(role)}` : `${theirName} goes first`
  else if (myTurn) status = state.forced == null ? 'your turn · play anywhere' : 'your turn'
  else status = `${theirName}’s turn`

  const zone = state.forced ?? picked
  const keysNote = myTurn && zone == null ? '1–9 pick a board' : '1–9 or arrows pick a cell'

  return (
    <div className="pong-game c4-game">
      <div className="pong-score">
        <span className={`pong-name ${role === 'host' ? 'mine' : ''}`}>{left} · x</span>
        <span className="pong-points">
          {wins[0]} <span className="pong-dash">–</span> {wins[1]}
        </span>
        <span className={`pong-name right ${role === 'guest' ? 'mine' : ''}`}>o · {right}</span>
      </div>
      <p className={`c4-status ${myTurn || state.winner === role ? 'mine' : ''} ${started ? '' : 'pong-wait'}`} aria-live="polite">
        {status}
      </p>
      <Board
        state={state}
        role={role}
        live={myTurn}
        zone={zone}
        cursor={myTurn && aiming ? cursor : null}
        onHover={(c) => {
          setCursor(c)
          setAiming(true)
        }}
        onLeave={() => setAiming(false)}
        onPick={(c) => act.current.play(c)}
      />
      <p className="pong-note">
        {keysNote} · ↵ play · {solo ? `${solo} · ` : ''}esc leaves
      </p>
    </div>
  )
}

function Mark({ who, x, y, size, className }: { who: Side; x: number; y: number; size: number; className: string }) {
  const pad = size * 0.24
  if (markOf(who) === 'x')
    return (
      <g className={className}>
        <line x1={x + pad} y1={y + pad} x2={x + size - pad} y2={y + size - pad} />
        <line x1={x + size - pad} y1={y + pad} x2={x + pad} y2={y + size - pad} />
      </g>
    )
  return <circle className={className} cx={x + size / 2} cy={y + size / 2} r={size / 2 - pad} />
}

function Board({
  state,
  role,
  live,
  zone,
  cursor,
  onHover,
  onLeave,
  onPick,
}: {
  state: UtState
  role: Side
  // it's your move, so the boards you may play in light up
  live: boolean
  // the board the digits pick cells in, when there is one
  zone: number | null
  // the cell you're aiming at
  cursor: number | null
  onHover: (cell: number) => void
  onLeave: () => void
  onPick: (cell: number) => void
}) {
  const over = isOver(state)
  const won = new Set(state.line ?? [])
  const boards = []
  for (let b = 0; b < SIZE; b++) {
    const mark = state.boards[b]
    const open = live && boardOpen(state, b)
    const cls = ['ut-board', open ? 'open' : '', open && zone === b ? 'zone' : '', mark ? 'done' : '', won.has(b) ? 'win' : '']
    boards.push(<rect key={`b${b}`} className={cls.join(' ')} x={bx(b) - 3} y={by(b) - 3} width={BOARD + 6} height={BOARD + 6} rx={7} />)
  }
  const lines = []
  for (let b = 0; b < SIZE; b++) {
    for (const k of [1, 2]) {
      lines.push(<line key={`v${b}${k}`} className="ut-line" x1={bx(b) + k * CELL} y1={by(b) + 4} x2={bx(b) + k * CELL} y2={by(b) + BOARD - 4} />)
      lines.push(<line key={`h${b}${k}`} className="ut-line" x1={bx(b) + 4} y1={by(b) + k * CELL} x2={bx(b) + BOARD - 4} y2={by(b) + k * CELL} />)
    }
  }
  const marks = []
  for (let i = 0; i < CELLS; i++) {
    const who = state.cells[i]
    if (!who) continue
    const cls = ['ut-mark', who === role ? 'mine' : 'theirs']
    if (state.boards[boardOf(i)]) cls.push('dim')
    if (i === state.last) cls.push('last')
    marks.push(<Mark key={i} who={who} x={cx(i)} y={cy(i)} size={CELL} className={cls.join(' ')} />)
  }
  const big = []
  for (let b = 0; b < SIZE; b++) {
    const mark = state.boards[b]
    if (!mark || mark === 'draw') continue
    const cls = ['ut-mark', 'big', mark === role ? 'mine' : 'theirs']
    if (over && state.line && !won.has(b)) cls.push('dim')
    big.push(<Mark key={`m${b}`} who={mark} x={bx(b)} y={by(b)} size={BOARD} className={cls.join(' ')} />)
  }
  const aimOk = cursor != null && canPlay(state, cursor, role)
  return (
    <div className="c4-wrap">
      <svg className="ut-grid" viewBox={`-4 -4 ${W + 8} ${W + 8}`} role="grid" aria-label="board" onMouseLeave={onLeave}>
        {boards}
        {lines}
        {cursor != null ? <rect className={`ut-aim ${aimOk ? '' : 'blocked'}`} x={cx(cursor) + 2} y={cy(cursor) + 2} width={CELL - 4} height={CELL - 4} rx={5} /> : null}
        {marks}
        {big}
        {Array.from({ length: CELLS }, (_, i) => (
          <rect
            key={`c${i}`}
            className="c4-col"
            x={cx(i)}
            y={cy(i)}
            width={CELL}
            height={CELL}
            onMouseEnter={() => onHover(i)}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => onPick(i)}
          />
        ))}
      </svg>
    </div>
  )
}
