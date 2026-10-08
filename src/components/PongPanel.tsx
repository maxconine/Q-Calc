import { useCallback, useEffect, useRef, useState } from 'react'
import { nativeWindow } from '../lib/bridge'
import { closeReasonText, isPairingCode, type PeerInfo, type PeerTransport } from '../lib/peer'
import {
  BALL_SIZE,
  COURT_H,
  COURT_W,
  PADDLE_H,
  PADDLE_INSET,
  PADDLE_W,
  WIN_SCORE,
  type Side,
  type Snapshot,
} from '../lib/pong'
import { PongSession, type SessionView } from '../lib/pongSession'

type Screen =
  | { k: 'menu'; note?: string }
  | { k: 'hosting'; code: string | null; renewed: boolean }
  | { k: 'browse'; peers: PeerInfo[]; pick: number; denied: boolean }
  | { k: 'code'; peer: PeerInfo; digits: string; note?: string }
  | { k: 'pairing'; peer: PeerInfo }
  | { k: 'game'; role: Side; peer: string }

const FRIEND: PeerInfo = { id: '', name: 'your friend' }

const plainKey = (e: KeyboardEvent) => !e.metaKey && !e.ctrlKey && !e.altKey

// keys pong takes for itself; everything else (⌘C, ⌃D…) still reaches the bar. the mac app's boot script
// has already buffered it for the hidden input, so drop it from there too
function swallow(e: KeyboardEvent): void {
  e.preventDefault()
  e.stopPropagation()
  const w = nativeWindow()
  if (w?.__QCALC_KEYS?.length) w.__QCALC_KEYS = []
}

export function PongPanel({ transport, onClose }: { transport: PeerTransport; onClose: () => void }) {
  const [screen, setScreen] = useState<Screen>({ k: 'menu' })
  const screenRef = useRef(screen)
  screenRef.current = screen

  const host = useCallback(() => {
    setScreen({ k: 'hosting', code: null, renewed: false })
    transport.host()
  }, [transport])

  // the online link has no list of hosts: join goes straight to the code
  const browse = useCallback(() => {
    if (!transport.discovers) {
      setScreen({ k: 'code', peer: FRIEND, digits: '' })
      return
    }
    setScreen({ k: 'browse', peers: [], pick: 0, denied: false })
    transport.discover()
  }, [transport])

  const pickPeer = useCallback((peer: PeerInfo) => {
    setScreen({ k: 'code', peer, digits: '' })
  }, [])

  const toMenu = useCallback(
    (note?: string) => {
      transport.close()
      setScreen({ k: 'menu', note })
    },
    [transport],
  )

  // the link goes when pong does, however it closes
  useEffect(() => () => transport.close(), [transport])

  useEffect(
    () =>
      transport.subscribe((e) => {
        const cur = screenRef.current
        switch (e.type) {
          case 'hosting':
            if (cur.k === 'hosting') setScreen({ k: 'hosting', code: e.code, renewed: Boolean(e.renewed) })
            return
          case 'peers':
            if (cur.k === 'browse') {
              const keep = cur.peers[cur.pick]?.id
              const pick = Math.max(0, e.peers.findIndex((p) => p.id === keep))
              setScreen({ ...cur, peers: e.peers, pick })
            }
            return
          case 'denied':
            if (cur.k === 'browse') setScreen({ ...cur, denied: true })
            return
          case 'open':
            transport.stopDiscovery()
            setScreen({ k: 'game', role: e.role, peer: e.peer || 'friend' })
            return
          case 'closed':
            if (cur.k === 'pairing' && e.reason === 'wrong-code') {
              setScreen({ k: 'code', peer: cur.peer, digits: '', note: 'wrong code · try again' })
              return
            }
            // a game reports its own end
            if (cur.k !== 'game') setScreen({ k: 'menu', note: closeReasonText(e.reason, transport.peerWord) })
            return
        }
      }),
    [transport],
  )

  const submitCode = useCallback(
    (peer: PeerInfo, digits: string) => {
      if (!isPairingCode(digits, transport.codeLength)) return
      setScreen({ k: 'pairing', peer })
      transport.join(peer.id, digits)
    },
    [transport],
  )

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const cur = screenRef.current
      if (cur.k === 'game' || !plainKey(e) || e.key === 'Escape') return
      const key = e.key.toLowerCase()
      // enter, arrows and backspace would otherwise reach the hidden bar and commit or move through the tape
      const barKey = e.key.length === 1 || e.key === 'Enter' || e.key.startsWith('Arrow') || e.key === 'Backspace'
      if (cur.k === 'menu') {
        if (key === 'h') {
          swallow(e)
          host()
        } else if (key === 'j') {
          swallow(e)
          browse()
        } else if (barKey) swallow(e)
        return
      }
      if (cur.k === 'browse') {
        swallow(e)
        const n = cur.peers.length
        if (e.key === 'ArrowDown' && n) setScreen({ ...cur, pick: (cur.pick + 1) % n })
        else if (e.key === 'ArrowUp' && n) setScreen({ ...cur, pick: (cur.pick - 1 + n) % n })
        else if (e.key === 'Enter' && cur.peers[cur.pick]) pickPeer(cur.peers[cur.pick]!)
        else if (e.key === 'Backspace') toMenu()
        return
      }
      if (cur.k === 'code') {
        swallow(e)
        const n = transport.codeLength
        if (/^[0-9]$/.test(e.key) && cur.digits.length < n) {
          const digits = cur.digits + e.key
          if (digits.length === n) submitCode(cur.peer, digits)
          else setScreen({ ...cur, digits, note: undefined })
        } else if (e.key === 'Backspace') {
          if (cur.digits) setScreen({ ...cur, digits: cur.digits.slice(0, -1) })
          else if (transport.discovers) browse()
          else toMenu()
        }
        return
      }
      if (barKey) swallow(e)
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [browse, host, pickPeer, submitCode, toMenu, transport])

  return (
    <div className="pong" aria-label="Pong">
      {screen.k === 'game' ? (
        <PongGame transport={transport} role={screen.role} peer={screen.peer} onEnd={toMenu} />
      ) : (
        <div className="pong-lobby">
          <div className="pong-head">
            <span className="pong-title">pong</span>
            <span className="pong-sub">{transport.label}</span>
            <button type="button" className="pong-leave" onClick={onClose}>
              esc
            </button>
          </div>
          <Lobby screen={screen} transport={transport} onHost={host} onJoin={browse} onPick={pickPeer} onBack={() => toMenu()} />
        </div>
      )}
    </div>
  )
}

function Lobby({
  screen,
  transport,
  onHost,
  onJoin,
  onPick,
  onBack,
}: {
  screen: Exclude<Screen, { k: 'game' }>
  transport: PeerTransport
  onHost: () => void
  onJoin: () => void
  onPick: (p: PeerInfo) => void
  onBack: () => void
}) {
  const word = transport.peerWord
  const n = transport.codeLength
  switch (screen.k) {
    case 'menu':
      return (
        <div className="pong-body">
          <p className="pong-line">two players on two {word}s. first to {WIN_SCORE}.</p>
          <div className="pong-actions">
            <button type="button" className="pong-button" onClick={onHost}>
              host <kbd>H</kbd>
            </button>
            <button type="button" className="pong-button" onClick={onJoin}>
              join <kbd>J</kbd>
            </button>
          </div>
          {screen.note ? <p className="pong-note">{screen.note}</p> : null}
        </div>
      )
    case 'hosting':
      return (
        <div className="pong-body">
          <div className="pong-code" aria-label="pairing code">
            {(screen.code ?? '·'.repeat(n)).split('').map((d, i) => (
              <span key={i}>{d}</span>
            ))}
          </div>
          <p className="pong-line">
            {screen.renewed ? 'too many wrong codes, so here’s a new one. ' : ''}
            {transport.discovers
              ? `on the other ${word}: type pong, join, pick this ${word}, enter the code`
              : 'send your friend this code · on their Q Calc: type pong, join, enter the code'}
          </p>
          <p className="pong-note pong-wait">waiting for your friend…</p>
        </div>
      )
    case 'browse':
      return (
        <div className="pong-body">
          {screen.denied ? (
            <p className="pong-note">{closeReasonText('denied', word)}</p>
          ) : screen.peers.length ? (
            <ul className="pong-peers" role="listbox">
              {screen.peers.map((p, i) => (
                <li key={p.id} role="option" aria-selected={i === screen.pick}>
                  <button type="button" className={i === screen.pick ? 'on' : undefined} onClick={() => onPick(p)}>
                    {p.name}
                  </button>
                </li>
              ))}
            </ul>
          ) : (
            <p className="pong-line pong-wait">looking for {word}s hosting pong…</p>
          )}
          <p className="pong-note">
            ↑↓ pick · ↵ choose ·{' '}
            <button type="button" className="pong-link" onClick={onBack}>
              ⌫ back
            </button>
          </p>
        </div>
      )
    case 'code':
    case 'pairing': {
      const digits = screen.k === 'code' ? screen.digits : '·'.repeat(n)
      return (
        <div className="pong-body">
          <p className="pong-line">{transport.discovers ? `code shown on ${screen.peer.name}` : 'the code your friend sent'}</p>
          <div className={`pong-code entry ${screen.k === 'pairing' ? 'busy' : ''}`}>
            {Array.from({ length: n }, (_, i) => (
              <span key={i} className={i === digits.length && screen.k === 'code' ? 'caret' : undefined}>
                {screen.k === 'pairing' ? '•' : (digits[i] ?? '')}
              </span>
            ))}
          </div>
          <p className="pong-note">{screen.k === 'pairing' ? 'pairing…' : (screen.note ?? `type the ${n} digits · ⌫ back`)}</p>
        </div>
      )
    }
  }
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
function PongGame({ transport, role, peer, onEnd }: { transport: PeerTransport; role: Side; peer: string; onEnd: (note?: string) => void }) {
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
    const session = new PongSession(role, (text) => transport.send(text), performance.now())

    const off = transport.subscribe((e) => {
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
      off()
      window.removeEventListener('keydown', onDown, true)
      window.removeEventListener('keyup', onUp, true)
      window.removeEventListener('blur', onBlur)
    }
  }, [transport, role])

  const left = role === 'host' ? 'you' : peer
  const right = role === 'guest' ? 'you' : peer
  const { again } = view
  let banner = ''
  if (!view.started) banner = `waiting for ${peer}…`
  else if (view.phase === 'over') {
    const who = view.winner === role ? 'you win' : `${peer} wins`
    banner = `${who} · ${again.mine ? 'waiting for a rematch…' : again.theirs ? `${peer} wants a rematch · ↵` : '↵ rematch'}`
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
      <p className="pong-note">↑↓ or W S to move · esc leaves</p>
    </div>
  )
}
