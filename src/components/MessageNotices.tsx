import { useCallback, useEffect, useImperativeHandle, useRef, useState, type Ref } from 'react'
import { flushSync } from 'react-dom'
import { nativeHandler, nativeWindow } from '../lib/bridge'
import {
  addCards,
  BANNER_MS,
  cardHeld,
  dropAnswered,
  parseNativeInbox,
  replyState,
  replyText,
  SENT_MS,
  type BannerCard,
  type ReplyState,
} from '../lib/messageBanner'
import {
  allowSender,
  arrival,
  blockSender,
  InboxLink,
  loadMessaging,
  nativeMessaging,
  rememberRecipient,
  saveMessaging,
  sendMessage,
  TEXT_MAX,
  type Messaging,
} from '../lib/messaging'
import './MessageNotices.css'

// messages under the bar, as cards with a line to reply on. the mac app holds the inbox and hands over what the
// bar hasn't shown yet (__qcalcInbox), and is told once it's up so it isn't handed again; everywhere else the bar
// holds the inbox itself while it runs. an accepted name's message is a card, someone new's is a request to allow
// or block, and a blocked name's never shows. a card goes after a while unless it's in use; nothing is kept past
// the page

export type MessageNoticesHandle = {
  // the bar went away or came back: cards up are counted as seen, ones that came meanwhile wait for it
  setShown: (on: boolean) => void
  // esc: true when there were cards to close
  close: () => boolean
}

type InboxWindow = Window & { __qcalcInbox?: (items: unknown) => void }

export function MessageNotices({
  messaging,
  hidden,
  handle,
  onOpen,
  onFocusBar,
}: {
  messaging: Messaging
  hidden: boolean
  handle?: Ref<MessageNoticesHandle>
  onOpen?: (open: boolean) => void
  onFocusBar?: () => void
}) {
  const [kept, setCards] = useState<BannerCard[]>([])
  // an ask the lists have answered since (settings, a notification's buttons) goes, as does a blocked name's card
  const cards = dropAnswered(kept, messaging)
  const [hover, setHover] = useState(false)
  // what came while the bar was away or a game had it
  const waiting = useRef<BannerCard[]>([])
  const shown = useRef(true)
  const hiddenRef = useRef(hidden)
  // what each asking name said last, shown once they're allowed (the mac app keeps its own)
  const held = useRef(new Map<string, string>())
  const nextId = useRef(1)
  const stackRef = useRef<HTMLDivElement>(null)
  const native = nativeMessaging()
  const { name, key } = messaging

  const take = useCallback(
    (incoming: BannerCard[], sync = false) => {
      if (!incoming.length) return
      if (!shown.current || hiddenRef.current) {
        waiting.current.push(...incoming)
        return
      }
      const put = () => setCards((cur) => addCards(cur, incoming))
      // the mac app asks for the bar's height straight after, so the cards are laid out by then
      if (sync) flushSync(put)
      else put()
      if (native) nativeHandler()?.postMessage({ type: 'messagesSeen', ids: incoming.map((c) => c.id) })
    },
    [native],
  )

  const flush = useCallback(() => {
    const cards = waiting.current
    waiting.current = []
    take(cards)
  }, [take])

  useImperativeHandle(
    handle,
    () => ({
      setShown: (on) => {
        shown.current = on
        if (on) flush()
        else {
          setCards([])
          setHover(false)
        }
      },
      close: () => {
        if (!cards.length) return false
        setCards([])
        setHover(false)
        onFocusBar?.()
        return true
      },
    }),
    [cards.length, flush, onFocusBar],
  )

  useEffect(() => {
    hiddenRef.current = hidden
    if (!hidden) flush()
  }, [hidden, flush])

  const open = !hidden && cards.length > 0
  useEffect(() => onOpen?.(open), [open, onOpen])

  useEffect(() => {
    if (!native) return
    const w = nativeWindow() as InboxWindow | undefined
    if (!w) return
    w.__qcalcInbox = (items) => take(parseNativeInbox(items), true)
    return () => {
      if (w.__qcalcInbox) delete w.__qcalcInbox
    }
  }, [native, take])

  // one link per name; the bar remounts this when the name changes, so an old name's cards go with it
  useEffect(() => {
    if (native || !name || !key) return
    const link = new InboxLink((e) => {
      if (e.type !== 'message') return
      const how = arrival(loadMessaging(), e.from)
      if (how === 'drop') return
      if (how === 'ask') held.current.set(e.from, e.text)
      take([{ id: `w${nextId.current++}`, from: e.from, text: how === 'ask' ? '' : e.text, ask: how === 'ask' }])
    })
    link.start(name, key)
    const retry = () => link.retryNow()
    window.addEventListener('online', retry)
    return () => {
      window.removeEventListener('online', retry)
      link.stop()
    }
  }, [native, name, key, take])

  const drop = useCallback(
    (id: string) => {
      // a reply field that had the keys hands them back to the bar
      if (stackRef.current?.contains(document.activeElement)) onFocusBar?.()
      setCards((cur) => cur.filter((c) => c.id !== id))
    },
    [onFocusBar],
  )

  const answer = (card: BannerCard, allow: boolean) => {
    if (native) {
      // the mac app keeps what they said, and hands it over as a card of its own once they're allowed
      nativeHandler()?.postMessage({ type: 'messageAnswer', from: card.from, allow })
      drop(card.id)
      return
    }
    const m = loadMessaging()
    saveMessaging(allow ? allowSender(m, card.from) : blockSender(m, card.from))
    const said = held.current.get(card.from)
    held.current.delete(card.from)
    setCards((cur) => cur.flatMap((c) => (c.id !== card.id ? [c] : allow && said ? [{ ...c, text: said, ask: false }] : [])))
  }

  if (hidden || !name || !cards.length) return null

  return (
    <div
      ref={stackRef}
      className="message-banner"
      role="status"
      aria-live="polite"
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
    >
      {cards.map((c) =>
        c.ask ? (
          <AskCard key={c.id} card={c} hover={hover} onClose={drop} onAnswer={answer} />
        ) : (
          <MessageCard key={c.id} card={c} hover={hover} onClose={drop} />
        ),
      )}
    </div>
  )
}

function useAutoClose(id: string, held: boolean, onClose: (id: string) => void, ms = BANNER_MS) {
  useEffect(() => {
    if (held) return
    const t = window.setTimeout(() => onClose(id), ms)
    return () => window.clearTimeout(t)
  }, [id, held, onClose, ms])
}

function AskCard({
  card,
  hover,
  onClose,
  onAnswer,
}: {
  card: BannerCard
  hover: boolean
  onClose: (id: string) => void
  onAnswer: (card: BannerCard, allow: boolean) => void
}) {
  useAutoClose(card.id, hover, onClose)
  return (
    <div className="message-card message-card-ask">
      <div className="message-card-head">
        <span className="message-card-from">{card.from}</span>
        <span className="message-card-asks">wants to message you</span>
        <button type="button" className="message-card-act" onClick={() => onAnswer(card, true)}>
          Allow
        </button>
        <button type="button" className="message-card-act" onClick={() => onAnswer(card, false)}>
          Block
        </button>
      </div>
    </div>
  )
}

function MessageCard({ card, hover, onClose }: { card: BannerCard; hover: boolean; onClose: (id: string) => void }) {
  const [draft, setDraft] = useState('')
  const [focus, setFocus] = useState(false)
  const [reply, setReply] = useState<ReplyState>({ kind: 'idle' })
  const sending = reply.kind === 'sending'
  const sent = reply.kind === 'sent'
  useAutoClose(card.id, sent || cardHeld({ hover, focus, draft, sending }), onClose)
  useAutoClose(card.id, !sent, onClose, SENT_MS)

  const send = async () => {
    const text = replyText(draft)
    if (!text || sending || sent) return
    setReply({ kind: 'sending' })
    const status = await sendMessage(loadMessaging(), card.from, text)
    if (status === 'delivered') rememberRecipient(card.from)
    setReply(replyState(status, card.from))
  }

  return (
    <div className="message-card">
      <div className="message-card-head">
        <span className="message-card-from">{card.from}</span>
        <button type="button" className="message-card-act message-card-close" aria-label="Dismiss" onClick={() => onClose(card.id)}>
          ×
        </button>
      </div>
      <p className="message-card-text">{card.text}</p>
      <form
        className="message-card-reply"
        onSubmit={(e) => {
          e.preventDefault()
          void send()
        }}
      >
        <input
          className="message-card-field"
          aria-label={`Reply to ${card.from}`}
          placeholder={`Reply to ${card.from}`}
          spellCheck={false}
          maxLength={TEXT_MAX}
          value={draft}
          // read only rather than disabled, so the field keeps the keys while it sends
          readOnly={sending || sent}
          onFocus={() => setFocus(true)}
          onBlur={() => setFocus(false)}
          onChange={(e) => {
            setDraft(e.target.value)
            if (reply.kind === 'failed') setReply({ kind: 'idle' })
          }}
        />
        {sent ? (
          <span className="message-card-note message-card-sent">sent</span>
        ) : sending ? (
          <span className="message-card-note">sending…</span>
        ) : reply.kind === 'failed' ? (
          <span className="message-card-note">{reply.note}</span>
        ) : null}
      </form>
    </div>
  )
}
