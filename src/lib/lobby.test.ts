import { describe, expect, it } from 'vitest'
import { BY_CODE, firstScreen, homeScreen, lobbyBack, lobbyEvent, lobbyHoldsLink, lobbyKey, type LobbyCtx, type LobbyState } from './lobby'
import { closeReasonText, type RoomInfo } from './peer'

// pong's lobby: three levels and a room list
const ctx = (over: Partial<LobbyCtx> = {}): LobbyCtx => ({
  online: true,
  codeOnly: false,
  codeLength: 6,
  solo: 3,
  together: 0,
  closeText: (r) => closeReasonText(r, 'computer'),
  randomName: () => 'plum newt',
  ...over,
})

const open: RoomInfo = { id: 'a', name: 'teal otter', locked: false }
const locked: RoomInfo = { id: 'b', name: 'amber fox', locked: true }
const start = homeScreen('start')
const friend = homeScreen('friend')
const withRooms = (pick = 0): LobbyState => ({ k: 'home', screen: 'friend', rooms: [open, locked], pick })

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

describe('lobby first screen', () => {
  it('opens on the choice only when there’s both a computer and a friend to play', () => {
    expect(firstScreen(ctx())).toBe('start')
    // trails with no link: the friend is on the same keyboard
    expect(firstScreen(ctx({ online: false, solo: 3, together: 1 }))).toBe('start')
    // ember: no computer
    expect(firstScreen(ctx({ solo: 0, together: 1 }))).toBe('friend')
    // pong with no link, crack, equal, 24
    expect(firstScreen(ctx({ online: false }))).toBe('solo')
  })

  it('1 is vs computer, 2 vs a friend, and the arrows and ↵ pick either', () => {
    expect(press(start, ['1']).state).toEqual(homeScreen('solo'))
    expect(press(start, ['2']).state).toEqual(friend)
    expect(press(start, ['3']).state).toEqual(start)
    expect(press(start, ['Enter']).state).toEqual(homeScreen('solo'))
    expect(press(start, ['ArrowRight', 'Enter']).state).toEqual(friend)
    expect(press(start, ['ArrowDown', 'ArrowDown', 'Enter']).state).toEqual(homeScreen('solo'))
    expect(press(start, ['ArrowLeft']).state).toMatchObject({ pick: 1 })
  })

  it('h and j go straight to hosting or the code; n renames and comes back; f goes full screen', () => {
    expect(press(start, ['h']).state).toEqual({ k: 'setup', locked: false })
    expect(press(start, ['h'], ctx({ codeOnly: true })).effects).toEqual([{ k: 'host', locked: true }])
    expect(press(start, ['j']).state).toEqual({ k: 'code', room: BY_CODE, digits: '' })
    expect(press(start, ['n']).state).toEqual({ k: 'rename', draft: 'teal otter', from: 'start' })
    expect(press(start, ['n', 'Enter'])).toEqual({ state: start, effects: [{ k: 'rename', name: 'teal otter' }] })
    expect(press(start, ['f'])).toEqual({ state: start, effects: [{ k: 'fullscreen' }] })
  })

  it('without a link, h j and n do nothing', () => {
    const c = ctx({ online: false, together: 1 })
    expect(press(start, ['h', 'j', 'n'], c).state).toEqual(start)
  })

  it('keeps the rooms it’s heard while the choice shows, for the friend screen', () => {
    const heard = lobbyEvent(start, { type: 'rooms', rooms: [open, locked] }, ctx())
    expect(heard).toMatchObject({ screen: 'start', pick: 0, rooms: [open, locked] })
    expect(press(heard, ['2']).state).toEqual(withRooms())
  })
})

describe('lobby vs computer', () => {
  const solo = homeScreen('solo')
  it('digits play a level, and ↑↓ ←→ with ↵ pick one', () => {
    expect(press(solo, ['2']).state).toEqual({ k: 'local', option: 1 })
    expect(press(solo, ['4']).state).toEqual(solo)
    expect(press(solo, ['ArrowDown', 'ArrowRight', 'Enter']).state).toEqual({ k: 'local', option: 2 })
    expect(press(solo, ['ArrowUp', 'Enter']).state).toEqual({ k: 'local', option: 2 })
  })

  it('⌫ goes back to the choice; h and n do nothing here', () => {
    expect(press(solo, ['Backspace']).state).toEqual(start)
    expect(press(solo, ['h', 'n']).state).toEqual(solo)
  })

  it('opens here for a game played alone, where ⌫ does nothing', () => {
    const c = ctx({ online: false, solo: 2 })
    expect(press(solo, ['ArrowDown', 'Enter'], c).state).toEqual({ k: 'local', option: 1 })
    expect(press(solo, ['Backspace'], c).state).toEqual(solo)
  })
})

describe('lobby vs a friend', () => {
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

  it('j types a code with no room picked', () => {
    const { state, effects } = press(withRooms(), ['j', '0', '4', '2', '0', '1', '7'])
    expect(state).toEqual({ k: 'pairing', room: BY_CODE })
    expect(effects).toEqual([{ k: 'join', room: BY_CODE, code: '042017' }])
  })

  it('↑↓ wrap round the rooms, and ⌫ on an empty code goes back to the friend screen', () => {
    expect(press(withRooms(), ['ArrowUp']).state).toMatchObject({ pick: 1 })
    expect(press(withRooms(1), ['ArrowDown']).state).toMatchObject({ pick: 0 })
    expect(press({ k: 'code', room: locked, digits: '' }, ['Backspace']).state).toEqual(friend)
  })

  it('hosts open or locked, starting from the last choice', () => {
    expect(press(friend, ['h']).state).toEqual({ k: 'setup', locked: false })
    expect(press(friend, ['h'], ctx(), true).state).toEqual({ k: 'setup', locked: true })
    expect(press(friend, ['h', 'Enter']).effects).toEqual([{ k: 'host', locked: false }])
    expect(press(friend, ['h', 'ArrowRight', 'Enter']).effects).toEqual([{ k: 'host', locked: true }])
    expect(press(friend, ['h', 'l']).state).toEqual({ k: 'hosting', locked: true, code: null, renewed: false })
    expect(press(friend, ['h', 'o']).effects).toEqual([{ k: 'host', locked: false }])
    expect(press(friend, ['h', 'Backspace']).state).toEqual(friend)
  })

  it('⌫ cancels hosting and pairing back to the friend screen', () => {
    for (const s of [{ k: 'hosting', locked: true, code: '123456', renewed: false }, { k: 'pairing', room: open }] as LobbyState[]) {
      expect(press(s, ['Backspace'])).toEqual({ state: friend, effects: [{ k: 'cancel' }] })
    }
  })

  it('⌫ goes back to the choice, keeping the rooms', () => {
    expect(press(withRooms(1), ['Backspace']).state).toEqual({ ...start, rooms: [open, locked] })
  })

  it('renames: typed letters, tab for a random one, ↵ keeps it', () => {
    const { state, effects } = press(friend, ['n', 'Backspace', 'Backspace', 'Backspace', 'Backspace', 'Backspace', 's', 'e', 'a', 'l', 'Enter'])
    expect(state).toEqual(friend)
    expect(effects).toEqual([{ k: 'rename', name: 'teal seal' }])
    expect(press(friend, ['n', 'Tab', 'Enter']).effects).toEqual([{ k: 'rename', name: 'plum newt' }])
    // nothing left to keep: ↵ waits
    const blank = { k: 'rename', draft: '  ', from: 'friend' } as LobbyState
    expect(press(blank, ['Enter']).state).toEqual(blank)
  })

  it('trails: digits play two on one keyboard, after the levels; the rooms still take ↑↓ ↵', () => {
    const c = ctx({ together: 1 })
    expect(press(withRooms(), ['1'], c).state).toEqual({ k: 'local', option: 3 })
    expect(press(withRooms(), ['2'], c).state).toEqual(withRooms())
    expect(press(withRooms(), ['ArrowDown', 'Enter'], c).state).toEqual({ k: 'code', room: locked, digits: '' })
  })

  it('ember opens here: same keyboard is 1, and ⌫ goes nowhere', () => {
    const c = ctx({ solo: 0, together: 1 })
    expect(press(friend, ['1'], c).state).toEqual({ k: 'local', option: 0 })
    expect(press(friend, ['Backspace'], c).state).toEqual(friend)
    expect(lobbyBack(friend, c)).toBeNull()
  })

  it('with no link, the arrows and ↵ go through the same-keyboard options', () => {
    const c = ctx({ online: false, together: 2 })
    expect(press(friend, ['ArrowDown', 'Enter'], c).state).toEqual({ k: 'local', option: 4 })
    expect(press(friend, ['h', 'j', 'n'], c).state).toEqual(friend)
  })
})

describe('lobby when the relay has no room list', () => {
  const old = ctx({ codeOnly: true })
  it('hosts with a code and joins by typing one, as before room lists', () => {
    expect(press(friend, ['h'], old).effects).toEqual([{ k: 'host', locked: true }])
    const { state, effects } = press(friend, ['j', '0', '4', '2', '0', '1', '7'], old)
    expect(state).toEqual({ k: 'pairing', room: BY_CODE })
    expect(effects).toEqual([{ k: 'join', room: BY_CODE, code: '042017' }])
    expect(press(friend, ['Enter', 'ArrowDown'], old).state).toEqual(friend)
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
    expect(lobbyEvent({ k: 'pairing', room: open }, { type: 'closed', reason: 'gone' }, ctx())).toEqual(homeScreen('friend', closeReasonText('gone', 'computer')))
    // the game reports its own end
    const game: LobbyState = { k: 'game', role: 'guest', peer: 'x' }
    expect(lobbyEvent(game, { type: 'closed', reason: 'bye' }, ctx())).toBe(game)
  })
})

describe('lobby back and the link', () => {
  it('esc steps back to the choice, and from it closes the game', () => {
    expect(lobbyBack(start, ctx())).toBeNull()
    expect(lobbyBack(homeScreen('solo'), ctx())?.state).toEqual(start)
    expect(lobbyBack(friend, ctx())?.state).toEqual(start)
    expect(lobbyBack(homeScreen('solo'), ctx({ online: false }))).toBeNull()
    expect(lobbyBack({ k: 'setup', locked: true }, ctx())?.state).toEqual(friend)
    expect(lobbyBack({ k: 'code', room: BY_CODE, digits: '12' }, ctx())?.state).toEqual(friend)
    expect(lobbyBack({ k: 'rename', draft: 'x', from: 'start' }, ctx())?.state).toEqual(start)
    expect(lobbyBack({ k: 'hosting', locked: false, code: '', renewed: false }, ctx())).toEqual({ state: friend, effect: { k: 'cancel' }, handled: true })
  })

  it('holds the link while hosting, pairing or playing', () => {
    expect(lobbyHoldsLink({ k: 'hosting', locked: false, code: '', renewed: false })).toBe(true)
    expect(lobbyHoldsLink({ k: 'game', role: 'host', peer: '' })).toBe(true)
    expect(lobbyHoldsLink(start)).toBe(false)
    expect(lobbyHoldsLink({ k: 'local', option: 0 })).toBe(false)
  })
})
