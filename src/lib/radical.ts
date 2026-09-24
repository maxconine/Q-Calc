import { SCIENTIFIC_NAMES } from '../engine/scientific'

export type RadicalSpan = { sign: number; end: number }

const SIGNS = '√∛'

// what the engine takes as a bare radicand (BARE_ATOM_RE in engine/scientific.ts, read on the raw text):
// a number with an optional π or pi coefficient and powers, or π or τ alone; `√2x` is √2 times x, `√2π` is √(2π) but `√2τ` is √2·τ;
// a coefficient right before a letter (or a symbol that becomes one) stays outside: `√2πx` is √2·π·x
const BARE_RADICAND =
  /^(?:[πτ]|\d+(?:\.\d+)?(?:e[+-]?\d+)?(?:(?:[*×·⋅∙]?(?:π|pi|tau)|[*×·⋅∙]τ)(?![A-Za-z0-9_√∛∞θ]))?)(?:\s*\^\s*(?:[-−]?\d+(?:\.\d+)?(?![\d.])|[πτ]|\([^()πτ%|°]*\)))*/i

const WORDS = [...new Set(`${SCIENTIFIC_NAMES}|theta`.toLowerCase().split('|'))]

// the engine roots a lone letter only (`√xy` is √x·y), so the bar covers just that letter; a word like `√sin` isn't one
function bareLetter(rest: string): boolean {
  if (!/^[A-Za-z](?![0-9_(])/.test(rest)) return false
  const word = rest.match(/^[A-Za-z]+/)![0].toLowerCase()
  return word.length === 1 || !WORDS.some((w) => word.startsWith(w))
}

function closingParen(text: string, open: number): number {
  let depth = 0
  for (let i = open; i < text.length; i++) {
    if (text[i] === '(') depth++
    else if (text[i] === ')' && --depth === 0) return i + 1
  }
  // still being typed: the bar runs to the end
  return text.length
}

function radicandEnd(text: string, after: number): number | null {
  let j = after
  while (text[j] === ' ') j++
  if (text[j] === '(') return closingParen(text, j)
  const letter = bareLetter(text.slice(j))
  if (letter) return j + 1
  const m = BARE_RADICAND.exec(text.slice(j))
  if (!m) return null
  let end = j + m[0].length
  // `√4%` is √0.04: the percent turns into a group before the root is read
  const pct = /^\s*%/.exec(text.slice(end))
  if (pct && /\d$/.test(m[0])) end += pct[0].length
  return end
}

/** Each √ or ∛ with the end of the radicand the engine reads under it; one it can't read gets no span. */
export function radicalSpans(text: string): RadicalSpan[] {
  const out: RadicalSpan[] = []
  for (let i = 0; i < text.length; i++) {
    if (!SIGNS.includes(text[i]!)) continue
    const end = radicandEnd(text, i + 1)
    if (end != null && end > i + 1) out.push({ sign: i, end })
  }
  return out
}

const ANSWER_MATH = /^[\d\s+\-−±∓*/^().,πτei]*$/

/** Display only: an exact answer's `sqrt(3)` reads `√3`, `sqrt(2 + sqrt(3))` reads `√(2 + √3)`. */
export function radicalAnswer(s: string): string {
  if (!/sqrt\(|cbrt\(/.test(s) || !ANSWER_MATH.test(s.replace(/(?:sqrt|cbrt)\(/g, '('))) return s
  let out = ''
  let i = 0
  while (i < s.length) {
    const m = /^(sqrt|cbrt)\(/.exec(s.slice(i))
    if (!m) {
      out += s[i]
      i++
      continue
    }
    const open = i + m[1]!.length
    const close = closingParen(s, open)
    const inner = radicalAnswer(s.slice(open + 1, close - 1))
    const sign = m[1] === 'sqrt' ? '√' : '∛'
    // dropping the parens must not pull what follows under the root: `sqrt(2)pi` stays `√(2)π`
    const bare = /^\d+(?:\.\d+)?$/.test(inner) && !/^[\d.^*×·⋅∙πτ(%A-Za-z]/.test(s.slice(close))
    out += bare ? sign + inner : `${sign}(${inner})`
    i = close
  }
  return out
}

// a one-string answer (`x = a ≈ b`, an example or a held one) in pieces, so each side reads its roots like the live answer
export function answerParts(s: string): string[] {
  return s.split(/(^\S+ = | ≈ )/).filter(Boolean)
}
