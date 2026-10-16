import { Fragment, type RefObject } from 'react'
import { prettyAnswer } from '../engine/format'
import {
  hasDualAnswer,
  insertableAnswer,
  insertableHistoryAnswer,
  prettyRoots,
  solveInsert,
  solveRoots,
  visibleAnswer,
  type AnswerForm,
} from '../lib/answer'
import { keepFocus } from '../lib/dom'
import type { HistoryRow } from '../lib/history'
import { MathText } from './Bounds'
import { RadicalText } from './Radical'
import type { GameKind } from '../lib/games'

type Props = {
  history: HistoryRow[]
  selected: number | null
  // on the selected row, the part ← → have picked: 0 the expression, then each answer in the order they read
  pick?: number
  answerForm: AnswerForm
  sigFigs: number
  tapeRef: RefObject<HTMLDivElement | null>
  onInsert: (text: string) => void
  onInsertExpr: (index: number) => void
  // the letter before a solve's roots copies them all
  onCopy?: (text: string) => void
  onInsertAnswer: (index: number) => void
  onOpenSystem?: (index: number) => void
  // rows before this stay hidden; the recent view is a tail of the tape
  from?: number
}

// a list of roots or a message is shown as written, not as a single pretty number
function solveList(row: HistoryRow): boolean {
  return Boolean(row.solve && (row.solve.outcome !== 'roots' || row.solve.roots.length > 1))
}

function historyAnswerText(row: HistoryRow, answerForm: AnswerForm): string {
  const shown = visibleAnswer(row, answerForm)
  if (row.kind === 'definition') return shown
  if (solveList(row)) return prettyRoots(shown)
  return prettyAnswer(shown)
}

function solvedFor(row: HistoryRow) {
  return row.solve?.outcome === 'roots' ? (
    <span className="tape-var" aria-hidden>
      {row.solve.variable} =
    </span>
  ) : null
}

// a row with several roots: each is its own button, so a click puts just that one in the bar
function RootPicks({
  row,
  sigFigs,
  onInsert,
  lead = true,
  picked,
  onCopy,
}: {
  row: HistoryRow
  sigFigs?: number
  onInsert: (text: string) => void
  lead?: boolean
  // the root ← → are on, from 0
  picked?: number
  onCopy?: (text: string) => void
}) {
  const roots = solveRoots(row, sigFigs) ?? []
  // the tape shows each root as the row reads; it inserts at full precision
  const shown = row.display.split(', ').filter((part) => part !== '…')
  const labels = shown.length === roots.length ? shown : roots
  return (
    <div className="tape-a tape-roots" role="group" aria-label="Solutions: click one to insert it">
      {lead && row.solve ? (
        <button type="button" className="tape-var tape-all" title="Copy all the solutions" onMouseDown={keepFocus} onClick={() => onCopy?.(row.display)}>
          {row.solve.variable} =
        </button>
      ) : null}
      {roots.map((root, k) => (
        <Fragment key={k}>
          {k ? <span className="tape-root-sep">, </span> : null}
          <button type="button" className={`tape-root${picked === k ? ' picked' : ''}`} title="Insert this solution at the cursor" onMouseDown={keepFocus} onClick={() => onInsert(root)}>
            {prettyRoots(labels[k]!)}
          </button>
        </Fragment>
      ))}
      {row.solve?.more ? <span className="tape-root-sep">, …</span> : null}
    </div>
  )
}

function answerTitle(row: HistoryRow, answerForm: AnswerForm): string {
  if (row.kind === 'definition') return 'Insert word at the cursor'
  if (answerForm === 'exact' && row.exact) return 'Insert exact value at the cursor'
  return 'Insert approximation at the cursor'
}

export function HistoryTape({ history, selected, pick, answerForm, sigFigs, tapeRef, onInsert, onCopy, onInsertExpr, onInsertAnswer, onOpenSystem, from = 0 }: Props) {
  return (
    <div className="tape" ref={tapeRef} aria-label="Calculation history">
      {history.map((row, i) => {
        if (i < from) return null
        // which part of this row ← → have picked, if it's the selected one
        const at = selected === i ? pick : undefined
        const on = (n: number) => (at === n ? ' picked' : '')
        return (
        <div className={`tape-row ${selected === i ? 'selected' : ''}`} data-hist={i} key={row.id}>
          <button
            type="button"
            className={`tape-q${on(0)}`}
            title={row.kind === 'system' ? 'Edit equations' : row.kind === 'definition' ? 'Insert word at the cursor' : 'Insert expression at the cursor'}
            onMouseDown={keepFocus}
            onClick={() => (row.kind === 'system' && onOpenSystem ? onOpenSystem(i) : onInsertExpr(i))}
          >
            <MathText text={row.expr} />
          </button>
          {hasDualAnswer(row) ? (
            <div className="tape-a-dual" role="group" aria-label="History answer">
              {solvedFor(row)}
              <button
                type="button"
                className={`tape-a${solveRoots(row) ? '' : on(1)}`}
                title="Insert exact value at the cursor"
                onMouseDown={keepFocus}
                onClick={() => onInsert(solveInsert(row, sigFigs) ?? insertableAnswer(row.exact!))}
              >
                <RadicalText text={row.solve ? prettyRoots(row.exact!) : prettyAnswer(row.exact!)} answer />
              </button>
              <span className="tape-eq" aria-hidden>
                ≈
              </span>
              {solveRoots(row) ? (
                <RootPicks row={row} sigFigs={sigFigs} onInsert={onInsert} onCopy={onCopy} lead={false} picked={at == null ? undefined : at - 1} />
              ) : (
                <button
                  type="button"
                  className={`tape-a${on(2)}`}
                  title="Insert approximation at the cursor"
                  onMouseDown={keepFocus}
                  onClick={() => onInsert(insertableHistoryAnswer(row, 'approx', sigFigs))}
                >
                  {row.solve ? prettyRoots(row.display) : prettyAnswer(row.display)}
                </button>
              )}
            </div>
          ) : solveRoots(row) ? (
            <RootPicks row={row} sigFigs={sigFigs} onInsert={onInsert} onCopy={onCopy} picked={at == null ? undefined : at - 1} />
          ) : (
            <button
              type="button"
              className={`tape-a ${row.solve && row.solve.outcome !== 'roots' ? 'tape-message' : ''}${on(1)}`}
              title={answerTitle(row, answerForm)}
              onMouseDown={keepFocus}
              onClick={() => {
                if (row.kind === 'definition') onInsertAnswer(i)
                else onInsert(insertableHistoryAnswer(row, answerForm, sigFigs))
              }}
            >
              {solvedFor(row)}
              <RadicalText text={historyAnswerText(row, answerForm)} answer={row.kind !== 'definition'} />
            </button>
          )}
        </div>
        )
      })}
    </div>
  )
}

// `games`: each row opens its game
export function GamesSheet({
  games,
  selected = null,
  onPlay,
}: {
  games: ReadonlyArray<{ kind: GameKind; name: string; blurb: string }>
  // the row ↑ ↓ picked, which ↵ plays
  selected?: number | null
  onPlay: (kind: GameKind) => void
}) {
  return (
    <div className="tape cheats games-sheet" aria-label="Games">
      <div className="cheat-col">
        {games.map((g, i) => (
          <button
            type="button"
            className={i === selected ? 'tape-row cheat-row game-row selected' : 'tape-row cheat-row game-row'}
            aria-selected={i === selected}
            key={g.kind}
            onClick={() => onPlay(g.kind)}
          >
            <span className="cheat-key">{g.name}</span>
            <span className="cheat-label">{g.blurb}</span>
          </button>
        ))}
      </div>
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
