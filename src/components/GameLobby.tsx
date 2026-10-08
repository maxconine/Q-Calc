import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import { plainKey, swallow } from '../lib/gameKeys'
import { closeReasonText, isPairingCode, type PeerInfo, type PeerTransport } from '../lib/peer'
import type { Side } from '../lib/pong'

// the host / join / pick / code screens every game over the peer link starts from. once the link opens,
// the game takes the panel and talks to the transport itself; when it ends it hands back a line to show

type Screen =
  | { k: 'menu'; note?: string }
  | { k: 'hosting'; code: string | null; renewed: boolean }
  | { k: 'browse'; peers: PeerInfo[]; pick: number; denied: boolean }
  | { k: 'code'; peer: PeerInfo; digits: string; note?: string }
  | { k: 'pairing'; peer: PeerInfo }
  | { k: 'game'; role: Side; peer: string }
  | { k: 'levels' }
  | { k: 'solo'; level: string }

const FRIEND: PeerInfo = { id: '', name: 'your friend' }

export type GameProps = { transport: PeerTransport; role: Side; peer: string; onEnd: (note?: string) => void }

// a game that can also be played alone, against the computer, at one of its levels (picked with 1, 2, 3…)
export type SoloGame = { levels: readonly string[]; play: (level: string, onEnd: (note?: string) => void) => ReactNode }


export function GameLobby({
  transport,
  name,
  blurb,
  onClose,
  game,
  solo,
}: {
  transport: PeerTransport
  // the game's name, as typed in the bar
  name: string
  // the menu's line under the title, after "two players on two Macs."
  blurb: string
  onClose: () => void
  game: (props: GameProps) => ReactNode
  solo?: SoloGame
}) {
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

  // the link goes when the game does, however it closes
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
      if (cur.k === 'game' || cur.k === 'solo' || !plainKey(e) || e.key === 'Escape') return
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
        } else if (key === 's' && solo) {
          swallow(e)
          setScreen({ k: 'levels' })
        } else if (barKey) swallow(e)
        return
      }
      if (cur.k === 'levels' && solo) {
        swallow(e)
        const level = solo.levels[Number(e.key) - 1]
        if (/^[1-9]$/.test(e.key) && level) setScreen({ k: 'solo', level })
        else if (e.key === 'Backspace') setScreen({ k: 'menu' })
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
  }, [browse, host, pickPeer, solo, submitCode, toMenu, transport])

  if (screen.k === 'solo') return solo ? solo.play(screen.level, (note) => setScreen({ k: 'menu', note })) : null
  return screen.k === 'game' ? (
    game({ transport, role: screen.role, peer: screen.peer, onEnd: toMenu })
  ) : (
    <div className="pong-lobby">
      <div className="pong-head">
        <span className="pong-title">{name}</span>
        <span className="pong-sub">{transport.label}</span>
        <button type="button" className="pong-leave" onClick={onClose}>
          esc
        </button>
      </div>
      <Lobby
        screen={screen}
        name={name}
        blurb={blurb}
        transport={transport}
        solo={solo}
        onSolo={() => setScreen({ k: 'levels' })}
        onLevel={(level) => setScreen({ k: 'solo', level })}
        onHost={host}
        onJoin={browse}
        onPick={pickPeer}
        onBack={() => toMenu()}
      />
    </div>
  )
}

function Lobby({
  screen,
  name,
  blurb,
  transport,
  solo,
  onSolo,
  onLevel,
  onHost,
  onJoin,
  onPick,
  onBack,
}: {
  screen: Exclude<Screen, { k: 'game' } | { k: 'solo' }>
  name: string
  blurb: string
  transport: PeerTransport
  solo?: SoloGame
  onSolo: () => void
  onLevel: (level: string) => void
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
          <p className="pong-line">
            {solo ? `play the computer, or a friend on another ${word}. ${blurb}` : `two players on two ${word}s. ${blurb}`}
          </p>
          <div className="pong-actions">
            {solo ? (
              <button type="button" className="pong-button" onClick={onSolo}>
                solo <kbd>S</kbd>
              </button>
            ) : null}
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
    case 'levels':
      return (
        <div className="pong-body">
          <p className="pong-line">how good is the computer?</p>
          <div className="pong-actions">
            {solo?.levels.map((level, i) => (
              <button type="button" className="pong-button" key={level} onClick={() => onLevel(level)}>
                {level} <kbd>{i + 1}</kbd>
              </button>
            ))}
          </div>
          <p className="pong-note">
            <button type="button" className="pong-link" onClick={onBack}>
              ⌫ back
            </button>
          </p>
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
              ? `on the other ${word}: type ${name}, join, pick this ${word}, enter the code`
              : `send your friend this code · on their Q Calc: type ${name}, join, enter the code`}
          </p>
          <p className="pong-note pong-wait">waiting for your friend…</p>
        </div>
      )
    case 'browse':
      return (
        <div className="pong-body">
          {screen.denied ? (
            <p className="pong-note">{closeReasonText('denied', transport.peerWord)}</p>
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
            <p className="pong-line pong-wait">
              looking for {word}s hosting {name}…
            </p>
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
