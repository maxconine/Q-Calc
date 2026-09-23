import type { RefObject } from 'react'
import {
  hasDualAnswer,
  insertableAnswer,
  insertableHistoryAnswer,
  visibleAnswer,
  type AnswerForm,
} from '../lib/answer'
import { keepFocus } from '../lib/dom'
import type { HistoryRow } from '../lib/history'

type Props = {
  history: HistoryRow[]
  selected: number | null
  answerForm: AnswerForm
  sigFigs: number
  tapeRef: RefObject<HTMLDivElement | null>
  onInsert: (text: string) => void
  onInsertExpr: (index: number) => void
  onInsertAnswer: (index: number) => void
}

function answerTitle(row: HistoryRow, answerForm: AnswerForm): string {
  if (row.kind === 'definition') return 'Insert word at the cursor'
  if (answerForm === 'exact' && row.exact) return 'Insert exact value at the cursor'
  return 'Insert approximation at the cursor'
}

export function HistoryTape({ history, selected, answerForm, sigFigs, tapeRef, onInsert, onInsertExpr, onInsertAnswer }: Props) {
  return (
    <div className="tape" ref={tapeRef} aria-label="Calculation history">
      {history.map((row, i) => (
        <div className={`tape-row ${selected === i ? 'selected' : ''}`} data-hist={i} key={row.id}>
          <button
            type="button"
            className="tape-q"
            title={row.kind === 'definition' ? 'Insert word at the cursor' : 'Insert expression at the cursor'}
            onMouseDown={keepFocus}
            onClick={() => onInsertExpr(i)}
          >
            {row.expr}
          </button>
          {hasDualAnswer(row) ? (
            <div className="tape-a-dual" role="group" aria-label="History answer">
              <button
                type="button"
                className="tape-a"
                title="Insert exact value at the cursor"
                onMouseDown={keepFocus}
                onClick={() => onInsert(insertableAnswer(row.exact!))}
              >
                {row.exact}
              </button>
              <span className="tape-eq" aria-hidden>
                ≈
              </span>
              <button
                type="button"
                className="tape-a"
                title="Insert approximation at the cursor"
                onMouseDown={keepFocus}
                onClick={() => onInsert(insertableAnswer(row.display, row.n, sigFigs))}
              >
                {row.display}
              </button>
            </div>
          ) : (
            <button
              type="button"
              className="tape-a"
              title={answerTitle(row, answerForm)}
              onMouseDown={keepFocus}
              onClick={() => {
                if (row.kind === 'definition') onInsertAnswer(i)
                else onInsert(insertableHistoryAnswer(row, answerForm, sigFigs))
              }}
            >
              {visibleAnswer(row, answerForm)}
            </button>
          )}
        </div>
      ))}
    </div>
  )
}

export function CheatSheet({ cheats }: { cheats: Array<[key: string, label: string]> }) {
  return (
    <div
      className="tape cheats"
      aria-label="Keyboard shortcuts"
      style={{ gridTemplateRows: `repeat(${Math.ceil(cheats.length / 2)}, auto)` }}
    >
      {cheats.map(([key, label]) => (
        <div className="tape-row cheat-row" key={key}>
          <kbd className="cheat-key">{key}</kbd>
          <span className="cheat-label">{label}</span>
        </div>
      ))}
    </div>
  )
}
