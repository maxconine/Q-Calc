import { useCallback, useEffect, useMemo, useRef, useState, type MutableRefObject, type ReactNode } from 'react'
import { pauseGameClock, resumeGameClock } from '../lib/gameClock'
import { setKeyGate } from '../lib/gameKeys'
import { ARM_MS, escAction, pausedKey, type ShellMode } from '../lib/gameShell'
import { FullscreenContext, ShellContext, useShellFullscreen, type GameShellApi, type ShellHooks } from '../lib/gameShellContext'
import { fullscreenLabel, macLike } from '../lib/gameFullscreen'
import { isWindowsHost } from '../lib/platform'
import { useGameFullscreen, type GameFullscreenOptions } from './useGameFullscreen'
import './GameShell.css'

// what wraps whichever game is open: esc (leave full screen, step back through the lobby, pause, warn before
// leaving an online game), the paused overlay, and full screen. the lobby tells it where the player is; a game
// with screens of its own (ember's level picker) can say when nothing is running and how to leave.
// full screen is useGameFullscreen (⌃F, GAME_FULLSCREEN.md) on the shell's root, so every game has it; f in
// the lobby or the paused card and the small buttons toggle the same one. in the apps it's a big window

// sent when the window hides with a game kept open (someone's waiting to join, or playing online)
export const GAME_HIDE_EVENT = 'qcalc-game-hide'
// and when it shows again with that game still there
export const GAME_SHOW_EVENT = 'qcalc-game-show'

// a game that doesn't lay itself out: a margin round the game area, which the css sizes
const DEFAULT_LAYOUT: GameFullscreenOptions = { pad: 16 }

// ⌃F as the hook words it here
const chord = () => (macLike(isWindowsHost(), typeof navigator === 'undefined' ? '' : navigator.platform) ? '⌃F' : 'Ctrl+F')

export function GameShell({
  children,
  escRef,
  holdRef,
  onClose,
}: {
  children: ReactNode
  onClose: () => void
  // esc from the app: true while the game stays open, false to close it
  escRef: MutableRefObject<(() => boolean) | null>
  // whether the open game must stay open while the window is hidden
  holdRef: MutableRefObject<(() => boolean) | null>
}) {
  const [mode, setModeState] = useState<ShellMode>('lobby')
  const [paused, setPaused] = useState(false)
  const [armedAt, setArmedAt] = useState<number | null>(null)
  const [layout, setLayout] = useState<GameFullscreenOptions>(DEFAULT_LAYOUT)
  const fs = useGameFullscreen<HTMLDivElement>(layout)
  const fullscreen = fs.on
  const live = useRef({ mode, paused, armedAt, fullscreen })
  live.current = { mode, paused, armedAt, fullscreen }
  const hooksRef = useRef<ShellHooks | null>(null)

  const pause = useCallback(() => {
    if (live.current.paused) return
    pauseGameClock()
    live.current.paused = true
    setPaused(true)
  }, [])

  const resume = useCallback(() => {
    if (!live.current.paused) return
    resumeGameClock()
    live.current.paused = false
    setPaused(false)
  }, [])

  const { toggle: toggleFullscreen, exit: exitFullscreen } = fs

  const fullscreenLayout = useCallback((opts: GameFullscreenOptions) => {
    setLayout(opts)
    return () => setLayout(DEFAULT_LAYOUT)
  }, [])

  const setMode = useCallback(
    (next: ShellMode) => {
      if (next !== 'local') resume()
      setArmedAt(null)
      setModeState(next)
    },
    [resume],
  )

  const onEscape = useCallback((): boolean => {
    const s = live.current
    const action = escAction({ ...s, idle: Boolean(hooksRef.current?.idle()), now: Date.now() })
    switch (action) {
      case 'unfullscreen':
        exitFullscreen()
        return true
      case 'lobby':
        return Boolean(hooksRef.current?.back())
      case 'pause':
        pause()
        return true
      case 'arm':
        setArmedAt(Date.now())
        return true
      case 'leave':
        resume()
        setArmedAt(null)
        hooksRef.current?.leave()
        return true
    }
  }, [exitFullscreen, pause, resume])

  // the page's own esc buttons: a lobby screen that can't step back closes the game
  const escape = useCallback(() => {
    if (!onEscape()) onClose()
  }, [onEscape, onClose])

  useEffect(() => {
    escRef.current = onEscape
    holdRef.current = () => Boolean(hooksRef.current?.hold())
    return () => {
      if (escRef.current === onEscape) escRef.current = null
      holdRef.current = null
    }
  }, [escRef, holdRef, onEscape])

  // "esc again to leave" goes after a moment
  useEffect(() => {
    if (armedAt == null) return
    const t = window.setTimeout(() => setArmedAt(null), ARM_MS)
    return () => window.clearTimeout(t)
  }, [armedAt])

  // keys while paused go to the overlay, not the game (⌃F passes on to useGameFullscreen)
  useEffect(
    () =>
      setKeyGate((e) => {
        if (!live.current.paused) return false
        const what = pausedKey(e)
        if (what === 'pass') return false
        if (e.repeat) return true
        if (what === 'fullscreen') toggleFullscreen()
        else resume()
        return true
      }),
    [resume, toggleFullscreen],
  )

  // a game on this computer pauses when the window loses focus, now that clicking away leaves it up
  useEffect(() => {
    const away = () => {
      if (live.current.mode === 'local' && !hooksRef.current?.idle()) pause()
    }
    const hidden = () => {
      if (document.visibilityState === 'hidden') away()
    }
    // the app shrinks the window back to the bar as it hides (and tells the hook), so full screen goes too
    const hide = () => {
      exitFullscreen()
      away()
    }
    window.addEventListener('blur', away)
    window.addEventListener(GAME_HIDE_EVENT, hide)
    document.addEventListener('visibilitychange', hidden)
    return () => {
      window.removeEventListener('blur', away)
      window.removeEventListener(GAME_HIDE_EVENT, hide)
      document.removeEventListener('visibilitychange', hidden)
    }
  }, [exitFullscreen, pause])

  // the game closing ends everything it started; the hook leaves full screen itself
  useEffect(() => () => resumeGameClock(), [])

  const register = useCallback((hooks: ShellHooks) => {
    hooksRef.current = hooks
    return () => {
      if (hooksRef.current === hooks) hooksRef.current = null
    }
  }, [])

  const api = useMemo<GameShellApi>(
    () => ({ setMode, register, toggleFullscreen, fullscreenLayout, escape }),
    [setMode, register, toggleFullscreen, fullscreenLayout, escape],
  )

  // an online game shows the lobby's header, button and all; a game on this computer gets one in the corner
  const corner = mode === 'local'
  return (
    <ShellContext.Provider value={api}>
      <FullscreenContext.Provider value={fs}>
        <div ref={fs.ref} className="game-shell">
          <div className="game-area">{children}</div>
          {corner ? <FullscreenButton className="game-corner" on={fullscreen} onClick={toggleFullscreen} /> : null}
          {paused ? (
            <div className="game-paused" role="dialog" aria-label="paused" onMouseDown={(e) => e.target === e.currentTarget && resume()}>
              <div className="game-paused-card">
                <p className="game-paused-title">paused</p>
                <p className="game-paused-line">any key carries on · esc leaves to the lobby</p>
                <div className="game-paused-actions">
                  <button type="button" className="pong-button" onClick={resume}>
                    resume <kbd>↵</kbd>
                  </button>
                  <button type="button" className="pong-button" onClick={toggleFullscreen}>
                    {fullscreenLabel(fullscreen, fs.windowed)} <kbd>F</kbd>
                  </button>
                </div>
              </div>
            </div>
          ) : null}
          {armedAt != null ? (
            <div className="game-toast" role="status">
              esc again to leave
            </div>
          ) : null}
        </div>
      </FullscreenContext.Provider>
    </ShellContext.Provider>
  )
}

// the small button beside a game's esc: in the lobby's header, and in a corner while playing
export function FullscreenButton({ on, onClick, className = '' }: { on: boolean; onClick: () => void; className?: string }) {
  const label = fullscreenLabel(on, Boolean(useShellFullscreen()?.windowed))
  return (
    <button
      type="button"
      className={`game-full-btn ${className}`}
      aria-label={label}
      title={on ? `${label} (F, ${chord()} or esc)` : `${label} (F or ${chord()})`}
      onMouseDown={(e) => e.preventDefault()}
      onClick={onClick}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        {on ? (
          <path d="M6 2v4H2M10 2v4h4M6 14v-4H2M10 14v-4h4" />
        ) : (
          <path d="M2 6V2h4M14 6V2h-4M2 10v4h4M14 10v4h-4" />
        )}
      </svg>
    </button>
  )
}
