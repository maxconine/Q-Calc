import type { EvaluateOptions, LineResult, Meas, SheetInputLine, UserFunction, Value } from './types'
import { DEFAULT_SIG_FIGS, formatValue, num } from './format'
import { formatMeasured, hasPlusMinus, measure } from './measure'
import { tryPlainMath } from './plainMath'
import { formatAsFraction, SCIENTIFIC_NAMES } from './scientific'
import { exactForm, wantsExactForm } from './simplify'

const RESERVED = new Set(`${SCIENTIFIC_NAMES}|e`.split('|'))

function isIdent(name: string): boolean {
  return /^[A-Za-z][A-Za-z0-9]*$/.test(name)
}

/** `x = 2+3` → name `x` and rhs `2+3`. Built-in names like `pi` are not assignments. */
export function parseAssignment(trimmed: string): { variable: string; expr: string } | null {
  const assign = trimmed.match(/^([A-Za-z][A-Za-z0-9]*)\s*=\s*(.+)$/)
  if (!assign) return null
  const variable = assign[1]!
  if (RESERVED.has(variable.toLowerCase())) return null
  return { variable, expr: assign[2]!.trim() }
}

/** `f(x) = x^2` / `g(a, b) = a+b`. Parsed before scalar assignment. */
export function parseFunctionDef(
  trimmed: string,
): { name: string; params: string[]; body: string } | null {
  const m = trimmed.match(/^([A-Za-z][A-Za-z0-9]*)\s*\((.*)\)\s*=\s*(.+)$/s)
  if (!m) return null
  const name = m[1]!
  if (RESERVED.has(name.toLowerCase())) return null
  const rawParams = m[2]!.trim()
  const params = rawParams === '' ? [] : rawParams.split(',').map((p) => p.trim())
  if (params.some((p) => !isIdent(p))) return null
  if (params.some((p) => RESERVED.has(p.toLowerCase()))) return null
  const lower = params.map((p) => p.toLowerCase())
  if (new Set(lower).size !== params.length) return null
  const body = m[3]!.trim()
  if (!body) return null
  return { name, params, body }
}

function withUnit(text: string, unit?: string): string {
  return unit ? `${text} ${unit}` : text
}

function show(value: Value, fractionMode: boolean, sigFigs: number): string {
  if (value.kind === 'text' && value.text) return value.text
  if (fractionMode && value.kind === 'number') {
    const f = formatAsFraction(value.n)
    if (f) return withUnit(f, value.unit)
  }
  return formatValue(value, sigFigs)
}

/** Measurement metadata when it matters for this line; the walker's value must agree with the engine's. */
function measured(
  expr: string,
  value: Value,
  ctx: Parameters<typeof measure>[1],
  wanted: boolean,
): Meas | undefined {
  if (!wanted || value.kind !== 'number' || value.unit || !Number.isFinite(value.n)) return undefined
  const m = measure(expr, ctx)
  if (!m || Math.abs(m.v - value.n) > 1e-9 * Math.max(1, Math.abs(value.n))) return undefined
  return m.meas
}

function numeric(value: Value | undefined): number | undefined {
  if (!value || value.kind === 'text') return undefined
  if (!Number.isFinite(value.n)) return undefined
  return value.n
}

export function evaluateSheet(lines: SheetInputLine[] | string[], options: EvaluateOptions = {}): LineResult[] {
  const texts = lines.map((l) => (typeof l === 'string' ? l : l.text))
  const angleMode = options.angleMode ?? 'deg'
  const fractionMode = options.fractionMode ?? false
  const sigFigs = options.sigFigs ?? DEFAULT_SIG_FIGS
  const sigFigMode = options.sigFigMode ?? false
  const measures: Record<string, Meas> = { ...options.measures }
  const variables: Record<string, number> = { ...options.variables }
  const functions: Record<string, UserFunction> = { ...options.functions }
  let lastAns = options.ans
  const results: LineResult[] = []

  for (const raw of texts) {
    const trimmed = raw.trim()
    if (!trimmed) {
      results.push({ raw, kind: 'empty', display: '' })
      continue
    }

    const fnDef = parseFunctionDef(trimmed)
    if (fnDef) {
      functions[fnDef.name] = { params: fnDef.params, body: fnDef.body }
      const sig = `${fnDef.name}(${fnDef.params.join(', ')})`
      results.push({
        raw,
        kind: 'function',
        display: `${sig} = ${fnDef.body}`,
        fnName: fnDef.name,
        fnParams: fnDef.params,
        fnBody: fnDef.body,
      })
      continue
    }

    let expr = trimmed
    let variable: string | undefined
    const assign = parseAssignment(trimmed)
    if (assign) {
      variable = assign.variable
      expr = assign.expr
    }

    const ctx = { ans: lastAns, angleMode, variables, functions, measures }
    // `±` is only understood by the measurement walker; its central value is the answer.
    const plusMinus = hasPlusMinus(expr)
    let value: Value | null = null
    try {
      if (plusMinus) {
        const m = measure(expr, ctx)
        value = m ? num(m.v) : null
      } else {
        value = tryPlainMath(expr, { ...ctx, defaultUnits: options.defaultUnits })
      }
    } catch {
      value = null
    }
    if (!value) {
      results.push({
        raw,
        kind: variable ? 'assignment' : 'expression',
        display: '',
        variable,
      })
      continue
    }

    const wantMeas = sigFigMode || plusMinus || Boolean(variable) || Object.values(measures).some((m) => m.unc)
    let meas: Meas | undefined
    try {
      meas = measured(expr, value, ctx, wantMeas)
    } catch {
      meas = undefined
    }
    const n = numeric(value)
    if (n !== undefined) {
      lastAns = n
      if (meas) measures.ans = meas
      else delete measures.ans
    }
    if (variable && n !== undefined) {
      variables[variable] = n
      if (meas) measures[variable] = meas
      else delete measures[variable]
    }

    let display = ''
    try {
      display = (meas && formatMeasured(value.n, meas, sigFigMode)) || show(value, fractionMode, sigFigs)
    } catch {
      display = ''
    }
    const measuredDisplay = Boolean(meas && (meas.unc || (sigFigMode && meas.sig != null)))
    const form =
      !measuredDisplay && value.kind === 'number' && Number.isFinite(value.n) && wantsExactForm(expr)
        ? exactForm(value.n, { rationalize: options.rationalize })
        : null
    const exact = form ? withUnit(form, value.unit) : undefined
    results.push({
      raw,
      kind: variable ? 'assignment' : 'expression',
      value,
      display,
      exact,
      meas,
      variable,
    })
  }

  return results
}

export function evaluateLine(text: string, options: EvaluateOptions = {}): LineResult {
  return evaluateSheet([text], options)[0]!
}
