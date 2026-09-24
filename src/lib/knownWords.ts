import { SCIENTIFIC_NAMES } from '../engine/scientific'
import type { Span } from './blankReason'

const NAMES = new Set(
  SCIENTIFIC_NAMES.split('|')
    .filter((n) => !['pi', 'tau', 'inf', 'infinity', 'ans'].includes(n))
    .map((n) => n.toLowerCase()),
)
const LONGEST_FIRST = [...NAMES].sort((a, b) => b.length - a.length)

// the words the engine reads as a function, or ans once there's an answer, so they can light up once they're whole.
// `logx` lights just the log, the same way the engine splits it
export function knownWordSpans(text: string, functions: string[] = [], ans = false): Span[] {
  const extra = new Set(functions.map((f) => f.toLowerCase()))
  if (ans) extra.add('ans')
  const spans: Span[] = []
  for (const m of text.matchAll(/(?<![A-Za-z_])[A-Za-z][A-Za-z0-9]*/g)) {
    const word = m[0].toLowerCase()
    const start = m.index!
    if (NAMES.has(word) || extra.has(word)) {
      spans.push({ start, end: start + word.length })
      continue
    }
    const fn = LONGEST_FIRST.find((n) => word.startsWith(n) && /^(x|y|z|theta)$/.test(word.slice(n.length)))
    if (fn) spans.push({ start, end: start + fn.length })
  }
  return spans
}
