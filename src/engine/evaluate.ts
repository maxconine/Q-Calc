import type { EvaluateOptions, LineResult, SheetInputLine, Value } from './types'
import { DEFAULT_SIG_FIGS, formatValue } from './format'
import { tryPlainMath } from './plainMath'
import { formatAsFraction, SCIENTIFIC_NAMES } from './scientific'
import { exactForm } from './simplify'

const RESERVED = new Set(`${SCIENTIFIC_NAMES}|e`.split('|'))

/** `x = 2+3` → name `x` and rhs `2+3`. Built-in names like `pi` are not assignments. */
export function parseAssignment(trimmed: string): { variable: string; expr: string } | null {
  const assign = trimmed.match(/^([A-Za-z][A-Za-z0-9]*)\s*=\s*(.+)$/)
  if (!assign) return null
  const variable = assign[1]!
  if (RESERVED.has(variable.toLowerCase())) return null
  return { variable, expr: assign[2]!.trim() }
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
  const variables: Record<string, number> = { ...options.variables }
  let lastAns = options.ans
  const results: LineResult[] = []

  for (const raw of texts) {
    const trimmed = raw.trim()
    if (!trimmed) {
      results.push({ raw, kind: 'empty', display: '' })
      continue
    }

    let expr = trimmed
    let variable: string | undefined
    const assign = parseAssignment(trimmed)
    if (assign) {
      variable = assign.variable
      expr = assign.expr
    }

    let value: Value | null = null
    try {
      value = tryPlainMath(expr, { ans: lastAns, angleMode, variables, defaultUnits: options.defaultUnits })
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

    const n = numeric(value)
    if (n !== undefined) lastAns = n
    if (variable && n !== undefined) variables[variable] = n

    let display = ''
    try {
      display = show(value, fractionMode, sigFigs)
    } catch {
      display = ''
    }
    const form =
      value.kind === 'number' && Number.isFinite(value.n)
        ? exactForm(value.n, { rationalize: options.rationalize })
        : null
    const exact = form ? withUnit(form, value.unit) : undefined
    results.push({
      raw,
      kind: variable ? 'assignment' : 'expression',
      value,
      display,
      exact,
      variable,
    })
  }

  return results
}

export function evaluateLine(text: string, options: EvaluateOptions = {}): LineResult {
  return evaluateSheet([text], options)[0]!
}
