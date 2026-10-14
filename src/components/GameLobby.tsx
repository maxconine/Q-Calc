import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { plainKey, swallow } from '../lib/gameKeys'
import { lobbyBack, lobbyEvent, lobbyHoldsLink, lobbyKey, listScreen, type LobbyCtx, type LobbyEffect, type LobbyState, type Side } from '../lib/lobby'
import { loadNickname, randomNickname, saveNickname } from '../lib/nickname'
import { closeReasonText, type PeerTransport, type RoomInfo } from '../lib/peer'
import { useGameShell, useShellFullscreen } from '../lib/gameShellContext'
import { pauseGameClock, resumeGameClock } from '../lib/gameClock'
import { FullscreenButton, GAME_HIDE_EVENT, GAME_SHOW_EVENT } from './GameShell'

// the lobby every game opens on (lib/lobby.ts has its rules). over the link: the open rooms for this game, by
// the host's nickname, 🔒 when they take a code; ↑↓ pick, ↵ join, h host, n renames you. on this computer: the
// game's own options, 1, 2, 3… once a game starts it takes the panel and talks to the transport itself; when it
// ends it hands back a line to show

export type GameProps = { transport: PeerTransport; role: Side; peer: string; onEnd: (note?: string) => void }

// what a game offers on this computer: solo levels, modes, a friend on the same keyboard
export type LocalGames = {
  // the line over the options
  prompt: string
  options: readonly string[]
  play: (option: number, onEnd: (note?: string) => void) => ReactNode
}

const LOCKED_KEY = 'qcalc-host-locked'

function lockedPref(): boolean {
  try {
    return localStorage.getItem(LOCKED_KEY) === '1'
  } catch {
    return false
  }
}

function saveLockedPref(locked: boolean): void {
  try {
    localStorage.setItem(LOCKED_KEY, locked ? '1' : '0')
  } catch {
    // private window: open again next time
  }
}

// enter, arrows, backspace and letters would otherwise reach the hidden bar and commit or move through the tape
const barKey = (e: KeyboardEvent) => e.key.length === 1 || e.key === 'Enter' || e.key.startsWith('Arrow') || e.key === 'Backspace' || e.key === 'Tab'

export function GameLobby({
  game,
  name,
  blurb,
  transport,
  online,
  onOpen,
  local,
  sub,
  footer,
  onKey,
  idle,
  leave,
  frame = false,
  ownHead = false,
}: {
  // the game's kind, which the room lists go by
  game: string
  // the game's name, as typed in the bar
  name: string
  // the line under the title
  blurb: string
  // null where there's no link: only the local options show
  transport: PeerTransport | null
  // the game over the link
  online?: (props: GameProps) => ReactNode
  // the link just opened, before the next message: a game that must exist by then makes it here
  onOpen?: (role: Side, peer: string) => void
  local?: LocalGames
  // the header's line when there's no link
  sub?: string
  // more under the options (the controls, a setting)
  footer?: ReactNode
  // the game's own lobby keys, before the lobby's; true takes the key
  onKey?: (e: KeyboardEvent) => boolean
  // esc while playing: nothing is running, so leave without a pause
  idle?: () => boolean
  // esc's leave while playing, when the game has somewhere nearer to go than the lobby; false goes to the lobby
  leave?: () => boolean
  // the lobby brings its own panel padding: the game it opens has a panel of its own instead of sitting in one
  frame?: boolean
  // the game over the link draws its own header (ember's screens), so the lobby's would be a second one
  ownHead?: boolean
}) {
  const link = online ? transport : null
  const shell = useGameShell()
  const fullscreen = Boolean(useShellFullscreen()?.on)
  const [state, setState] = useState<LobbyState>(listScreen)
  const stateRef = useRef(state)
  const [nick, setNick] = useState(loadNickname)
  // the relay sent no room list, so rooms go by code
  const [codeOnly, setCodeOnly] = useState(false)
  // mid-game, whose link dropped: the game stands still until they're back
  const [away, setAway] = useState<'peer' | 'self' | null>(null)
  const nickRef = useRef(nick)
  nickRef.current = nick
  const callbacks = useRef({ onOpen, onKey, idle, leave })
  callbacks.current = { onOpen, onKey, idle, leave }

  const set = useCallback((next: LobbyState) => {
    stateRef.current = next
    setState(next)
  }, [])

  const ctx = useMemo<LobbyCtx>(
    () => ({
      online: Boolean(link),
      codeOnly,
      codeLength: link?.codeLength ?? 0,
      options: local?.options.length ?? 0,
      closeText: (reason) => closeReasonText(reason, link?.peerWord),
      randomName: () => randomNickname(),
    }),
    [codeOnly, link, local?.options.length],
  )

  const run = useCallback(
    (effect: LobbyEffect | undefined) => {
      if (!effect) return
      switch (effect.k) {
        case 'host':
          saveLockedPref(effect.locked)
          link?.host({ game, name: nickRef.current, locked: effect.locked })
          return
        case 'join':
          link?.join(effect.room.id, effect.code, nickRef.current)
          return
        case 'cancel':
          link?.close()
          return
        case 'rename':
          setNick(saveNickname(effect.name))
          return
        case 'fullscreen':
          shell?.toggleFullscreen()
      }
    },
    [game, link, shell],
  )

  const toList = useCallback(
    (note?: string) => {
      link?.close()
      set(listScreen(note))
    },
    [link, set],
  )

  // where the player is, for esc: the lobby, a game on this computer, or one over the link
  useEffect(() => {
    shell?.setMode(state.k === 'game' ? 'online' : state.k === 'local' ? 'local' : 'lobby')
  }, [shell, state.k])

  useEffect(
    () =>
      shell?.register({
        back: () => {
          const back = lobbyBack(stateRef.current)
          if (!back) return false
          set(back.state)
          run(back.effect)
          return true
        },
        leave: () => {
          if (callbacks.current.leave?.()) return
          toList()
        },
        idle: () => Boolean(callbacks.current.idle?.()),
        hold: () => lobbyHoldsLink(stateRef.current),
      }),
    [run, set, shell, toList],
  )

  // the list is watched only while it shows
  const watching = Boolean(link) && state.k === 'list'
  useEffect(() => {
    if (!watching || !link) return
    link.watch(game)
    return () => link.unwatch()
  }, [game, link, watching])

  // the link goes when the game does, however it closes
  useEffect(() => () => link?.close(), [link])

  // hiding q calc mid-game steps away: the seat is kept and the game waits, and showing it again goes back
  useEffect(() => {
    if (!link) return
    const hide = () => {
      if (stateRef.current.k === 'game') link.stepAway?.()
    }
    const show = () => link.comeBack?.()
    window.addEventListener(GAME_HIDE_EVENT, hide)
    window.addEventListener(GAME_SHOW_EVENT, show)
    return () => {
      window.removeEventListener(GAME_HIDE_EVENT, hide)
      window.removeEventListener(GAME_SHOW_EVENT, show)
    }
  }, [link])

  useEffect(() => {
    if (!link) return
    return link.subscribe((e) => {
      if (e.type === 'rooms') setCodeOnly(Boolean(e.codeOnly))
      if (e.type === 'away') {
        pauseGameClock()
        setAway(e.who)
      } else if (e.type === 'back' || e.type === 'closed') {
        resumeGameClock()
        setAway(null)
      }
      const cur = stateRef.current
      const next = lobbyEvent(cur, e, ctx)
      if (next === cur) return
      if (next.k === 'game' && e.type === 'open') callbacks.current.onOpen?.(next.role, next.peer)
      set(next)
    })
  }, [ctx, link, set])

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const cur = stateRef.current
      if (cur.k === 'game' || cur.k === 'local' || !plainKey(e) || e.key === 'Escape') return
      if (cur.k === 'list' && callbacks.current.onKey?.(e)) {
        swallow(e)
        return
      }
      const step = lobbyKey(cur, e.key, ctx, lockedPref(), nickRef.current)
      if (step.handled || barKey(e)) swallow(e)
      if (e.repeat && step.effect) return
      if (step.state !== cur) set(step.state)
      run(step.effect)
    }
    window.addEventListener('keydown', onKeyDown, true)
    return () => window.removeEventListener('keydown', onKeyDown, true)
  }, [ctx, run, set])

  if (state.k === 'local' && local) return <>{local.play(state.option, (note) => set(listScreen(note)))}</>
  if (state.k === 'game' && link && online) {
    const waiting = away ? (
      <div className="game-toast" role="status">
        {away === 'peer' ? `${state.peer} stepped away · the game waits for them` : 'reconnecting to your game…'}
      </div>
    ) : null
    if (ownHead) return <>{online({ transport: link, role: state.role, peer: state.peer, onEnd: toList })}{waiting}</>
    return (
      <>
        <LobbyHead name={name} sub={`with ${state.peer}`} fullscreen={fullscreen} onFullscreen={() => shell?.toggleFullscreen()} onEsc={() => shell?.escape()} />
        {online({ transport: link, role: state.role, peer: state.peer, onEnd: toList })}
        {waiting}
      </>
    )
  }

  return (
    // game-fs-screen: drawn larger, centred, while the game is fullscreen
    <div className={`${frame ? 'pong pong-lobby' : 'pong-lobby'} game-fs-screen`}>
      <LobbyHead name={name} sub={link ? link.label : (sub ?? '')} fullscreen={fullscreen} onFullscreen={() => shell?.toggleFullscreen()} onEsc={() => shell?.escape()} />
      <LobbyBody
        state={state.k === 'game' || state.k === 'local' ? listScreen() : state}
        name={name}
        blurb={blurb}
        link={link}
        local={local}
        footer={footer}
        nick={nick}
        codeOnly={codeOnly}
        onLocal={(option) => set({ k: 'local', option })}
        onJoin={(room) => {
          const step = lobbyKey({ k: 'list', rooms: [room], pick: 0 }, 'Enter', ctx, false, nick)
          set(step.state)
          run(step.effect)
        }}
        onKey={(key) => {
          const step = lobbyKey(stateRef.current, key, ctx, lockedPref(), nickRef.current)
          set(step.state)
          run(step.effect)
        }}
      />
    </div>
  )
}

function LobbyHead({ name, sub, fullscreen, onFullscreen, onEsc }: { name: string; sub: string; fullscreen: boolean; onFullscreen: () => void; onEsc: () => void }) {
  return (
    <div className="pong-head">
      <span className="pong-title">{name}</span>
      <span className="pong-sub">{sub}</span>
      <FullscreenButton className="lobby-full" on={fullscreen} onClick={onFullscreen} />
      <button type="button" className="pong-leave" onClick={onEsc}>
        esc
      </button>
    </div>
  )
}

function LobbyBody({
  state,
  name,
  blurb,
  link,
  local,
  footer,
  nick,
  codeOnly,
  onLocal,
  onJoin,
  onKey,
}: {
  state: Exclude<LobbyState, { k: 'game' } | { k: 'local' }>
  name: string
  blurb: string
  link: PeerTransport | null
  local?: LocalGames
  footer?: ReactNode
  nick: string
  codeOnly: boolean
  onLocal: (option: number) => void
  onJoin: (room: RoomInfo) => void
  // a key's worth, from a click
  onKey: (key: string) => void
}) {
  const back = (
    <button type="button" className="pong-link" onClick={() => onKey('Backspace')}>
      ⌫ back
    </button>
  )
  switch (state.k) {
    case 'list':
      return (
        <div className="pong-body lobby-body">
          <p className="pong-line">{blurb}</p>
          {local ? (
            <div className="lobby-local">
              {local.prompt ? <span className="lobby-label">{local.prompt}</span> : null}
              <div className="pong-actions">
                {local.options.map((option, i) => (
                  <button type="button" key={option} className={`pong-button ${!link && i === state.pick ? 'on' : ''}`} onClick={() => onLocal(i)}>
                    {option} <kbd>{i + 1}</kbd>
                  </button>
                ))}
              </div>
            </div>
          ) : null}
          {footer}
          {link && codeOnly ? (
            <div className="lobby-rooms">
              <div className="pong-actions">
                <button type="button" className="pong-button" onClick={() => onKey('h')}>
                  host <kbd>H</kbd>
                </button>
                <button type="button" className="pong-button" onClick={() => onKey('j')}>
                  join with a code <kbd>J</kbd>
                </button>
              </div>
              <p className="pong-note">
                no room list from the game server yet, so rooms go by code · you’re{' '}
                <button type="button" className="pong-link lobby-nick" onClick={() => onKey('n')}>
                  {nick}
                </button>
              </p>
            </div>
          ) : link ? (
            <div className="lobby-rooms">
              <span className="lobby-label">rooms open now</span>
              {state.offline ? (
                <p className="pong-note pong-wait">{closeReasonText('network', link.peerWord)}</p>
              ) : state.rooms.length ? (
                <ul className="pong-peers lobby-list" role="listbox" aria-label="rooms">
                  {state.rooms.map((r, i) => (
                    <li key={r.id} role="option" aria-selected={i === state.pick}>
                      <button type="button" className={i === state.pick ? 'on' : undefined} onClick={() => onJoin(r)}>
                        <span className="lobby-host">{r.name}</span>
                        <span className="lobby-game">{name}</span>
                        <span className="lobby-lock" aria-label={r.locked ? 'locked' : 'open'}>
                          {r.locked ? '🔒' : ''}
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="pong-note lobby-empty">nobody’s hosting {name} right now</p>
              )}
              <p className="pong-note">
                {state.rooms.length ? '↑↓ pick · ↵ join · ' : ''}
                <button type="button" className="pong-link" onClick={() => onKey('h')}>
                  h host
                </button>{' '}
                · you’re{' '}
                <button type="button" className="pong-link lobby-nick" onClick={() => onKey('n')}>
                  {nick}
                </button>{' '}
                · n renames
              </p>
            </div>
          ) : local ? (
            <p className="pong-note">{local.options.length > 1 ? '↑↓ pick · ↵ play · ' : '↵ plays · '}f full screen</p>
          ) : null}
          {state.note ? <p className="pong-note lobby-note">{state.note}</p> : null}
        </div>
      )
    case 'setup':
      return (
        <div className="pong-body">
          <p className="pong-line">host a room as {nick}</p>
          <div className="pong-actions">
            <button type="button" className={`pong-button ${state.locked ? '' : 'on'}`} onClick={() => onKey('o')}>
              open <kbd>O</kbd>
            </button>
            <button type="button" className={`pong-button ${state.locked ? 'on' : ''}`} onClick={() => onKey('l')}>
              🔒 locked <kbd>L</kbd>
            </button>
          </div>
          <p className="pong-note">
            {state.locked ? 'only someone with the code you send can join' : 'anyone who opens ' + name + ' can join'} · ↵ host · {back}
          </p>
        </div>
      )
    case 'hosting': {
      const n = link?.codeLength ?? 0
      return (
        <div className="pong-body">
          {state.locked ? (
            <>
              <div className="pong-code" aria-label="room code">
                {(state.code || '·'.repeat(n)).split('').map((d, i) => (
                  <span key={i}>{d}</span>
                ))}
              </div>
              <p className="pong-line">
                {state.renewed ? 'too many wrong codes, so here’s a new one. ' : ''}
                {codeOnly ? `send your friend this code · on their Q Calc: type ${name}, J, then the code` : `send your friend this code · they pick ${nick} in the ${name} list`}
              </p>
            </>
          ) : (
            <p className="pong-line">
              your room is open · it’s listed as <b>{nick}</b>
            </p>
          )}
          <p className="pong-note pong-wait">waiting for someone to join…</p>
          <p className="pong-note">{back}</p>
        </div>
      )
    }
    case 'code':
    case 'pairing': {
      const n = link?.codeLength ?? 0
      if (state.k === 'pairing' && !state.room.locked) {
        return (
          <div className="pong-body">
            <p className="pong-line pong-wait">joining {state.room.name}…</p>
            <p className="pong-note">{back}</p>
          </div>
        )
      }
      const digits = state.k === 'code' ? state.digits : ''
      return (
        <div className="pong-body">
          <p className="pong-line">{state.room.id ? `🔒 the code for ${state.room.name}’s room` : 'the code your friend sent'}</p>
          <div className={`pong-code entry ${state.k === 'pairing' ? 'busy' : ''}`}>
            {Array.from({ length: n }, (_, i) => (
              <span key={i} className={i === digits.length && state.k === 'code' ? 'caret' : undefined}>
                {state.k === 'pairing' ? '•' : (digits[i] ?? '')}
              </span>
            ))}
          </div>
          <p className="pong-note">
            {state.k === 'pairing' ? 'joining… · ' : state.note ? `${state.note} · ` : `type the ${n} digits · `}
            {back}
          </p>
        </div>
      )
    }
    case 'rename':
      return (
        <div className="pong-body">
          <p className="pong-line">what the room lists call you</p>
          <div className="lobby-rename" aria-label="nickname">
            {state.draft}
            <span className="lobby-caret" />
          </div>
          <p className="pong-note">↵ keep · tab picks another · esc back</p>
        </div>
      )
  }
}
