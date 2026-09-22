import { parseAssignment } from '../engine/evaluate'

export type HistoryRow = {
  id: string
  expr: string
  display: string
  exact?: string
  n?: number
  kind?: 'definition'
}

export const MAX_HISTORY = 10
/** Stored expression text. Long pastes are truncated so the tape stays cheap to render. */
export const MAX_HISTORY_EXPR = 240
export const MAX_HISTORY_DISPLAY = 96
export const MAX_HISTORY_EXACT = 72
/** Drop oldest slimmed rows if a corrupted store is still oversized. */
export const MAX_HISTORY_JSON = 16_384

export function ellipsize(text: string, max: number): string {
  if (max <= 0) return ''
  if (text.length <= max) return text
  if (max === 1) return '…'
  return `${text.slice(0, max - 1).trimEnd()}…`
}

function slimExpr(expr: string): string {
  if (expr.length <= MAX_HISTORY_EXPR) return expr
  const parsed = parseAssignment(expr.trim())
  if (parsed) {
    const prefix = `${parsed.variable} = `
    const restMax = Math.max(8, MAX_HISTORY_EXPR - prefix.length)
    return `${prefix}${ellipsize(parsed.expr, restMax)}`
  }
  return ellipsize(expr, MAX_HISTORY_EXPR)
}

/** Keep a compact copy of a history row. Returns null when nothing useful remains. */
export function slimHistoryRow(row: HistoryRow): HistoryRow | null {
  const expr = slimExpr(row.expr)
  const display = ellipsize(row.display, MAX_HISTORY_DISPLAY)
  let exact = row.exact ? ellipsize(row.exact, MAX_HISTORY_EXACT) : undefined
  if (exact && exact === display) exact = undefined
  if (!expr.trim() && !display.trim()) return null
  return {
    id: ellipsize(row.id, 48) || 'row',
    expr,
    display,
    exact,
    n: row.n != null && Number.isFinite(row.n) ? row.n : undefined,
    kind: row.kind === 'definition' ? 'definition' : undefined,
  }
}

export function normalizeHistoryRow(
  row: Partial<HistoryRow> & { latex?: string },
  fallbackId: string,
): HistoryRow | null {
  return slimHistoryRow({
    id: typeof row.id === 'string' && row.id ? row.id : fallbackId,
    expr: typeof row.expr === 'string' ? row.expr : typeof row.latex === 'string' ? row.latex : '',
    display: typeof row.display === 'string' ? row.display : '',
    exact: typeof row.exact === 'string' ? row.exact : undefined,
    n: typeof row.n === 'number' ? row.n : undefined,
    kind: row.kind === 'definition' ? 'definition' : undefined,
  })
}

export function persistableHistory(rows: HistoryRow[]): HistoryRow[] {
  const slimmed: HistoryRow[] = []
  for (const row of rows.slice(-MAX_HISTORY)) {
    const next = slimHistoryRow(row)
    if (next) slimmed.push(next)
  }
  let out = slimmed
  while (out.length && JSON.stringify(out).length > MAX_HISTORY_JSON) {
    out = out.slice(1)
  }
  return out
}

/** Numeric assignments from history, newest wins — used instead of re-evaluating old expressions. */
export function historyVariables(rows: HistoryRow[]): Record<string, number> {
  const vars: Record<string, number> = {}
  for (const row of rows) {
    if (row.kind === 'definition') continue
    if (row.n == null || !Number.isFinite(row.n)) continue
    const parsed = parseAssignment(row.expr.trim())
    if (parsed) vars[parsed.variable] = row.n
  }
  return vars
}

export function lastHistoryNumber(rows: HistoryRow[]): number | undefined {
  for (let i = rows.length - 1; i >= 0; i--) {
    const n = rows[i]?.n
    if (n != null && Number.isFinite(n)) return n
  }
  return undefined
}
