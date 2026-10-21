import { DEFAULT_SIG_FIGS, formatNumber, prettyAnswer } from '../engine/format'
import type { Meas, SolveInfo } from '../engine/types'
import { isImproperUnitConversion } from '../engine/units'

const PLAIN_NUMBER = /^[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$/

function stripGroupingCommas(s: string): string {
  return s.replace(/,(?=\d{3}(?:\D|$))/g, '')
}

// inserts the displayed magnitude at sig figs, never the engine's full-precision `n`
export function insertableAnswer(display: string, n?: number, sigFigs = DEFAULT_SIG_FIGS): string {
  const plain = stripGroupingCommas(display).trim()
  if (isImproperUnitConversion(plain)) return ''
  // parenthesized so `ans*2` doesn't bind to the uncertainty alone
  if (plain.includes('±')) return `(${plain})`
  if (plain && !PLAIN_NUMBER.test(plain)) return plain
  if (n != null && Number.isFinite(n)) return formatNumber(n, sigFigs)
  return plain
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

// a number as answers show it: sign, grouping commas, decimals, ×10ⁿ or e notation, a ± part, π and roots in an
// exact form, a fraction. no latin letter but the e of e notation, so a unit can't hide inside it
const SHOWN_NUMBER = String.raw`[+\-−±]?(?:[\d.,√∛π/^⁰¹²³⁴⁵⁶⁷⁸⁹⁻·×]|(?<=\d)e[+\-−]?(?=\d)|\s(?=[±×·]))*\d[\d.⁰¹²³⁴⁵⁶⁷⁸⁹⁻]*(?:\s*±\s*[\d.,]+)?`
// a unit as answers show it: letters, °, µ, Ω and Å, with powers, products and quotients (`kg·m/s²`, `ft·lbf`)
const SHOWN_UNIT = String.raw`[A-Za-z°µμΩÅ][A-Za-z°µμΩÅ²³⁻¹^\d·*/]*`
// a number and its unit standing alone: at the start, after `=`, `(`, `, ` or ` or `, and before the end, `,`, `)` or ` or`
const NUMBER_WITH_UNIT = new RegExp(
  String.raw`(^|=\s*|\(\s*|,\s*|\sor\s+)(${SHOWN_NUMBER})\s+${SHOWN_UNIT}(?=\s*$|\s*,|\s*\)|\s+or\b)`,
  'g',
)

/**
 * An answer copied without its unit: `15 m²` is `15`, `x = 6 m, y = 4 m` is `x = 6, y = 4`. A number that runs
 * into more units (`5 ft 3 in`), money (`$10.00`), a date or a message is copied as shown.
 */
export function unitlessAnswer(text: string): string {
  return text.replace(NUMBER_WITH_UNIT, (_, lead: string, n: string) => `${lead}${n.trim()}`)
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

/** A solved row's roots as each one inserts, when it has more than one; null for any other row. */
export function solveRoots(row: { solve?: SolveInfo }, sigFigs = DEFAULT_SIG_FIGS): string[] | null {
  const s = row.solve
  if (s?.outcome !== 'roots' || s.roots.length < 2) return null
  return s.roots.map((r) => formatNumber(r, sigFigs))
}

/**
 * A solved row with several roots inserts its first one (each root in the tape is its own button, so a click picks
 * another), never the whole list; a message row inserts nothing; null for any other row.
 */
export function solveInsert(row: { solve?: SolveInfo }, sigFigs = DEFAULT_SIG_FIGS): string | null {
  const s = row.solve
  if (!s) return null
  if (s.outcome !== 'roots') return ''
  return solveRoots(row, sigFigs)?.[0] ?? null
}

export function hasDualAnswer(row: { display: string; exact?: string }): boolean {
  const display = row.display.trim()
  const exact = row.exact?.trim()
  if (!display || !exact || exact === display) return false
  return !isImproperUnitConversion(display) && !isImproperUnitConversion(exact)
}

export function visibleAnswer(row: { display: string; exact?: string }, form: AnswerForm): string {
  return form === 'exact' && row.exact ? row.exact : row.display
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

/** The side of the history rows ← and → last chose: ↑ and ↓ keep to it from row to row. */
export type HistorySide = 'expr' | 'answer'

/**
 * Where ↑ or ↓ land on the next row: on the expression side, its expression; on the answer side, the answer the row
 * reads as (or its expression when it has none, though the side stays the answer's for the rows after).
 */
export function historyPickOn(side: HistorySide, picks: { start: number }): number {
  return side === 'expr' ? 0 : picks.start
}

/** The side a pick is on. */
export function historySideOf(pick: number): HistorySide {
  return pick === 0 ? 'expr' : 'answer'
}

/**
 * What ← and → step through on a highlighted history row: the expression (pick 0), then each answer the row shows,
 * as each inserts: a solve's roots one by one, the exact value and then the approximation, or the one answer.
 * `start` is the pick ↑ lands on: the answer the row reads as, or the expression when there is none.
 */
export function historyPicks(
  row: HistoryAnswer & { expr: string; kind?: string },
  form: AnswerForm,
  sigFigs = DEFAULT_SIG_FIGS,
): { answers: string[]; start: number } {
  if (row.kind === 'function' || row.kind === 'system') return { answers: [], start: 0 }
  if (row.kind === 'definition') return { answers: [insertableHistoryReuse(row, form, 'answer', sigFigs)], start: 1 }
  if (row.solve && row.solve.outcome !== 'roots') return { answers: [], start: 0 }
  const roots = solveRoots(row, sigFigs)
  if (roots) return { answers: roots, start: 1 }
  if (hasDualAnswer(row)) {
    return {
      answers: [insertableAnswer(row.exact!), insertableHistoryAnswer(row, 'approx', sigFigs)],
      start: form === 'exact' ? 1 : 2,
    }
  }
  return { answers: [insertableHistoryAnswer(row, form, sigFigs)], start: 1 }
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
