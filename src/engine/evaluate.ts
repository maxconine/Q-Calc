import { chemAnswer } from './chem'
import { splitLetters } from './letters'
import { embedIntegrals, evaluateCalculus, type CalculusResult } from './calculus'
import type { EvaluateOptions, LineResult, Meas, SheetInputLine, UserFunction, Value } from './types'
import { DEFAULT_SIG_FIGS, formatValue, num, textVal } from './format'
import { formatMeasured, hasPlusMinus, measure, type MeasureContext } from './measure'
import { latexToAscii, looksLikeLatex, tryPlainMath } from './plainMath'
import { typstToAscii } from './typstInput'
import { formatAsFraction, SCIENTIFIC_NAMES, splitGluedFunctions } from './scientific'
import { exactForm, wantsExactForm } from './simplify'
import { formatSolve, solveEquation } from './solve'
import { normalizeSums, sumAnswer } from './sums'
import { quantityText, readsAsUnit, tryConvert } from './units'

const RESERVED = new Set(`${SCIENTIFIC_NAMES}|e`.split('|'))

function isReserved(name: string): boolean {
  return RESERVED.has(name.toLowerCase())
}

/** Built-in names like `pi` can't be assigned. */
export function parseAssignment(trimmed: string): { variable: string; expr: string } | null {
  const m = trimmed.match(/^([A-Za-z][A-Za-z0-9]*)\s*=\s*(.+)$/)
  if (!m) return null
  const variable = m[1]!
  if (isReserved(variable)) return null
  return { variable, expr: m[2]!.trim() }
}

/** `2+3=` is `2+3`; only one `=` goes, and never the end of `==`, `<=`, `>=` or `!=`. */
export function stripTrailingEquals(trimmed: string): string {
  return trimmed.match(/^(.*[^=<>!\s])\s*=$/s)?.[1] ?? trimmed
}

export function parseFunctionDef(
  trimmed: string,
): { name: string; params: string[]; body: string } | null {
  const m = trimmed.match(/^([A-Za-z][A-Za-z0-9]*)\s*\((.*)\)\s*=\s*(.+)$/s)
  if (!m) return null
  const name = m[1]!
  if (isReserved(name)) return null
  const paramText = m[2]!.trim()
  const params = paramText === '' ? [] : paramText.split(',').map((p) => p.trim())
  if (params.some((p) => !/^[A-Za-z][A-Za-z0-9]*$/.test(p) || isReserved(p))) return null
  if (new Set(params.map((p) => p.toLowerCase())).size !== params.length) return null
  const body = m[3]!.trim()
  if (!body) return null
  return { name, params, body }
}

function withUnit(text: string, unit?: string): string {
  return unit ? `${text} ${unit}` : text
}

/** `d * 2` with `d = 5 cm` becomes `(5 cm) * 2`, so the unit parser sees the unit. */
function withQuantities(expr: string, quantities: Record<string, string>): string {
  const names = Object.keys(quantities).sort((a, b) => b.length - a.length)
  if (!names.length) return expr
  const re = new RegExp(`(?<![A-Za-z_])(?:${names.join('|')})(?![A-Za-z0-9_])`, 'g')
  return expr.replace(re, (name) => `(${quantities[name]})`)
}

function show(value: Value, fractionMode: boolean, sigFigs: number): string {
  if (value.kind === 'text' && value.text) return value.text
  if (fractionMode && value.kind === 'number') {
    const f = formatAsFraction(value.n)
    if (f) return withUnit(f, value.unit)
  }
  return formatValue(value, sigFigs)
}

/** The walker's value must agree with the engine's, or its metadata belongs to some other reading. */
function measured(
  expr: string,
  value: Value,
  ctx: MeasureContext,
  wanted: boolean,
): Meas | undefined {
  if (!wanted || value.kind !== 'number' || !Number.isFinite(value.n)) return undefined
  if (value.unit || value.meas) return value.meas
  const m = measure(expr, ctx)
  if (!m || Math.abs(m.v - value.n) > 1e-9 * Math.max(1, Math.abs(value.n))) return undefined
  return m.meas
}

/** Whether `expr` names a variable (or ans) that carries a ±. */
function usesUncertain(expr: string, measures: Record<string, Meas>): boolean {
  return Object.entries(measures).some(([name, m]) => m.unc && new RegExp(`(?<![A-Za-z0-9_])${name}(?![A-Za-z0-9_])`).test(expr))
}

function setOrDelete<T>(record: Record<string, T>, key: string, value: T | undefined): void {
  if (value != null) record[key] = value
  else delete record[key]
}

export function evaluateSheet(lines: SheetInputLine[] | string[], options: EvaluateOptions = {}): LineResult[] {
  const texts = lines.map((l) => (typeof l === 'string' ? l : l.text))
  const angleMode = options.angleMode ?? 'deg'
  const fractionMode = options.fractionMode ?? false
  const sigFigs = options.sigFigs ?? DEFAULT_SIG_FIGS
  const sigFigMode = options.sigFigMode ?? false
  const measures: Record<string, Meas> = { ...options.measures }
  const variables: Record<string, number> = { ...options.variables }
  const quantities: Record<string, string> = { ...options.quantities }
  const functions: Record<string, UserFunction> = { ...options.functions }
  let lastAns = options.ans
  const results: LineResult[] = []

  const known = (name: string) => name in variables || name in quantities || name in measures
  for (const raw of texts) {
    const source = stripTrailingEquals(typstToAscii(raw.trim()))
    const line = splitGluedFunctions(looksLikeLatex(source) ? latexToAscii(source) : source, known)
    // the input field turns a typed theta into θ, which is also a variable name; solve keeps θ as its unknown
    const trimmedLine = splitGluedFunctions(line.replace(/θ/g, 'theta'), known)
    if (!trimmedLine) {
      results.push({ raw, kind: 'empty', display: '' })
      continue
    }

    const fnDef = parseFunctionDef(trimmedLine)
    if (fnDef) {
      fnDef.body = splitLetters(fnDef.body, (n) => fnDef.params.includes(n) || known(n), fnDef.params)
      functions[fnDef.name] = { params: fnDef.params, body: fnDef.body }
      results.push({
        raw,
        kind: 'function',
        display: `${fnDef.name}(${fnDef.params.join(', ')}) = ${fnDef.body}`,
        fnName: fnDef.name,
        fnParams: fnDef.params,
        fnBody: fnDef.body,
      })
      continue
    }

    const chem = chemAnswer(trimmedLine, (name) => name in variables || name in quantities || name in functions)
    if (chem != null) {
      results.push({ raw, kind: 'expression', display: chem, value: chem ? textVal(chem) : undefined })
      continue
    }
    const typed = splitLetters(line, known)
    const trimmed = splitLetters(trimmedLine, known)
    const assign = parseAssignment(trimmed)
    // `V = 12 V` stores 12 volts; as an equation it could only ever say V = 0
    const unitSelf = assign != null && readsAsUnit(assign.variable) && tryConvert(assign.expr)?.unit != null
    if (unitSelf) {
      delete variables[assign.variable]
      delete quantities[assign.variable]
      delete measures[assign.variable]
    }
    if (!hasPlusMinus(trimmed) && !unitSelf) {
      const eq = withQuantities(typed, quantities)
      const solved = solveEquation(eq, { ans: lastAns, angleMode, variables, functions, rationalize: options.rationalize })
      // solve can't carry a ±, and a bare root would look exact
      if (solved && usesUncertain(typed, measures)) {
        results.push({ raw, kind: 'expression', display: '' })
        continue
      }
      if (solved) {
        const { display, exact } = formatSolve(solved, { sigFigs, fractionMode })
        const [root] = solved.info.roots
        const single = solved.info.outcome === 'roots' && solved.info.roots.length === 1 && !solved.info.more
        // the root is never stored as the variable; `ans` carries it
        if (single) {
          lastAns = root
          delete measures.ans
          delete quantities.ans
        }
        results.push({ raw, kind: 'solve', value: single ? num(root!) : textVal(display), display, exact, solve: solved.info })
        continue
      }
    }

    const variable = assign?.variable
    // ∓ is treated as ± until correlation is modelled
    let expr = normalizeSums(withQuantities(assign?.expr ?? trimmed, quantities).replace(/∓/g, '±'))

    const ctx = { ans: lastAns, angleMode, variables, functions, measures }
    const plusMinus = hasPlusMinus(expr)
    let value: Value | null = null
    let sum: ReturnType<typeof sumAnswer> = null
    let calc: CalculusResult | null = null
    try {
      calc = plusMinus ? null : evaluateCalculus(expr, ctx)
      // a scalar or other term beside an integral is not a calculus line, so the integral is folded in
      if (!plusMinus && !calc?.value) {
        for (let n = 0; n < 8 && !calc?.value; n++) {
          const next = embedIntegrals(expr, ctx)
          if (next === 'fail') {
            calc = { value: null }
            break
          }
          if (next == null) break
          expr = next
          calc = evaluateCalculus(expr, ctx)
        }
      }
      sum = calc ? null : sumAnswer(expr, { ...ctx, defaultUnits: options.defaultUnits, rationalize: options.rationalize })
      const m = plusMinus ? measure(expr, ctx) : null
      value = calc ? calc.value : m ? num(m.v) : sum ? sum.value : tryPlainMath(expr, { ...ctx, defaultUnits: options.defaultUnits })
      // a ± answer that lost its uncertainty would be a confidently wrong bare number
      if (plusMinus && !m && value?.kind === 'number' && !value.meas?.unc) value = null
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
      meas = calc ? undefined : measured(expr, value, ctx, wantMeas)
    } catch {
      meas = undefined
    }
    // a ± that went in and didn't come out would show a bare number as if exact
    if (!meas?.unc && usesUncertain(assign?.expr ?? trimmed, measures)) {
      results.push({ raw, kind: variable ? 'assignment' : 'expression', display: '', variable })
      continue
    }
    const finite = value.kind === 'number' && Number.isFinite(value.n)
    // '' (a unit the parser can't read back) makes later uses blank rather than unit-less.
    const quantity = finite && value.unit ? (quantityText(value) ?? '') : undefined
    if (finite) {
      const plainMeas = quantity == null ? meas : undefined
      lastAns = value.n
      setOrDelete(measures, 'ans', plainMeas)
      setOrDelete(quantities, 'ans', quantity)
      if (variable) {
        setOrDelete(variables, variable, quantity == null ? value.n : undefined)
        setOrDelete(quantities, variable, quantity)
        setOrDelete(measures, variable, plainMeas)
      }
    }

    let display = ''
    try {
      const measuredText = meas && formatMeasured(value.n, meas, sigFigMode)
      display = measuredText ? withUnit(measuredText, value.unit) : show(value, fractionMode, sigFigs)
    } catch {
      display = ''
    }
    const measuredDisplay = Boolean(meas && (meas.unc || (sigFigMode && meas.sig != null)))
    const form =
      !calc && !measuredDisplay && finite && !sum && wantsExactForm(expr)
        ? exactForm(value.n, { rationalize: options.rationalize })
        : null
    const exact = calc?.exact ?? ((!measuredDisplay && sum?.exact) || (form ? withUnit(form, value.unit) : undefined))
    results.push({
      raw,
      kind: variable ? 'assignment' : 'expression',
      value,
      display,
      exact,
      meas,
      quantity,
      variable,
      closedForm: calc?.job,
    })
  }

  return results
}

export function evaluateLine(text: string, options: EvaluateOptions = {}): LineResult {
  return evaluateSheet([text], options)[0]!
}
