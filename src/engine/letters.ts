import { SCIENTIFIC_NAMES } from './scientific'
import { isCalculusInput } from './calculus'
import { readsAsUnit } from './units'

// the words solve, sums, graph and calculus read, which `for y` or `at 3` would lose if split
const KEYWORDS = 'at|for|from|over|solve|prod|product|lim|limit|int|integral|integrate|derivative|the|second|approaches|graph|theta|inf|infinity'
const TAKEN = new Set(`${SCIENTIFIC_NAMES}|e|ans|in|to|as|of|per|and|or|not|xor|mod|${KEYWORDS}`.toLowerCase().split('|'))

/**
 * `xy` reads as `x y` when every letter is a defined variable and `xy` means nothing on its own.
 * A word before `(` stays whole, since that's a function call.
 * `params` are a function's own names, which win over a unit they spell: f(m,s)=ms is m times s.
 */
export function splitLetters(expr: string, known: (name: string) => boolean, params: string[] = []): string {
  // `xy = 5` names xy; only what follows the = is split
  const lhs = expr.match(/^\s*[A-Za-z][A-Za-z0-9]*\s*=(?!=)/)?.[0] ?? ''
  // in calculus `dx` is the differential, never d times x
  const calculus = isCalculusInput(expr)
  return lhs + expr.slice(lhs.length).replace(/(?<![A-Za-z_\\.])[A-Za-z]{2,}(?![A-Za-z0-9_.]|\s*\()/g, (word) => {
    const letters = [...word]
    const own = params.length > 0 && !params.includes(word) && letters.every((c) => params.includes(c) || known(c)) && letters.some((c) => params.includes(c))
    if (own) return letters.join(' ')
    if (known(word) || TAKEN.has(word.toLowerCase()) || readsAsUnit(word)) return word
    if (calculus && /^d[A-Za-z]$/.test(word)) return word
    // spaces, so `2xy^2` and `1/xy` read exactly as `2x y^2` and `1/x y` would
    return letters.every(known) ? letters.join(' ') : word
  })
}
