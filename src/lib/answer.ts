import { DEFAULT_SIG_FIGS, formatNumber, prettyAnswer } from '../engine/format'
import type { Meas, SolveInfo } from '../engine/types'
import { isImproperUnitConversion } from '../engine/units'

const PLAIN_NUMBER = /^[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$/

function stripGroupingCommas(s: string): string {
  return s.replace(/,(?=\d{3}(?:\D|$))/g, '')
}

// inserts the displayed magnitude at sig figs, never the engine's full-precision `n`
export function insertableAnswer(display: string, n?: number, sigFigs = DEFAULT_SIG_FIGS): string {
  const shown = stripGroupingCommas(display).trim()
  if (isImproperUnitConversion(shown)) return ''
  // parenthesized so `ans*2` doesn't bind to the uncertainty alone
  if (shown.includes('±')) return `(${shown})`
  if (shown && !PLAIN_NUMBER.test(shown)) return shown
  if (n != null && Number.isFinite(n)) return formatNumber(n, sigFigs)
  return shown
}

const ATOM = /^(?:(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?|[A-Za-zπτ]+)$/i

function isWrapped(s: string): boolean {
  if (!s.startsWith('(') || !s.endsWith(')')) return false
  let depth = 0
  for (let i = 0; i < s.length; i++) {
    if (s[i] === '(') depth++
    else if (s[i] === ')' && --depth === 0 && i < s.length - 1) return false
  }
  return true
}

// an answer placed among other text keeps its value: `ans^2` with ans -3 is (-3)^2, not -3^2
export function answerAmong(answer: string, alone: boolean): string {
  const a = answer.trim()
  if (alone || !a || ATOM.test(a) || isWrapped(a.replace(/^[A-Za-z]\w*(?=\()/, ''))) return answer
  return `(${a})`
}

// display only: `±1000000` and `-2, 3` get the same grouping and minus as a single answer
export function prettyRoots(s: string): string {
  return s
    .split(', ')
    .map((part) => (part.startsWith('±') ? `±${prettyAnswer(part.slice(1))}` : prettyAnswer(part)))
    .join(', ')
}

export type AnswerForm = 'exact' | 'approx'
export type HistoryInsert = 'expr' | 'answer'

export const HISTORY_INSERT_OPTIONS: Array<{ id: HistoryInsert; label: string }> = [
  { id: 'expr', label: 'Expression' },
  { id: 'answer', label: 'Answer' },
]

export function normalizeHistoryInsert(value: unknown): HistoryInsert {
  return value === 'answer' ? 'answer' : 'expr'
}

type HistoryAnswer = {
  display: string
  exact?: string
  n?: number
  // a measured answer inserts as shown (`5.00`), not re-rounded from `n`
  meas?: Meas
  solve?: SolveInfo
}

/** A solved row with several roots inserts them as a list, a message row nothing; null for any other row. */
export function solveInsert(row: { solve?: SolveInfo }, sigFigs = DEFAULT_SIG_FIGS): string | null {
  const s = row.solve
  if (!s) return null
  if (s.outcome !== 'roots') return ''
  return s.roots.length > 1 ? `[${s.roots.map((r) => formatNumber(r, sigFigs)).join(', ')}]` : null
}

export function hasDualAnswer(row: { display: string; exact?: string }): boolean {
  const display = row.display.trim()
  const exact = row.exact?.trim()
  if (!display || !exact || exact === display) return false
  return !isImproperUnitConversion(display) && !isImproperUnitConversion(exact)
}

export function visibleAnswer(row: { display: string; exact?: string }, form: AnswerForm): string {
  if (form === 'exact' && row.exact) return row.exact
  return row.display
}

export function insertableHistoryAnswer(
  row: HistoryAnswer,
  form: AnswerForm,
  sigFigs = DEFAULT_SIG_FIGS,
): string {
  const solved = solveInsert(row, sigFigs)
  if (solved != null) return solved
  if (form === 'exact' && row.exact) return insertableAnswer(row.exact)
  return insertableAnswer(row.display, row.meas ? undefined : row.n, sigFigs)
}

// what enter on a highlighted history row inserts; clicks always insert the side they hit
export function insertableHistoryReuse(
  row: HistoryAnswer & { expr: string; kind?: string },
  form: AnswerForm,
  insert: HistoryInsert,
  sigFigs = DEFAULT_SIG_FIGS,
): string {
  if (row.kind === 'definition' || insert === 'expr') return row.expr
  return insertableHistoryAnswer(row, form, sigFigs)
}
