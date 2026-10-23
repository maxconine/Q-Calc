import { useEffect, useRef, useState } from 'react'
import {
  allowSender,
  arrival,
  blockSender,
  InboxLink,
  loadMessaging,
  nativeMessaging,
  saveMessaging,
  type Messaging,
} from '../lib/messaging'
import './MessageNotices.css'

// messages under the bar, for hosts without the mac app's own notifications (windows, the browser). the bar
// holds the inbox while it runs; a message from an accepted name shows as `name: text`, someone new shows as a
// request to allow or block, and a blocked name's messages never show. nothing here is kept past the page

type Notice = { id: number; from: string; text: string; ask: boolean }

// the newest few; older ones make way
const SHOWN = 3

export function MessageNotices({ messaging, hidden }: { messaging: Messaging; hidden: boolean }) {
  const [notices, setNotices] = useState<Notice[]>([])
  // what each asking name said last, shown once they're allowed
  const held = useRef(new Map<string, string>())
  const nextId = useRef(1)
  const native = nativeMessaging()
  const { name, key } = messaging

  // one link per name; the bar remounts this when the name changes, so an old name's notices go with it
  useEffect(() => {
    if (native || !name || !key) return
    const link = new InboxLink((e) => {
      if (e.type !== 'message') return
      const m = loadMessaging()
      const how = arrival(m, e.from)
      if (how === 'drop') return
      const asked = held.current.has(e.from)
      if (how === 'ask') held.current.set(e.from, e.text)
      // a name already asking isn't asked about again
      if (how === 'ask' && asked) return
      const notice = { id: nextId.current++, from: e.from, text: e.text, ask: how === 'ask' }
      setNotices((cur) => [...cur, notice].slice(-SHOWN))
    })
    link.start(name, key)
    const retry = () => link.retryNow()
    window.addEventListener('online', retry)
    return () => {
      window.removeEventListener('online', retry)
      link.stop()
    }
  }, [native, name, key])

  if (native || hidden || !name || !notices.length) return null

  const drop = (id: number) => setNotices((cur) => cur.filter((n) => n.id !== id))
  const answer = (n: Notice, allow: boolean) => {
    const m = loadMessaging()
    saveMessaging(allow ? allowSender(m, n.from) : blockSender(m, n.from))
    const said = held.current.get(n.from)
    held.current.delete(n.from)
    // allowed, the request becomes what they said; blocked, it goes
    setNotices((cur) => cur.flatMap((x) => (x.id !== n.id ? [x] : allow && said ? [{ ...x, text: said, ask: false }] : [])))
  }

  return (
    <div className="message-notices" role="status" aria-live="polite">
      {notices.map((n) => (
        <div className="message-notice" key={n.id}>
          {n.ask ? (
            <>
              <span className="message-notice-text">
                <b>{n.from}</b> wants to message you
              </span>
              <button type="button" className="message-notice-act" onClick={() => answer(n, true)}>
                Allow
              </button>
              <button type="button" className="message-notice-act" onClick={() => answer(n, false)}>
                Block
              </button>
            </>
          ) : (
            <>
              <span className="message-notice-text">
                <b>{n.from}</b>: {n.text}
              </span>
              <button type="button" className="message-notice-act" aria-label="Dismiss" onClick={() => drop(n.id)}>
                ×
              </button>
            </>
          )}
        </div>
      ))}
    </div>
  )
}
