import { chemAnswer, gasConstant, molarMassAnswer } from './chem'
import { splitLetters } from './letters'
import { embedIntegrals, evaluateCalculus, type CalculusResult } from './calculus'
import { joinDots } from './dots'
import type { EvaluateOptions, LineResult, Meas, SheetInputLine, UserFunction, Value } from './types'
import { DEFAULT_SIG_FIGS, formatValue, num, textVal } from './format'
import { formatMeasured, hasPlusMinus, measure, type MeasureContext } from './measure'
import { latexToAscii, looksLikeLatex, tryPlainMath, withSpaceThousands } from './plainMath'
import { typstToAscii } from './typstInput'
import { formatAsFraction, SCIENTIFIC_NAMES, splitGluedFunctions } from './scientific'
import { looksLikeMatrix, matrixAnswer } from './matrix'
import { exactForm, wantsExactForm } from './simplify'
import { isolateVariable, parseNamedSolve } from './isolate'
import { formatSolve, isEquation, solveEquation, type Solved } from './solve'
import { parseSystemCall, prepUnits, solveSystemCall } from './system'
import { clearlyUnits, isolateWithUnits, solveWithUnitsOne } from './unitSolve'
import { normalizeSums, sumAnswer } from './sums'
import { isImproperUnitConversion, quantityText, readsAsUnit, tryConvert } from './units'
import { angleExponent, callsWorkedOut } from './angles'

const RESERVED = new Set(`${SCIENTIFIC_NAMES}|e`.split('|'))

/** Capital E is free (Young's modulus, energy); lowercase e stays Euler's number. */
function isReserved(name: string): boolean {
  return name !== 'E' && RESERVED.has(name.toLowerCase())
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

/**
 * `ans ft` when ans is already 5 m reads as ans in feet, not m·ft: a unit of the same kind converts,
 * so `ans ft` is `ans to ft` and `ans ft to in` is `ans to in`. any other unit still multiplies.
 * a temperature converts only unbracketed (`212 °F to K`), since an offset scale can't be multiplied out
 */
function ansInUnit(expr: string, ansQty: string | undefined): string {
  // a plain number has no unit to convert: `ans in mm` is `ans to mm` (no answer), not ans inches times mm
  const plain = ansQty ? null : expr.match(/^ans\s+(?:to|into|in)\s+(?!(?:to|into|in)\b)(\S.*)$/i)
  if (plain) return `ans to ${plain[1]}`
  const m = ansQty ? expr.match(/^ans\s+(?:(?:to|into|in)\s+(.+)|([A-Za-zµμ°Ω].*?)(?:\s+(?:to|into|in)\s+(.+))?)$/i) : null
  if (!m) return expr
  const kind = m[1] ?? m[2]!
  const target = m[1] ?? m[3] ?? kind
  for (const [left, out] of [[`(${ansQty})`, `ans to ${target}`], [ansQty!, `${ansQty} to ${target}`]] as const) {
    if (tryConvert(`${left} to ${kind}`)?.kind === 'number') return out
  }
  return expr
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

type Base = 2 | 8 | 10 | 16
const BASE_WORDS: Record<string, Base> = { hex: 16, hexadecimal: 16, binary: 2, bin: 2, octal: 8, oct: 8, decimal: 10 }
const BASE_PREFIX: Record<Base, string> = { 2: '0b', 8: '0o', 10: '', 16: '0x' }

/** `<expr> in hex`, `<expr> to binary`, `<expr> as octal`, or `hex(<expr>)`, `bin(…)`, `oct(…)`. */
function baseRequest(expr: string): { left: string; base: Base } | null {
  const tail = expr.match(/^(.*\S)\s+(?:in|to|as|into)\s+(hex|hexadecimal|binary|bin|octal|oct|decimal)\s*$/i)
  if (tail) return { left: tail[1]!, base: BASE_WORDS[tail[2]!.toLowerCase()]! }
  const call = expr.match(/^\s*(hex|bin|oct)\s*\((.*)\)\s*$/i)
  if (!call) return null
  // the bracket that opens the call must be the one that closes the line: hex(2)+(3) is not a call around it all
  let depth = 0
  for (const ch of call[2]!) {
    if (ch === '(') depth++
    else if (ch === ')' && --depth < 0) return null
  }
  return depth === 0 ? { left: call[2]!, base: BASE_WORDS[call[1]!.toLowerCase()]! } : null
}

/** A whole number written in the base: 255 → 0xFF, 10 → 0b1010, -8 → -0o10. Null for anything else. */
function inBase(n: number, base: Base): string | null {
  const whole = Math.round(n)
  if (!Number.isSafeInteger(whole) || Math.abs(n - whole) > 1e-9 * Math.max(1, Math.abs(n))) return null
  const digits = Math.abs(whole).toString(base)
  return `${whole < 0 ? '-' : ''}${BASE_PREFIX[base]}${base === 16 ? digits.toUpperCase() : digits}`
}

export function evaluateSheet(lines: SheetInputLine[] | string[], options: EvaluateOptions = {}): LineResult[] {
  return withSpaceThousands(Boolean(options.spaceThousands), () => evaluateLines(lines, options))
}

function evaluateLines(lines: SheetInputLine[] | string[], options: EvaluateOptions): LineResult[] {
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
    // `\dot\theta`, `dot(theta)` and `θ̇` all read as `thetadot` before anything else rewrites them
    // LaTeX pasted inside a line (`d/dt $x\sin\theta$`) loses its $ signs; money (`$10`) has no backslash
    const unwrapped = raw.trim().replace(/\$([^$]*\\[A-Za-z][^$]*)\$/g, '$1')
    const source = stripTrailingEquals(typstToAscii(joinDots(unwrapped)))
    const line = splitGluedFunctions(looksLikeLatex(source) ? latexToAscii(source) : source, known)
    // the input field turns a typed theta into θ, which is also a variable name; solve keeps θ as its unknown
    const trimmedLine = splitGluedFunctions(line.replace(/θ/g, 'theta'), known)
    if (!trimmedLine) {
      results.push({ raw, kind: 'empty', display: '' })
      continue
    }

    const pushSolved = (solved: Solved) => {
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
    }
    // an equation with units (`0.5 = e^(-t/0.384 ms)`): solved in SI, the root back in the unit typed for it
    const pushUnitSolved = (eq: string, variable?: string, near?: string): boolean => {
      const u = solveWithUnitsOne(eq, {
        near,
        names: [...Object.keys(variables), ...Object.keys(quantities), ...Object.keys(functions)],
        ans: lastAns,
        angleMode,
        variables,
        functions,
        rationalize: options.rationalize,
        variable,
        defaults: options.defaultUnits,
        sigFigs,
      })
      if (!u) return false
      if (!u.info) {
        results.push({ raw, kind: 'expression', display: u.display })
        return true
      }
      if (u.value) {
        // `ans` is the quantity, like any unit answer
        delete measures.ans
        const literal = quantityText(u.value)
        if (literal) quantities.ans = literal
        else delete quantities.ans
        lastAns = undefined
      }
      results.push({ raw, kind: 'solve', value: u.value ?? textVal(u.display), display: u.display, solve: u.info })
      return true
    }
    const pushIsolated = (text: string, known?: Record<string, number>): boolean => {
      const iso = isolateVariable(text, { variables: known })
      if (iso) results.push({ raw, kind: 'expression', display: iso.display, value: textVal(iso.display) })
      return Boolean(iso)
    }

    // `solve({eq1, eq2}, x = 1, y = 2)`: a system, solved exactly where it can be, else numerically
    const systemCall = parseSystemCall(line)
    if (systemCall) {
      const answer = solveSystemCall(systemCall.equations, systemCall.guessText, { angleMode, defaults: options.defaultUnits })
      results.push({ raw, kind: 'expression', display: answer?.display ?? '', value: answer ? textVal(answer.display) : undefined })
      continue
    }

    // `isolate x in …` rearranges symbolically; stored values stay letters
    if (/\bisolate\b/i.test(line)) {
      if (!pushIsolated(line)) results.push({ raw, kind: 'expression', display: '' })
      continue
    }

    // `solve x in …` or `… for x` is a number when the other letters are known, else x on its own
    const named = hasPlusMinus(line) ? null : parseNamedSolve(line)
    if (named) {
      const unitEq = withQuantities(named.eq, quantities)
      if (pushUnitSolved(unitEq, named.variable, named.near)) continue
      // with units and letters left over, a formula, its quantities kept whole (`h = v^2/(2*(9.8 m/s^2))`)
      const formula = isolateWithUnits(unitEq, named.variable, variables)
      if (formula) {
        results.push({ raw, kind: 'expression', display: formula, value: textVal(formula) })
        continue
      }
      // clear units that won't rearrange stay blank, rather than read as letters (m and s for m/s)
      const units = prepUnits([unitEq], [named.variable, ...Object.keys(variables)])
      if (units && clearlyUnits(units.texts)) {
        results.push({ raw, kind: 'expression', display: '' })
        continue
      }
      const typedNamed = splitLetters(named.numeric, known)
      if (usesUncertain(typedNamed, measures)) {
        results.push({ raw, kind: 'expression', display: '' })
        continue
      }
      const eq = withQuantities(typedNamed, quantities)
      const solved = solveEquation(eq, { ans: lastAns, angleMode, variables, functions, rationalize: options.rationalize })
      // a formula beats "no solution found", which only means the search came up empty
      if (solved && solved.info.outcome !== 'noneFound') pushSolved(solved)
      // a solve uses what's stored, like the numeric solve would; isolate keeps every letter
      else if (!pushIsolated(named.isolate, variables)) {
        if (solved) pushSolved(solved)
        else results.push({ raw, kind: 'expression', display: '' })
      }
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

    // the gas constant has no mol unit to carry, so `ans` after it is the bare number
    const gas = gasConstant(trimmedLine)
    if (gas) {
      const value = { ...num(gas.n), unit: gas.unit }
      results.push({ raw, kind: 'expression', display: show(value, false, sigFigs), value })
      lastAns = gas.n
      delete measures.ans
      delete quantities.ans
      continue
    }
    const isName = (name: string) => name in variables || name in quantities || name in functions
    const molar = molarMassAnswer(trimmedLine, isName)
    if (molar) {
      lastAns = molar.mass
      delete measures.ans
      delete quantities.ans
      results.push({ raw, kind: 'expression', display: molar.display, value: num(molar.mass) })
      continue
    }
    const chem = chemAnswer(trimmedLine, isName)
    if (chem != null) {
      results.push({ raw, kind: 'expression', display: chem, value: chem ? textVal(chem) : undefined })
      continue
    }
    const typed = splitLetters(line, known)
    const trimmed = splitLetters(trimmedLine, known)
    const assign = parseAssignment(trimmed)
    // `V = 12 V` stores 12 volts; as an equation it could only ever say V = 0
    // `B = A^-1` with a stored matrix A is a matrix, though B is a byte and A an ampere
    const unitSelf =
      assign != null &&
      readsAsUnit(assign.variable) &&
      !looksLikeMatrix(withQuantities(assign.expr, quantities), Object.keys(functions)) &&
      tryConvert(assign.expr)?.unit != null
    if (unitSelf) {
      delete variables[assign.variable]
      delete quantities[assign.variable]
      delete measures[assign.variable]
    }
    const scalar = (text: string) => {
      const v = tryPlainMath(text, { ans: lastAns, angleMode, variables, functions })
      return v?.kind === 'number' && !v.unit ? v.n : null
    }
    const matrix = unitSelf
      ? null
      : matrixAnswer(withQuantities(assign?.expr ?? trimmed, quantities), { variables, sigFigs, fractionMode, functions: Object.keys(functions), scalar })
    if (matrix) {
      const variable = matrix.message ? undefined : assign?.variable
      if (variable) {
        // a matrix is stored as its literal, the way a unit answer is, and read back wherever the name appears
        delete measures[variable]
        setOrDelete(variables, variable, matrix.n)
        setOrDelete(quantities, variable, matrix.literal)
      }
      if (matrix.n != null) {
        lastAns = matrix.n
        delete measures.ans
        delete quantities.ans
      } else if (matrix.literal) {
        // `ans` is the whole matrix, read back like a unit answer
        delete measures.ans
        quantities.ans = matrix.literal
      }
      results.push({
        raw,
        kind: variable ? 'assignment' : 'expression',
        value: matrix.n != null ? num(matrix.n) : textVal(matrix.display),
        display: matrix.display,
        exact: matrix.exact,
        quantity: matrix.literal,
        variable,
      })
      continue
    }
    if (!hasPlusMinus(trimmed) && !unitSelf) {
      const eq = withQuantities(typed, quantities)
      // `5 m = x` is an assignment written backwards, not a solve; the bare name side says so
      const bareSide = eq.split('=').some((side) => /^\s*[A-Za-zθ][A-Za-z0-9_]*\s*$/.test(side))
      if (isEquation(eq) && !bareSide && pushUnitSolved(eq)) continue
      const solved = solveEquation(eq, { ans: lastAns, angleMode, variables, functions, rationalize: options.rationalize })
      // solve can't carry a ±, and a bare root would look exact
      if (solved && usesUncertain(typed, measures)) {
        results.push({ raw, kind: 'expression', display: '' })
        continue
      }
      if (solved) {
        pushSolved(solved)
        continue
      }
    }

    const variable = assign?.variable
    // ∓ is treated as ± until correlation is modelled
    let expr = normalizeSums(withQuantities(ansInUnit(assign?.expr ?? trimmed, quantities.ans), quantities).replace(/∓/g, '±'))

    // `255 in hex`, `0xff + 1 to binary`, `hex(255)`: worked out as usual, then written in that base
    const based = variable ? null : baseRequest(expr)
    if (based) expr = based.left

    const ctx = { ans: lastAns, angleMode, variables, functions, measures, timeVarying: options.timeVarying }
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
      value = calc ? calc.value : m ? num(m.v) : sum ? sum.value : tryPlainMath(expr, { ...ctx, defaultUnits: options.defaultUnits, keepUnits: Boolean(variable) })
      // a ± answer that lost its uncertainty would be a confidently wrong bare number
      if (plusMinus && !m && value?.kind === 'number' && !value.meas?.unc) value = null
    } catch {
      value = null
    }
    // functions among typed units, which the unit reader can't call: `10 N * cos(30)`, `180 deg - asin(0.5)`,
    // `sqrt((3 m)^2 + (4 m)^2)`, `tan^-1(x) to rad`. each call is worked out and stands in as its number or angle
    // (a conversion the unit reader gave up on reads as improper, not blank, so that gets a second go too)
    const improper = value?.kind === 'text' && isImproperUnitConversion(value.text ?? '')
    if ((!value || improper) && !plusMinus) {
      try {
        const plain = { ...ctx, defaultUnits: options.defaultUnits }
        const worked = callsWorkedOut(expr, angleMode, (call) => {
          const v = tryPlainMath(call, plain)
          return v?.kind === 'number' && !v.unit ? v.n : null
        })
        const second = worked ? tryPlainMath(worked, { ...plain, keepUnits: Boolean(variable) }) : null
        // a root that leaves half a unit (sqrt(5 m) as m^0.5) is no answer
        if (second && !(second.kind === 'number' && /\^-?\d*\.\d/.test(second.unit ?? ''))) value = second
      } catch {
        // the first reading stands
      }
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
    // an inverse trig answer is an angle, so it says which: `asin(0.5)` is 30 deg in deg mode, 0.5236 rad in rad mode
    if (!calc && !sum && value.kind === 'number' && !value.unit && Number.isFinite(value.n) && angleExponent(expr) === 1) {
      value = { ...value, unit: angleMode, unitId: angleMode }
    }
    const finite = value.kind === 'number' && Number.isFinite(value.n)
    // '' (a unit the parser can't read back) makes later uses blank rather than unit-less.
    // a complex answer chains as its own text at full precision, the way a unit answer does
    const z = value.complex
    const quantity =
      finite && value.unit ? (quantityText(value, angleMode) ?? '') : z ? `${z.re} ${z.im < 0 ? '-' : '+'} ${Math.abs(z.im)}i` : undefined
    if (z && quantity) {
      setOrDelete(measures, 'ans', undefined)
      setOrDelete(quantities, 'ans', quantity)
      if (variable) {
        setOrDelete(variables, variable, undefined)
        setOrDelete(quantities, variable, quantity)
        setOrDelete(measures, variable, undefined)
      }
    }
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
    if (based) {
      // only a whole number without a unit has a hex or binary form
      const written = value.kind === 'number' && !value.unit ? inBase(value.n, based.base) : null
      if (written == null) {
        results.push({ raw, kind: 'expression', display: '' })
        continue
      }
      display = written
    }
    const measuredDisplay = Boolean(meas && (meas.unc || (sigFigMode && meas.sig != null)))
    const form =
      !based && !calc && !measuredDisplay && finite && !sum && wantsExactForm(expr)
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
      ...(calc?.time ? { time: calc.time } : {}),
    })
  }

  return results
}

export function evaluateLine(text: string, options: EvaluateOptions = {}): LineResult {
  return evaluateSheet([text], options)[0]!
}
