import { useMemo, useRef, useState, type PointerEvent } from 'react'
import { ELEMENTS, gridCell, insertableMass, type Element } from '../lib/elements'
import './PeriodicCard.css'

// the browser build's stand-in for the mac app's periodic table window
export function PeriodicCard({ onPick, onClose }: { onPick: (text: string) => void; onClose: () => void }) {
  const [offset, setOffset] = useState({ x: 0, y: 0 })
  const [shown, setShown] = useState<Element | null>(null)
  const dragStart = useRef<{ x: number; y: number; from: { x: number; y: number } } | null>(null)
  // built once, so a drag, a hover or a keystroke in the overlay doesn't re-render all 118 tiles
  const tiles = useMemo(
    () =>
      ELEMENTS.map((e) => {
        const { row, col } = gridCell(e)
        return (
          <button
            key={e.n}
            type="button"
            className={`periodic-tile periodic-${e.category}`}
            style={{ gridRow: row, gridColumn: col }}
            title={`${e.name} · ${e.symbol} · ${e.n}`}
            onMouseEnter={() => setShown(e)}
            onFocus={() => setShown(e)}
            onBlur={() => setShown(null)}
            onClick={() => onPick(insertableMass(e))}
          >
            <span className="periodic-n">{e.n}</span>
            <span className="periodic-symbol">{e.symbol}</span>
            <span className="periodic-mass">{e.mass}</span>
          </button>
        )
      }),
    [onPick],
  )

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    // clicks on the card keep the input's focus and caret
    e.preventDefault()
    if ((e.target as HTMLElement).closest('button')) return
    dragStart.current = { x: e.clientX, y: e.clientY, from: offset }
    e.currentTarget.setPointerCapture(e.pointerId)
  }
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const d = dragStart.current
    if (d) setOffset({ x: d.from.x + e.clientX - d.x, y: d.from.y + e.clientY - d.y })
  }

  return (
    <div
      className="periodic"
      role="dialog"
      aria-label="Periodic table"
      style={{ transform: `translate(calc(-50% + ${offset.x}px), ${offset.y}px)` }}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={() => (dragStart.current = null)}
      onMouseDown={(e) => e.preventDefault()}
    >
      <button type="button" className="periodic-close" aria-label="Close" onClick={onClose}>
        ×
      </button>
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
        {tiles}
      </div>
    </div>
  )
}
