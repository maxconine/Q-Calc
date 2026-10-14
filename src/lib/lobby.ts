import type { CloseReason, PeerEvent, RoomInfo } from './peer'
import { cleanNickname, NICKNAME_MAX } from './nickname'

// the lobby every game opens on, as plain state: its screens, what each key does on them, and what the link's
// events change. GameLobby draws it and carries out the effects. a game with a computer to play and a friend
// to play first asks which (1 vs computer, 2 vs a friend); vs computer shows the levels, vs a friend the
// same-keyboard options, h host, j join with a code, and the open rooms for it over the link (↑↓ pick, ↵ join;
// a locked room asks for its code first). a game with only one of the two opens on that screen

export type Side = 'host' | 'guest'

// the lobby's own screens: the first choice, the game alone on this computer, and playing a friend
export type Screen = 'start' | 'solo' | 'friend'

export type LobbyState =
  // rooms are kept on every screen, so the friend screen has them as it opens; pick is the screen's own
  | { k: 'home'; screen: Screen; rooms: RoomInfo[]; pick: number; note?: string; offline?: boolean }
  // hosting: open, or locked with a code
  | { k: 'setup'; locked: boolean }
  // code is '' for an open room, null until the link hands one out
  | { k: 'hosting'; locked: boolean; code: string | null; renewed: boolean }
  | { k: 'code'; room: RoomInfo; digits: string; note?: string }
  | { k: 'pairing'; room: RoomInfo }
  | { k: 'rename'; draft: string; from: Screen }
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
  // the game's options on this computer: alone (levels, modes), then two on one keyboard. local options
  // count through both, solo ones first
  solo: number
  together: number
  closeText: (reason: CloseReason) => string
  randomName: () => string
}

export type LobbyStep = { state: LobbyState; effect?: LobbyEffect; handled: boolean }

// a room joined by its code alone, when there's no list to pick it from
export const BY_CODE: RoomInfo = { id: '', name: 'your friend', locked: true }

export const homeScreen = (screen: Screen, note?: string): Extract<LobbyState, { k: 'home' }> => ({
  k: 'home',
  screen,
  rooms: [],
  pick: 0,
  ...(note ? { note } : {}),
})

// the screen the lobby opens on: the first choice only when there's one to make
export function firstScreen(c: Pick<LobbyCtx, 'online' | 'solo' | 'together'>): Screen {
  const friend = c.online || c.together > 0
  if (c.solo && friend) return 'start'
  return c.solo ? 'solo' : 'friend'
}

// the screen a local option goes back to when it ends
export const optionScreen = (option: number, c: Pick<LobbyCtx, 'solo'>): Screen => (option < c.solo ? 'solo' : 'friend')

// the key's name as a lobby reads it: letters lower case, anything longer as it is
const keyName = (key: string) => (key.length === 1 ? key.toLowerCase() : key)

const step = (state: LobbyState, effect?: LobbyEffect): LobbyStep => ({ state, handled: true, ...(effect ? { effect } : {}) })

// another home screen, keeping the rooms
const go = (state: Extract<LobbyState, { k: 'home' }>, screen: Screen): LobbyState => ({ k: 'home', screen, rooms: state.rooms, pick: 0, ...(state.offline ? { offline: true } : {}) })

const ahead = (key: string) => key === 'ArrowDown' || key === 'ArrowRight'
const behind = (key: string) => key === 'ArrowUp' || key === 'ArrowLeft'

// ↑↓ (and ←→) round n things, ↵ takes the picked one
function cycle(state: Extract<LobbyState, { k: 'home' }>, key: string, n: number, pick: (i: number) => LobbyStep): LobbyStep {
  if (n && ahead(key)) return step({ ...state, pick: (state.pick + 1) % n })
  if (n && behind(key)) return step({ ...state, pick: (state.pick - 1 + n) % n })
  if (n && key === 'Enter') return pick(Math.min(state.pick, n - 1))
  return step(state)
}

export function joinRoom(room: RoomInfo): LobbyStep {
  if (room.locked) return step({ k: 'code', room, digits: '' })
  return step({ k: 'pairing', room }, { k: 'join', room, code: '' })
}

// h, j and n, on the first screen and the friend screen
function friendKey(state: Extract<LobbyState, { k: 'home' }>, key: string, ctx: LobbyCtx, lockedPref: boolean, name: string): LobbyStep | null {
  if (!ctx.online) return null
  if (key === 'h') {
    if (ctx.codeOnly) return step({ k: 'hosting', locked: true, code: null, renewed: false }, { k: 'host', locked: true })
    return step({ k: 'setup', locked: lockedPref })
  }
  if (key === 'j') return step({ k: 'code', room: BY_CODE, digits: '' })
  if (key === 'n') return step({ k: 'rename', draft: name, from: state.screen })
  return null
}

// a plain key on the lobby; handled false lets it through (only on the game's own screens)
export function lobbyKey(state: LobbyState, rawKey: string, ctx: LobbyCtx, lockedPref: boolean, name: string): LobbyStep {
  const key = keyName(rawKey)
  const same = step(state)
  switch (state.k) {
    case 'home': {
      if (key === 'f') return step(state, { k: 'fullscreen' })
      const first = firstScreen(ctx)
      if (key === 'Backspace') return state.screen === first ? same : step(go(state, first))
      const digit = /^[1-9]$/.test(key) ? Number(key) : 0
      if (state.screen === 'start') {
        const to = (i: number) => step(go(state, i ? 'friend' : 'solo'))
        if (digit === 1 || digit === 2) return to(digit - 1)
        return friendKey(state, key, ctx, lockedPref, name) ?? cycle(state, key, 2, to)
      }
      if (state.screen === 'solo') {
        if (digit && digit <= ctx.solo) return step({ k: 'local', option: digit - 1 })
        return cycle(state, key, ctx.solo, (i) => step({ k: 'local', option: i }))
      }
      if (digit && digit <= ctx.together) return step({ k: 'local', option: ctx.solo + digit - 1 })
      // no link: the arrows and ↵ go through the same-keyboard options
      if (!ctx.online) return cycle(state, key, ctx.together, (i) => step({ k: 'local', option: ctx.solo + i }))
      const friend = friendKey(state, key, ctx, lockedPref, name)
      if (friend) return friend
      if (ctx.codeOnly) return same
      return cycle(state, key, state.rooms.length, (i) => joinRoom(state.rooms[i]!))
    }
    case 'setup':
      if (key === 'f') return step(state, { k: 'fullscreen' })
      if (key.startsWith('Arrow') || key === 'Tab') return step({ ...state, locked: !state.locked })
      if (key === 'o' || key === 'l' || key === 'Enter') {
        const locked = key === 'Enter' ? state.locked : key === 'l'
        return step({ k: 'hosting', locked, code: null, renewed: false }, { k: 'host', locked })
      }
      if (key === 'Backspace') return step(homeScreen('friend'))
      return same
    case 'hosting':
    case 'pairing':
      if (key === 'f') return step(state, { k: 'fullscreen' })
      if (key === 'Backspace') return step(homeScreen('friend'), { k: 'cancel' })
      return same
    case 'code': {
      if (/^[0-9]$/.test(key) && state.digits.length < ctx.codeLength) {
        const digits = state.digits + key
        if (digits.length === ctx.codeLength) return step({ k: 'pairing', room: state.room }, { k: 'join', room: state.room, code: digits })
        return step({ k: 'code', room: state.room, digits })
      }
      if (key === 'Backspace') return state.digits ? step({ ...state, digits: state.digits.slice(0, -1), note: undefined }) : step(homeScreen('friend'))
      if (key === 'f') return step(state, { k: 'fullscreen' })
      return same
    }
    case 'rename': {
      if (rawKey === 'Enter') {
        const next = cleanNickname(state.draft)
        return next ? step(homeScreen(state.from), { k: 'rename', name: next }) : same
      }
      if (rawKey === 'Tab') return step({ ...state, draft: ctx.randomName() })
      if (rawKey === 'Backspace') return step({ ...state, draft: state.draft.slice(0, -1) })
      if (rawKey.length === 1 && state.draft.length < NICKNAME_MAX) return step({ ...state, draft: (state.draft + rawKey).replace(/^\s+/, '') })
      return same
    }
    case 'game':
    case 'local':
      return { state, handled: false }
  }
}

// esc on a lobby screen: one step back, or null on the screen the lobby opened on, where esc closes the game
export function lobbyBack(state: LobbyState, ctx: Pick<LobbyCtx, 'online' | 'solo' | 'together'>): LobbyStep | null {
  switch (state.k) {
    case 'home': {
      const first = firstScreen(ctx)
      return state.screen === first ? null : step(go(state, first))
    }
    case 'setup':
    case 'code':
      return step(homeScreen('friend'))
    case 'rename':
      return step(homeScreen(state.from))
    case 'hosting':
    case 'pairing':
      return step(homeScreen('friend'), { k: 'cancel' })
    default:
      return null
  }
}

// what the link says, on whichever screen is up
export function lobbyEvent(state: LobbyState, e: PeerEvent, ctx: LobbyCtx): LobbyState {
  switch (e.type) {
    case 'rooms': {
      if (state.k !== 'home') return state
      const offline = Boolean(e.offline)
      // the picked room stays picked as the list changes; other screens' picks aren't rooms
      if (state.screen !== 'friend') return { ...state, rooms: e.rooms, offline }
      const keep = state.rooms[state.pick]?.id
      const pick = Math.max(0, e.rooms.findIndex((r) => r.id === keep))
      return { ...state, rooms: e.rooms, pick: Math.min(pick, Math.max(0, e.rooms.length - 1)), offline }
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
      if (state.k === 'hosting' || state.k === 'pairing') return homeScreen('friend', ctx.closeText(e.reason))
      return state
    default:
      return state
  }
}

// whether the link must stay up while the window is hidden: someone is waiting to join, or playing
export function lobbyHoldsLink(state: LobbyState): boolean {
  return state.k === 'hosting' || state.k === 'pairing' || state.k === 'game'
}
