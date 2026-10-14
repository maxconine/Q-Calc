import type { CloseReason, PeerEvent, RoomInfo } from './peer'
import { cleanNickname, NICKNAME_MAX } from './nickname'

// the lobby every game opens on, as plain state: its screens, what each key does on them, and what the link's
// events change. GameLobby draws it and carries out the effects. a game played over the link lists the open
// rooms for it (↑↓ pick, ↵ join, h host); a locked room asks for its code first. games played on this
// computer show their own options (1, 2, 3…), and solo-only games show nothing else

export type Side = 'host' | 'guest'

export type LobbyState =
  | { k: 'list'; rooms: RoomInfo[]; pick: number; note?: string; offline?: boolean }
  // hosting: open, or locked with a code
  | { k: 'setup'; locked: boolean }
  // code is '' for an open room, null until the link hands one out
  | { k: 'hosting'; locked: boolean; code: string | null; renewed: boolean }
  | { k: 'code'; room: RoomInfo; digits: string; note?: string }
  | { k: 'pairing'; room: RoomInfo }
  | { k: 'rename'; draft: string }
  | { k: 'game'; role: Side; peer: string }
  | { k: 'local'; option: number }

export type LobbyEffect =
  | { k: 'host'; locked: boolean }
  | { k: 'join'; room: RoomInfo; code: string }
  // stop hosting or joining; the list watches again
  | { k: 'cancel' }
  | { k: 'rename'; name: string }
  | { k: 'fullscreen' }

export type LobbyCtx = {
  // whether there's a link to play over at all
  online: boolean
  // the relay has no room list (one from before them, or out of reach): h hosts with a code, j types one
  codeOnly: boolean
  codeLength: number
  // how many local options the game has (solo levels, modes)
  options: number
  closeText: (reason: CloseReason) => string
  randomName: () => string
}

export type LobbyStep = { state: LobbyState; effect?: LobbyEffect; handled: boolean }

// a room joined by its code alone, when there's no list to pick it from
export const BY_CODE: RoomInfo = { id: '', name: 'your friend', locked: true }

export const listScreen = (note?: string): Extract<LobbyState, { k: 'list' }> => ({ k: 'list', rooms: [], pick: 0, ...(note ? { note } : {}) })

// the key's name as a lobby reads it: letters lower case, anything longer as it is
const keyName = (key: string) => (key.length === 1 ? key.toLowerCase() : key)

const step = (state: LobbyState, effect?: LobbyEffect): LobbyStep => ({ state, handled: true, ...(effect ? { effect } : {}) })

function joinRoom(room: RoomInfo): LobbyStep {
  if (room.locked) return step({ k: 'code', room, digits: '' })
  return step({ k: 'pairing', room }, { k: 'join', room, code: '' })
}

// a plain key on the lobby; handled false lets it through (only on the game's own screens)
export function lobbyKey(state: LobbyState, rawKey: string, ctx: LobbyCtx, lockedPref: boolean, name: string): LobbyStep {
  const key = keyName(rawKey)
  const same = step(state)
  switch (state.k) {
    case 'list': {
      if (key === 'f') return step(state, { k: 'fullscreen' })
      if (/^[1-9]$/.test(key) && Number(key) <= ctx.options) return step({ k: 'local', option: Number(key) - 1 })
      if (!ctx.online) {
        // nothing to list: the arrows and ↵ go through the game's own options
        const n = ctx.options
        if (n && (key === 'ArrowDown' || key === 'ArrowRight')) return step({ ...state, pick: (state.pick + 1) % n })
        if (n && (key === 'ArrowUp' || key === 'ArrowLeft')) return step({ ...state, pick: (state.pick - 1 + n) % n })
        if (key === 'Enter' && n) return step({ k: 'local', option: Math.min(state.pick, n - 1) })
        return same
      }
      if (ctx.codeOnly) {
        if (key === 'h') return step({ k: 'hosting', locked: true, code: null, renewed: false }, { k: 'host', locked: true })
        if (key === 'j') return step({ k: 'code', room: BY_CODE, digits: '' })
        if (key === 'n') return step({ k: 'rename', draft: name })
        return same
      }
      const n = state.rooms.length
      if (key === 'ArrowDown' && n) return step({ ...state, pick: (state.pick + 1) % n })
      if (key === 'ArrowUp' && n) return step({ ...state, pick: (state.pick - 1 + n) % n })
      if (key === 'Enter' && state.rooms[state.pick]) return joinRoom(state.rooms[state.pick]!)
      if (key === 'h') return step({ k: 'setup', locked: lockedPref })
      if (key === 'n') return step({ k: 'rename', draft: name })
      return same
    }
    case 'setup':
      if (key === 'f') return step(state, { k: 'fullscreen' })
      if (key.startsWith('Arrow') || key === 'Tab') return step({ ...state, locked: !state.locked })
      if (key === 'o' || key === 'l' || key === 'Enter') {
        const locked = key === 'Enter' ? state.locked : key === 'l'
        return step({ k: 'hosting', locked, code: null, renewed: false }, { k: 'host', locked })
      }
      if (key === 'Backspace') return step(listScreen())
      return same
    case 'hosting':
    case 'pairing':
      if (key === 'f') return step(state, { k: 'fullscreen' })
      if (key === 'Backspace') return step(listScreen(), { k: 'cancel' })
      return same
    case 'code': {
      if (/^[0-9]$/.test(key) && state.digits.length < ctx.codeLength) {
        const digits = state.digits + key
        if (digits.length === ctx.codeLength) return step({ k: 'pairing', room: state.room }, { k: 'join', room: state.room, code: digits })
        return step({ k: 'code', room: state.room, digits })
      }
      if (key === 'Backspace') return state.digits ? step({ ...state, digits: state.digits.slice(0, -1), note: undefined }) : step(listScreen())
      if (key === 'f') return step(state, { k: 'fullscreen' })
      return same
    }
    case 'rename': {
      if (rawKey === 'Enter') {
        const next = cleanNickname(state.draft)
        return next ? step(listScreen(), { k: 'rename', name: next }) : same
      }
      if (rawKey === 'Tab') return step({ k: 'rename', draft: ctx.randomName() })
      if (rawKey === 'Backspace') return step({ k: 'rename', draft: state.draft.slice(0, -1) })
      if (rawKey.length === 1 && state.draft.length < NICKNAME_MAX) return step({ k: 'rename', draft: (state.draft + rawKey).replace(/^\s+/, '') })
      return same
    }
    case 'game':
    case 'local':
      return { state, handled: false }
  }
}

// esc on a lobby screen: one step back, or null on the list itself, where esc closes the game
export function lobbyBack(state: LobbyState): LobbyStep | null {
  switch (state.k) {
    case 'setup':
    case 'code':
    case 'rename':
      return step(listScreen())
    case 'hosting':
    case 'pairing':
      return step(listScreen(), { k: 'cancel' })
    default:
      return null
  }
}

// what the link says, on whichever screen is up
export function lobbyEvent(state: LobbyState, e: PeerEvent, ctx: LobbyCtx): LobbyState {
  switch (e.type) {
    case 'rooms': {
      if (state.k !== 'list') return state
      const keep = state.rooms[state.pick]?.id
      const pick = Math.max(0, e.rooms.findIndex((r) => r.id === keep))
      return { ...state, rooms: e.rooms, pick: Math.min(pick, Math.max(0, e.rooms.length - 1)), offline: Boolean(e.offline) }
    }
    case 'hosting':
      return state.k === 'hosting' ? { ...state, code: e.code, renewed: Boolean(e.renewed) } : state
    case 'open':
      return state.k === 'hosting' || state.k === 'pairing' ? { k: 'game', role: e.role, peer: e.peer || 'friend' } : state
    case 'closed':
      if (state.k === 'pairing' && e.reason === 'wrong-code' && state.room.locked) {
        return { k: 'code', room: state.room, digits: '', note: ctx.closeText('wrong-code') }
      }
      // a game reports its own end
      if (state.k === 'hosting' || state.k === 'pairing') return listScreen(ctx.closeText(e.reason))
      return state
    default:
      return state
  }
}

// whether the link must stay up while the window is hidden: someone is waiting to join, or playing
export function lobbyHoldsLink(state: LobbyState): boolean {
  return state.k === 'hosting' || state.k === 'pairing' || state.k === 'game'
}
