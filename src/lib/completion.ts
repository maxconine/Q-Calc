import { SCIENTIFIC_NAMES } from '../engine/scientific'
import { isUnitName } from '../engine/units'

// ranked: the first name a prefix reaches is the one offered
export const COMPLETION_FUNCTIONS = [
  'sqrt', 'sin', 'cos', 'tan', 'log', 'abs', 'exp', 'floor', 'ceil', 'round', 'factorial',
  'asin', 'acos', 'atan', 'sinh', 'cosh', 'tanh', 'arcsin', 'arccos', 'arctan',
  'gcd', 'lcm', 'mean', 'median', 'sum', 'max', 'min', 'nthRoot', 'hypot', 'combinations', 'permutations',
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

export type CompletionNames = { variables?: string[]; functions?: string[] }

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
  const m = text.match(/(^|[^A-Za-z])([A-Za-z]{2,})$/)
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
