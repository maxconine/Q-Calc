import { isImproperUnitConversion } from '../engine/units'

const PLAIN_NUMBER = /^[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$/

function stripGroupingCommas(s: string): string {
  return s.replace(/,(?=\d{3}(?:\D|$))/g, '')
}

function insertableNumber(n: number): string {
  if (Math.abs(n) >= 1e12 || (n !== 0 && Math.abs(n) < 1e-6)) return n.toExponential()
  if (Number.isInteger(n) && Math.abs(n) < 1e12) return String(n)
  const s = String(n)
  if (s.includes('e') || s.includes('E')) return s
  return s
}

export function insertableAnswer(display: string, n?: number): string {
  const shown = stripGroupingCommas(display).trim()
  if (isImproperUnitConversion(shown)) return ''
  if (shown && !PLAIN_NUMBER.test(shown)) return shown
  if (n != null && Number.isFinite(n)) return insertableNumber(n)
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

export type HistoryAnswer = {
  display: string
  exact?: string
  n?: number
}

/** True when both a closed form and a distinct decimal can be shown. */
export function hasDualAnswer(row: { display: string; exact?: string }): boolean {
  const display = row.display.trim()
  const exact = row.exact?.trim()
  if (!display || !exact || exact === display) return false
  return !isImproperUnitConversion(display) && !isImproperUnitConversion(exact)
}

/** The value shown for a result: exact form by default, decimal when approx is on. */
export function visibleAnswer(row: { display: string; exact?: string }, form: AnswerForm): string {
  if (form === 'exact' && row.exact) return row.exact
  return row.display
}

/** Previous-answer insert: exact form when that mode is on and one exists, otherwise the approximation. */
export function insertableHistoryAnswer(row: HistoryAnswer, form: AnswerForm): string {
  if (form === 'exact' && row.exact) return insertableAnswer(row.exact)
  return insertableAnswer(row.display, row.n)
}

/** What Enter on a highlighted history row inserts. Clicking the expression always uses `expr`; clicking the answer always uses the answer. */
export function insertableHistoryReuse(
  row: HistoryAnswer & { expr: string; kind?: string },
  form: AnswerForm,
  insert: HistoryInsert,
): string {
  if (row.kind === 'definition' || insert === 'expr') return row.expr
  return insertableHistoryAnswer(row, form)
}
