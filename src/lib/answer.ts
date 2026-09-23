import { DEFAULT_SIG_FIGS, formatNumber } from '../engine/format'
import type { Meas } from '../engine/types'
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
