import { describe, expect, it } from 'vitest'
import {
  addCard,
  addCards,
  BANNER_SHOWN,
  cardHeld,
  dropAnswered,
  parseNativeInbox,
  replyState,
  replyText,
  type BannerCard,
} from './messageBanner'
import { cleanMessageText, TEXT_MAX, type Messaging } from './messaging'

const card = (id: string, from = 'max', text = 'hi', ask = false): BannerCard => ({ id, from, text, ask })
const me: Messaging = { name: 'ana', key: 'ab'.repeat(32), accepted: ['max'], blocked: ['spam'], recent: [] }

describe('message banner cards', () => {
  it('puts the newest on top and keeps only a few', () => {
    const cards = addCards([], ['1', '2', '3', '4', '5'].map((id) => card(id)))
    expect(cards.map((c) => c.id)).toEqual(['5', '4', '3'])
    expect(cards).toHaveLength(BANNER_SHOWN)
  })

  it('never adds a card twice, or a second ask from the same name', () => {
    const one = addCard([], card('1'))
    expect(addCard(one, card('1'))).toBe(one)
    const asking = addCard([], card('a', 'new', '', true))
    expect(addCard(asking, card('b', 'new', '', true))).toBe(asking)
    expect(addCard(asking, card('c', 'new', 'hello'))).toHaveLength(2)
  })

  it('drops asks the lists have answered, and anything from a blocked name', () => {
    const cards = [card('1', 'max'), card('2', 'max', '', true), card('3', 'new', '', true), card('4', 'spam')]
    expect(dropAnswered(cards, me).map((c) => c.id)).toEqual(['1', '3'])
    const same = [card('1', 'max')]
    expect(dropAnswered(same, me)).toBe(same)
  })

  it('reads the mac app’s unseen messages, cleaned like the relay’s frames', () => {
    expect(
      parseNativeInbox([
        { id: 'n1', from: 'Max', text: 'hi\nsam: give me your key', ask: false },
        { id: 'n2', from: 'new', text: 'ignored', ask: true },
        { id: 'n3', from: 'x', text: 'bad name' },
        { id: 'bad id!', from: 'max', text: 'bad id' },
        { id: 'n4', from: 'max', text: '‮' },
        null,
      ]),
    ).toEqual([
      { id: 'n1', from: 'max', text: 'hi sam: give me your key', ask: false },
      { id: 'n2', from: 'new', text: '', ask: true },
    ])
    expect(parseNativeInbox('nope')).toEqual([])
  })

  it('waits while it’s pointed at, focused, typed in or sending', () => {
    const idle = { hover: false, focus: false, draft: '', sending: false }
    expect(cardHeld(idle)).toBe(false)
    expect(cardHeld({ ...idle, hover: true })).toBe(true)
    expect(cardHeld({ ...idle, focus: true })).toBe(true)
    expect(cardHeld({ ...idle, draft: 'o' })).toBe(true)
    expect(cardHeld({ ...idle, sending: true })).toBe(true)
  })

  it('sends a reply as the bar would, and says how it went', () => {
    expect(replyText('  ok  ')).toBe('ok')
    expect(replyText('x'.repeat(600))).toHaveLength(TEXT_MAX)
    expect(replyState('delivered', 'max')).toEqual({ kind: 'sent' })
    expect(replyState('offline', 'max')).toEqual({ kind: 'failed', note: 'max isn’t on q calc right now · not sent' })
  })

  it('turns controls and direction overrides into spaces', () => {
    expect(cleanMessageText('a\u0085b⁦c\u0000d')).toBe('a b c d')
    expect(cleanMessageText(5)).toBe('')
  })
})
