import { useEffect, useRef, useState } from 'react'
import { plainKey, swallow } from '../lib/gameKeys'
import {
  botTurn,
  COLS,
  DX,
  DY,
  LEVELS,
  matchWinner,
  newRound,
  queueTurn,
  ROWS,
  scoreRound,
  steer,
  step,
  TICK_MS,
  WIN_ROUNDS,
  type Cycle,
  type Dir,
  type Level,
  type Player,
  type Round,
} from '../lib/trails'
import './TrailsPanel.css'

// trails: `trails` then enter. no link needed: the computer, or two players on one keyboard. the start screen
// picks which; ⌫ goes back to it, and esc closes the panel (the bar's escapeLayer, so not handled here)

type Vs = Level | 'friend'

// logical px per cell; the canvas is COLS × ROWS cells at this, times devicePixelRatio
const CELL = 10
const COUNT_MS = 550
const POINT_MS = 1300

// enter, arrows, backspace and letters would otherwise reach the hidden bar and commit or move through the tape
const barKey = (e: KeyboardEvent) => e.key.length === 1 || e.key === 'Enter' || e.key.startsWith('Arrow') || e.key === 'Backspace'

const WASD: Record<string, Dir> = { w: 0, d: 1, s: 2, a: 3 }
const ARROWS: Record<string, Dir> = { ArrowUp: 0, ArrowRight: 1, ArrowDown: 2, ArrowLeft: 3 }

export function TrailsPanel({ onClose }: { onClose: () => void }) {
  const [vs, setVs] = useState<Vs | null>(null)
  // a fresh match each time one starts, even at the same level
  const [match, setMatch] = useState(0)

  useEffect(() => {
    if (vs) return
    const onKey = (e: KeyboardEvent) => {
      if (!plainKey(e) || e.key === 'Escape' || !barKey(e)) return
      swallow(e)
      if (e.repeat) return
      const pick = e.key === '4' ? 'friend' : LEVELS[Number(e.key) - 1]
      if (/^[1-4]$/.test(e.key) && pick) {
        setVs(pick)
        setMatch((m) => m + 1)
      }
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [vs])

  return (
    <div className="trails" aria-label="Trails">
      <div className="pong-head">
        <span className="pong-title">trails</span>
        <span className="pong-sub">{vs == null ? `first to ${WIN_ROUNDS} rounds` : vs === 'friend' ? 'one keyboard' : `vs the computer · ${vs}`}</span>
        <button type="button" className="pong-leave" onClick={onClose}>
          esc
        </button>
      </div>
      {vs ? (
        <TrailsGame key={match} vs={vs} onMenu={() => setVs(null)} />
      ) : (
        <div className="pong-body">
          <p className="pong-line">leave a trail, don’t hit one. the last one moving takes the round.</p>
          <div className="pong-actions">
            {LEVELS.map((level, i) => (
              <button
                type="button"
                className="pong-button"
                key={level}
                onClick={() => {
                  setVs(level)
                  setMatch((m) => m + 1)
                }}
              >
                {level} <kbd>{i + 1}</kbd>
              </button>
            ))}
          </div>
          <button
            type="button"
            className="pong-button trails-friend"
            onClick={() => {
              setVs('friend')
              setMatch((m) => m + 1)
            }}
          >
            two players, one keyboard <kbd>4</kbd>
          </button>
          <p className="pong-note">vs the computer, steer with WASD or the arrows · two players: WASD and arrows</p>
        </div>
      )}
    </div>
  )
}

type Colors = { ink: string; muted: string; bg: string; mine: string; rival: string }

function readColors(el: Element): Colors {
  const cs = getComputedStyle(el)
  const v = (name: string, fallback: string) => cs.getPropertyValue(name).trim() || fallback
  return {
    ink: v('--ink', '#1d1d1f'),
    muted: v('--muted', '#86868b'),
    bg: v('--bg', '#ffffff'),
    mine: v('--accent', '#1d7a4c'),
    rival: v('--trails-rival', '#d4520f'),
  }
}

function draw(canvas: HTMLCanvasElement, colors: Colors, r: Round, dpr: number) {
  const ctx = canvas.getContext('2d')
  if (!ctx) return
  const c = canvas.width / COLS
  ctx.clearRect(0, 0, canvas.width, canvas.height)

  // a faint grid every 4 cells, on whole device pixels
  ctx.fillStyle = colors.muted
  ctx.globalAlpha = 0.1
  for (let x = 4; x < COLS; x += 4) ctx.fillRect(Math.round(x * c), 0, dpr, canvas.height)
  for (let y = 4; y < ROWS; y += 4) ctx.fillRect(0, Math.round(y * c), canvas.width, dpr)
  ctx.globalAlpha = 1

  const centre = (i: number): [number, number] => [((i % COLS) + 0.5) * c, (Math.floor(i / COLS) + 0.5) * c]
  const colorOf = (p: Player) => (p === 0 ? colors.mine : colors.rival)

  // the walls: one line per cycle through its cell centres
  r.cycles.forEach((cy, p) => {
    ctx.strokeStyle = colorOf(p as Player)
    ctx.globalAlpha = cy.alive ? 0.8 : 0.45
    ctx.lineWidth = Math.round(c * 0.5)
    ctx.lineCap = 'square'
    ctx.lineJoin = 'miter'
    ctx.beginPath()
    cy.path.forEach((i, n) => {
      const [x, y] = centre(i)
      if (n === 0) ctx.moveTo(x, y)
      else ctx.lineTo(x, y)
    })
    ctx.stroke()
  })
  ctx.globalAlpha = 1

  r.cycles.forEach((cy, p) => drawHead(ctx, cy, colorOf(p as Player), colors, c, dpr))
}

function drawHead(ctx: CanvasRenderingContext2D, cy: Cycle, color: string, colors: Colors, c: number, dpr: number) {
  const cx = (cy.x + 0.5) * c
  const cyy = (cy.y + 0.5) * c
  if (!cy.alive) {
    // a burst where it hit, halfway into the cell it couldn't enter
    const bx = cx + DX[cy.dir] * c * 0.5
    const by = cyy + DY[cy.dir] * c * 0.5
    ctx.strokeStyle = color
    ctx.lineWidth = Math.max(1, Math.round(c * 0.16))
    ctx.lineCap = 'round'
    ctx.shadowColor = color
    ctx.shadowBlur = 8 * dpr
    for (let k = 0; k < 8; k++) {
      const a = (k * Math.PI) / 4 + Math.PI / 8
      const r0 = c * 0.45
      const r1 = c * (k % 2 ? 1.1 : 1.6)
      ctx.beginPath()
      ctx.moveTo(bx + Math.cos(a) * r0, by + Math.sin(a) * r0)
      ctx.lineTo(bx + Math.cos(a) * r1, by + Math.sin(a) * r1)
      ctx.stroke()
    }
    ctx.shadowBlur = 0
    ctx.fillStyle = colors.ink
    ctx.beginPath()
    ctx.arc(bx, by, c * 0.22, 0, Math.PI * 2)
    ctx.fill()
    return
  }
  const s = Math.round(c * 0.9)
  const x0 = Math.round(cx - s / 2)
  const y0 = Math.round(cyy - s / 2)
  ctx.shadowColor = color
  ctx.shadowBlur = 12 * dpr
  ctx.fillStyle = color
  ctx.beginPath()
  ctx.roundRect(x0, y0, s, s, Math.round(c * 0.2))
  ctx.fill()
  ctx.shadowBlur = 0
  // a lit nose, toward where it's going
  const n = Math.max(dpr, Math.round(c * 0.28))
  ctx.fillStyle = colors.bg
  ctx.globalAlpha = 0.85
  ctx.fillRect(Math.round(cx + DX[cy.dir] * c * 0.18 - n / 2), Math.round(cyy + DY[cy.dir] * c * 0.18 - n / 2), n, n)
  ctx.globalAlpha = 1
}

type Phase = 'count' | 'run' | 'point' | 'over'
type View = { score: [number, number]; phase: Phase; count: number; last: Player | null; winner: Player | null }

// the arena: holds the round, the clock and the score, steps the logic and draws it
function TrailsGame({ vs, onMenu }: { vs: Vs; onMenu: () => void }) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const [view, setView] = useState<View>({ score: [0, 0], phase: 'count', count: 3, last: null, winner: null })
  const onMenuRef = useRef(onMenu)
  onMenuRef.current = onMenu

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    const dpr = Math.max(1, Math.round(window.devicePixelRatio || 1))
    canvas.width = COLS * CELL * dpr
    canvas.height = ROWS * CELL * dpr
    let colors = readColors(canvas)
    let frames = 0
    let raf = 0
    let shown = ''

    let round = newRound()
    let score: [number, number] = [0, 0]
    let phase: Phase = 'count'
    let phaseAt = performance.now()
    let last = phaseAt
    let acc = 0
    let lastWinner: Player | null = null

    const restart = (now: number) => {
      round = newRound()
      score = [0, 0]
      lastWinner = null
      phase = 'count'
      phaseAt = now
    }

    const onKey = (e: KeyboardEvent) => {
      if (!plainKey(e) || e.key === 'Escape' || !barKey(e)) return
      swallow(e)
      const key = e.key.length === 1 ? e.key.toLowerCase() : e.key
      if (key === 'Backspace') {
        onMenuRef.current()
        return
      }
      if (key === 'Enter') {
        if (phase === 'over' && !e.repeat) restart(performance.now())
        return
      }
      const fromWasd = WASD[key]
      const fromArrows = ARROWS[key]
      const dir = fromWasd ?? fromArrows
      if (dir == null || (phase !== 'count' && phase !== 'run')) return
      // vs the computer, either set steers you; two players: WASD is the left cycle, arrows the right
      const who: Player = vs !== 'friend' || fromWasd != null ? 0 : 1
      round = queueTurn(round, who, dir)
    }
    window.addEventListener('keydown', onKey, true)

    const frame = (now: number) => {
      raf = requestAnimationFrame(frame)
      if (++frames % 30 === 0) colors = readColors(canvas)
      if (phase === 'count' && now - phaseAt >= COUNT_MS * 3) {
        phase = 'run'
        last = now
        acc = 0
      }
      if (phase === 'run') {
        let dt = now - last
        last = now
        // the window was hidden or the tab asleep: pick up where it left off rather than catch up
        if (dt > 250) dt = 0
        acc += dt
        while (acc >= TICK_MS && phase === 'run') {
          acc -= TICK_MS
          if (vs !== 'friend') round = steer(round, 1, botTurn(round, 1, vs, Math.random))
          round = step(round)
          if (round.result) {
            score = scoreRound(score, round.result)
            lastWinner = round.result.winner
            phase = matchWinner(score) != null ? 'over' : 'point'
            phaseAt = now
          }
        }
      }
      if (phase === 'point' && now - phaseAt >= POINT_MS) {
        round = newRound()
        phase = 'count'
        phaseAt = now
      }
      draw(canvas, colors, round, dpr)

      const count = phase === 'count' ? 3 - Math.floor((now - phaseAt) / COUNT_MS) : 0
      const key = `${score.join(':')}|${phase}|${count}|${lastWinner}`
      if (key === shown) return
      shown = key
      setView({ score, phase, count, last: lastWinner, winner: matchWinner(score) })
    }
    raf = requestAnimationFrame(frame)

    return () => {
      cancelAnimationFrame(raf)
      window.removeEventListener('keydown', onKey, true)
    }
  }, [vs])

  const solo = vs !== 'friend'
  const names: [string, string] = solo ? ['you', 'computer'] : ['WASD', 'arrows']
  const takes = (p: Player) => (solo ? (p === 0 ? 'you take the round' : 'the computer takes it') : `${names[p]} take${p === 0 ? 's' : ''} the round`)
  let banner = ''
  let big = false
  if (view.phase === 'count') {
    banner = String(Math.max(1, view.count))
    big = true
  } else if (view.phase === 'point') banner = view.last == null ? 'a draw · no point' : takes(view.last)
  else if (view.phase === 'over' && view.winner != null) {
    const who = solo ? (view.winner === 0 ? 'you win' : 'the computer wins') : `${names[view.winner]} win${view.winner === 0 ? 's' : ''}`
    banner = `${who} ${view.score[0]}–${view.score[1]} · ↵ again`
  }

  return (
    <div className="pong-game">
      <div className="pong-score">
        <span className="pong-name trails-mine">{names[0]}</span>
        <span className="pong-points">
          {view.score[0]} <span className="pong-dash">–</span> {view.score[1]}
        </span>
        <span className="pong-name right trails-rival">{names[1]}</span>
      </div>
      <div className="pong-court">
        <canvas ref={canvasRef} className="trails-canvas" aria-label="arena" />
        {banner ? <div className={big ? 'pong-banner trails-count' : 'pong-banner'}>{banner}</div> : null}
      </div>
      <p className="pong-note">
        {solo ? 'WASD or arrows to turn' : 'left: WASD · right: arrows'} · first to {WIN_ROUNDS} · ⌫ menu · esc leaves
      </p>
    </div>
  )
}
