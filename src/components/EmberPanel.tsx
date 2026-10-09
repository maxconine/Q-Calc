import { useCallback, useEffect, useMemo, useRef, useState, type MutableRefObject, type ReactNode } from 'react'
import { nativeWindow } from '../lib/bridge'
import { closeReasonText, type PeerInfo, type PeerTransport } from '../lib/peer'
import { compileLevel, scoreOf } from '../lib/emberEngine'
import { LEVELS } from '../lib/emberLevels'
import { loadProgress, recordResult, saveProgress, unlockedCount, type EmberProgress } from '../lib/emberProgress'
import { createLocalGame, createOnlineGame, type EmberGame, type EmberScreen, type EmberView } from '../lib/emberSession'
import { createEmberSound } from '../lib/emberSound'
import { IN_DOWN, IN_LEFT, IN_RIGHT, IN_UP, type DeathCause, type Element, type GameState, type Level, type PlayerIndex, type Score } from '../lib/emberTypes'
import { drawThumb, EmberRenderer } from './emberDraw'
import './EmberPanel.css'

// ember & frost: `ember` then enter. the panel owns the title, the online lobby and the progress file;
// emberSession runs the game (select screen, levels, clock, link) and this feeds it keys and frames.
// esc goes back one screen: the app hands every esc (a keydown in the browser, __qcalcEscape in the mac
// app) to escapeLayer, which asks escapeRef first and closes the panel only when it says false

type Screen =
  | { k: 'title'; note?: string }
  | { k: 'lobby'; note?: string }
  | { k: 'game'; game: EmberGame; online: boolean; peer: string; id: number }

const NAMES: Record<PlayerIndex, string> = { 0: 'ember', 1: 'frost' }
const COLS = 4

const plainKey = (e: KeyboardEvent) => !e.metaKey && !e.ctrlKey && !e.altKey

// keys the game takes for itself; everything else (⌘C, ⌃D…) still reaches the bar. the mac app's boot script
// has already buffered it for the hidden input, so drop it from there too
function swallow(e: KeyboardEvent): void {
  e.preventDefault()
  e.stopPropagation()
  const w = nativeWindow()
  if (w?.__QCALC_KEYS?.length) w.__QCALC_KEYS = []
}

// enter, arrows, backspace and letters would otherwise reach the hidden bar and commit or move through the tape
const barKey = (e: KeyboardEvent) => e.key.length === 1 || e.key === 'Enter' || e.key.startsWith('Arrow') || e.key === 'Backspace'

// a level hint about who uses which keys is wrong online, where both sets steer your own character
const CONTROLS_HINT = /WASD|arrows/i

function fmtTime(secs: number): string {
  const s = Math.max(0, secs)
  const m = Math.floor(s / 60)
  const rest = s - m * 60
  return `${m}:${rest < 10 ? '0' : ''}${rest.toFixed(1)}`
}

// compiled once per level for thumbnails; a broken level just draws no picture
const compiledCache = new Map<number, Level | null>()
function compiledAt(i: number): Level | null {
  if (!compiledCache.has(i)) {
    let level: Level | null = null
    try {
      level = compileLevel(LEVELS[i]!)
    } catch {
      level = null
    }
    compiledCache.set(i, level)
  }
  return compiledCache.get(i) ?? null
}

export function EmberPanel({
  transport,
  escapeRef,
  onClose,
}: {
  transport: PeerTransport | null
  escapeRef: MutableRefObject<(() => boolean) | null>
  onClose: () => void
}) {
  const [screen, setScreen] = useState<Screen>({ k: 'title' })
  const screenRef = useRef(screen)
  screenRef.current = screen
  const [progress, setProgress] = useState<EmberProgress>(loadProgress)
  const progressRef = useRef(progress)
  progressRef.current = progress
  // what the running game shows, kept by EmberPlay so esc knows whether it's in a level or on select
  const gameScreenRef = useRef<EmberScreen>('select')
  // the lobby's own esc: steps back inside it, false when it's already on host/join
  const lobbyBackRef = useRef<(() => boolean) | null>(null)
  const gameIds = useRef(0)

  const unlocked = unlockedCount(progress, LEVELS)

  const startLocal = useCallback(() => {
    try {
      const game = createLocalGame(performance.now(), unlockedCount(progressRef.current, LEVELS))
      gameScreenRef.current = 'select'
      setScreen({ k: 'game', game, online: false, peer: '', id: ++gameIds.current })
    } catch {
      setScreen({ k: 'title', note: 'couldn’t start the game' })
    }
  }, [])

  // created right inside the lobby's 'open' handler, so the first message after it already has a game
  const startOnline = useCallback(
    (role: 'host' | 'guest', peer: string) => {
      if (!transport) return
      try {
        const game = createOnlineGame(role, (text) => transport.send(text), performance.now(), unlockedCount(progressRef.current, LEVELS))
        gameScreenRef.current = 'wait'
        const next: Screen = { k: 'game', game, online: true, peer: peer || 'friend', id: ++gameIds.current }
        screenRef.current = next
        setScreen(next)
      } catch {
        transport.close()
        setScreen({ k: 'lobby', note: 'couldn’t start the game' })
      }
    },
    [transport],
  )

  const endGame = useCallback(
    (to: 'title' | 'lobby', note?: string, quiet = false) => {
      const cur = screenRef.current
      if (cur.k === 'game') {
        if (!quiet) cur.game.leave()
        if (cur.online) transport?.close()
      }
      const next: Screen = to === 'lobby' && transport ? { k: 'lobby', note } : { k: 'title', note }
      screenRef.current = next
      setScreen(next)
    },
    [transport],
  )

  // messages and the link's end, for a running online game
  useEffect(() => {
    if (!transport) return
    return transport.subscribe((e) => {
      const cur = screenRef.current
      if (cur.k !== 'game' || !cur.online) return
      if (e.type === 'message') cur.game.receive(e.data, performance.now())
      else if (e.type === 'closed') endGame('lobby', closeReasonText(e.reason, transport.peerWord), true)
    })
  }, [transport, endGame])

  // the link and the game go when the panel does, however it closes
  useEffect(
    () => () => {
      const cur = screenRef.current
      if (cur.k === 'game') cur.game.leave()
      transport?.close()
    },
    [transport],
  )

  const back = useCallback((): boolean => {
    const cur = screenRef.current
    if (cur.k === 'title') return false
    if (cur.k === 'lobby') {
      if (lobbyBackRef.current?.()) return true
      transport?.close()
      setScreen({ k: 'title' })
      return true
    }
    if (gameScreenRef.current === 'play') cur.game.menu()
    else endGame(cur.online ? 'lobby' : 'title')
    return true
  }, [endGame, transport])

  useEffect(() => {
    escapeRef.current = back
    return () => {
      if (escapeRef.current === back) escapeRef.current = null
    }
  }, [back, escapeRef])

  const record = useCallback((levelId: string, score: Score) => {
    const next = recordResult(progressRef.current, levelId, score)
    progressRef.current = next
    setProgress(next)
    saveProgress(next)
  }, [])

  const toggleMute = useCallback(() => {
    const next = { ...progressRef.current, muted: !progressRef.current.muted }
    progressRef.current = next
    setProgress(next)
    saveProgress(next)
  }, [])

  // title keys; the lobby and the game listen for their own
  useEffect(() => {
    if (screen.k !== 'title') return
    const onKey = (e: KeyboardEvent) => {
      if (!plainKey(e) || e.key === 'Escape') return
      const key = e.key.toLowerCase()
      if (key === 'l' || (e.key === 'Enter' && !e.repeat)) {
        swallow(e)
        if (!e.repeat) startLocal()
      } else if (key === 'o' && transport) {
        swallow(e)
        if (!e.repeat) setScreen({ k: 'lobby' })
      } else if (barKey(e)) swallow(e)
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [screen.k, startLocal, transport])

  const head = (sub: string, mark = true) => (
    <div className="ember-head">
      {mark ? (
        <span className="ember-mark">
          <span className="ember-fire">ember</span> <span className="ember-amp">&amp;</span> <span className="ember-frost">frost</span>
        </span>
      ) : null}
      {sub ? <span className="ember-sub">{sub}</span> : null}
      <button type="button" className="ember-leave" onClick={() => back() || onClose()}>
        esc
      </button>
    </div>
  )

  return (
    <div className="ember" aria-label="ember and frost">
      {screen.k === 'title' ? (
        <>
          {head('', false)}
          <Title note={screen.note} online={Boolean(transport)} done={Object.keys(progress.levels).length} onLocal={startLocal} onOnline={() => setScreen({ k: 'lobby' })} />
        </>
      ) : screen.k === 'lobby' && transport ? (
        <>
          {head(transport.label)}
          <EmberLobby transport={transport} note={screen.note} backRef={lobbyBackRef} onOpen={startOnline} onBack={() => back()} />
        </>
      ) : screen.k === 'game' ? (
        <EmberPlay
          key={screen.id}
          game={screen.game}
          online={screen.online}
          peer={screen.peer}
          progress={progress}
          unlocked={unlocked}
          screenRef={gameScreenRef}
          head={head}
          onRecord={record}
          onMute={toggleMute}
          onEnded={(note) => endGame('lobby', note, true)}
        />
      ) : null}
    </div>
  )
}

function Title({ note, online, done, onLocal, onOnline }: { note?: string; online: boolean; done: number; onLocal: () => void; onOnline: () => void }) {
  return (
    <div className="ember-body ember-title">
      <h2 className="ember-logo" aria-label="ember and frost">
        <span className="ember-fire">ember</span>
        <span className="ember-amp">&amp;</span>
        <span className="ember-frost">frost</span>
      </h2>
      <p className="ember-line">two of you, one keyboard or two Macs. both get to their doors.</p>
      <div className="ember-actions">
        <button type="button" className="ember-button" onClick={onLocal}>
          same keyboard <kbd>L</kbd>
        </button>
        {online ? (
          <button type="button" className="ember-button" onClick={onOnline}>
            online <kbd>O</kbd>
          </button>
        ) : null}
      </div>
      <div className="ember-controls">
        <span>
          <b className="ember-fire">ember</b> <kbd>W</kbd>
          <kbd>A</kbd>
          <kbd>S</kbd>
          <kbd>D</kbd>
        </span>
        <span>
          <b className="ember-frost">frost</b> <kbd>←</kbd>
          <kbd>↑</kbd>
          <kbd>→</kbd>
          <kbd>↓</kbd>
        </span>
        <span>
          <kbd>S</kbd> / <kbd>↓</kbd> use a lever or mirror
        </span>
      </div>
      {note ? <p className="ember-note">{note}</p> : done ? <p className="ember-note">{`${done} of ${LEVELS.length} levels done`}</p> : null}
    </div>
  )
}

// ——— the online lobby: host shows a code, join finds a host (wi-fi) or goes straight to the code (relay).
// kept to itself so it can give way to a shared GameLobby later

type Lobby =
  | { k: 'menu'; note?: string }
  | { k: 'hosting'; code: string | null; renewed: boolean }
  | { k: 'browse'; peers: PeerInfo[]; pick: number; denied: boolean }
  | { k: 'code'; peer: PeerInfo | null; digits: string; note?: string }
  | { k: 'pairing'; peer: PeerInfo | null }

function EmberLobby({
  transport,
  note,
  backRef,
  onOpen,
  onBack,
}: {
  transport: PeerTransport
  note?: string
  backRef: MutableRefObject<(() => boolean) | null>
  onOpen: (role: 'host' | 'guest', peer: string) => void
  onBack: () => void
}) {
  const [lobby, setLobby] = useState<Lobby>({ k: 'menu', note })
  const lobbyRef = useRef(lobby)
  lobbyRef.current = lobby
  const discovers = transport.discovers
  const codeLength = transport.codeLength
  const onOpenRef = useRef(onOpen)
  onOpenRef.current = onOpen

  const set = useCallback((next: Lobby) => {
    lobbyRef.current = next
    setLobby(next)
  }, [])

  const host = useCallback(() => {
    set({ k: 'hosting', code: null, renewed: false })
    transport.host()
  }, [set, transport])

  const join = useCallback(() => {
    if (!transport.discovers) {
      set({ k: 'code', peer: null, digits: '' })
      return
    }
    set({ k: 'browse', peers: [], pick: 0, denied: false })
    transport.discover()
  }, [set, transport])

  const toMenu = useCallback(
    (note?: string) => {
      transport.close()
      set({ k: 'menu', note })
    },
    [set, transport],
  )

  useEffect(() => {
    backRef.current = () => {
      if (lobbyRef.current.k === 'menu') return false
      toMenu()
      return true
    }
    return () => {
      backRef.current = null
    }
  }, [backRef, toMenu])

  useEffect(
    () =>
      transport.subscribe((e) => {
        const cur = lobbyRef.current
        switch (e.type) {
          case 'hosting':
            if (cur.k === 'hosting') set({ k: 'hosting', code: e.code, renewed: Boolean(e.renewed) })
            return
          case 'peers':
            if (cur.k === 'browse') {
              const keep = cur.peers[cur.pick]?.id
              const pick = Math.max(0, e.peers.findIndex((p) => p.id === keep))
              set({ ...cur, peers: e.peers, pick })
            }
            return
          case 'denied':
            if (cur.k === 'browse') set({ ...cur, denied: true })
            return
          case 'open':
            if (transport.discovers) transport.stopDiscovery()
            onOpenRef.current(e.role, e.peer)
            return
          case 'closed':
            if (cur.k === 'pairing' && e.reason === 'wrong-code') {
              set({ k: 'code', peer: cur.peer, digits: '', note: 'wrong code · try again' })
              return
            }
            set({ k: 'menu', note: closeReasonText(e.reason, transport.peerWord) })
            return
        }
      }),
    [set, transport],
  )

  const submitCode = useCallback(
    (peer: PeerInfo | null, digits: string) => {
      if (!/^[0-9]+$/.test(digits) || digits.length !== transport.codeLength) return
      set({ k: 'pairing', peer })
      transport.join(peer?.id ?? '', digits)
    },
    [set, transport],
  )

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const cur = lobbyRef.current
      if (!plainKey(e) || e.key === 'Escape') return
      const key = e.key.toLowerCase()
      if (cur.k === 'menu') {
        if (key === 'h') {
          swallow(e)
          if (!e.repeat) host()
        } else if (key === 'j') {
          swallow(e)
          if (!e.repeat) join()
        } else if (e.key === 'Backspace') {
          swallow(e)
          onBack()
        } else if (barKey(e)) swallow(e)
        return
      }
      if (cur.k === 'browse') {
        swallow(e)
        const n = cur.peers.length
        if (e.key === 'ArrowDown' && n) set({ ...cur, pick: (cur.pick + 1) % n })
        else if (e.key === 'ArrowUp' && n) set({ ...cur, pick: (cur.pick - 1 + n) % n })
        else if (e.key === 'Enter' && cur.peers[cur.pick]) set({ k: 'code', peer: cur.peers[cur.pick]!, digits: '' })
        else if (e.key === 'Backspace') toMenu()
        return
      }
      if (cur.k === 'code') {
        swallow(e)
        const len = transport.codeLength
        if (/^[0-9]$/.test(e.key) && cur.digits.length < len) {
          const digits = cur.digits + e.key
          if (digits.length === len) submitCode(cur.peer, digits)
          else set({ ...cur, digits, note: undefined })
        } else if (e.key === 'Backspace') {
          if (cur.digits) set({ ...cur, digits: cur.digits.slice(0, -1) })
          else if (cur.peer) join()
          else toMenu()
        }
        return
      }
      if (cur.k === 'hosting' && e.key === 'Backspace') {
        swallow(e)
        toMenu()
        return
      }
      if (barKey(e)) swallow(e)
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [host, join, onBack, set, submitCode, toMenu, transport])

  const word = transport.peerWord
  const slots = Array.from({ length: codeLength }, (_, i) => i)
  switch (lobby.k) {
    case 'menu':
      return (
        <div className="ember-body">
          <p className="ember-line">
            you’re <b className="ember-fire">ember</b> if you host, <b className="ember-frost">frost</b> if you join
          </p>
          <div className="ember-actions">
            <button type="button" className="ember-button" onClick={host}>
              host <kbd>H</kbd>
            </button>
            <button type="button" className="ember-button" onClick={join}>
              join <kbd>J</kbd>
            </button>
          </div>
          <p className="ember-note">
            {lobby.note ? `${lobby.note} · ` : ''}
            <button type="button" className="ember-link" onClick={onBack}>
              ⌫ back
            </button>
          </p>
        </div>
      )
    case 'hosting':
      return (
        <div className="ember-body">
          <div className="ember-code" aria-label="pairing code">
            {slots.map((i) => (
              <span key={i}>{lobby.code?.[i] ?? '·'}</span>
            ))}
          </div>
          <p className="ember-line">
            {lobby.renewed ? 'too many wrong codes, so here’s a new one. ' : ''}
            {discovers ? `on the other ${word}: type ember, online, join, pick this ${word}, enter the code` : 'on the other side: type ember, online, join, enter the code'}
          </p>
          <p className="ember-note ember-wait">waiting for frost…</p>
        </div>
      )
    case 'browse':
      return (
        <div className="ember-body">
          {lobby.denied ? (
            <p className="ember-note">{closeReasonText('denied', transport.peerWord)}</p>
          ) : lobby.peers.length ? (
            <ul className="ember-peers" role="listbox">
              {lobby.peers.map((p, i) => (
                <li key={p.id} role="option" aria-selected={i === lobby.pick}>
                  <button type="button" className={i === lobby.pick ? 'on' : undefined} onClick={() => set({ k: 'code', peer: p, digits: '' })}>
                    {p.name}
                  </button>
                </li>
              ))}
            </ul>
          ) : (
            <p className="ember-line ember-wait">looking for {word}s hosting ember & frost…</p>
          )}
          <p className="ember-note">
            ↑↓ pick · ↵ choose ·{' '}
            <button type="button" className="ember-link" onClick={() => toMenu()}>
              ⌫ back
            </button>
          </p>
        </div>
      )
    case 'code':
    case 'pairing': {
      const digits = lobby.k === 'code' ? lobby.digits : ''
      return (
        <div className="ember-body">
          <p className="ember-line">{lobby.peer ? `code shown on ${lobby.peer.name}` : 'the code your friend’s Q Calc shows'}</p>
          <div className={`ember-code entry ${lobby.k === 'pairing' ? 'busy' : ''}`}>
            {slots.map((i) => (
              <span key={i} className={i === digits.length && lobby.k === 'code' ? 'caret' : undefined}>
                {lobby.k === 'pairing' ? '•' : (digits[i] ?? '')}
              </span>
            ))}
          </div>
          <p className="ember-note">{lobby.k === 'pairing' ? 'pairing…' : (lobby.note ?? `type the ${codeLength} digits · ⌫ back`)}</p>
        </div>
      )
    }
  }
}

// ——— the game: select screen and play, one frame loop for both

const MOVE: Record<string, { p: PlayerIndex; bit: number }> = {
  KeyA: { p: 0, bit: IN_LEFT },
  KeyD: { p: 0, bit: IN_RIGHT },
  KeyW: { p: 0, bit: IN_UP },
  KeyS: { p: 0, bit: IN_DOWN },
  ArrowLeft: { p: 1, bit: IN_LEFT },
  ArrowRight: { p: 1, bit: IN_RIGHT },
  ArrowUp: { p: 1, bit: IN_UP },
  ArrowDown: { p: 1, bit: IN_DOWN },
}

const OUCH: Record<DeathCause, Record<PlayerIndex, string>> = {
  lava: { 0: 'ember tripped in lava? odd · again', 1: 'frost melted · again' },
  water: { 0: 'ember fizzled out · again', 1: 'frost took a swim · again' },
  goo: { 0: 'ember got gooed · again', 1: 'frost got gooed · again' },
}

type Hud = {
  screen: EmberScreen
  level: number
  me: PlayerIndex | null
  unlocked: number
  note: string | null
  phase: GameState['phase'] | null
  deaths: number
  gems: Array<{ el: Element; taken: boolean }>
  ouch: string | null
  // bumps each time a level is entered, to replay the hint
  entry: number
  score: Score | null
}

const EMPTY_HUD: Hud = { screen: 'wait', level: 0, me: null, unlocked: 1, note: null, phase: null, deaths: 0, gems: [], ouch: null, entry: 0, score: null }

function gemsOf(level: Level | null, s: GameState | null): Hud['gems'] {
  if (!level || !s) return []
  return level.gems.map((g, i) => ({ el: g.el, taken: Boolean(s.taken[i]) }))
}

function EmberPlay({
  game,
  online,
  peer,
  progress,
  unlocked,
  screenRef,
  head,
  onRecord,
  onMute,
  onEnded,
}: {
  game: EmberGame
  online: boolean
  peer: string
  progress: EmberProgress
  unlocked: number
  screenRef: MutableRefObject<EmberScreen>
  head: (sub: string) => ReactNode
  onRecord: (levelId: string, score: Score) => void
  onMute: () => void
  onEnded: (note?: string) => void
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const timerRef = useRef<HTMLSpanElement>(null)
  const [hud, setHud] = useState<Hud>(EMPTY_HUD)
  const hudRef = useRef(hud)
  hudRef.current = hud
  const cb = useRef({ onRecord, onMute, onEnded })
  cb.current = { onRecord, onMute, onEnded }
  const muted = progress.muted
  const sound = useMemo(() => createEmberSound(), [])
  useEffect(() => sound.setMuted(muted), [sound, muted])

  // the frame loop: tick the session, draw, play sounds, and push the slow-changing bits into react
  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    let renderer: EmberRenderer | null = null
    try {
      renderer = new EmberRenderer(canvas)
      renderer.resize()
    } catch {
      renderer = null
    }
    const reduceMotion = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false
    let raf = 0
    let failed = false
    let shown = ''
    let entry = 0
    let lastLevel = -1
    let lastScreen: EmberScreen | null = null
    let ouch: string | null = null
    let score: Score | null = null

    const resize = () => renderer?.resize()
    const recolor = () => renderer?.refreshColors()
    const ro = typeof ResizeObserver !== 'undefined' ? new ResizeObserver(resize) : null
    ro?.observe(canvas)
    const dark = window.matchMedia?.('(prefers-color-scheme: dark)')
    dark?.addEventListener?.('change', recolor)
    const mo = new MutationObserver(recolor)
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme', 'class'] })

    const frame = (now: number) => {
      raf = requestAnimationFrame(frame)
      let v: EmberView
      try {
        v = game.tick(now)
      } catch (err) {
        // say it once, so a broken frame shows up without flooding the console 60 times a second
        if (!failed) console.error('ember & frost', err)
        failed = true
        return
      }
      if (v.ended != null) {
        cancelAnimationFrame(raf)
        cb.current.onEnded(v.ended || undefined)
        return
      }
      screenRef.current = v.screen
      const playing = v.screen === 'play' && v.compiled && v.state
      if (v.screen !== lastScreen || v.level !== lastLevel) {
        if (v.screen === 'play') {
          entry++
          renderer?.reset()
          // the canvas was hidden on select; its size may only be right now
          resize()
        }
        ouch = null
        score = null
        lastScreen = v.screen
        lastLevel = v.level
      }
      for (const e of v.events) {
        sound.play(e)
        if (e.k === 'die') ouch = e.cause ? OUCH[e.cause][e.p] : 'ouch · again'
        if (e.k === 'win' && v.compiled && v.state) {
          score = scoreOf(v.compiled, v.state)
          cb.current.onRecord(v.compiled.def.id, score)
        }
      }
      if (playing) {
        const s = v.state!
        if (s.phase === 'play' && s.tick < 2) ouch = null
        if (s.phase === 'won' && !score) score = scoreOf(v.compiled!, s)
        renderer?.draw(v.compiled!, v.prev, s, v.alpha, v.events, now, { me: v.me, reduceMotion })
        if (timerRef.current) timerRef.current.textContent = fmtTime(s.tick / 60)
      }
      const s = playing ? v.state! : null
      const gems = gemsOf(v.compiled, s)
      const key = [v.screen, v.level, v.me, v.unlocked, v.note, s?.phase, s?.deaths, gems.map((g) => +g.taken).join(''), ouch, entry, score?.stars].join('|')
      if (key === shown) return
      shown = key
      setHud({ screen: v.screen, level: v.level, me: v.me, unlocked: v.unlocked, note: v.note, phase: s?.phase ?? null, deaths: s?.deaths ?? 0, gems, ouch, entry, score })
    }
    raf = requestAnimationFrame(frame)

    return () => {
      cancelAnimationFrame(raf)
      ro?.disconnect()
      mo.disconnect()
      dark?.removeEventListener?.('change', recolor)
    }
  }, [game, screenRef, sound])

  // keys. held movement keys are a set so key repeat and overlapping presses don't matter
  useEffect(() => {
    const held = new Set<string>()
    const push = () => {
      const bits: [number, number] = [0, 0]
      for (const code of held) {
        const m = MOVE[code]
        if (m) bits[m.p] |= m.bit
      }
      const me = hudRef.current.me
      try {
        if (online && me != null) game.keys(me, bits[0] | bits[1])
        else {
          game.keys(0, bits[0])
          game.keys(1, bits[1])
        }
      } catch {
        // a session that isn't ready yet has nothing to steer
      }
    }
    // presses quicker than a render all see the same hud, so they chain from the last ask until the hud moves
    let asked: { from: number; to: number } | null = null
    const pick = (dx: number, dy: number) => {
      const h = hudRef.current
      const base = asked && asked.from === h.level ? asked.to : h.level
      const next = base + dx + dy * COLS
      if (next < 0 || next >= Math.min(LEVELS.length, h.unlocked)) return
      asked = { from: h.level, to: next }
      game.pick(next)
    }
    const onDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') return
      const move = MOVE[e.code]
      // a key already held keeps going even if ⌘ joins it; new modified presses belong to the bar
      if (!plainKey(e) && !(move && held.has(e.code))) return
      const h = hudRef.current
      if (h.screen === 'select') {
        swallow(e)
        // held arrows walk the grid at the system's repeat rate
        if (move) {
          if (move.bit === IN_LEFT) pick(-1, 0)
          else if (move.bit === IN_RIGHT) pick(1, 0)
          else if (move.bit === IN_UP) pick(0, -1)
          else pick(0, 1)
        } else if ((e.key === 'Enter' || e.key === ' ') && !e.repeat && h.level < h.unlocked) game.play()
        else if (e.key.toLowerCase() === 'm' && !e.repeat) cb.current.onMute()
        return
      }
      if (h.screen !== 'play') {
        if (barKey(e)) swallow(e)
        return
      }
      if (move) {
        swallow(e)
        if (!held.has(e.code)) {
          held.add(e.code)
          push()
        }
        return
      }
      if (!barKey(e)) return
      swallow(e)
      if (e.repeat) return
      const key = e.key.toLowerCase()
      if (key === 'r') game.retry()
      else if (key === 'm') cb.current.onMute()
      else if ((e.key === 'Enter' || e.key === ' ') && h.phase === 'won') game.next()
    }
    const onUp = (e: KeyboardEvent) => {
      if (!held.delete(e.code)) return
      swallow(e)
      push()
    }
    const onBlur = () => {
      held.clear()
      try {
        game.release()
      } catch {
        // same as above
      }
    }
    window.addEventListener('keydown', onDown, true)
    window.addEventListener('keyup', onUp, true)
    window.addEventListener('blur', onBlur)
    return () => {
      window.removeEventListener('keydown', onDown, true)
      window.removeEventListener('keyup', onUp, true)
      window.removeEventListener('blur', onBlur)
    }
  }, [game, online])

  const meName = hud.me != null ? NAMES[hud.me] : null
  const you = meName ? (
    <span className={`ember-you ${hud.me === 0 ? 'ember-fire' : 'ember-frost'}`}>you’re {meName}</span>
  ) : null
  const def = LEVELS[hud.level]

  return (
    <>
      {hud.screen === 'select' ? (
        <>
          {head(online ? `with ${peer}` : 'same keyboard')}
          <Select
            level={hud.level}
            unlocked={Math.max(hud.unlocked, online ? 0 : unlocked)}
            progress={progress}
            you={you}
            note={hud.note}
            muted={muted}
            onPick={(i) => game.pick(i)}
            onPlay={(i) => {
              game.pick(i)
              game.play()
            }}
          />
        </>
      ) : hud.screen === 'wait' ? (
        <>
          {head(online ? `with ${peer}` : '')}
          <div className="ember-body">
            <p className="ember-line ember-wait">{online ? `waiting for ${peer}…` : 'getting ready…'}</p>
          </div>
        </>
      ) : null}
      <div className={`ember-game ${hud.screen === 'play' ? '' : 'ember-hidden'}`}>
        <div className="ember-hud">
          <span className="ember-hud-level">
            <b>{hud.level + 1}</b> {def?.name ?? ''}
          </span>
          <span className="ember-gems" aria-label="gems">
            {hud.gems.map((g, i) => (
              <i key={i} className={`ember-gem ${g.el} ${g.taken ? 'got' : ''}`} />
            ))}
          </span>
          {hud.deaths ? <span className="ember-deaths">{hud.deaths === 1 ? '1 try' : `${hud.deaths} tries`}</span> : null}
          {you}
          <span className="ember-timer" ref={timerRef}>
            0:00.0
          </span>
        </div>
        <div className="ember-stage">
          <canvas ref={canvasRef} className="ember-canvas" aria-label="level" />
          {def?.hint && hud.phase === 'play' && !(online && CONTROLS_HINT.test(def.hint)) ? (
            <div key={hud.entry} className="ember-hint">
              {def.hint}
            </div>
          ) : null}
          {hud.ouch && hud.phase === 'dead' ? <div className="ember-ouch">{hud.ouch}</div> : null}
          {hud.phase === 'won' && hud.score && def ? <Win score={hud.score} par={def.par} last={hud.level >= LEVELS.length - 1} /> : null}
          {hud.note ? <div className="ember-peer-note">{hud.note}</div> : null}
        </div>
        <p className="ember-note ember-keys">
          {online ? 'WASD or arrows' : 'ember WASD · frost arrows'} · S/↓ use · R retry · M {muted ? 'unmute' : 'mute'} · esc levels
        </p>
      </div>
    </>
  )
}

function Stars({ n, pop = false }: { n: number; pop?: boolean }) {
  return (
    <span className={`ember-stars ${pop ? 'pop' : ''}`} aria-label={`${n} of 3 stars`}>
      {[0, 1, 2].map((i) => (
        <span key={i} className={i < n ? 'on' : undefined} style={pop ? { animationDelay: `${180 + i * 160}ms` } : undefined}>
          ★
        </span>
      ))}
    </span>
  )
}

function Win({ score, par, last }: { score: Score; par: number; last: boolean }) {
  const got = score.gems[0] + score.gems[1]
  const all = score.total[0] + score.total[1]
  return (
    <div className="ember-win" role="status">
      <p className="ember-win-title">both home</p>
      <Stars n={score.stars} pop />
      <p className="ember-win-line">
        <span className={score.secs <= par ? 'ember-good' : undefined}>{fmtTime(score.secs)}</span>
        <span className="ember-dim"> / par {fmtTime(par)}</span>
        {all ? <span> · gems {got}/{all}</span> : null}
      </p>
      <p className="ember-note">{last ? 'that was the last level · R retry · esc levels' : '↵ next level · R retry · esc levels'}</p>
    </div>
  )
}

function Select({
  level,
  unlocked,
  progress,
  you,
  note,
  muted,
  onPick,
  onPlay,
}: {
  level: number
  unlocked: number
  progress: EmberProgress
  you: ReactNode
  note: string | null
  muted: boolean
  onPick: (i: number) => void
  onPlay: (i: number) => void
}) {
  const gridRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    gridRef.current?.querySelector<HTMLElement>('.ember-card.on')?.scrollIntoView({ block: 'nearest' })
  }, [level])
  return (
    <div className="ember-select">
      <div className="ember-grid" ref={gridRef} role="listbox" aria-label="levels">
        {LEVELS.map((def, i) => {
          const best = progress.levels[def.id]
          const locked = i >= unlocked
          return (
            <button
              key={def.id}
              type="button"
              role="option"
              aria-selected={i === level}
              aria-disabled={locked}
              className={`ember-card ${i === level ? 'on' : ''} ${locked ? 'locked' : ''}`}
              onClick={() => (locked ? onPick(i) : onPlay(i))}
            >
              <Thumb index={i} />
              <span className="ember-card-name">
                <b>{i + 1}</b> {def.name}
              </span>
              <span className="ember-card-meta">
                {locked ? (
                  <span className="ember-lock">locked</span>
                ) : (
                  <>
                    <Stars n={best?.stars ?? 0} />
                    {best?.secs != null ? <span>{fmtTime(best.secs)}</span> : null}
                    {best?.gems ? <span className="ember-card-gems">◆ {best.gems}</span> : null}
                  </>
                )}
              </span>
            </button>
          )
        })}
      </div>
      <p className="ember-note">
        {note ? `${note} · ` : ''}
        {you}
        {you ? ' · ' : ''}arrows or WASD pick · ↵ play · M {muted ? 'unmute' : 'mute'} · esc back
      </p>
    </div>
  )
}

function Thumb({ index }: { index: number }) {
  const ref = useRef<HTMLCanvasElement>(null)
  useEffect(() => {
    const canvas = ref.current
    const level = compiledAt(index)
    if (!canvas || !level) return
    const dpr = Math.max(1, Math.round(window.devicePixelRatio || 1))
    canvas.width = 144 * dpr
    canvas.height = 81 * dpr
    try {
      drawThumb(canvas, level)
    } catch {
      // no picture beats no select screen
    }
  }, [index])
  return (
    <span className="ember-thumb">
      <canvas ref={ref} aria-hidden="true" />
    </span>
  )
}
