import type { RefObject } from 'react'
import { prettyAnswer } from '../engine/format'
import {
  hasDualAnswer,
  insertableAnswer,
  insertableHistoryAnswer,
  prettyRoots,
  solveInsert,
  visibleAnswer,
  type AnswerForm,
} from '../lib/answer'
import { alignDecimals } from '../lib/decimalAlign'
import { keepFocus } from '../lib/dom'
import type { HistoryRow } from '../lib/history'
import { RadicalText } from './Radical'

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

// a list of roots or a message doesn't line up on a decimal point
function unaligned(row: HistoryRow): boolean {
  return Boolean(row.solve && (row.solve.outcome !== 'roots' || row.solve.roots.length > 1))
}

function solvedFor(row: HistoryRow) {
  return row.solve?.outcome === 'roots' ? (
    <span className="tape-var" aria-hidden>
      {row.solve.variable} =
    </span>
  ) : null
}

function answerTitle(row: HistoryRow, answerForm: AnswerForm): string {
  if (row.kind === 'definition') return 'Insert word at the cursor'
  if (answerForm === 'exact' && row.exact) return 'Insert exact value at the cursor'
  return 'Insert approximation at the cursor'
}

export function HistoryTape({ history, selected, answerForm, sigFigs, tapeRef, onInsert, onInsertExpr, onInsertAnswer }: Props) {
  const singles = alignDecimals(
    history.map((row) =>
      row.kind === 'definition' || unaligned(row) || hasDualAnswer(row) ? null : prettyAnswer(visibleAnswer(row, answerForm)),
    ),
  )
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
            <RadicalText text={row.expr} />
          </button>
          {hasDualAnswer(row) ? (
            <div className="tape-a-dual" role="group" aria-label="History answer">
              {solvedFor(row)}
              <button
                type="button"
                className="tape-a"
                title="Insert exact value at the cursor"
                onMouseDown={keepFocus}
                onClick={() => onInsert(solveInsert(row, sigFigs) ?? insertableAnswer(row.exact!))}
              >
                <RadicalText text={row.solve ? prettyRoots(row.exact!) : prettyAnswer(row.exact!)} answer />
              </button>
              <span className="tape-eq" aria-hidden>
                ≈
              </span>
              <button
                type="button"
                className="tape-a"
                title="Insert approximation at the cursor"
                onMouseDown={keepFocus}
                onClick={() => onInsert(solveInsert(row, sigFigs) ?? insertableAnswer(row.display, row.n, sigFigs))}
              >
                {row.solve ? prettyRoots(row.display) : prettyAnswer(row.display)}
              </button>
            </div>
          ) : (
            <button
              type="button"
              className={`tape-a ${row.solve && row.solve.outcome !== 'roots' ? 'tape-message' : ''}`}
              title={answerTitle(row, answerForm)}
              onMouseDown={keepFocus}
              onClick={() => {
                if (row.kind === 'definition') onInsertAnswer(i)
                else onInsert(insertableHistoryAnswer(row, answerForm, sigFigs))
              }}
            >
              {solvedFor(row)}
              <RadicalText
                text={singles[i] ?? (row.solve ? prettyRoots(visibleAnswer(row, answerForm)) : visibleAnswer(row, answerForm))}
                answer={row.kind !== 'definition'}
              />
            </button>
          )}
        </div>
      ))}
    </div>
  )
}

export function CheatSheet({ cheats }: { cheats: Array<Array<[key: string, label: string]>> }) {
  return (
    <div className="tape cheats" aria-label="Shortcuts">
      {cheats.map((column, i) => (
        <div className="cheat-col" key={i}>
          {column.map(([key, label]) => (
            <div className="tape-row cheat-row" key={key}>
              <kbd className="cheat-key">{key}</kbd>
              <span className="cheat-label">{label}</span>
            </div>
          ))}
        </div>
      ))}
    </div>
  )
}
