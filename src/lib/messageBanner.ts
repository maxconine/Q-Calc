import { arrival, cleanMessageText, cleanName, sendStatusText, TEXT_MAX, type Messaging, type SendStatus } from './messaging'

// the cards under the bar when a message comes in: who sent it, what they said, and a line to answer them.
// someone new's card asks to allow or block them instead. the newest is on top and only a few are kept

export type BannerCard = { id: string; from: string; text: string; ask: boolean }

export const BANNER_SHOWN = 3
// how long a card stays when nobody's using it
export const BANNER_MS = 10_000
// how long "sent" shows before the card goes
export const SENT_MS = 1_200

// a card already up isn't added again, nor a second ask from a name that's asking
export function addCard(cards: BannerCard[], card: BannerCard, cap = BANNER_SHOWN): BannerCard[] {
  if (cards.some((c) => c.id === card.id || (card.ask && c.ask && c.from === card.from))) return cards
  return [card, ...cards].slice(0, cap)
}

export function addCards(cards: BannerCard[], incoming: BannerCard[], cap = BANNER_SHOWN): BannerCard[] {
  return incoming.reduce((list, card) => addCard(list, card, cap), cards)
}

// the mac app's unseen messages, oldest first, checked again here as the relay's frames are
export function parseNativeInbox(raw: unknown): BannerCard[] {
  if (!Array.isArray(raw)) return []
  return raw.flatMap((item): BannerCard[] => {
    const o = item && typeof item === 'object' ? (item as Record<string, unknown>) : {}
    const id = typeof o.id === 'string' && /^[\w-]{1,64}$/.test(o.id) ? o.id : ''
    const from = cleanName(o.from)
    const ask = o.ask === true
    const text = ask ? '' : cleanMessageText(o.text)
    return id && from && (ask || text.trim()) ? [{ id, from, text, ask }] : []
  })
}

// the lists changed (settings, a notification's buttons): an answered ask goes, and a blocked name's cards
export function dropAnswered(cards: BannerCard[], m: Messaging): BannerCard[] {
  const next = cards.filter((c) => {
    const how = arrival(m, c.from)
    return how !== 'drop' && (!c.ask || how === 'ask')
  })
  return next.length === cards.length ? cards : next
}

// a card waits while the pointer's on the cards, or while its reply is being typed or sent
export function cardHeld(o: { hover: boolean; focus: boolean; draft: string; sending: boolean }): boolean {
  return o.hover || o.focus || o.sending || o.draft.length > 0
}

export function replyText(draft: string): string {
  return draft.trim().slice(0, TEXT_MAX)
}

export type ReplyState = { kind: 'idle' } | { kind: 'sending' } | { kind: 'sent' } | { kind: 'failed'; note: string }

export function replyState(status: SendStatus, to: string): ReplyState {
  return status === 'delivered' ? { kind: 'sent' } : { kind: 'failed', note: sendStatusText(status, to) }
}
