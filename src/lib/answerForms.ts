import { evaluateLine } from '../engine/evaluate'
import { clampSigFigs, formatNumber, formatScientific, formatValue } from '../engine/format'
import { formatAsFraction } from '../engine/scientific'
import { exactForm } from '../engine/simplify'
import type { Meas, Value } from '../engine/types'
import { unitAlternatives } from '../engine/units'

// `value` is set when the form is a different quantity of units (26.5 lb as 423 oz)
export type AnswerView = { display: string; value?: Value }

export type FormSource = {
  value: Value
  // what the answer slot already shows; forms equal to these are skipped
  display: string
  exact?: string
  meas?: Meas
  sigFigs: number
  sigFigMode?: boolean
  rationalize?: boolean
}

// a closed form has to name this very number, not a neighbor of it (355/113 is not π)
const SAME_NUMBER = 1e-14

function sameNumber(text: string, n: number): boolean {
  const back = evaluateLine(text, { angleMode: 'rad' }).value
  if (back?.kind !== 'number' || back.unit || !Number.isFinite(back.n)) return false
  return Math.abs(back.n - n) <= SAME_NUMBER * Math.max(1, Math.abs(n))
}

function withUnit(text: string, unit: string | undefined): string {
  return unit ? `${text} ${unit}` : text
}

/** The other ways Tab can show an answer, in cycle order; the answer as shown is not included. */
export function answerForms(src: FormSource): AnswerView[] {
  const { value, sigFigs } = src
  if (value.kind !== 'number' || !Number.isFinite(value.n)) return []
  // a measured answer's digits are its precision; re-expressing them would claim more or less
  if (src.meas?.unc || value.meas?.unc || (src.sigFigMode && (src.meas?.sig != null || value.meas?.sig != null))) return []
  const n = value.n
  const unit = value.unit
  const out: AnswerView[] = []
  const add = (display: string | null, v?: Value) => {
    if (display) out.push(v ? { display, value: v } : { display })
  }

  const dual = Boolean(src.exact && src.exact.trim() !== src.display.trim())
  add(withUnit(formatNumber(n, sigFigs), unit))
  // a dual answer can also be taken one side at a time
  if (dual) add(src.exact ?? null)
  const fraction = formatAsFraction(n)
  // 200/9 °C restates a decimal nobody reads as a fraction; so does 123456789/1000
  if (fraction?.includes('/') && !unit?.includes('°') && fraction.replace(/\D/g, '').length <= 8 && sameNumber(fraction, n)) {
    add(withUnit(fraction, unit))
  }
  const exact = exactForm(n, { rationalize: src.rationalize })
  if (exact && /[π√]|pi|sqrt/.test(exact) && !/e[+-]/.test(exact) && exact.replace(/\D/g, '').length <= 6 && sameNumber(exact, n)) add(withUnit(exact, unit))
  const mag = n === 0 ? 0 : Math.floor(Math.log10(Math.abs(n)))
  if (Math.abs(mag) >= 3) add(withUnit(formatScientific(n, clampSigFigs(sigFigs)), unit))
  for (const alt of unitAlternatives(value)) add(formatValue(alt, sigFigs), alt)

  const shown = new Set(dual ? [] : [src.display.trim()])
  const seen = new Set<string>()
  return out.filter((form) => {
    const key = form.display.trim()
    if (shown.has(key) || seen.has(key) || /undefined|∞/.test(key)) return false
    seen.add(key)
    return true
  })
}
