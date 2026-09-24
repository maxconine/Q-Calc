import type { ClosedFormJob } from './closedForm'
import type { DefaultUnits } from './units'

export type ValueKind = 'number' | 'text'

export interface Value {
  kind: ValueKind
  n: number
  text?: string
  unit?: string
  /** Table id behind `unit`, for SI prefix stepping. */
  unitId?: string
  /** Sig figs and ± of a unit answer, in its own unit. */
  meas?: Meas
}

/** Absent sig/dp means exact. */
export interface Meas {
  sig?: number
  dp?: number
  unc?: number
}

export type LineKind = 'empty' | 'expression' | 'assignment' | 'function' | 'solve'

export type SolveOutcome = 'roots' | 'none' | 'contradiction' | 'noneFound' | 'all'

export interface SolveInfo {
  variable: string
  /** Ascending; at most MAX_SHOWN_ROOTS, the ones nearest 0 when there are more. */
  roots: number[]
  outcome: SolveOutcome
  more?: boolean
}

export interface UserFunction {
  params: string[]
  body: string
}

export interface LineResult {
  raw: string
  kind: LineKind
  value?: Value
  display: string
  /** Simplified exact form, only for trig and square-root expressions (e.g. `2sqrt(3)`). */
  exact?: string
  error?: string
  meas?: Meas
  /** A unit answer as text the unit parser reads back (`5 cm`); '' when it can't be read back. */
  quantity?: string
  variable?: string
  fnName?: string
  fnParams?: string[]
  fnBody?: string
  solve?: SolveInfo
  /** Calculus answers the closed-form worker may still upgrade to an exact form. */
  closedForm?: ClosedFormJob
}

export interface SheetInputLine {
  text: string
}

export interface EvaluateOptions {
  angleMode?: 'deg' | 'rad'
  ans?: number
  variables?: Record<string, number>
  functions?: Record<string, UserFunction>
  fractionMode?: boolean
  /** Move square roots out of the denominator. Default true. */
  rationalize?: boolean
  sigFigs?: number
  /** Round measured answers to the sig figs their inputs carry. */
  sigFigMode?: boolean
  /** Measurement metadata for `variables` and `ans`. */
  measures?: Record<string, Meas>
  /** Unit-valued variables and `ans`, as `LineResult.quantity` text. */
  quantities?: Record<string, string>
  defaultUnits?: DefaultUnits
}
