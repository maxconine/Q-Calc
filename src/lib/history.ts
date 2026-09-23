import { evaluateLine, parseAssignment, parseFunctionDef } from '../engine/evaluate'
import { sanitizeMeas } from '../engine/measure'
import type { Meas, UserFunction } from '../engine/types'

export type HistoryRow = {
  id: string
  expr: string
  display: string
  exact?: string
  n?: number
  /** Sig figs / ± uncertainty of a measured answer, so `x` and `ans` keep them. */
  meas?: Meas
  /** A unit answer as parseable text (`LineResult.quantity`), so `ans` and variables keep the unit. */
  quantity?: string
  kind?: 'definition' | 'function'
  /** Present when kind is `'function'` (survives expr truncation). */
  fnName?: string
  fnParams?: string[]
  fnBody?: string
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
  const trimmed = expr.trim()
  const fn = parseFunctionDef(trimmed)
  if (fn) {
    const prefix = `${fn.name}(${fn.params.join(', ')}) = `
    const restMax = Math.max(8, MAX_HISTORY_EXPR - prefix.length)
    return `${prefix}${ellipsize(fn.body, restMax)}`
  }
  const parsed = parseAssignment(trimmed)
  if (parsed) {
    const prefix = `${parsed.variable} = `
    const restMax = Math.max(8, MAX_HISTORY_EXPR - prefix.length)
    return `${prefix}${ellipsize(parsed.expr, restMax)}`
  }
  return ellipsize(expr, MAX_HISTORY_EXPR)
}

function slimFunctionFields(row: HistoryRow): Pick<HistoryRow, 'fnName' | 'fnParams' | 'fnBody'> {
  if (row.kind !== 'function') return {}
  const name =
    typeof row.fnName === 'string' && row.fnName
      ? row.fnName
      : parseFunctionDef(row.expr.trim())?.name
  if (!name) return {}
  const params = Array.isArray(row.fnParams)
    ? row.fnParams.filter((p): p is string => typeof p === 'string')
    : parseFunctionDef(row.expr.trim())?.params ?? []
  let body = typeof row.fnBody === 'string' ? row.fnBody : parseFunctionDef(row.expr.trim())?.body
  if (body == null) return { fnName: name, fnParams: params }
  if (body.length > MAX_HISTORY_EXPR) body = ellipsize(body, MAX_HISTORY_EXPR)
  return { fnName: name, fnParams: params, fnBody: body }
}

/** Keep a compact copy of a history row. Returns null when nothing useful remains. */
export function slimHistoryRow(row: HistoryRow): HistoryRow | null {
  const expr = slimExpr(row.expr)
  const display = ellipsize(row.display, MAX_HISTORY_DISPLAY)
  let exact = row.exact ? ellipsize(row.exact, MAX_HISTORY_EXACT) : undefined
  if (exact && exact === display) exact = undefined
  if (!expr.trim() && !display.trim()) return null
  const kind =
    row.kind === 'definition' ? 'definition' : row.kind === 'function' ? 'function' : undefined
  const fnFields = slimFunctionFields({ ...row, kind, expr })
  return {
    id: ellipsize(row.id, 48) || 'row',
    expr,
    display,
    exact,
    n: kind === 'function' || row.n == null || !Number.isFinite(row.n) ? undefined : row.n,
    meas: kind ? undefined : sanitizeMeas(row.meas),
    // Never truncated: a cut-off quantity would parse as a different one.
    quantity: kind || typeof row.quantity !== 'string' ? undefined : row.quantity.length <= MAX_HISTORY_EXPR ? row.quantity : '',
    kind,
    ...fnFields,
  }
}

export function normalizeHistoryRow(
  row: Partial<HistoryRow> & { latex?: string },
  fallbackId: string,
): HistoryRow | null {
  const kind =
    row.kind === 'definition' ? 'definition' : row.kind === 'function' ? 'function' : undefined
  return slimHistoryRow({
    id: typeof row.id === 'string' && row.id ? row.id : fallbackId,
    expr: typeof row.expr === 'string' ? row.expr : typeof row.latex === 'string' ? row.latex : '',
    display: typeof row.display === 'string' ? row.display : '',
    exact: typeof row.exact === 'string' ? row.exact : undefined,
    n: typeof row.n === 'number' ? row.n : undefined,
    meas: row.meas,
    quantity: row.quantity,
    kind,
    fnName: typeof row.fnName === 'string' ? row.fnName : undefined,
    fnParams: Array.isArray(row.fnParams)
      ? row.fnParams.filter((p): p is string => typeof p === 'string')
      : undefined,
    fnBody: typeof row.fnBody === 'string' ? row.fnBody : undefined,
  })
}

/** Variable or function a row defines (`x = 5`, `f(x) = …`), keyed so the two namespaces stay apart. */
function definedName(row: HistoryRow): string | undefined {
  if (row.kind === 'definition') return undefined
  const expr = row.expr.trim()
  const fn = row.kind === 'function' ? (row.fnName ?? parseFunctionDef(expr)?.name) : parseFunctionDef(expr)?.name
  if (fn) return `fn:${fn}`
  const variable = row.kind === 'function' ? undefined : parseAssignment(expr)?.variable
  return variable ? `var:${variable}` : undefined
}

/**
 * The newest MAX_HISTORY rows, plus the newest older row for each variable or function
 * they do not redefine, so `x = 5` still works after ten more calculations.
 */
export function withStickyDefinitions(rows: HistoryRow[]): HistoryRow[] {
  const recent = rows.slice(-MAX_HISTORY)
  const seen = new Set(recent.map(definedName).filter(Boolean))
  const kept: HistoryRow[] = []
  for (let i = rows.length - recent.length - 1; i >= 0; i--) {
    const name = definedName(rows[i]!)
    if (!name || seen.has(name)) continue
    seen.add(name)
    kept.unshift(rows[i]!)
  }
  return [...kept, ...recent]
}

export function persistableHistory(rows: HistoryRow[]): HistoryRow[] {
  const slimmed: HistoryRow[] = []
  for (const row of withStickyDefinitions(rows)) {
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
    if (row.kind === 'definition' || row.kind === 'function') continue
    const parsed = parseAssignment(row.expr.trim())
    if (!parsed) continue
    if (row.quantity != null) {
      delete vars[parsed.variable]
      continue
    }
    if (row.n != null && Number.isFinite(row.n)) {
      vars[parsed.variable] = row.n
      continue
    }
    // Legacy rows may lack `n` — recover from display or by re-evaluating the assignment.
    const fromDisplay = Number(String(row.display).trim())
    if (String(row.display).trim() !== '' && Number.isFinite(fromDisplay)) {
      vars[parsed.variable] = fromDisplay
      continue
    }
    try {
      const result = evaluateLine(row.expr, { variables: { ...vars } })
      const n = result.value?.kind === 'number' ? result.value.n : undefined
      if (n != null && Number.isFinite(n)) vars[parsed.variable] = n
    } catch {
      // skip unrecoverable legacy rows
    }
  }
  return vars
}

/** Measurement metadata of history variables (newest wins) plus `ans` for the last answer. */
export function historyMeasures(rows: HistoryRow[]): Record<string, Meas> {
  const out: Record<string, Meas> = {}
  let last: HistoryRow | undefined
  for (const row of rows) {
    if (row.kind === 'definition' || row.kind === 'function') continue
    if (row.n != null && Number.isFinite(row.n)) last = row
    const parsed = parseAssignment(row.expr.trim())
    if (!parsed) continue
    if (row.meas && row.quantity == null) out[parsed.variable] = row.meas
    else delete out[parsed.variable]
  }
  if (last?.meas && last.quantity == null) out.ans = last.meas
  return out
}

/** Unit-valued variables from history (newest wins), plus `ans` when the last answer had a unit. */
export function historyQuantities(rows: HistoryRow[]): Record<string, string> {
  const out: Record<string, string> = {}
  let last: HistoryRow | undefined
  for (const row of rows) {
    if (row.kind === 'definition' || row.kind === 'function') continue
    if (row.n != null && Number.isFinite(row.n)) last = row
    const parsed = parseAssignment(row.expr.trim())
    if (!parsed) continue
    if (row.quantity != null) out[parsed.variable] = row.quantity
    else delete out[parsed.variable]
  }
  if (last?.quantity != null) out.ans = last.quantity
  return out
}

/** User function definitions from history, newest wins. */
export function historyFunctions(rows: HistoryRow[]): Record<string, UserFunction> {
  const fns: Record<string, UserFunction> = {}
  for (const row of rows) {
    if (row.kind === 'definition') continue
    if (row.kind === 'function' && row.fnName && row.fnBody != null) {
      fns[row.fnName] = {
        params: Array.isArray(row.fnParams) ? row.fnParams : [],
        body: row.fnBody,
      }
      continue
    }
    const parsed = parseFunctionDef(row.expr.trim())
    if (parsed) fns[parsed.name] = { params: parsed.params, body: parsed.body }
  }
  return fns
}

export function lastHistoryNumber(rows: HistoryRow[]): number | undefined {
  for (let i = rows.length - 1; i >= 0; i--) {
    const row = rows[i]
    if (!row || row.kind === 'definition' || row.kind === 'function') continue
    const n = row.n
    // A unit answer is no plain number: `ans` comes from historyQuantities instead.
    if (n != null && Number.isFinite(n)) return row.quantity == null ? n : undefined
  }
  return undefined
}
