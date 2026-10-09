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

// how the frame is drawn: `t` is how far the heads are into the next cell (0–1) so they glide between ticks;
// `now` drives the pulse; `crashMs` is how long ago the round ended, for the burst
type Moment = { t: number; now: number; crashMs: number | null }

// a colour from a CSS value (#rgb, #rrggbb or rgb()), so glows can be drawn at any alpha
function rgbOf(color: string): [number, number, number] {
  const hex = color.trim().match(/^#([0-9a-f]{3}|[0-9a-f]{6})$/i)?.[1]
  if (hex) {
    const full = hex.length === 3 ? [...hex].map((h) => h + h).join('') : hex
    return [0, 2, 4].map((i) => parseInt(full.slice(i, i + 2), 16)) as [number, number, number]
  }
  const m = color.match(/rgba?\(\s*([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)/i)
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : [128, 128, 128]
}

const rgba = ([r, g, b]: [number, number, number], a: number) => `rgba(${r}, ${g}, ${b}, ${a})`

function isDark(bg: string): boolean {
  const [r, g, b] = rgbOf(bg)
  return 0.2126 * r + 0.7152 * g + 0.0722 * b < 128
}

// whiter, for the hot core of a light wall
function hot([r, g, b]: [number, number, number], k: number): [number, number, number] {
  return [r + (255 - r) * k, g + (255 - g) * k, b + (255 - b) * k].map(Math.round) as [number, number, number]
}

const BURST_MS = 900
// how many cells near the head burn brighter
const HOT_CELLS = 14

function draw(canvas: HTMLCanvasElement, colors: Colors, r: Round, dpr: number, m: Moment) {
  const ctx = canvas.getContext('2d')
  if (!ctx) return
  const W = canvas.width
  const H = canvas.height
  const c = W / COLS
  const dark = isDark(colors.bg)
  const ink = rgbOf(colors.ink)
  ctx.clearRect(0, 0, W, H)

  // the floor: a little deeper than the panel, lit from the middle
  const floor = ctx.createRadialGradient(W / 2, H / 2, H * 0.1, W / 2, H / 2, W * 0.62)
  floor.addColorStop(0, rgba(ink, dark ? 0.06 : 0.025))
  floor.addColorStop(1, rgba(ink, dark ? 0.015 : 0.06))
  ctx.fillStyle = floor
  ctx.beginPath()
  ctx.roundRect(0, 0, W, H, 6 * dpr)
  ctx.fill()

  // the grid: every cell faintly, every fourth a little brighter
  for (let x = 1; x < COLS; x++) {
    ctx.fillStyle = rgba(ink, x % 4 ? (dark ? 0.035 : 0.04) : dark ? 0.08 : 0.09)
    ctx.fillRect(Math.round(x * c), 0, dpr, H)
  }
  for (let y = 1; y < ROWS; y++) {
    ctx.fillStyle = rgba(ink, y % 4 ? (dark ? 0.035 : 0.04) : dark ? 0.08 : 0.09)
    ctx.fillRect(0, Math.round(y * c), W, dpr)
  }

  const mine = rgbOf(colors.mine)
  const rival = rgbOf(colors.rival)
  const colorOf = (p: Player) => (p === 0 ? mine : rival)

  // the rim glows in both colours, each from its own side
  const rim = ctx.createLinearGradient(0, 0, W, 0)
  rim.addColorStop(0, rgba(mine, dark ? 0.55 : 0.45))
  rim.addColorStop(0.5, rgba(ink, dark ? 0.18 : 0.15))
  rim.addColorStop(1, rgba(rival, dark ? 0.55 : 0.45))
  ctx.strokeStyle = rim
  ctx.lineWidth = 1.5 * dpr
  ctx.beginPath()
  ctx.roundRect(dpr, dpr, W - 2 * dpr, H - 2 * dpr, 6 * dpr)
  ctx.stroke()

  const centre = (i: number): [number, number] => [((i % COLS) + 0.5) * c, (Math.floor(i / COLS) + 0.5) * c]
  // where a head is drawn: its cell, or part way into the next one while it runs
  const headAt = (cy: Cycle): [number, number] => {
    const glide = cy.alive && r.result == null ? m.t : 0
    // a turn already queued is where it's going next, so it glides that way, not straight on and then snaps
    const next = cy.queue[0]
    const dir = next != null && (next + 2) % 4 !== cy.dir ? next : cy.dir
    return [(cy.x + 0.5 + DX[dir] * glide) * c, (cy.y + 0.5 + DY[dir] * glide) * c]
  }

  const wall = (cy: Cycle, from: number, width: number, style: string, blur: number, glow: string) => {
    const pts = cy.path.slice(from).map(centre)
    if (!pts.length) return
    pts.push(headAt(cy))
    ctx.strokeStyle = style
    ctx.lineWidth = width
    ctx.lineCap = 'round'
    ctx.lineJoin = 'round'
    ctx.shadowColor = glow
    ctx.shadowBlur = blur
    ctx.beginPath()
    pts.forEach(([x, y], n) => (n ? ctx.lineTo(x, y) : ctx.moveTo(x, y)))
    ctx.stroke()
    ctx.shadowBlur = 0
  }

  // the light walls: a soft glow, a solid body, a hot core, and the newest stretch brighter still
  r.cycles.forEach((cy, p) => {
    const col = colorOf(p as Player)
    const dim = cy.alive ? 1 : 0.55
    wall(cy, 0, c * 1.2, rgba(col, (dark ? 0.16 : 0.12) * dim), 0, 'transparent')
    wall(cy, 0, c * 0.6, rgba(col, 0.9 * dim), dark ? 10 * dpr : 4 * dpr, rgba(col, dark ? 0.8 : 0.45))
    wall(cy, 0, Math.max(dpr, c * 0.18), rgba(hot(col, dark ? 0.65 : 0.45), 0.9 * dim), 0, 'transparent')
    if (cy.alive) wall(cy, Math.max(0, cy.path.length - HOT_CELLS), c * 0.62, rgba(hot(col, 0.3), 0.55), 0, 'transparent')
  })

  r.cycles.forEach((cy, p) => {
    if (cy.alive) drawHead(ctx, cy, colorOf(p as Player), headAt(cy), c, dpr, m.now, dark)
  })
  r.cycles.forEach((cy, p) => {
    if (!cy.alive) drawBurst(ctx, cy, colorOf(p as Player), c, dpr, m.crashMs ?? BURST_MS, dark)
  })
}

// a light cycle: a bright rounded body pointing the way it goes, a pulsing halo and a hot nose
function drawHead(
  ctx: CanvasRenderingContext2D,
  cy: Cycle,
  col: [number, number, number],
  [x, y]: [number, number],
  c: number,
  dpr: number,
  now: number,
  dark: boolean,
) {
  const pulse = 0.5 + 0.5 * Math.sin(now / 160)
  const halo = ctx.createRadialGradient(x, y, 0, x, y, c * (2.3 + 0.45 * pulse))
  halo.addColorStop(0, rgba(col, dark ? 0.55 : 0.35))
  halo.addColorStop(1, rgba(col, 0))
  ctx.fillStyle = halo
  ctx.beginPath()
  ctx.arc(x, y, c * (2.3 + 0.45 * pulse), 0, Math.PI * 2)
  ctx.fill()

  ctx.save()
  ctx.translate(x, y)
  ctx.rotate(cy.dir * (Math.PI / 2))
  // pointing up before the turn: long along the way it goes
  ctx.shadowColor = rgba(col, 0.9)
  ctx.shadowBlur = (dark ? 14 : 6) * dpr
  ctx.fillStyle = rgba(col, 1)
  ctx.beginPath()
  ctx.roundRect(-c * 0.55, -c * 0.8, c * 1.1, c * 1.45, c * 0.5)
  ctx.fill()
  ctx.shadowBlur = 0
  // the nose, lit
  ctx.fillStyle = rgba(hot(col, 0.85), 0.95)
  ctx.beginPath()
  ctx.ellipse(0, -c * 0.42, c * 0.28, c * 0.22, 0, 0, Math.PI * 2)
  ctx.fill()
  ctx.restore()
}

// a crash: a flash, a ring going out and sparks thrown from where it hit, fading over BURST_MS
function drawBurst(ctx: CanvasRenderingContext2D, cy: Cycle, col: [number, number, number], c: number, dpr: number, ms: number, dark: boolean) {
  const bx = (cy.x + 0.5 + DX[cy.dir] * 0.5) * c
  const by = (cy.y + 0.5 + DY[cy.dir] * 0.5) * c
  const k = Math.min(1, ms / BURST_MS)
  const fade = 1 - k
  const eased = 1 - (1 - k) ** 3

  if (fade > 0) {
    const flash = ctx.createRadialGradient(bx, by, 0, bx, by, c * 3)
    flash.addColorStop(0, rgba(hot(col, 0.7), 0.9 * fade))
    flash.addColorStop(1, rgba(col, 0))
    ctx.fillStyle = flash
    ctx.beginPath()
    ctx.arc(bx, by, c * 3, 0, Math.PI * 2)
    ctx.fill()

    ctx.strokeStyle = rgba(col, 0.8 * fade)
    ctx.lineWidth = Math.max(dpr, c * 0.18 * fade)
    ctx.beginPath()
    ctx.arc(bx, by, c * (0.6 + 4.5 * eased), 0, Math.PI * 2)
    ctx.stroke()
  }

  // sparks: fixed angles and speeds from the cycle's position, so a frame redrawn looks the same
  ctx.lineCap = 'round'
  for (let n = 0; n < 14; n++) {
    const seed = Math.sin((cy.x * 31 + cy.y * 17 + n * 7.3) * 12.9898) * 43758.5453
    const rnd = seed - Math.floor(seed)
    const a = (n / 14) * Math.PI * 2 + rnd * 0.6
    const reach = c * (1.6 + rnd * 2.6) * eased
    const len = c * (0.5 + rnd * 0.6) * fade
    const sx = bx + Math.cos(a) * reach
    const sy = by + Math.sin(a) * reach
    ctx.strokeStyle = rgba(n % 3 ? col : hot(col, 0.7), Math.max(0, fade))
    ctx.lineWidth = Math.max(dpr, c * 0.12)
    ctx.beginPath()
    ctx.moveTo(sx, sy)
    ctx.lineTo(sx - Math.cos(a) * len, sy - Math.sin(a) * len)
    ctx.stroke()
  }

  // what's left once it's over: a scorched mark
  ctx.fillStyle = rgba(col, dark ? 0.35 : 0.3)
  ctx.beginPath()
  ctx.arc(bx, by, c * 0.35, 0, Math.PI * 2)
  ctx.fill()
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
      const ended = phase === 'point' || phase === 'over'
      draw(canvas, colors, round, dpr, {
        t: phase === 'run' ? Math.min(1, acc / TICK_MS) : 0,
        now,
        crashMs: ended ? now - phaseAt : null,
      })

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
