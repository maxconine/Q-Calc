import { useMemo, useRef, useState, type PointerEvent } from 'react'
import { ELEMENTS, formulaOfPicks, gridCell, type Element } from '../lib/elements'
import './PeriodicCard.css'

// the browser build's stand-in for the mac app's periodic table window; `windowed`, it fills a window of its own (the
// windows shell's), which has its own title bar to move and close it. each click adds an atom to one formula (H, H, O
// is H₂O), a right click or a click on its part takes one away, and a click on the formula puts its molar mass in the bar
// the last of an element picked comes off
const dropped = (symbol: string) => (picks: string[]) => {
  const at = picks.lastIndexOf(symbol)
  return at < 0 ? picks : [...picks.slice(0, at), ...picks.slice(at + 1)]
}

export function PeriodicCard({ onPick, onClose, windowed = false }: { onPick: (text: string) => void; onClose: () => void; windowed?: boolean }) {
  const [offset, setOffset] = useState({ x: 0, y: 0 })
  const [shown, setShown] = useState<Element | null>(null)
  const [picks, setPicks] = useState<string[]>([])
  const dragStart = useRef<{ x: number; y: number; from: { x: number; y: number } } | null>(null)
  const formula = useMemo(() => formulaOfPicks(picks), [picks])
  // built only when the picks change, so a drag, a hover or a keystroke in the overlay doesn't re-render all 118 tiles
  const counts = useMemo(() => new Map(formula?.parts.map((p) => [p.symbol, p.count])), [formula])
  const tiles = useMemo(
    () =>
      ELEMENTS.map((e) => {
        const { row, col } = gridCell(e)
        const count = counts.get(e.symbol)
        return (
          <button
            key={e.n}
            type="button"
            className={`periodic-tile periodic-${e.category}${count ? ' picked' : ''}`}
            style={{ gridRow: row, gridColumn: col }}
            title={`${e.name} · ${e.symbol} · ${e.n}`}
            aria-label={`${e.name}, ${e.n}, ${e.mass}${count ? `, ${count} in the formula` : ''}`}
            onMouseEnter={() => setShown(e)}
            onFocus={() => setShown(e)}
            onBlur={() => setShown(null)}
            onClick={() => setPicks((p) => [...p, e.symbol])}
            onContextMenu={(ev) => {
              ev.preventDefault()
              setPicks(dropped(e.symbol))
            }}
          >
            <span className="periodic-n">{e.n}</span>
            {count && count > 1 ? <span className="periodic-count">×{count}</span> : null}
            <span className="periodic-symbol">{e.symbol}</span>
            <span className="periodic-mass">{e.mass}</span>
          </button>
        )
      }),
    [counts],
  )

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    // clicks on the card keep the input's focus and caret
    e.preventDefault()
    if (windowed || (e.target as HTMLElement).closest('button')) return
    dragStart.current = { x: e.clientX, y: e.clientY, from: offset }
    e.currentTarget.setPointerCapture(e.pointerId)
  }
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const d = dragStart.current
    if (d) setOffset({ x: d.from.x + e.clientX - d.x, y: d.from.y + e.clientY - d.y })
  }

  return (
    <div
      className={`periodic${windowed ? ' windowed' : ''}`}
      role="dialog"
      aria-label="Periodic table"
      style={windowed ? undefined : { transform: `translate(calc(-50% + ${offset.x}px), ${offset.y}px)` }}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={() => (dragStart.current = null)}
      onMouseDown={(e) => e.preventDefault()}
    >
      {windowed ? null : (
        <button type="button" className="periodic-close" aria-label="Close" onClick={onClose}>
          ×
        </button>
      )}
      <div className="periodic-grid" onMouseLeave={() => setShown(null)}>
        <div className="periodic-detail" aria-live="polite">
          {shown ? (
            <>
              <span className="periodic-detail-name">{shown.name}</span>
              <span className="periodic-detail-sub">
                {shown.symbol} · {shown.n}
              </span>
            </>
          ) : null}
        </div>
        <div className="periodic-formula" aria-live="polite">
          {formula ? (
            <>
              <button type="button" className="periodic-formula-main" title="Put the molar mass in the bar" onClick={() => onPick(formula.mass)}>
                <span className="periodic-formula-name">{formula.formula}</span>
                <span className="periodic-formula-mass">{formula.mass} g/mol</span>
              </button>
              <span className="periodic-formula-parts">
                {formula.parts.map((p) => (
                  <button key={p.symbol} type="button" title={`Take one ${p.symbol} away`} onClick={() => setPicks(dropped(p.symbol))}>
                    {p.text} {p.mass}
                  </button>
                ))}
                <button type="button" className="periodic-formula-clear" aria-label="Clear the formula" onClick={() => setPicks([])}>
                  ×
                </button>
              </span>
            </>
          ) : (
            <span className="periodic-formula-hint">click elements to build a formula</span>
          )}
        </div>
        {tiles}
      </div>
    </div>
  )
}
