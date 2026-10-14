import { describe, expect, it } from 'vitest'
import { BY_CODE, lobbyBack, lobbyEvent, lobbyHoldsLink, lobbyKey, listScreen, type LobbyCtx, type LobbyState } from './lobby'
import { closeReasonText, type RoomInfo } from './peer'

const ctx = (over: Partial<LobbyCtx> = {}): LobbyCtx => ({
  online: true,
  codeOnly: false,
  codeLength: 6,
  options: 3,
  closeText: (r) => closeReasonText(r, 'computer'),
  randomName: () => 'plum newt',
  ...over,
})

const open: RoomInfo = { id: 'a', name: 'teal otter', locked: false }
const locked: RoomInfo = { id: 'b', name: 'amber fox', locked: true }
const withRooms = (pick = 0): LobbyState => ({ k: 'list', rooms: [open, locked], pick })

// keys in a row from a state, keeping the effects
function press(state: LobbyState, keys: string[], c = ctx(), pref = false) {
  const effects = []
  for (const k of keys) {
    const s = lobbyKey(state, k, c, pref, 'teal otter')
    state = s.state
    if (s.effect) effects.push(s.effect)
  }
  return { state, effects }
}

describe('lobby list', () => {
  it('joins an open room in one key', () => {
    const { state, effects } = press(withRooms(), ['Enter'])
    expect(state).toEqual({ k: 'pairing', room: open })
    expect(effects).toEqual([{ k: 'join', room: open, code: '' }])
  })

  it('asks a locked room for its code, then joins when the digits are in', () => {
    const { state, effects } = press(withRooms(), ['ArrowDown', 'Enter', '1', '2', '3', 'Backspace', '3', '4', '5', '6'])
    expect(state).toEqual({ k: 'pairing', room: locked })
    expect(effects).toEqual([{ k: 'join', room: locked, code: '123456' }])
  })

  it('↑↓ wrap round the rooms, and ⌫ on an empty code goes back to the list', () => {
    expect(press(withRooms(), ['ArrowUp']).state).toMatchObject({ pick: 1 })
    expect(press(withRooms(1), ['ArrowDown']).state).toMatchObject({ pick: 0 })
    expect(press({ k: 'code', room: locked, digits: '' }, ['Backspace']).state).toEqual(listScreen())
  })

  it('hosts open or locked, starting from the last choice', () => {
    expect(press(listScreen(), ['h']).state).toEqual({ k: 'setup', locked: false })
    expect(press(listScreen(), ['h'], ctx(), true).state).toEqual({ k: 'setup', locked: true })
    expect(press(listScreen(), ['h', 'Enter']).effects).toEqual([{ k: 'host', locked: false }])
    expect(press(listScreen(), ['h', 'ArrowRight', 'Enter']).effects).toEqual([{ k: 'host', locked: true }])
    expect(press(listScreen(), ['h', 'l']).state).toEqual({ k: 'hosting', locked: true, code: null, renewed: false })
    expect(press(listScreen(), ['h', 'o']).effects).toEqual([{ k: 'host', locked: false }])
  })

  it('⌫ cancels hosting and pairing back to the list', () => {
    for (const s of [{ k: 'hosting', locked: true, code: '123456', renewed: false }, { k: 'pairing', room: open }] as LobbyState[]) {
      expect(press(s, ['Backspace'])).toEqual({ state: listScreen(), effects: [{ k: 'cancel' }] })
    }
  })

  it('digits start the game’s own options; f goes full screen', () => {
    expect(press(withRooms(), ['2']).state).toEqual({ k: 'local', option: 1 })
    expect(press(withRooms(), ['4']).state).toEqual(withRooms())
    expect(press(withRooms(), ['f'])).toEqual({ state: withRooms(), effects: [{ k: 'fullscreen' }] })
  })

  it('renames: typed letters, tab for a random one, ↵ keeps it', () => {
    const { state, effects } = press(listScreen(), ['n', 'Backspace', 'Backspace', 'Backspace', 'Backspace', 'Backspace', 's', 'e', 'a', 'l', 'Enter'])
    expect(state).toEqual(listScreen())
    expect(effects).toEqual([{ k: 'rename', name: 'teal seal' }])
    expect(press(listScreen(), ['n', 'Tab', 'Enter']).effects).toEqual([{ k: 'rename', name: 'plum newt' }])
    // nothing left to keep: ↵ waits
    const blank = { k: 'rename', draft: '  ' } as LobbyState
    expect(press(blank, ['Enter']).state).toEqual(blank)
  })
})

describe('lobby when the relay has no room list', () => {
  const old = ctx({ codeOnly: true })
  it('hosts with a code and joins by typing one, as before room lists', () => {
    expect(press(listScreen(), ['h'], old).effects).toEqual([{ k: 'host', locked: true }])
    const { state, effects } = press(listScreen(), ['j', '0', '4', '2', '0', '1', '7'], old)
    expect(state).toEqual({ k: 'pairing', room: BY_CODE })
    expect(effects).toEqual([{ k: 'join', room: BY_CODE, code: '042017' }])
    expect(press(listScreen(), ['Enter', 'ArrowDown'], old).state).toEqual(listScreen())
  })
})

describe('lobby without a link', () => {
  const solo = ctx({ online: false, options: 2 })
  it('↑↓ and ↵ go through the options, and h or n do nothing', () => {
    expect(press(listScreen(), ['ArrowDown', 'Enter'], solo).state).toEqual({ k: 'local', option: 1 })
    expect(press(listScreen(), ['ArrowUp'], solo).state).toMatchObject({ pick: 1 })
    expect(press(listScreen(), ['h', 'n'], solo).state).toEqual(listScreen())
  })
})

describe('lobby events', () => {
  it('keeps the picked room picked as the list changes', () => {
    const next = lobbyEvent(withRooms(1), { type: 'rooms', rooms: [{ id: 'z', name: 'z', locked: false }, locked] }, ctx())
    expect(next).toMatchObject({ pick: 1 })
    expect(lobbyEvent(withRooms(1), { type: 'rooms', rooms: [] }, ctx())).toMatchObject({ pick: 0, rooms: [] })
    expect(lobbyEvent(withRooms(), { type: 'rooms', rooms: [], offline: true }, ctx())).toMatchObject({ offline: true })
  })

  it('shows the code, opens the game, and a wrong code asks again in the transport’s words', () => {
    const hosting: LobbyState = { k: 'hosting', locked: true, code: null, renewed: false }
    expect(lobbyEvent(hosting, { type: 'hosting', code: '042017' }, ctx())).toMatchObject({ code: '042017' })
    expect(lobbyEvent(hosting, { type: 'open', role: 'host', peer: 'jade owl' }, ctx())).toEqual({ k: 'game', role: 'host', peer: 'jade owl' })
    expect(lobbyEvent({ k: 'pairing', room: locked }, { type: 'closed', reason: 'wrong-code' }, ctx())).toEqual({
      k: 'code',
      room: locked,
      digits: '',
      note: closeReasonText('wrong-code', 'computer'),
    })
    expect(lobbyEvent({ k: 'pairing', room: open }, { type: 'closed', reason: 'gone' }, ctx())).toEqual(listScreen(closeReasonText('gone', 'computer')))
    // the game reports its own end
    const game: LobbyState = { k: 'game', role: 'guest', peer: 'x' }
    expect(lobbyEvent(game, { type: 'closed', reason: 'bye' }, ctx())).toBe(game)
  })
})

describe('lobby back and the link', () => {
  it('esc steps back from every screen but the list', () => {
    expect(lobbyBack(listScreen())).toBeNull()
    expect(lobbyBack({ k: 'setup', locked: true })?.state).toEqual(listScreen())
    expect(lobbyBack({ k: 'hosting', locked: false, code: '', renewed: false })?.effect).toEqual({ k: 'cancel' })
  })

  it('holds the link while hosting, pairing or playing', () => {
    expect(lobbyHoldsLink({ k: 'hosting', locked: false, code: '', renewed: false })).toBe(true)
    expect(lobbyHoldsLink({ k: 'game', role: 'host', peer: '' })).toBe(true)
    expect(lobbyHoldsLink(listScreen())).toBe(false)
    expect(lobbyHoldsLink({ k: 'local', option: 0 })).toBe(false)
  })
})
