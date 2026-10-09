import { useEffect, useRef, useState, type CSSProperties } from 'react'
import { cellAt, COLS, isOver, landingRow, ROWS, type C4State } from '../lib/connect4'
import { C4_SKILLS, type C4Skill } from '../lib/connect4Bot'
import { Connect4Session, type C4View } from '../lib/connect4Session'
import { plainKey, swallow } from '../lib/gameKeys'
import { closeReasonText, type PeerTransport } from '../lib/peer'
import type { Side } from '../lib/pong'
import { GameLobby, type GameProps } from './GameLobby'

export function Connect4Panel({ transport, onClose }: { transport: PeerTransport; onClose: () => void }) {
  return (
    <div className="pong" aria-label="Connect 4">
      <GameLobby
        transport={transport}
        name="connect 4"
        blurb="four in a row wins."
        onClose={onClose}
        game={(p) => <Connect4Game {...p} />}
        solo={{
          levels: C4_SKILLS,
          play: (level, onEnd) => <Connect4Game transport={null} role="host" peer="computer" solo={level as C4Skill} onEnd={onEnd} />,
        }}
      />
    </div>
  )
}

// board geometry in svg units: a lane on top for the disc you're holding, then the 7×6 grid
const CELL = 40
const R = 15
const LANE = CELL
const W = COLS * CELL
const H = LANE + ROWS * CELL
const cx = (col: number) => col * CELL + CELL / 2
const cy = (row: number) => LANE + (ROWS - 1 - row) * CELL + CELL / 2

const KEEP_TICK_MS = 200

// whether the landing preview shows at all; G flips it, and it's kept for next time
const GHOST_KEY = 'qcalc-c4-ghost'

function ghostSaved(): boolean {
  try {
    return localStorage.getItem(GHOST_KEY) !== 'off'
  } catch {
    return true
  }
}

function saveGhost(on: boolean): void {
  try {
    localStorage.setItem(GHOST_KEY, on ? 'on' : 'off')
  } catch {
    // private window: it just isn't remembered
  }
}

type Actions = { play: (col: number) => void; rematch: () => void }

// solo: against the computer, at that level; there's no link then
function Connect4Game({
  transport,
  role,
  peer,
  onEnd,
  solo = null,
}: Omit<GameProps, 'transport'> & { transport: GameProps['transport'] | null; solo?: C4Skill | null }) {
  const [view, setView] = useState<C4View | null>(null)
  const [col, setCol] = useState(Math.floor(COLS / 2))
  const [ghostOn, setGhostOn] = useState(ghostSaved)
  // aiming: the mouse is over the board, or a column was just picked with the keys; the preview only shows then
  const [aiming, setAiming] = useState(false)
  const colRef = useRef(col)
  colRef.current = col
  const onEndRef = useRef(onEnd)
  onEndRef.current = onEnd
  const act = useRef<Actions>({ play: () => {}, rematch: () => {} })

  useEffect(() => {
    const session = new Connect4Session(role, (text) => transport?.send(text), performance.now(), solo)
    let shown = -1
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
      const n = /^[1-9]$/.test(e.key) ? Number(e.key) : 0
      if (e.key === 'ArrowLeft' || e.key === 'ArrowRight' || (n >= 1 && n <= COLS)) setAiming(true)
      if (e.key === 'ArrowLeft') setCol((c) => (c - 1 + COLS) % COLS)
      else if (e.key === 'ArrowRight') setCol((c) => (c + 1) % COLS)
      else if (n >= 1 && n <= COLS) setCol(n - 1)
      else if (e.key.toLowerCase() === 'g') {
        if (!e.repeat)
          setGhostOn((on) => {
            saveGhost(!on)
            return !on
          })
      } else if (e.key === 'Enter' || e.key === ' ') {
        if (!e.repeat) {
          if (isOver(session.view().state)) act.current.rematch()
          else act.current.play(colRef.current)
        }
      } else if (e.key.length !== 1 && e.key !== 'Backspace' && !e.key.startsWith('Arrow')) return
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
  else if (state.moves === 0) status = myTurn ? 'you go first' : `${theirName} goes first`
  else status = myTurn ? 'your turn' : `${theirName}’s turn`

  return (
    <div className="pong-game c4-game">
      <div className="pong-score">
        <span className={`pong-name ${role === 'host' ? 'mine' : ''}`}>{left}</span>
        <span className="pong-points">
          {wins[0]} <span className="pong-dash">–</span> {wins[1]}
        </span>
        <span className={`pong-name right ${role === 'guest' ? 'mine' : ''}`}>{right}</span>
      </div>
      <p className={`c4-status ${myTurn || state.winner === role ? 'mine' : ''} ${started ? '' : 'pong-wait'}`} aria-live="polite">
        {status}
      </p>
      <Board
        state={state}
        role={role}
        cursor={myTurn ? col : null}
        ghost={ghostOn && aiming}
        onHover={(c) => {
          setCol(c)
          setAiming(true)
        }}
        onLeave={() => setAiming(false)}
        onPick={(c) => act.current.play(c)}
      />
      <p className="pong-note">
        ← → or 1–{COLS} to pick · ↵ drop · G {ghostOn ? 'hides' : 'shows'} the preview · {solo ? `${solo} · ` : ''}esc leaves
      </p>
    </div>
  )
}

function Board({
  state,
  role,
  cursor,
  ghost,
  onHover,
  onLeave,
  onPick,
}: {
  state: C4State
  role: Side
  // the column of the disc you're holding, when it's your turn
  cursor: number | null
  // whether to show where the held disc would land
  ghost: boolean
  onHover: (col: number) => void
  onLeave: () => void
  onPick: (col: number) => void
}) {
  const over = isOver(state)
  const line = new Set(state.line ?? [])
  // the newest disc falls in
  const last = state.last ?? -1
  const holes = []
  const discs = []
  for (let row = 0; row < ROWS; row++) {
    for (let c = 0; c < COLS; c++) {
      const i = cellAt(c, row)
      holes.push(<circle key={i} className="c4-hole" cx={cx(c)} cy={cy(row)} r={R} />)
      const who = state.board[i]
      if (!who) continue
      const cls = ['c4-disc', who === role ? 'mine' : 'theirs']
      if (over && state.line && !line.has(i)) cls.push('dim')
      if (line.has(i)) cls.push('win')
      if (i === last) cls.push('fall')
      discs.push(
        <circle
          key={i}
          className={cls.join(' ')}
          cx={cx(c)}
          cy={cy(row)}
          r={R}
          style={i === last ? ({ '--fall': `${-(cy(row) - LANE / 2)}px` } as CSSProperties) : undefined}
        />,
      )
    }
  }
  const landing = cursor != null ? landingRow(state, cursor) : -1
  return (
    <div className="c4-wrap">
      <svg className="c4-board" viewBox={`0 0 ${W} ${H}`} role="grid" aria-label="board" onMouseLeave={onLeave}>
        <rect className="c4-frame" x={0} y={LANE} width={W} height={ROWS * CELL} rx={10} />
        {holes}
        {ghost && cursor != null && landing >= 0 ? <circle className="c4-ghost" cx={cx(cursor)} cy={cy(landing)} r={R} /> : null}
        {discs}
        {cursor != null ? <circle className={`c4-disc mine held ${landing < 0 ? 'full' : ''}`} cx={cx(cursor)} cy={LANE / 2} r={R} /> : null}
        {Array.from({ length: COLS }, (_, c) => (
          <rect
            key={c}
            className="c4-col"
            x={c * CELL}
            y={0}
            width={CELL}
            height={H}
            onMouseEnter={() => onHover(c)}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => onPick(c)}
          />
        ))}
      </svg>
    </div>
  )
}
