import { unitInfo } from '../engine/units'
import type { Span } from './blankReason'

export type UnitSpan = Span & { name: string }

// words that are never a unit here, whatever the table says: constants and the conversion words themselves
const NOT_UNITS = new Set(['e', 'i', 'pi', 'tau', 'ans', 'to', 'into', 'as', 'per', 'of', 'and', 'or'])
const CONVERSION_WORDS = new Set(['to', 'in', 'into', 'as'])
// a word may sit right against its number (`5m`), but not inside another word
const WORD = /(?<![A-Za-z_µμΩÅ°])°?[A-Za-zµμΩÅ][A-Za-zµμΩÅ]*/g
const TEMPERATURES = new Set(['celsius', 'fahrenheit', 'kelvin', 'rankine'])

/**
 * The words in a typed line that read as units, with what each one is: the `m` in `5 m`, both halves of `m/s`,
 * the `mi` in `10 km to mi`. A word only counts where a unit goes (after a number, in a unit's product or
 * quotient, after `to`), so a variable `m`, the function `min(…)` or the `in` of `10 km in mi` stays a word.
 */
export function unitSpans(text: string, variables: string[] = [], functions: string[] = []): UnitSpan[] {
  const taken = new Set([...variables, ...functions])
  const spans: UnitSpan[] = []
  for (const m of text.matchAll(WORD)) {
    const word = m[0]
    const start = m.index!
    const end = start + word.length
    if (NOT_UNITS.has(word.toLowerCase()) || taken.has(word)) continue
    // a call, not a unit
    if (/^\s*\(/.test(text.slice(end))) continue
    if (!unitPlace(text, start, spans)) continue
    const info = unitInfo(word)
    if (info) spans.push({ start, end, name: info.name })
  }
  return temperatures(text, spans)
}

// `20 C to F` is celsius to fahrenheit, as the calculator reads it, not coulombs to farads
function temperatures(text: string, spans: UnitSpan[]): UnitSpan[] {
  const word = (u: UnitSpan) => text.slice(u.start, u.end)
  const letters = new Set(spans.map(word).filter((w) => w === 'C' || w === 'F'))
  const hot = spans.some((u) => TEMPERATURES.has(u.name)) || letters.size === 2
  if (!hot) return spans
  return spans.map((u) => (word(u) === 'C' ? { ...u, name: 'celsius' } : word(u) === 'F' ? { ...u, name: 'fahrenheit' } : u))
}

// whether what comes before `start` is where a unit goes
function unitPlace(text: string, start: number, found: UnitSpan[]): boolean {
  const before = text.slice(0, start).trimEnd()
  const last = before.at(-1)
  if (!last) return false
  // `5 m`, `(2 + 3) kg`, `3² m`
  if (/[\d)²³⁰-⁹.]/.test(last)) return true
  // `m/s`, `kg·m/s²`, `50/hr`: an operator right after a number or another unit
  if (/[/*·]/.test(last)) {
    const operand = before.slice(0, -1).trimEnd()
    if (/[\d²³⁰-⁹)]$/.test(operand)) return true
    return found.some((u) => u.end === operand.length || text.slice(u.end, operand.length).match(/^\^-?\d+$/) != null)
  }
  // `10 km to mi`
  const prior = /([A-Za-z]+)$/.exec(before)?.[1]
  return prior != null && CONVERSION_WORDS.has(prior.toLowerCase())
}
