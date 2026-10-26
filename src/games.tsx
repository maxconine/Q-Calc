// The games-only page the native Mac app loads in a web view that exists only while a game (or one of the
// 420/67/69 effects) is up: games.html#pong, #connect4, #ultimate, #ember, #crack, #equal, #numbers, #trails, and
// games.html?x=…&y=…&w=…&h=…#smoke / #arms / #fold for the effects, drawn around a bar-sized box at (x, y).
// The calculator itself never loads this (native/ARCHITECTURE.md §7). Same bridge messages as quick.html:
// game, gameSave (through installGameSaves), nickname, copy, escapeLayer, gameFullscreen; effectDone when an
// effect ends.
import { StrictMode, useEffect, useRef, type ReactNode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import './windows.css'
import { Connect4Panel } from './components/Connect4Panel'
import { CrackPanel } from './components/CrackPanel'
import { EmberPanel } from './components/EmberPanel'
import { EqualPanel } from './components/EqualPanel'
import { FourTwentySmoke } from './components/FourTwentySmoke'
import { GameShell } from './components/GameShell'
import { NumbersPanel } from './components/NumbersPanel'
import { PongPanel } from './components/PongPanel'
import { SixtyNineFold } from './components/SixtyNineFold'
import { SixtySevenArms } from './components/SixtySevenArms'
import { TrailsPanel } from './components/TrailsPanel'
import { UltimatePanel } from './components/UltimatePanel'
import { nativeHandler } from './lib/bridge'
import { installGameSaves } from './lib/gameSaves'
import type { GameKind } from './lib/games'
import { peerTransport } from './lib/peer'

installGameSaves()

const EFFECT_MS = 2700
const post = (msg: Record<string, unknown>) => nativeHandler()?.postMessage(msg)

function panel(kind: GameKind): ReactNode {
  switch (kind) {
    case 'pong':
      return <PongPanel transport={peerTransport()} />
    case 'connect4':
      return <Connect4Panel transport={peerTransport()} />
    case 'ultimate':
      return <UltimatePanel transport={peerTransport()} />
    case 'ember':
      return <EmberPanel transport={peerTransport()} />
    case 'crack':
      return <CrackPanel />
    case 'equal':
      return <EqualPanel />
    case 'numbers':
      return <NumbersPanel />
    default:
      return <TrailsPanel transport={peerTransport()} />
  }
}

function Game({ kind }: { kind: GameKind }) {
  const escRef = useRef<(() => boolean) | null>(null)
  const holdRef = useRef<(() => boolean) | null>(null)
  useEffect(() => {
    post({ type: 'game', open: true })
    post({ type: 'escapeLayer', on: true })
    // esc steps back through the shell (full screen, lobby, pause); from the top it closes the window
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return
      e.preventDefault()
      if (!escRef.current?.()) post({ type: 'game', open: false })
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])
  return (
    <div className="spotlight games-page">
      <GameShell escRef={escRef} holdRef={holdRef} onClose={() => post({ type: 'game', open: false })}>
        {panel(kind)}
      </GameShell>
    </div>
  )
}

// an effect drawn around where the bar is; the app's click-through window is the bar plus a margin
function Effect({ kind }: { kind: 'smoke' | 'arms' | 'fold' }) {
  const q = new URLSearchParams(location.search)
  const n = (k: string, d: number) => Number(q.get(k) ?? d) || d
  useEffect(() => {
    const t = window.setTimeout(() => post({ type: 'effectDone' }), EFFECT_MS + 300)
    return () => window.clearTimeout(t)
  }, [])
  return (
    <div
      className="quick-effect-bar"
      style={{ position: 'absolute', left: n('x', 200), top: n('y', 200), width: n('w', 680), height: n('h', 64) }}
    >
      {kind === 'smoke' ? <FourTwentySmoke smoking /> : kind === 'arms' ? <SixtySevenArms shaking /> : <SixtyNineFold active />}
    </div>
  )
}

const GAMES: GameKind[] = ['pong', 'connect4', 'ultimate', 'ember', 'crack', 'equal', 'numbers', 'trails']
const which = location.hash.replace(/^#/, '')
document.documentElement.classList.add('quick-native', 'games-native')
if (which === 'smoke' || which === 'arms' || which === 'fold') {
  document.documentElement.style.background = 'transparent'
  document.body.style.background = 'transparent'
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    {which === 'smoke' || which === 'arms' || which === 'fold' ? (
      <Effect kind={which} />
    ) : (
      <Game kind={(GAMES as string[]).includes(which) ? (which as GameKind) : 'pong'} />
    )}
  </StrictMode>,
)
