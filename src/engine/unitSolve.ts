import { formatValue, num } from './format'
import { isolateVariable } from './isolate'
import type { ScientificContext } from './scientific'
import { formatSolve, solveEquation } from './solve'
import { inferDims, prepUnits } from './system'
import type { SolveInfo, Value } from './types'
import { siValue, type DefaultUnits } from './units'

export type UnitSolved = {
  /** Null for a message (units that can't match), which isn't a solve. */
  info: SolveInfo | null
  display: string
  /** One root, with its unit, for `ans` and copying. */
  value?: Value
  message?: boolean
}

type Ctx = ScientificContext & {
  rationalize?: boolean
  variable?: string
  defaults?: DefaultUnits
  sigFigs: number
  /** Stored names, which are never units (`2H` with H = 3 is 6). */
  names?: string[]
}

/**
 * One equation with units, `0.5 = e^(-t/0.384 ms)`: every quantity goes to SI, the plain equation is solved as
 * usual, and the unknown's unit is read off the others (here a time, since an exponent has none). The root is
 * shown in the unit typed for that dimension, so t comes back in ms. Null when no quantity has a unit.
 */
export function solveWithUnitsOne(equation: string, ctx: Ctx): UnitSolved | null {
  const prep = prepUnits([equation], [...(ctx.variable ? [ctx.variable] : []), ...(ctx.names ?? [])])
  if (!prep) return null
  const plain = prep.plain[0]!
  const solved = solveEquation(ctx.variable ? `solve ${plain} for ${ctx.variable}` : plain, ctx)
  if (!solved) return null
  const { info } = solved
  if (info.outcome !== 'roots') return { info, ...formatSolve(solved, { sigFigs: ctx.sigFigs }) }
  const dims = inferDims(prep.marked, prep.quantities, [info.variable])
  if (!dims) return { info: null, display: "units don't match", message: true }
  const dim = dims.get(info.variable)
  if (!dim) return null
  const plainNumber = dim.every((x) => x === 0)
  // the unit typed for this dimension (`ms`), else the default or named one
  const unitId = prep.quantities.find((q) => q.unitId && q.dim.every((x, i) => x === dim[i]))?.unitId
  const values = info.roots.map((root) => (plainNumber ? num(root) : siValue(root, dim, unitId, ctx.defaults)))
  if (values.some((v) => !v || v.kind !== 'number')) return null
  const texts = values.map((v) => formatValue(v!, ctx.sigFigs))
  // `x^2 = 9 m^2` is ±3 m, like a plain ±3
  const pair = values.length === 2 && !info.more && info.roots[0]! < 0 && Math.abs(info.roots[0]! + info.roots[1]!) <= 1e-12 * info.roots[1]!
  const shown = pair ? `±${texts[1]}` : texts.join(', ') + (info.more ? ', …' : '')
  return { info, display: shown, value: values.length === 1 && !info.more ? values[0]! : undefined }
}

const SPARE_LETTERS = 'QWZYKJUVBDGHLNPRSTX'

/**
 * A formula for `variable` when the equation has units and more than one unknown (`v^2 = 2 * 9.8 m/s^2 * h` for h):
 * each quantity stands in as a letter of its own while isolate rearranges, then goes back as typed, so its unit
 * reads as a unit and not as letters m and s. Null without units, or when it can't be rearranged.
 */
export function isolateWithUnits(equation: string, variable: string, variables?: Record<string, number>): string | null {
  const prep = prepUnits([equation], [variable, ...Object.keys(variables ?? {})])
  if (!prep || !clearlyUnits(prep.texts)) return null
  const used = new Set(prep.marked[0]!.match(/[A-Za-z]/g) ?? [])
  const spare = [...SPARE_LETTERS].filter((c) => !used.has(c) && c !== variable)
  if (spare.length < prep.texts.length) return null
  const stand = prep.texts.map((_, i) => spare[i]!)
  const text = prep.marked[0]!.replace(/\(__q(\d+)\)/g, (_, i: string) => stand[Number(i)]!)
  const iso = isolateVariable(`isolate ${variable} in ${text}`, { variables })
  if (!iso) return null
  // a quantity alone keeps no parens (`2 * 9.8 m/s^2`), one inside a product or quotient does
  return iso.display.replace(/[A-Z]/g, (c) => {
    const i = stand.indexOf(c)
    return i < 0 ? c : `(${prep.texts[i]})`
  })
}

/**
 * Whether the quantities are unmistakably units: a unit of two letters or more (`ms`), a compound (`m/s^2`), or a
 * decimal amount (`9.8 m`). `1/2 a` in a formula is half of a, not half an are.
 */
export function clearlyUnits(texts: string[]): boolean {
  return texts.some((t) => {
    const m = /^([\d.]+(?:e[+-]?\d+)?)\s*(.*)$/i.exec(t.trim())
    if (!m) return false
    const unit = m[2]!
    return m[1]!.includes('.') || /[/^·*]/.test(unit) || /[A-Za-zµμΩ°]{2}/.test(unit)
  })
}
