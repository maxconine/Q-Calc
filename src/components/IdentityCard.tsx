import { useEffect, useMemo, useRef, useState, type PointerEvent } from 'react'
import { copyText } from '../lib/dom'
import { identityLabel, identityRuns, type Identity, type IdentitySheet } from '../lib/identities'
import './PeriodicCard.css'
import './IdentityCard.css'

function IdentityText({ show }: { show: string }) {
  return (
    <>
      {identityRuns(show).map((r, i) =>
        r.shift === 'sup' ? (
          <sup key={i}>{r.text}</sup>
        ) : r.shift === 'sub' ? (
          <sub key={i}>{r.text}</sub>
        ) : r.shift === 'supsub' ? (
          <sup key={i}>
            <sub>{r.text}</sub>
          </sup>
        ) : (
          <span key={i}>{r.text}</span>
        ),
      )}
    </>
  )
}

// the browser build's stand-in for the mac app's identity sheet window; a click copies the line
export function IdentityCard({ sheet, onClose }: { sheet: IdentitySheet; onClose: () => void }) {
  const [offset, setOffset] = useState({ x: 0, y: 0 })
  const [copied, setCopied] = useState<Identity | null>(null)
  const copiedTimer = useRef(0)
  const dragStart = useRef<{ x: number; y: number; from: { x: number; y: number } } | null>(null)
  useEffect(() => () => window.clearTimeout(copiedTimer.current), [])

  const sections = useMemo(
    () =>
      sheet.sections.map((s) => (
        <section key={s.title} className="identity-section">
          <h2 className="identity-section-title">{s.title}</h2>
          <div className="identity-groups">
            {s.groups.map((g) => (
              <div key={g.title} className="identity-group">
                <h3 className="identity-group-title">{g.title}</h3>
                {g.items.map((item) => (
                  <button
                    key={item.copy}
                    type="button"
                    className={`identity-row${copied === item ? ' copied' : ''}`}
                    aria-label={`${identityLabel(item.show)}${item.note ? `, ${item.note}` : ''}, copy`}
                    onClick={() => {
                      copyText(item.copy)
                      setCopied(item)
                      window.clearTimeout(copiedTimer.current)
                      copiedTimer.current = window.setTimeout(() => setCopied(null), 1200)
                    }}
                  >
                    <span className="identity-math">
                      <IdentityText show={item.show} />
                    </span>
                    {copied === item ? (
                      <span className="identity-note" aria-hidden>
                        ✓
                      </span>
                    ) : item.note ? (
                      <span className="identity-note">{item.note}</span>
                    ) : null}
                  </button>
                ))}
              </div>
            ))}
          </div>
        </section>
      )),
    [sheet, copied],
  )

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    // clicks on the card keep the input's focus and caret
    e.preventDefault()
    if ((e.target as HTMLElement).closest('button, .identity-body')) return
    dragStart.current = { x: e.clientX, y: e.clientY, from: offset }
    e.currentTarget.setPointerCapture(e.pointerId)
  }
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const d = dragStart.current
    if (d) setOffset({ x: d.from.x + e.clientX - d.x, y: d.from.y + e.clientY - d.y })
  }

  return (
    <div
      className="periodic identity"
      role="dialog"
      aria-label={sheet.title}
      style={{ transform: `translate(calc(-50% + ${offset.x}px), ${offset.y}px)` }}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={() => (dragStart.current = null)}
      onMouseDown={(e) => e.preventDefault()}
    >
      <button type="button" className="periodic-close" aria-label="Close" onClick={onClose}>
        ×
      </button>
      <div className="identity-head">
        <span className="identity-title">{sheet.title}</span>
        <span className={`identity-copied${copied ? ' shown' : ''}`} aria-live="polite">
          {copied ? '✓ copied' : 'click to copy'}
        </span>
      </div>
      <div className="identity-body">{sections}</div>
    </div>
  )
}
