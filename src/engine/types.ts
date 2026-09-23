import type { DefaultUnits } from './units'

export type ValueKind = 'number' | 'text'

export interface Value {
  kind: ValueKind
  n: number
  text?: string
  unit?: string
  unitId?: string // table id behind `unit`, for SI prefix stepping
}

/** Measurement metadata: significant figures and decimal places (absent = exact), plus a ± uncertainty. */
export interface Meas {
  sig?: number
  dp?: number
  unc?: number
}

export type LineKind = 'empty' | 'expression' | 'assignment' | 'function'

/** User-defined function stored by name for later evaluation. */
export interface UserFunction {
  params: string[]
  body: string
}

export interface LineResult {
  raw: string
  kind: LineKind
  value?: Value
  display: string
  /** Simplified exact form when the expression is trig or a square root (e.g. `2sqrt(3)`). */
  exact?: string
  error?: string
  /** Present when the answer is measured (sig figs from the input) or carries a ± uncertainty. */
  meas?: Meas
  variable?: string
  /** Present when kind is `'function'`. */
  fnName?: string
  fnParams?: string[]
  fnBody?: string
}

export interface SheetInputLine {
  text: string
}

export interface EvaluateOptions {
  angleMode?: 'deg' | 'rad'
  ans?: number
  /** Named values from earlier lines (or stored history) so later expressions can use them. */
  variables?: Record<string, number>
  /** User functions from earlier lines (or stored history). */
  functions?: Record<string, UserFunction>
  fractionMode?: boolean
  /** Move square roots out of the denominator. Default true. */
  rationalize?: boolean
  sigFigs?: number
  /** Round measured answers to the sig figs their inputs carry. */
  sigFigMode?: boolean
  /** Measurement metadata of `variables` (and `ans`) from earlier lines or history. */
  measures?: Record<string, Meas>
  defaultUnits?: DefaultUnits
}
