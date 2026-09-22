import type { DefaultUnits } from './units'

export type ValueKind = 'number' | 'text'

export interface Value {
  kind: ValueKind
  n: number
  text?: string
  unit?: string
}

export type LineKind = 'empty' | 'expression' | 'assignment'

export interface LineResult {
  raw: string
  kind: LineKind
  value?: Value
  display: string
  /** Simplified exact form when one exists (e.g. `2sqrt(3)`). */
  exact?: string
  error?: string
  variable?: string
}

export interface SheetInputLine {
  text: string
}

export interface EvaluateOptions {
  angleMode?: 'deg' | 'rad'
  ans?: number
  /** Named values from earlier lines (or stored history) so later expressions can use them. */
  variables?: Record<string, number>
  fractionMode?: boolean
  /** Move square roots out of the denominator. Default true. */
  rationalize?: boolean
  sigFigs?: number
  defaultUnits?: DefaultUnits
}
