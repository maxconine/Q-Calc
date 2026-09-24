import { useEffect, useLayoutEffect, useRef } from 'react'
import { prettyTokens } from './QuickInput'

type SystemPanelProps = {
  lines: string[]
  onChange: (index: number, text: string) => void
  onEnter: (index: number) => void
  /** Up from the first equation returns to the main search field. */
  onFocusMain: () => void
}

function focusEq(el: HTMLInputElement) {
  el.focus()
  const end = el.value.length
  el.setSelectionRange(end, end)
}

export function SystemPanel({ lines, onChange, onEnter, onFocusMain }: SystemPanelProps) {
  const first = useRef<HTMLInputElement>(null)
  const caret = useRef<{ index: number; at: number } | null>(null)
  const caretRef = (index: number, at: number) => {
    caret.current = { index, at }
  }

  useEffect(() => {
    const el = first.current
    const frame = window.requestAnimationFrame(() => el?.focus())
    return () => window.cancelAnimationFrame(frame)
  }, [lines.length])

  useLayoutEffect(() => {
    const saved = caret.current
    if (!saved) return
    const el = document.querySelectorAll<HTMLInputElement>('.sys-eq')[saved.index]
    el?.setSelectionRange(saved.at, saved.at)
  }, [lines])

  return (
    <div className="sys" role="group" aria-label="System of equations">
      {lines.map((line, i) => (
        <label key={i} className="sys-row">
          <input
            ref={i === 0 ? first : undefined}
            className="sys-eq"
            value={line}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            placeholder={`equation ${i + 1}`}
            aria-label={`Equation ${i + 1}`}
            onChange={(e) => {
              const el = e.currentTarget
              const caret = el.selectionStart ?? el.value.length
              const next = prettyTokens(el.value, caret)
              caretRef(i, caret + (next.length - el.value.length))
              onChange(i, next)
            }}
            onKeyDown={(e) => {
              const plain = !e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey
              if (plain && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
                e.preventDefault()
                e.stopPropagation()
                const rows = e.currentTarget.closest('.sys')?.querySelectorAll<HTMLInputElement>('.sys-eq')
                const dest = rows?.[e.key === 'ArrowDown' ? i + 1 : i - 1]
                if (dest) focusEq(dest)
                else if (e.key === 'ArrowUp') onFocusMain()
                return
              }
              if (e.key !== 'Enter' || !plain) return
              e.preventDefault()
              e.stopPropagation()
              onEnter(i)
            }}
          />
        </label>
      ))}
    </div>
  )
}
