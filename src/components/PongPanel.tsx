import { useEffect, useRef, useState } from 'react'
import { closeReasonText, type PeerTransport } from '../lib/peer'
import {
  BALL_SIZE,
  COURT_H,
  COURT_W,
  PADDLE_H,
  PADDLE_INSET,
  PADDLE_W,
  SKILLS,
  WIN_SCORE,
  type Side,
  type Skill,
  type Snapshot,
} from '../lib/pong'
import { PongSession, type SessionView } from '../lib/pongSession'
import { plainKey, swallow } from '../lib/gameKeys'
import { GameLobby, type GameProps } from './GameLobby'

export function PongPanel({ transport, onClose }: { transport: PeerTransport; onClose: () => void }) {
  return (
    <div className="pong" aria-label="Pong">
      <GameLobby
        transport={transport}
        name="pong"
        blurb={`first to ${WIN_SCORE}.`}
        onClose={onClose}
        game={(p) => <PongGame {...p} />}
        solo={{
          levels: SKILLS,
          play: (level, onEnd) => <PongGame transport={null} role="host" peer="computer" solo={level as Skill} onEnd={onEnd} />,
        }}
      />
    </div>
  )
}

type Colors = { ink: string; accent: string; muted: string }

function readColors(el: Element): Colors {
  const cs = getComputedStyle(el)
  const v = (name: string, fallback: string) => cs.getPropertyValue(name).trim() || fallback
  return { ink: v('--ink', '#1d1d1f'), accent: v('--accent', '#1d7a4c'), muted: v('--muted', '#86868b') }
}

function draw(canvas: HTMLCanvasElement, colors: Colors, view: Snapshot, mine: Side) {
  const ctx = canvas.getContext('2d')
  if (!ctx) return
  const k = canvas.width / COURT_W
  // whole device pixels, so edges stay sharp
  const rect = (x: number, y: number, w: number, h: number) => {
    const x0 = Math.round(x * k)
    const y0 = Math.round(y * k)
    ctx.fillRect(x0, y0, Math.round((x + w) * k) - x0, Math.round((y + h) * k) - y0)
  }
  ctx.clearRect(0, 0, canvas.width, canvas.height)
  ctx.fillStyle = colors.muted
  ctx.globalAlpha = 0.35
  for (let y = 4; y < COURT_H; y += 16) rect(COURT_W / 2 - 1, y, 2, 8)
  ctx.globalAlpha = 1
  const paddle = (side: Side, y: number) => {
    ctx.fillStyle = side === mine ? colors.accent : colors.ink
    const x = side === 'host' ? PADDLE_INSET : COURT_W - PADDLE_INSET - PADDLE_W
    rect(x, y - PADDLE_H / 2, PADDLE_W, PADDLE_H)
  }
  paddle('host', view.paddles[0])
  paddle('guest', view.paddles[1])
  if (view.phase === 'play') {
    ctx.fillStyle = colors.ink
    rect(view.ball.x - BALL_SIZE / 2, view.ball.y - BALL_SIZE / 2, BALL_SIZE, BALL_SIZE)
  }
}

// the court: PongSession runs the game, this feeds it keys and frames and draws what it returns
// solo: against the computer, at that level; there's no link then
function PongGame({
  transport,
  role,
  peer,
  onEnd,
  solo = null,
}: Omit<GameProps, 'transport'> & { transport: GameProps['transport'] | null; solo?: Skill | null }) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const [view, setView] = useState<Pick<Snapshot, 'score' | 'phase' | 'winner'> & Pick<SessionView, 'started' | 'again'>>({
    score: [0, 0],
    phase: 'serve',
    started: false,
    again: { mine: false, theirs: false },
  })
  const onEndRef = useRef(onEnd)
  onEndRef.current = onEnd

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    const dpr = Math.max(1, Math.round(window.devicePixelRatio || 1))
    canvas.width = COURT_W * dpr
    canvas.height = COURT_H * dpr
    let colors = readColors(canvas)
    let frames = 0
    let raf = 0
    let shown = ''
    const session = new PongSession(role, (text) => transport?.send(text), performance.now(), Math.random, solo)

    const off = transport?.subscribe((e) => {
      if (e.type === 'message') session.receive(e.data, performance.now())
      else if (e.type === 'closed') onEndRef.current(closeReasonText(e.reason, transport.peerWord))
    })

    const onKey = (e: KeyboardEvent, down: boolean) => {
      if (!plainKey(e) || e.key === 'Escape') return
      const key = e.key.toLowerCase()
      if (e.key === 'ArrowUp' || key === 'w') session.key('up', down)
      else if (e.key === 'ArrowDown' || key === 's') session.key('down', down)
      else if (e.key === 'Enter') {
        if (down && !e.repeat) session.rematch()
      } else if (e.key.length !== 1 && e.key !== 'Backspace') return
      swallow(e)
    }
    const onDown = (e: KeyboardEvent) => onKey(e, true)
    const onUp = (e: KeyboardEvent) => onKey(e, false)
    const onBlur = () => session.release()
    window.addEventListener('keydown', onDown, true)
    window.addEventListener('keyup', onUp, true)
    window.addEventListener('blur', onBlur)

    const frame = (now: number) => {
      raf = requestAnimationFrame(frame)
      if (++frames % 30 === 0) colors = readColors(canvas)
      const v = session.tick(now)
      if (v.ended != null) {
        cancelAnimationFrame(raf)
        onEndRef.current(v.ended || undefined)
        return
      }
      if (v.snap) draw(canvas, colors, v.snap, role)
      const s = v.snap ?? { score: [0, 0] as [number, number], phase: 'serve' as const }
      const key = `${s.score.join(':')}|${s.phase}|${v.started}|${v.again.mine}|${v.again.theirs}`
      if (key === shown) return
      shown = key
      setView({ score: s.score, phase: s.phase, winner: v.snap?.winner, started: v.started, again: v.again })
    }
    raf = requestAnimationFrame(frame)

    return () => {
      cancelAnimationFrame(raf)
      off?.()
      window.removeEventListener('keydown', onDown, true)
      window.removeEventListener('keyup', onUp, true)
      window.removeEventListener('blur', onBlur)
    }
  }, [transport, role, solo])

  const left = role === 'host' ? 'you' : peer
  const right = role === 'guest' ? 'you' : peer
  const { again } = view
  let banner = ''
  if (!view.started) banner = `waiting for ${peer}…`
  else if (view.phase === 'over') {
    const who = view.winner === role ? 'you win' : `${peer} wins`
    banner = solo
      ? `${who} · ↵ play again`
      : `${who} · ${again.mine ? 'waiting for a rematch…' : again.theirs ? `${peer} wants a rematch · ↵` : '↵ rematch'}`
  } else if (view.phase === 'serve' && view.score[0] + view.score[1] === 0) banner = `first to ${WIN_SCORE}`

  return (
    <div className="pong-game">
      <div className="pong-score">
        <span className={`pong-name ${role === 'host' ? 'mine' : ''}`}>{left}</span>
        <span className="pong-points">
          {view.score[0]} <span className="pong-dash">–</span> {view.score[1]}
        </span>
        <span className={`pong-name right ${role === 'guest' ? 'mine' : ''}`}>{right}</span>
      </div>
      <div className="pong-court">
        <canvas ref={canvasRef} className="pong-canvas" aria-label="court" />
        {banner ? <div className="pong-banner">{banner}</div> : null}
      </div>
      <p className="pong-note">↑↓ or W S to move · {solo ? `${solo} · ` : ''}esc leaves</p>
    </div>
  )
}
