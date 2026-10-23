import { useEffect, useState } from 'react'
import {
  claimName,
  cleanName,
  forgetSender,
  loadMessaging,
  newKey,
  onMessagingChange,
  releaseName,
  saveMessaging,
  type ClaimStatus,
} from '../lib/messaging'

const CLAIM_NOTES: Record<Exclude<ClaimStatus, 'ok'>, (name: string) => string> = {
  taken: (name) => `${name} is taken · try another`,
  'too-many': () => 'too many tries · wait a minute',
  bad: () => 'a name is 2–20 letters, digits, - or _',
  network: () => 'couldn’t reach the relay · try again',
}

// the windows settings window's messaging rows: your name, which holds it on the relay and turns receiving on,
// and the names you've accepted or blocked. the mac app has the same rows in its own settings window
export function MessagingSettings() {
  const [m, setM] = useState(loadMessaging)
  const [draft, setDraft] = useState(m.name)
  const [note, setNote] = useState('')
  const [busy, setBusy] = useState(false)

  useEffect(() => onMessagingChange(setM), [])

  const save = async () => {
    const name = cleanName(draft)
    if (!draft.trim()) return clear()
    if (!name) return setNote(CLAIM_NOTES.bad(''))
    if (name === m.name) return setNote('')
    setBusy(true)
    const cur = loadMessaging()
    const key = cur.key || newKey()
    const status = await claimName(name, key)
    setBusy(false)
    if (status !== 'ok') {
      setNote(CLAIM_NOTES[status](name))
      return
    }
    if (cur.name) void releaseName(cur.name, key)
    saveMessaging({ ...cur, name, key })
    setNote('')
  }

  const clear = async () => {
    const cur = loadMessaging()
    setDraft('')
    setNote('')
    if (!cur.name) return
    void releaseName(cur.name, cur.key)
    saveMessaging({ ...cur, name: '' })
  }

  const status = m.name ? `Receiving as ${m.name}. Type message <name> <text> in the bar to send.` : 'Off. Set a name to send and receive messages.'

  return (
    <>
      <section className="unit-settings" aria-label="Your name">
        <div className="unit-settings-head">
          <h2>Your name</h2>
          {m.name ? (
            <button type="button" className="unit-reset" disabled={busy} onClick={() => void clear()}>
              Turn off
            </button>
          ) : null}
        </div>
        <p className="unit-settings-hint" aria-live="polite">
          {note || status}
        </p>
        <form
          className="setting-name"
          onSubmit={(e) => {
            e.preventDefault()
            void save()
          }}
        >
          <input
            className="setting-text"
            aria-label="Your name"
            placeholder="your name"
            spellCheck={false}
            autoCapitalize="off"
            maxLength={20}
            value={draft}
            onChange={(e) => {
              setDraft(e.target.value.toLowerCase())
              setNote('')
            }}
          />
          <button type="submit" className="unit-reset" disabled={busy || cleanName(draft) === m.name}>
            {busy ? 'Checking…' : 'Save'}
          </button>
        </form>
      </section>
      <SenderList title="Accepted" hint="Their messages show. Anyone else’s first message asks you." names={m.accepted} action="Remove" />
      <SenderList title="Blocked" hint="Their messages never show, and they aren’t told." names={m.blocked} action="Unblock" />
    </>
  )
}

function SenderList({ title, hint, names, action }: { title: string; hint: string; names: string[]; action: string }) {
  return (
    <section className="unit-settings" aria-label={title}>
      <div className="unit-settings-head">
        <h2>{title}</h2>
      </div>
      <p className="unit-settings-hint">{names.length ? hint : `Nobody yet. ${hint}`}</p>
      {names.length ? (
        <div className="unit-group-body">
          {names.map((name) => (
            <div className="unit-row" key={name}>
              <span className="unit-row-label">{name}</span>
              <button type="button" className="unit-reset" onClick={() => saveMessaging(forgetSender(loadMessaging(), name))}>
                {action}
              </button>
            </div>
          ))}
        </div>
      ) : null}
    </section>
  )
}
