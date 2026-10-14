import { memo, useEffect, useRef, useState, type MouseEvent } from 'react'
import { gameNow } from '../lib/gameClock'
import { plainKey, swallow } from '../lib/gameKeys'
import { closeReasonText, type PeerTransport } from '../lib/peer'
import type { Side } from '../lib/pong'
import { boardOf, boardOpen, canPlay, cellIn, CELLS, isOver, SIZE, spotOf, type UtState } from '../lib/ultimate'
import { UT_SKILLS, type UtSkill } from '../lib/ultimateBot'
import { UltimateSession, type UtView } from '../lib/ultimateSession'
import { GameLobby, type GameProps } from './GameLobby'
import { chalkBox, chalkLine, chalkMark } from './ultimateChalk'

export function UltimatePanel({ transport }: { transport: PeerTransport | null }) {
  return (
    <div className="pong" aria-label="Ultimate tic-tac-toe">
      <GameLobby
        game="ultimate"
        transport={transport}
        name="ultimate"
        blurb="win three small boards in a row."
        online={(p) => <UltimateGame {...p} />}
        local={{
          prompt: 'play the computer',
          options: UT_SKILLS,
          play: (i, onEnd) => <UltimateGame transport={null} role="host" peer="computer" solo={UT_SKILLS[i] as UtSkill} onEnd={onEnd} />,
        }}
      />
    </div>
  )
}

// geometry in svg units: nine 3×3 boards on a slate, the big # between them
const CELL = 30
const BOARD = 3 * CELL
const GAP = 14
const W = 3 * BOARD + 2 * GAP
const PAD = 10
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
  const aimingRef = useRef(aiming)
  aimingRef.current = aiming
  const cursorRef = useRef(cursor)
  cursorRef.current = cursor
  const pickedRef = useRef(picked)
  pickedRef.current = picked
  const onEndRef = useRef(onEnd)
  onEndRef.current = onEnd
  const act = useRef<Actions>({ play: () => {}, rematch: () => {} })

  useEffect(() => {
    const session = new UltimateSession(role, (text) => transport?.send(text), gameNow(), solo)
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
        session.play(c, gameNow())
        sync()
      },
      rematch: () => {
        session.rematch(gameNow())
        sync()
      },
    }

    const off = transport?.subscribe((e) => {
      if (e.type === 'message') {
        session.receive(e.data, gameNow())
        sync()
      } else if (e.type === 'closed') finish(closeReasonText(e.reason, transport.peerWord))
    })
    const timer = window.setInterval(() => {
      session.tick(gameNow())
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
          // a hidden cursor shows first, so ↵ never plays a cell you can't see
          else if (!aimingRef.current) setAiming(true)
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
        <span className={`pong-name ut-name ${role === 'host' ? 'mine' : 'theirs'}`}>{left} · x</span>
        <span className="pong-points">
          {wins[0]} <span className="pong-dash">–</span> {wins[1]}
        </span>
        <span className={`pong-name right ut-name ${role === 'guest' ? 'mine' : 'theirs'}`}>o · {right}</span>
      </div>
      <p className={`c4-status ut-status ${myTurn || state.winner === role ? 'mine' : ''} ${started ? '' : 'pong-wait'}`} aria-live="polite">
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

// one chalk stroke: a soft wide pass under a crisp one, which reads as a glow without an svg filter
function Ink({ d, tone, className = '' }: { d: string; tone: 'mine' | 'theirs'; className?: string }) {
  return (
    <g className={`ut-ink ${tone} ${className}`}>
      <path className="ut-glow" d={d} pathLength={1} />
      <path className="ut-core" d={d} pathLength={1} />
    </g>
  )
}

// a few faint flecks of chalk dust, scattered once
const DUST = Array.from({ length: 70 }, (_, i) => {
  const h = Math.sin(i * 12.9898) * 43758.5453
  const k = Math.sin(i * 78.233) * 12345.678
  return { x: (h - Math.floor(h)) * (W + 2 * PAD) - PAD, y: (k - Math.floor(k)) * (W + 2 * PAD) - PAD, r: 0.4 + ((h * 7) % 1 + 1) % 1 * 0.9 }
})

// the slate, its dust and every grid line. it never changes, so it renders once
const Slate = memo(function Slate() {
  const lines = []
  for (const k of [1, 2]) {
    const p = k * (BOARD + GAP) - GAP / 2
    lines.push(<path key={`V${k}`} className="ut-big" d={chalkLine([p, -4], [p, W + 4], 100 + k, 1.6)} />)
    lines.push(<path key={`H${k}`} className="ut-big" d={chalkLine([-4, p], [W + 4, p], 200 + k, 1.6)} />)
  }
  for (let b = 0; b < SIZE; b++) {
    for (const k of [1, 2]) {
      lines.push(<path key={`v${b}${k}`} className="ut-small" d={chalkLine([bx(b) + k * CELL, by(b) + 5], [bx(b) + k * CELL, by(b) + BOARD - 5], b * 10 + k)} />)
      lines.push(<path key={`h${b}${k}`} className="ut-small" d={chalkLine([bx(b) + 5, by(b) + k * CELL], [bx(b) + BOARD - 5, by(b) + k * CELL], b * 10 + k + 5)} />)
    }
  }
  return (
    <g>
      <g className="ut-dust">
        {DUST.map((p, i) => (
          <circle key={i} cx={p.x} cy={p.y} r={p.r} />
        ))}
      </g>
      {lines}
    </g>
  )
})

// every x and o, the big ones on won boards and the line through the winning three. it only changes
// when a move is made; the newest mark draws itself in
const Pieces = memo(function Pieces({ state, role }: { state: UtState; role: Side }) {
  const tone = (who: Side) => (who === role ? 'mine' : 'theirs')
  const last = state.last ?? -1
  const out = []
  for (let b = 0; b < SIZE; b++) {
    const mark = state.boards[b]
    if (mark && mark !== 'draw') out.push(<ellipse key={`e${b}`} className="ut-erased" cx={bx(b) + BOARD / 2} cy={by(b) + BOARD / 2} rx={BOARD / 2} ry={BOARD / 2.3} />)
  }
  for (let i = 0; i < CELLS; i++) {
    const who = state.cells[i]
    if (!who) continue
    const cls = [state.boards[boardOf(i)] ? 'dim' : '', i === last ? 'fresh' : ''].join(' ')
    out.push(<Ink key={i} d={chalkMark(markOf(who), cx(i), cy(i), CELL, i)} tone={tone(who)} className={cls} />)
  }
  const won = new Set(state.line ?? [])
  for (let b = 0; b < SIZE; b++) {
    const mark = state.boards[b]
    if (!mark || mark === 'draw') continue
    // a decided board holding the last move was decided by it
    const cls = ['big', last >= 0 && boardOf(last) === b ? 'fresh' : '', state.line && !won.has(b) ? 'dim' : ''].join(' ')
    out.push(<Ink key={`m${b}`} d={chalkMark(markOf(mark), bx(b), by(b), BOARD, 50 + b)} tone={tone(mark)} className={cls} />)
  }
  if (state.line && state.winner) {
    const [a, , z] = state.line
    const ax = bx(a!) + BOARD / 2
    const ay = by(a!) + BOARD / 2
    const zx = bx(z!) + BOARD / 2
    const zy = by(z!) + BOARD / 2
    const ext = 0.18
    out.push(<Ink key="win" d={chalkLine([ax - (zx - ax) * ext, ay - (zy - ay) * ext], [zx + (zx - ax) * ext, zy + (zy - ay) * ext], 999, 2.5)} tone={tone(state.winner)} className="win fresh" />)
  }
  return <g>{out}</g>
})

// the cell under a point on the board, or -1 in a gap or off it
function cellAtPoint(svg: SVGSVGElement, clientX: number, clientY: number): number {
  const r = svg.getBoundingClientRect()
  const span = W + 2 * PAD
  const x = ((clientX - r.left) / r.width) * span - PAD
  const y = ((clientY - r.top) / r.height) * span - PAD
  const col = Math.floor(x / (BOARD + GAP))
  const row = Math.floor(y / (BOARD + GAP))
  const ix = x - col * (BOARD + GAP)
  const iy = y - row * (BOARD + GAP)
  if (col < 0 || col > 2 || row < 0 || row > 2 || ix >= BOARD || iy >= BOARD) return -1
  return cellIn(row * 3 + col, Math.floor(iy / CELL) * 3 + Math.floor(ix / CELL))
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
  // it's your move, so the boards you may play in are chalked round
  live: boolean
  // the board the digits pick cells in, when there is one
  zone: number | null
  // the cell you're aiming at
  cursor: number | null
  onHover: (cell: number) => void
  onLeave: () => void
  onPick: (cell: number) => void
}) {
  const hovered = useRef(-1)
  const move = (e: MouseEvent<SVGSVGElement>) => {
    const c = cellAtPoint(e.currentTarget, e.clientX, e.clientY)
    if (c === hovered.current) return
    hovered.current = c
    if (c >= 0) onHover(c)
  }
  const click = (e: MouseEvent<SVGSVGElement>) => {
    const c = cellAtPoint(e.currentTarget, e.clientX, e.clientY)
    if (c >= 0) onPick(c)
  }
  const open = []
  if (live) {
    for (let b = 0; b < SIZE; b++) {
      if (!boardOpen(state, b)) continue
      open.push(<path key={b} className={`ut-open ${zone === b ? 'zone' : ''}`} d={chalkBox(bx(b) - 4, by(b) - 4, BOARD + 8, BOARD + 8, 300 + b)} pathLength={1} />)
    }
  }
  const last = state.last ?? -1
  // where the last move sent you, as a dotted chalk arc
  const sent = live && last >= 0 && state.forced != null && boardOf(last) !== state.forced
  let trail = null
  if (sent) {
    const x1 = cx(last) + CELL / 2
    const y1 = cy(last) + CELL / 2
    const x2 = bx(state.forced!) + BOARD / 2
    const y2 = by(state.forced!) + BOARD / 2
    const bend = 0.18
    trail = <path key={`t${last}`} className="ut-trail" d={`M${x1} ${y1}Q${(x1 + x2) / 2 - (y2 - y1) * bend} ${(y1 + y2) / 2 + (x2 - x1) * bend} ${x2} ${y2}`} />
  }
  const aimOk = cursor != null && canPlay(state, cursor, role)
  return (
    <div className="ut-slate">
      <svg
        className="ut-grid"
        viewBox={`${-PAD} ${-PAD} ${W + 2 * PAD} ${W + 2 * PAD}`}
        role="grid"
        aria-label="board"
        onMouseMove={move}
        onMouseLeave={() => {
          hovered.current = -1
          onLeave()
        }}
        onMouseDown={(e) => e.preventDefault()}
        onClick={click}
      >
        <Slate />
        {open}
        {trail}
        {cursor != null ? <rect className={`ut-aim ${aimOk ? '' : 'blocked'}`} x={cx(cursor) + 3} y={cy(cursor) + 3} width={CELL - 6} height={CELL - 6} rx={7} /> : null}
        <Pieces state={state} role={role} />
      </svg>
    </div>
  )
}
