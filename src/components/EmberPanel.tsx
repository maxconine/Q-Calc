import { useCallback, useEffect, useMemo, useRef, useState, type MutableRefObject, type ReactNode } from 'react'
import { nativeWindow } from '../lib/bridge'
import { gameNow } from '../lib/gameClock'
import { useGameShell, useShellFullscreen } from '../lib/gameShellContext'
import { closeReasonText, type PeerTransport } from '../lib/peer'
import { fmtPar, fmtTime, levelDifficulty, levelLength, MAX_DIFFICULTY } from '../lib/ember'
import { compileLevel, scoreOf, stageOf } from '../lib/emberEngine'
import { LEVELS } from '../lib/emberLevels'
import { finishedCount, firstUnfinished, improved, loadProgress, recordResult, saveProgress, type EmberProgress } from '../lib/emberProgress'
import { createLocalGame, createOnlineGame, type EmberGame, type EmberScreen, type EmberView } from '../lib/emberSession'
import { createEmberSound } from '../lib/emberSound'
import { ELEMENTS, IN_DOWN, IN_LEFT, IN_RIGHT, IN_UP, STEP_HZ, type DeathCause, type GameState, type Level, type PlayerIndex, type Score, type Stage } from '../lib/emberTypes'
import { drawThumb, EmberRenderer } from './emberDraw'
import { GameLobby } from './GameLobby'
import './EmberPanel.css'

// ember & frost: `ember` then enter. the shared lobby picks one keyboard or a room online; the panel owns the
// progress file, and emberSession runs the game (select screen, levels, clock, link) while this feeds it keys
// and frames. esc in a level pauses (or warns, online) and then goes back to the level picker; from the picker
// it leaves to the lobby. ⌃F fills the screen (the game shell's useGameFullscreen, GAME_FULLSCREEN.md): the
// first esc then only leaves fullscreen

// what the play screen spends above and below the picture at the bar's size: the hud row, the keys line and
// the two gaps between them and the stage. the fullscreen layout keeps this much (scaled) clear of the picture
const FS_CHROME = 54
// the root's own padding above and below, which fullscreen replaces
const FS_PAD = 12

const NAMES: Record<PlayerIndex, string> = { 0: 'ember', 1: 'frost' }
// the select grid: this many cards a row (the css grid says the same), so 14 levels are three rows and fit
// the bar without scrolling
const COLS = 5
// a card's picture at the bar's size, css px
const THUMB_W = 144
const THUMB_H = 81

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

export function EmberPanel({ transport }: { transport: PeerTransport | null }) {
  const shell = useGameShell()
  // the shell's fullscreen (⌃F), laid out for ember's picture and hud
  const fs = useShellFullscreen()
  const [progress, setProgress] = useState<EmberProgress>(loadProgress)
  const progressRef = useRef(progress)
  progressRef.current = progress
  // what the running game shows, kept by EmberPlay so esc knows whether it's in a level or on select
  const gameScreenRef = useRef<EmberScreen>('select')
  // the game that's running, local or online
  const gameRef = useRef<EmberGame | null>(null)
  // an online game's end, for when the link closes under it
  const endRef = useRef<((note?: string) => void) | null>(null)

  useEffect(() => shell?.fullscreenLayout({ chrome: FS_CHROME, pad: FS_PAD }), [shell])

  // made right inside the lobby's 'open' handler, so the first message after it already has a game
  const startOnline = useCallback(
    (role: 'host' | 'guest') => {
      if (!transport) return
      try {
        gameRef.current = createOnlineGame(role, (text) => transport.send(text), gameNow(), firstUnfinished(progressRef.current, LEVELS))
        gameScreenRef.current = 'wait'
      } catch {
        gameRef.current = null
      }
    },
    [transport],
  )

  // messages and the link's end, for a running online game
  useEffect(() => {
    if (!transport) return
    return transport.subscribe((e) => {
      if (e.type === 'message') gameRef.current?.receive(e.data, gameNow())
      else if (e.type === 'closed') endRef.current?.(closeReasonText(e.reason, transport.peerWord))
    })
  }, [transport])

  const record = useCallback((levelId: string, score: Score, deaths: number) => {
    const next = recordResult(progressRef.current, levelId, score, deaths)
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

  const head = (sub: string) => (
    <div className="ember-head">
      <span className="ember-mark">
        <span className="ember-fire">ember</span> <span className="ember-amp">&amp;</span> <span className="ember-frost">frost</span>
      </span>
      {sub ? <span className="ember-sub">{sub}</span> : null}
      <button type="button" className="ember-leave" onClick={() => shell?.escape()}>
        esc
      </button>
    </div>
  )

  const fsHint = fs?.hint ?? ''
  const play = (game: EmberGame | null, online: boolean, peer: string, onEnd: (note?: string) => void) =>
    game ? (
      <EmberRun
        game={game}
        online={online}
        peer={peer}
        progress={progress}
        fsHint={fsHint}
        fsOn={Boolean(fs?.on)}
        screenRef={gameScreenRef}
        gameRef={gameRef}
        endRef={online ? endRef : null}
        head={head}
        onRecord={record}
        onMute={toggleMute}
        onEnded={onEnd}
      />
    ) : (
      <EmberFailed onEnd={onEnd} />
    )

  const done = finishedCount(progress, LEVELS)
  return (
    <div className="ember" aria-label="ember and frost">
      <GameLobby
        game="ember"
        name="ember & frost"
        blurb="two of you, on one keyboard or two computers anywhere. both get to their doors. you’re ember if you host, frost if you join."
        transport={transport}
        sub={done ? `${done} of ${LEVELS.length} levels done` : ''}
        online={(p) => play(gameRef.current, true, p.peer, p.onEnd)}
        onOpen={startOnline}
        local={{
          prompt: '',
          options: ['same keyboard'],
          play: (_, onEnd) => {
            gameScreenRef.current = 'select'
            return <EmberLocal start={firstUnfinished(progressRef.current, LEVELS)} play={(game) => play(game, false, '', onEnd)} />
          },
        }}
        footer={<Controls fsHint={fsHint} />}
        ownHead
        idle={() => gameScreenRef.current !== 'play'}
        leave={() => {
          // from a level, esc's leave goes back to the level picker, as it always has
          if (gameScreenRef.current !== 'play' || !gameRef.current) return false
          gameRef.current.menu()
          return true
        }}
      />
    </div>
  )
}

// a game on one keyboard, made once for as long as it's played
function EmberLocal({ start, play }: { start: number; play: (game: EmberGame | null) => ReactNode }) {
  const [game] = useState<EmberGame | null>(() => {
    try {
      return createLocalGame(gameNow(), start)
    } catch {
      return null
    }
  })
  return <>{play(game)}</>
}

function EmberFailed({ onEnd }: { onEnd: (note?: string) => void }) {
  useEffect(() => onEnd('couldn’t start the game'), [onEnd])
  return null
}

// the running game's lifetime: it's the one esc and the link talk to, and it's left when it goes
function EmberRun(props: Omit<Parameters<typeof EmberPlay>[0], 'onEnded'> & {
  gameRef: MutableRefObject<EmberGame | null>
  endRef: MutableRefObject<((note?: string) => void) | null> | null
  onEnded: (note?: string) => void
}) {
  const { game, gameRef, endRef, onEnded, ...rest } = props
  const ended = useRef(false)
  // the lobby hands a new onEnded each render; only the game going may leave it, not a re-render
  const endedRef = useRef(onEnded)
  endedRef.current = onEnded
  useEffect(() => {
    gameRef.current = game
    if (endRef) endRef.current = (note) => {
      ended.current = true
      endedRef.current(note)
    }
    return () => {
      if (gameRef.current === game) gameRef.current = null
      if (endRef) endRef.current = null
      // strict mode (development) runs this and the effect again at once: leave only if the game didn't come back
      queueMicrotask(() => {
        if (!ended.current && gameRef.current !== game) game.leave()
      })
    }
  }, [endRef, game, gameRef])
  return (
    <EmberPlay
      {...rest}
      game={game}
      onEnded={(note) => {
        ended.current = true
        endedRef.current(note)
      }}
    />
  )
}

function Controls({ fsHint }: { fsHint: string }) {
  return (
    <>
      <p className="ember-note ember-blurb">
        {LEVELS.length} long levels, every one open: lights out, gravity upside down, trampolines, keys, and checkpoints on the way.
      </p>
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
          <kbd>S</kbd> / <kbd>↓</kbd> use a lever, lamp or switch
        </span>
      </div>
      {/* said out loud for five seconds as the game opens; the keys line in a level keeps saying it */}
      {fsHint ? <p className="ember-fs-hint game-fs-hint">{fsHint}</p> : null}
    </>
  )
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
  lava: { 0: 'ember tripped in lava? odd', 1: 'frost melted' },
  water: { 0: 'ember fizzled out', 1: 'frost took a swim' },
  goo: { 0: 'ember got gooed', 1: 'frost got gooed' },
  crush: { 0: 'ember got walled in', 1: 'frost got walled in' },
}

// one line over the top of the picture at a time; the newest wins. hint: the level's or a stage's opening
// line. flag: a checkpoint taken. back: both players are back after a death. ouch: someone just died (stays
// while the death lasts). each new one gets a new id, which replays its animation
type Banner = { id: number; kind: 'hint' | 'flag' | 'back' | 'ouch'; title?: string; text: string }

type Hud = {
  screen: EmberScreen
  level: number
  me: PlayerIndex | null
  note: string | null
  phase: GameState['phase'] | null
  deaths: number
  // the stage being played and how many the level has
  stage: number
  stages: number
  // the checkpoint the pair would come back to (-1: the stage's start) and how many this stage has
  cp: number
  cps: number
  // over the whole level
  gems: [number, number]
  total: [number, number]
  keys: number
  // the clock has passed the target time
  over: boolean
  banner: Banner | null
  // bumps each time a level begins (entered, or begun again), to replay the hints
  entry: number
  score: Score | null
  // what this finish beat
  fresh: Fresh | null
}

type Fresh = ReturnType<typeof improved>

const EMPTY_HUD: Hud = {
  screen: 'wait',
  level: 0,
  me: null,
  note: null,
  phase: null,
  deaths: 0,
  stage: 0,
  stages: 1,
  cp: -1,
  cps: 0,
  gems: [0, 0],
  total: [0, 0],
  keys: 0,
  over: false,
  banner: null,
  entry: 0,
  score: null,
  fresh: null,
}

// where a death sends the pair, said two ways: while they're gone, and once they're back
function backTo(s: GameState, arrived: boolean): string {
  const at = arrived ? 'back at' : 'back to'
  if (s.cp >= 0) return `${at} checkpoint ${s.cp + 1}`
  return s.stage > 0 ? `${at} the start of the stage` : `${at} the start`
}

function stageTitle(level: Level, index: number): string | undefined {
  if (level.stages.length < 2) return undefined
  const name = level.stages[index]?.def.name
  return `stage ${index + 1} of ${level.stages.length}${name ? ` · ${name}` : ''}`
}

function EmberPlay({
  game,
  online,
  peer,
  progress,
  fsHint,
  fsOn,
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
  // how to go fullscreen (or leave it), worded for this platform, and whether it's on
  fsHint: string
  fsOn: boolean
  screenRef: MutableRefObject<EmberScreen>
  head: (sub: string) => ReactNode
  onRecord: (levelId: string, score: Score, deaths: number) => void
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
  const progressRef = useRef(progress)
  progressRef.current = progress
  const muted = progress.muted
  const sound = useMemo(() => createEmberSound(), [])
  useEffect(() => sound.setMuted(muted), [sound, muted])
  // browsers allow only a few audio contexts at once, so each game closes its own
  useEffect(() => () => sound.dispose(), [sound])

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
    let score: Score | null = null
    let fresh: Fresh | null = null
    let banner: Banner | null = null
    let banners = 0
    // a move to a screen with another pixel ratio doesn't resize the canvas, so the frame loop watches it
    let dpr = window.devicePixelRatio
    // the last frame's state, to tell what just happened. read from the state and not only from events, so
    // the guest's side of an online game says the same things even when an event was lost on the way
    let seen: { tick: number; stage: number; cp: number; deaths: number; phase: GameState['phase'] } | null = null
    // someone died and the pair isn't back yet
    let owed = false
    // why, from the die event, until the death is shown
    let cause: string | null = null

    const say = (kind: Banner['kind'], text: string, title?: string) => {
      banner = text || title ? { id: ++banners, kind, text, title } : null
    }

    const resize = () => renderer?.resize()
    const recolor = () => renderer?.refreshColors()
    const ro = typeof ResizeObserver !== 'undefined' ? new ResizeObserver(resize) : null
    ro?.observe(canvas)
    const dark = window.matchMedia?.('(prefers-color-scheme: dark)')
    dark?.addEventListener?.('change', recolor)
    const mo = new MutationObserver(recolor)
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme', 'class'] })

    const frame = () => {
      raf = requestAnimationFrame(frame)
      // the game clock stands still while the shell has the game paused
      const now = gameNow()
      if (window.devicePixelRatio !== dpr) {
        dpr = window.devicePixelRatio
        resize()
      }
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
      let entered = false
      if (v.screen !== lastScreen || v.level !== lastLevel) {
        entered = v.screen === 'play'
        lastScreen = v.screen
        lastLevel = v.level
        seen = null
        banner = null
      }
      for (const e of v.events) {
        sound.play(e)
        if (e.k === 'die') cause = e.cause ? OUCH[e.cause][e.p] : null
      }
      let sc: Score | null = null
      let stage: Stage | null = null
      if (playing) {
        const level = v.compiled!
        const s = v.state!
        stage = stageOf(level, s)
        sc = scoreOf(level, s)
        // the clock never goes back inside a level, and nor do the stage or the deaths: it began again (⇧R,
        // R after a win, the other side's restart). a guest's predicted clock can step back a frame or two
        const again = seen != null && (s.tick + 20 < seen.tick || s.stage < seen.stage || s.deaths < seen.deaths)
        if (entered || again || !seen) {
          entry++
          renderer?.reset()
          // the canvas was hidden on select; its size may only be right now
          resize()
          score = null
          fresh = null
          owed = false
          cause = null
          const hint = level.def.hint && !(online && CONTROLS_HINT.test(level.def.hint)) ? level.def.hint : (stage.def.hint ?? '')
          say('hint', hint, stageTitle(level, s.stage))
        } else if (s.stage !== seen.stage) {
          owed = false
          say('hint', stage.def.hint ?? '', stageTitle(level, s.stage))
        } else {
          if (s.deaths > seen.deaths) owed = true
          if (s.phase === 'dead' && seen.phase !== 'dead') {
            owed = true
            say('ouch', backTo(s, false), cause ?? 'ouch')
            cause = null
          } else if (owed && s.phase === 'play') {
            owed = false
            say('back', backTo(s, true))
          } else if (s.cp !== seen.cp && s.cp >= 0 && s.deaths === seen.deaths) {
            say('flag', stage.checkpoints.length > 1 ? `checkpoint ${s.cp + 1} of ${stage.checkpoints.length}` : 'checkpoint')
          }
        }
        if (s.phase === 'won' && !score) {
          // by the state, not the win event: the guest's copy of the event can be late or lost
          score = sc
          fresh = improved(progressRef.current.levels[level.def.id], sc, s.deaths)
          cb.current.onRecord(level.def.id, sc, s.deaths)
          banner = null
        }
        seen = { tick: s.tick, stage: s.stage, cp: s.cp, deaths: s.deaths, phase: s.phase }
        try {
          renderer?.draw(stage, v.prev, s, v.alpha, v.events, now, { me: v.me, reduceMotion })
        } catch (err) {
          // a picture that can't be drawn mustn't take the hud, the clock and esc with it. said once
          if (!failed) console.error('ember & frost', err)
          failed = true
        }
        if (timerRef.current) timerRef.current.textContent = fmtTime(s.tick / STEP_HZ)
      }
      const s = playing ? v.state! : null
      const over = Boolean(s && v.compiled && s.tick / STEP_HZ > v.compiled.def.par)
      const key = [v.screen, v.level, v.me, v.note, s?.phase, s?.stage, s?.cp, s?.deaths, s?.keys, sc?.gems.join(','), over, banner?.id, entry, score?.stars].join('|')
      if (key === shown) return
      shown = key
      setHud({
        screen: v.screen,
        level: v.level,
        me: v.me,
        note: v.note,
        phase: s?.phase ?? null,
        deaths: s?.deaths ?? 0,
        stage: s?.stage ?? 0,
        stages: v.compiled?.stages.length ?? 1,
        cp: s?.cp ?? -1,
        cps: stage?.checkpoints.length ?? 0,
        gems: sc?.gems ?? [0, 0],
        total: sc?.total ?? [0, 0],
        keys: s?.keys ?? 0,
        over,
        banner,
        entry,
        score,
        fresh,
      })
    }
    raf = requestAnimationFrame(frame)

    return () => {
      cancelAnimationFrame(raf)
      ro?.disconnect()
      mo.disconnect()
      dark?.removeEventListener?.('change', recolor)
    }
  }, [game, online, screenRef, sound])

  // keys. held movement keys are a set so key repeat and overlapping presses don't matter. it follows what's down on
  // every screen, so a level starts with whatever is already held
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
      const next = gridStep(base, dx, dy, LEVELS.length, COLS)
      if (next === base) return
      asked = { from: h.level, to: next }
      game.pick(next)
    }
    const onDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') return
      const move = MOVE[e.code]
      // a key already held keeps going even if ⌘ joins it; new modified presses belong to the bar
      if (!plainKey(e) && !(move && held.has(e.code))) return
      const h = hudRef.current
      // the screen as of the last frame: the hud trails it by a render, and a level can start in between
      const screen = screenRef.current
      if (move && !held.has(e.code)) {
        held.add(e.code)
        push()
      }
      if (screen === 'select') {
        swallow(e)
        // held arrows walk the grid at the system's repeat rate
        if (move) {
          if (move.bit === IN_LEFT) pick(-1, 0)
          else if (move.bit === IN_RIGHT) pick(1, 0)
          else if (move.bit === IN_UP) pick(0, -1)
          else pick(0, 1)
        } else if (e.key === 'Home') pick(-LEVELS.length, 0)
        else if (e.key === 'End') pick(LEVELS.length, 0)
        else if ((e.key === 'Enter' || e.key === ' ') && !e.repeat) game.play()
        else if (e.key.toLowerCase() === 'm' && !e.repeat) cb.current.onMute()
        return
      }
      if (screen !== 'play') {
        if (barKey(e)) swallow(e)
        return
      }
      if (move) {
        swallow(e)
        return
      }
      if (!barKey(e)) return
      swallow(e)
      if (e.repeat) return
      const key = e.key.toLowerCase()
      // R alone is the small undo, back to the checkpoint; the whole level again takes shift as well, so it
      // can't be hit by a thumb on the way to a jump. both go through the session, which asks the host online
      if (key === 'r') {
        if (e.shiftKey) game.restart()
        else game.retry()
      } else if (key === 'm') cb.current.onMute()
      else if ((e.key === 'Enter' || e.key === ' ') && h.phase === 'won') game.next()
    }
    const onUp = (e: KeyboardEvent) => {
      // macOS sends no keyup for a key let go while ⌘ is down, so letting go of ⌘ lets go of everything
      if (e.key === 'Meta' && held.size) {
        held.clear()
        push()
        return
      }
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
  }, [game, online, screenRef])

  const meName = hud.me != null ? NAMES[hud.me] : null
  const you = meName ? (
    <span className={`ember-you ${hud.me === 0 ? 'ember-fire' : 'ember-frost'}`}>you’re {meName}</span>
  ) : null
  const def = LEVELS[hud.level]
  const banner = hud.banner
  // a death's line stays for as long as the death does; the others time themselves out
  const showBanner = banner && (banner.kind === 'ouch' ? hud.phase === 'dead' : hud.phase === 'play' || hud.phase === 'dead')
  const allGems = hud.total[0] + hud.total[1]

  return (
    <>
      {hud.screen === 'select' ? (
        <div className="ember-screen game-fs-screen">
          {head(online ? `with ${peer}` : 'same keyboard')}
          <Select
            level={hud.level}
            progress={progress}
            you={you}
            note={hud.note}
            muted={muted}
            fsHint={fsHint}
            onPlay={(i) => {
              game.pick(i)
              game.play()
            }}
          />
        </div>
      ) : hud.screen === 'wait' ? (
        <div className="ember-screen game-fs-screen">
          {head(online ? `with ${peer}` : '')}
          <div className="ember-body">
            <p className="ember-line ember-wait">{online ? `waiting for ${peer}…` : 'getting ready…'}</p>
          </div>
        </div>
      ) : null}
      <div className={`ember-game game-fs-column ${hud.screen === 'play' ? '' : 'ember-hidden'}`}>
        <div className="ember-hud game-fs-ui">
          <span className="ember-hud-level">
            <b>{hud.level + 1}</b> {def?.name ?? ''}
          </span>
          {hud.stages > 1 ? (
            <span className="ember-chip" aria-label={`stage ${hud.stage + 1} of ${hud.stages}`}>
              stage {hud.stage + 1}/{hud.stages}
            </span>
          ) : null}
          {hud.cps ? (
            <span
              key={`cp${hud.stage}.${hud.cp}`}
              className={`ember-chip ember-flag ${hud.cp >= 0 ? 'on' : ''}`}
              aria-label={hud.cp >= 0 ? `checkpoint ${hud.cp + 1} of ${hud.cps}` : `no checkpoint yet, ${hud.cps} in this stage`}
            >
              ⚑ {Math.max(0, hud.cp + 1)}/{hud.cps}
            </span>
          ) : null}
          {allGems ? <Gems gems={hud.gems} total={hud.total} /> : null}
          {hud.keys ? (
            <span className="ember-keys-held" aria-label={`${hud.keys} ${hud.keys === 1 ? 'key' : 'keys'} carried`}>
              <i className="ember-key" />
              {hud.keys > 1 ? `×${hud.keys}` : ''}
            </span>
          ) : null}
          {hud.deaths ? <span className="ember-deaths">{hud.deaths === 1 ? '1 death' : `${hud.deaths} deaths`}</span> : null}
          {you}
          <span className={`ember-clock ${hud.over ? 'over' : ''}`} aria-label="time, and the target time">
            <span className="ember-timer" ref={timerRef}>
              0:00.0
            </span>
            {def ? <span className="ember-par"> / {fmtPar(def.par)}</span> : null}
          </span>
        </div>
        <div className="ember-stage">
          <canvas ref={canvasRef} className="ember-canvas" aria-label="level" />
          {/* everything written over the picture: one layer that grows with the hud in fullscreen. a sibling of
              the canvas, never an ancestor, so the canvas keeps its real css pixels */}
          <div className="ember-over game-fs-ui">
            {showBanner ? (
              <div key={banner.id} className={`ember-banner ${banner.kind}`} role="status">
                {banner.title ? <b>{banner.title}</b> : null}
                {banner.title && banner.text ? <span className="ember-dim"> · </span> : null}
                {banner.text}
              </div>
            ) : null}
            {hud.phase === 'clear' ? (
              <div className="ember-clear" role="status">
                <p className="ember-win-title">stage {hud.stage + 1} clear</p>
                <p className="ember-note">
                  {hud.stage + 2 >= hud.stages ? 'the last stage is next' : `stage ${hud.stage + 2} of ${hud.stages} is next`}
                </p>
              </div>
            ) : null}
            {hud.phase === 'won' && hud.score && def ? (
              <Win score={hud.score} par={def.par} deaths={hud.deaths} fresh={hud.fresh} last={hud.level >= LEVELS.length - 1} />
            ) : null}
            {hud.note ? <div className="ember-peer-note">{hud.note}</div> : null}
            {/* again as each level starts, small and out of the way */}
            {hud.screen === 'play' ? (
              <div key={`fs${hud.entry}`} className="ember-fs-tip game-fs-hint">
                {fsHint}
              </div>
            ) : null}
          </div>
        </div>
        <p className="ember-note ember-keys game-fs-ui">
          {online ? 'WASD or arrows' : 'ember WASD · frost arrows'} · S/↓ use · R checkpoint · ⇧R restart level · M {muted ? 'unmute' : 'mute'} · {fsHint}
          {fsOn ? '' : ' · esc levels'}
        </p>
      </div>
    </>
  )
}

// a level's gems, both colours, over all its stages. a handful shows as pips; a long level's as counts
function Gems({ gems, total }: { gems: [number, number]; total: [number, number] }) {
  const label = `gems: ember ${gems[0]} of ${total[0]}, frost ${gems[1]} of ${total[1]}`
  if (total[0] + total[1] <= 8) {
    return (
      <span className="ember-gems" aria-label={label}>
        {ELEMENTS.map((el, c) => Array.from({ length: total[c]! }, (_, i) => <i key={`${el}${i}`} className={`ember-gem ${el} ${i < gems[c]! ? 'got' : ''}`} />))}
      </span>
    )
  }
  return (
    <span className="ember-gems counts" aria-label={label}>
      {ELEMENTS.map((el, c) =>
        total[c] ? (
          <span key={el} className={`ember-gem-count ${el}`}>
            <i className={`ember-gem ${el} ${gems[c] === total[c] ? 'got' : ''}`} />
            {gems[c]}/{total[c]}
          </span>
        ) : null,
      )}
    </span>
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

function Pips({ n }: { n: number }) {
  return (
    <span className="ember-pips" aria-label={`difficulty ${n} of ${MAX_DIFFICULTY}`}>
      {Array.from({ length: MAX_DIFFICULTY }, (_, i) => (
        <i key={i} className={i < n ? 'on' : undefined} />
      ))}
    </span>
  )
}

function Win({ score, par, deaths, fresh, last }: { score: Score; par: number; deaths: number; fresh: Fresh | null; last: boolean }) {
  const got = score.gems[0] + score.gems[1]
  const all = score.total[0] + score.total[1]
  const news = fresh ? [fresh.secs ? 'best time' : '', fresh.gems ? 'most gems' : '', fresh.deaths ? 'fewest deaths' : ''].filter(Boolean) : []
  return (
    <div className="ember-win" role="status">
      <p className="ember-win-title">both home</p>
      <Stars n={score.stars} pop />
      <p className="ember-win-line">
        <span className={score.secs <= par ? 'ember-good' : undefined}>{fmtTime(score.secs)}</span>
        <span className="ember-dim"> / {fmtPar(par)}</span>
        {all ? (
          <span>
            {' '}
            · <span className={got === all ? 'ember-good' : undefined}>gems {got}/{all}</span>
          </span>
        ) : null}
        <span> · {deaths === 0 ? 'no deaths' : deaths === 1 ? '1 death' : `${deaths} deaths`}</span>
      </p>
      {news.length ? <p className="ember-win-new">new: {news.join(' · ')}</p> : null}
      <p className="ember-note">{last ? 'that was the last level · R again · esc levels' : '↵ next level · R again · esc levels'}</p>
    </div>
  )
}

// the select grid's arrows: sideways runs on through the rows; down from above a short last row lands on its
// last card; anything else that would leave the grid stays put
function gridStep(from: number, dx: number, dy: number, count: number, cols: number): number {
  if (count < 1) return 0
  if (dx) return Math.min(count - 1, Math.max(0, from + dx))
  const to = from + dy * cols
  if (to < 0) return from
  if (to < count) return to
  return Math.floor(from / cols) < Math.floor((count - 1) / cols) ? count - 1 : from
}

function Select({
  level,
  progress,
  you,
  note,
  muted,
  fsHint,
  onPlay,
}: {
  level: number
  progress: EmberProgress
  you: ReactNode
  note: string | null
  muted: boolean
  fsHint: string
  onPlay: (i: number) => void
}) {
  const gridRef = useRef<HTMLDivElement>(null)
  // the mouse only looks: the card under it fills the detail line, and the highlight (which both sides of
  // an online game share) moves with the arrows or a click
  // remembered with the highlight it was seen under, so moving the highlight by key shows that level again
  const [hover, setHover] = useState<{ i: number; under: number } | null>(null)
  useEffect(() => {
    gridRef.current?.querySelector<HTMLElement>('.ember-card.on')?.scrollIntoView({ block: 'nearest' })
  }, [level])
  const shown = hover && hover.under === level && LEVELS[hover.i] ? hover.i : level
  const def = LEVELS[shown]
  const best = def ? progress.levels[def.id] : undefined
  const totals = compiledAt(shown)?.gemTotals
  const allGems = totals ? totals[0] + totals[1] : 0
  return (
    <div className="ember-select">
      <div className="ember-grid" ref={gridRef} role="listbox" aria-label="levels" onMouseLeave={() => setHover(null)}>
        {LEVELS.map((d, i) => {
          const b = progress.levels[d.id]
          return (
            <button
              key={d.id}
              type="button"
              role="option"
              aria-selected={i === level}
              className={`ember-card ${i === level ? 'on' : ''} ${b?.completed ? 'done' : ''}`}
              onMouseEnter={() => setHover({ i, under: level })}
              onClick={() => onPlay(i)}
            >
              <Thumb index={i} stages={d.stages.length} />
              <span className="ember-card-name">
                <b>{i + 1}</b> {d.name}
              </span>
              <span className="ember-card-meta">
                <Stars n={b?.stars ?? 0} />
                {b?.secs != null ? <span>{fmtTime(b.secs)}</span> : null}
                <Pips n={levelDifficulty(i, LEVELS.length)} />
              </span>
            </button>
          )
        })}
      </div>
      {def ? (
        <p className="ember-detail" aria-live="polite">
          <span className="ember-detail-name">
            <b>{shown + 1}</b> {def.name}
          </span>
          <Pips n={levelDifficulty(shown, LEVELS.length)} />
          <span>{levelLength(def)}</span>
          <span>target {fmtPar(def.par)}</span>
          {best?.completed ? (
            <>
              <span className="ember-detail-best">
                best {best.secs != null ? fmtTime(best.secs) : '–'} <Stars n={best.stars} />
              </span>
              {allGems ? (
                <span>
                  gems {Math.min(best.gems, allGems)}/{allGems}
                </span>
              ) : null}
              {best.deaths != null ? <span>{best.deaths === 0 ? 'no deaths' : best.deaths === 1 ? '1 death' : `${best.deaths} deaths`}</span> : null}
            </>
          ) : (
            <span>
              not finished yet{allGems ? ` · ${allGems} gems` : ''}
            </span>
          )}
        </p>
      ) : null}
      <p className="ember-note">
        {note ? `${note} · ` : ''}
        {you}
        {you ? ' · ' : ''}every level is open · arrows or WASD pick · ↵ play · M {muted ? 'unmute' : 'mute'} · {fsHint} · esc back
      </p>
    </div>
  )
}

// a level's picture on its card: its first stage, whatever its shape (drawThumb fits it to the canvas), with
// the number of stages said on it when there's more than one
function Thumb({ index, stages }: { index: number; stages: number }) {
  const ref = useRef<HTMLCanvasElement>(null)
  useEffect(() => {
    const canvas = ref.current
    const stage = compiledAt(index)?.stages[0]
    if (!canvas || !stage) return
    // twice the card's size, so it is still sharp when fullscreen draws the select screen twice as large
    const k = Math.min(4, 2 * Math.max(1, Math.round(window.devicePixelRatio || 1)))
    canvas.width = THUMB_W * k
    canvas.height = THUMB_H * k
    try {
      drawThumb(canvas, stage, { stages: compiledAt(index)?.stages.length })
    } catch {
      // no picture beats no select screen
    }
  }, [index])
  return (
    <span className="ember-thumb">
      <canvas ref={ref} aria-hidden="true" />
      {stages > 1 ? <span className="ember-thumb-badge">{stages} stages</span> : null}
    </span>
  )
}
