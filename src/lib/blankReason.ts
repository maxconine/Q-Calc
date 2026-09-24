import { SCIENTIFIC_NAMES } from '../engine/scientific'

export type Span = { start: number; end: number }

const KNOWN = new Set(`${SCIENTIFIC_NAMES}|e|i|in|to|as|of|mod|and|or|not|xor|graph|sys`.toLowerCase().split('|'))
const OPERATOR = /[+\-−*×·/÷^=,.]/
const CAN_LEAD = /[+\-−.]/

type Names = { variables?: string[]; functions?: string[] }

/**
 * Where a blank answer stops reading: an unknown word, a unit the conversion can't reach, or a
 * stray operator. `ok` says whether a piece of text evaluates. Null when there's no confident
 * answer; a single operator at the end is unfinished, not wrong, so it's never flagged.
 */
export function blankReason(expr: string, ok: (text: string) => boolean, names: Names = {}): Span | null {
  if (!expr.trim() || ok(expr)) return null
  // `2x+3==11` would solve without its second `=`, so that's what's wrong, not the x
  if (expr.includes('=')) return strayOperator(expr, ok) ?? unknownWord(expr, ok, names) ?? unreachableUnit(expr, ok)
  return unknownWord(expr, ok, names) ?? unreachableUnit(expr, ok) ?? strayOperator(expr, ok)
}

function unknownWord(expr: string, ok: (text: string) => boolean, names: Names): Span | null {
  const user = new Set([...(names.variables ?? []), ...(names.functions ?? [])].map((n) => n.toLowerCase()))
  const words = [...expr.matchAll(/[A-Za-z]+/g)].map((m) => ({ word: m[0], start: m.index }))
  for (let i = 0; i < words.length; i++) {
    const { word, start } = words[i]!
    const lower = word.toLowerCase()
    if (KNOWN.has(lower) || user.has(lower)) continue
    const prev = words[i - 1]?.word
    const next = words[i + 1]?.word
    // units can take two words (sq ft, fl oz, nautical mile)
    if (ok(`1 ${word}`) || (next && ok(`1 ${word} ${next}`)) || (prev && ok(`1 ${prev} ${word}`))) continue
    return { start, end: start + word.length }
  }
  return null
}

function unreachableUnit(expr: string, ok: (text: string) => boolean): Span | null {
  const m = expr.match(/^(.*\S)\s+(?:in|to|as)\s+(\S.*?)\s*$/i)
  if (!m) return null
  const [, left, target] = m
  if (!ok(left!) || !ok(`1 ${target}`)) return null
  const start = expr.lastIndexOf(target!)
  return { start, end: start + target!.length }
}

function strayOperator(expr: string, ok: (text: string) => boolean): Span | null {
  const last = expr.trimEnd().length - 1
  for (let i = 0; i <= last; i++) {
    const ch = expr[i]!
    if (!OPERATOR.test(ch)) continue
    const before = expr.slice(0, i).trimEnd()
    const after = expr.slice(i + 1).trimStart()
    const leading = !before && !CAN_LEAD.test(ch)
    const doubled = Boolean(after) && OPERATOR.test(after[0]!) && !CAN_LEAD.test(after[0]!)
    const trailing = i === last && (ch === '=' || ch === ',')
    if (!leading && !doubled && !trailing) continue
    // the doubled one is the second of the pair
    const at = doubled ? expr.indexOf(after[0]!, i + 1) : i
    if (ok(expr.slice(0, at) + expr.slice(at + 1))) return { start: at, end: at + 1 }
  }
  return null
}
