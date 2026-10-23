import { GAS_CONSTANT_UNITS } from '../engine/chem'
import { SCIENTIFIC_NAMES } from '../engine/scientific'
import { isUnitName } from '../engine/units'
import { IDENTITY_PARTS } from './identities'

// ranked: the first name a prefix reaches is the one offered
export const COMPLETION_FUNCTIONS = [
  'sqrt', 'sin', 'cos', 'tan', 'log', 'abs', 'exp', 'floor', 'ceil', 'round', 'factorial',
  'asin', 'acos', 'atan', 'sinh', 'cosh', 'tanh', 'arcsin', 'arccos', 'arctan',
  'gcd', 'gcf', 'lcf', 'lcm', 'mean', 'median', 'sum', 'max', 'min', 'nthRoot', 'hypot', 'combinations', 'permutations',
]

export const COMPLETION_UNITS = [
  'kilogram', 'kilometer', 'meter', 'centimeter', 'millimeter', 'mile', 'minute', 'milliliter', 'milligram',
  'gram', 'pound', 'ounce', 'inch', 'feet', 'foot', 'yard', 'liter', 'gallon', 'quart', 'pint', 'cup',
  'tablespoon', 'teaspoon', 'second', 'hour', 'day', 'week', 'month', 'year',
  'celsius', 'fahrenheit', 'kelvin', 'joule', 'calorie', 'kilocalorie', 'watt', 'kilowatt', 'newton',
  'pascal', 'atmosphere', 'degree', 'radian', 'kilobyte', 'megabyte', 'gigabyte', 'terabyte',
  'acre', 'hectare', 'hertz', 'knot', 'horsepower', 'volt', 'ampere',
]

// these become √ and Σ, which read their argument without parens
const NO_PAREN = new Set(['sqrt', 'sum'])

const NAMES = new Set(`${SCIENTIFIC_NAMES}|e|in|to|as|of|per|and|or|mod`.toLowerCase().split('|'))
// a prefix on its own (`kilo`) still reads as the start of a longer unit
const PREFIX_WORDS = new Set(['kilo', 'mega', 'giga', 'tera', 'milli', 'micro', 'nano', 'centi'])

// contacts: the names `message` finishes, the ones accepted in settings
export type CompletionNames = { variables?: string[]; functions?: string[]; ans?: boolean; contacts?: string[] }

function isWholeWord(word: string, user: string[]): boolean {
  const lower = word.toLowerCase()
  if (NAMES.has(lower) || user.some((n) => n.toLowerCase() === lower)) return true
  return !PREFIX_WORDS.has(lower) && isUnitName(word)
}

/**
 * The rest of the word being typed at the end of `text`, or '' when nothing is a clear pick.
 * After a number or `in`/`to` a unit is likelier than a function; elsewhere functions come first.
 */
export function completionFor(text: string, names: CompletionNames = {}): string {
  // `mes` finishes `message`, its name finishes from the accepted ones, and what's said is never completed
  const typed = text.match(/^\s*([A-Za-z]{3,})$/)
  if (typed && 'message'.startsWith(typed[1]!.toLowerCase())) return 'message'.slice(typed[1]!.length)
  const to = text.match(/^\s*message\s+([a-z0-9_-]+)$/i)
  if (to) {
    const hit = (names.contacts ?? []).find((n) => n.length > to[1]!.length && n.startsWith(to[1]!.toLowerCase()))
    return hit ? hit.slice(to[1]!.length) : ''
  }
  if (/^\s*message\s/i.test(text)) return ''
  // `periodic table` is a command, not a unit: `periodic ta` finishes it, and `table` never becomes `tablespoon`
  const periodic = text.match(/^\s*periodic\s+([A-Za-z]*)$/i)
  if (periodic) return 'table'.startsWith(periodic[1]!.toLowerCase()) ? 'table'.slice(periodic[1]!.length) : ''
  // `chem R a` finishes `atm`: the units the gas constant comes in, not the unit list
  const gas = text.match(/^\s*chem\s+r\s+([A-Za-z]+)$/i)
  if (gas) {
    const hit = GAS_CONSTANT_UNITS.find((u) => u.length > gas[1]!.length && u.startsWith(gas[1]!.toLowerCase()))
    return hit ? hit.slice(gas[1]!.length) : ''
  }
  // `idt in` finishes the part of the sheet to open, `integration`; one that's no part goes on to the usual picks
  const part = text.match(/^\s*(?:identities|identity|idt)\s+([A-Za-z][A-Za-z ]*)$/i)?.[1]?.toLowerCase()
  const hit = part && IDENTITY_PARTS.find((p) => p.length > part.length && p.startsWith(part))
  if (hit) return hit.slice(part.length)
  const m =text.match(/(^|[^A-Za-z])([A-Za-z]{2,})$/)
  if (!m) return ''
  const word = m[2]!
  const before = text.slice(0, text.length - word.length)
  const vars = names.variables ?? []
  const fns = names.functions ?? []
  if (isWholeWord(word, [...vars, ...fns])) return ''
  const unitSpot = /(?:\d|\)|\b(?:in|to))\s*$/i.test(before)
  const lower = word.toLowerCase()
  const pick = (list: string[], tail = '') => {
    const hit = list.find((n) => n.length > word.length && n.toLowerCase().startsWith(lower))
    return hit ? hit.slice(word.length) + (NO_PAREN.has(hit) ? '' : tail) : ''
  }
  const fn = () => pick(fns, '(') || pick(COMPLETION_FUNCTIONS, '(')
  const unit = () => pick(COMPLETION_UNITS)
  return pick(vars) || (unitSpot ? unit() || fn() : fn() || unit())
}
