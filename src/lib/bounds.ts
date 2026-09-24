export type Slot = { start: number; end: number }

/**
 * A ∫, Σ or Π with its limits typed as `_lo^hi` (or `∫lo..hi`). Slots are the limits' own text;
 * everything else between the sign and `end` (`_`, `^`, `..`, braces) is hidden.
 */
export type Bound = {
  sign: number
  lower: Slot
  // null until the upper limit is started
  upper: Slot | null
  // the first offset after the limits
  end: number
}

export const BOUND_SIGNS = '∫Σ∑Π∏'

const isSpace = (ch: string | undefined) => ch != null && /\s/.test(ch)

// a group's matching close; -1 while it's still open
function closeOf(s: string, open: number, sameKind: boolean): number {
  const pairs: Record<string, string> = { '(': ')', '[': ']', '{': '}' }
  const stack: string[] = []
  for (let i = open; i < s.length; i++) {
    const ch = s[i]!
    if (sameKind) {
      if (ch === s[open]) stack.push(pairs[ch]!)
      else if (ch === pairs[s[open]!] && (stack.pop(), !stack.length)) return i
      continue
    }
    if (pairs[ch]) stack.push(pairs[ch]!)
    else if (ch === ')' || ch === ']' || ch === '}') {
      if (stack.pop() !== ch) return -1
      if (!stack.length) return i
    }
  }
  return -1
}

type Group = { slot: Slot; after: number }

function braced(text: string, at: number, sameKind: boolean): Group {
  const close = closeOf(text, at, sameKind)
  // still being typed: the group runs to the end
  if (close < 0) return { slot: { start: at + 1, end: text.length }, after: text.length }
  return { slot: { start: at + 1, end: close }, after: close + 1 }
}

// the ∫'s upper limit the way calculus.ts scanBound reads it: up to a space, or a `(` that starts the body
function integralUpper(text: string, from: number): Group {
  let at = from
  while (isSpace(text[at])) at++
  if (text[at] === '{') return braced(text, at, true)
  let depth = 0
  let i = at
  for (; i < text.length; i++) {
    const ch = text[i]!
    if (depth === 0 && isSpace(ch)) break
    if (ch === '(') {
      if (depth === 0 && i > at && !/[A-Za-z_]/.test(text[i - 1]!)) break
      depth++
    } else if (ch === ')') {
      if (depth === 0) break
      depth--
    }
  }
  return { slot: { start: at, end: i }, after: i }
}

// Σ and Π limits the way sums.ts grabGroup reads them
function sumGroup(text: string, at: number, stop: RegExp): Group {
  if (text[at] === '{' || text[at] === '(') return braced(text, at, false)
  let i = at
  while (i < text.length && !stop.test(text[i]!)) i++
  return { slot: { start: at, end: i }, after: i }
}

function boundAt(text: string, sign: number): Bound | null {
  const integral = text[sign] === '∫'
  const j = sign + 1
  if (j === text.length) return { sign, lower: { start: j, end: j }, upper: null, end: j }
  if (text[j] === '_') {
    let at = j + 1
    let lower: Group
    if (integral) {
      while (isSpace(text[at])) at++
      if (text[at] === '{') lower = braced(text, at, true)
      else {
        const k = text.indexOf('^', at)
        lower = { slot: { start: at, end: k < 0 ? text.length : k }, after: k < 0 ? text.length : k }
      }
    } else lower = sumGroup(text, at, /[\^\s]/)
    at = lower.after
    if (integral) while (isSpace(text[at])) at++
    if (text[at] !== '^') return { sign, lower: lower.slot, upper: null, end: lower.after }
    const upper = integral ? integralUpper(text, at + 1) : sumGroup(text, at + 1, /\s/)
    return { sign, lower: lower.slot, upper: upper.slot, end: upper.after }
  }
  if (!integral) return null
  // `∫0..1 x^2`: the lower limit runs to the `..` and holds no space
  let i = j
  while (i < text.length && !isSpace(text[i]) && !text.startsWith('..', i)) i++
  if (text[j] === '(' || isSpace(text[j])) return null
  if (!text.startsWith('..', i)) return i === text.length ? { sign, lower: { start: j, end: i }, upper: null, end: i } : null
  const upper = integralUpper(text, i + 2)
  return { sign, lower: { start: j, end: i }, upper: upper.slot, end: upper.after }
}

/** Each sign in `text` whose limits are typed next to it, in order. */
export function boundsIn(text: string): Bound[] {
  const out: Bound[] = []
  for (let i = 0; i < text.length; i++) {
    if (!BOUND_SIGNS.includes(text[i]!)) continue
    const b = boundAt(text, i)
    if (!b) continue
    out.push(b)
    // a sign inside a limit is part of that limit's text
    i = Math.max(i, b.end - 1)
  }
  return out
}

const inSlot = (s: Slot | null, o: number) => s != null && o >= s.start && o <= s.end

/** Offsets hidden among `_`, `^`, `..` and braces, where a caret would type into the wrong place. */
export function isHidden(bounds: Bound[], o: number): boolean {
  return bounds.some((b) => o > b.sign && o < b.end && !inSlot(b.lower, o) && !inSlot(b.upper, o))
}

/** The nearest offset a caret can sit at, moving in `dir` from a hidden one. */
export function snapOffset(bounds: Bound[], o: number, dir: 1 | -1, length: number): number {
  let p = o
  while (p >= 0 && p <= length && isHidden(bounds, p)) p += dir
  if (p < 0 || p > length) {
    p = o
    while (isHidden(bounds, p)) p -= dir
  }
  return p
}

export type Edit = { text: string; caret: number }

const splice = (text: string, from: number, to: number, put: string, caret: number): Edit => ({
  text: text.slice(0, from) + put + text.slice(to),
  caret,
})

// what can start a limit right after the sign; `(` and a space start the other forms
const STARTS_LIMIT = /[^\s(_^{},;)]/

/**
 * After one typed character at `caret`: a sign followed by a character starts its lower limit
 * (`∫0` becomes `∫_0`), and `..` in an `_` lower limit moves to the upper (`∫_0..` becomes `∫_0^`).
 */
export function afterTyping(text: string, caret: number): Edit | null {
  if (caret >= 2 && caret === text.length && BOUND_SIGNS.includes(text[caret - 2]!) && STARTS_LIMIT.test(text[caret - 1]!)) {
    return splice(text, caret - 1, caret - 1, '_', caret + 1)
  }
  if (caret >= 2 && text.startsWith('..', caret - 2)) {
    const b = boundsIn(text).find((x) => text[x.sign + 1] === '_' && !x.upper && caret === x.lower.end)
    if (b && caret - 2 >= b.lower.start) return splice(text, caret - 2, caret, '^', caret - 1)
  }
  return null
}

const slotOf = (bounds: Bound[], o: number) => {
  for (const b of bounds) {
    if (inSlot(b.lower, o)) return { b, which: 'lower' as const }
    if (inSlot(b.upper, o)) return { b, which: 'upper' as const }
  }
  return null
}

/** The first offset of the body after a sign's limits. */
function bodyStart(text: string, b: Bound): number {
  let i = b.end
  while (isSpace(text[i])) i++
  return i
}

const openParens = (s: string) => (s.match(/\(/g)?.length ?? 0) > (s.match(/\)/g)?.length ?? 0)

// a character between the sign and the end of its limits that isn't a limit's own text
const hiddenChar = (bounds: Bound[], c: number) =>
  bounds.some(
    (b) =>
      c > b.sign &&
      c < b.end &&
      !(c >= b.lower.start && c < b.lower.end) &&
      !(b.upper && c >= b.upper.start && c < b.upper.end),
  )

/**
 * A key pressed with the caret at `caret` (no selection), when it moves between the limits:
 * → or tab at the end of a limit goes to the next one, backspace at the start of one steps back.
 * null leaves the key to the input.
 */
export function boundKey(text: string, caret: number, key: string): Edit | null {
  const bounds = boundsIn(text)
  if (!bounds.length) return null
  const at = slotOf(bounds, caret)
  // ↑ or enter in the lower limit goes up to the upper one, ↓ in the upper comes back down
  if ((key === 'ArrowUp' || key === 'Enter') && at?.which === 'lower') {
    const end = at.b.lower.end
    return boundKey(text, end, 'ArrowRight') ?? { text, caret: end }
  }
  if (key === 'ArrowDown' && at?.which === 'upper') return { text, caret: at.b.lower.end }
  if (key === 'ArrowRight' || key === 'Tab' || key === ' ' || key === '^') {
    if (!at) return null
    const { b, which } = at
    const slot = b[which]!
    const empty = slot.start === slot.end
    // a bare sign with nothing after it: a space or `(` still starts the other forms
    const bare = b.end === b.sign + 1
    if (caret !== slot.end || openParens(text.slice(slot.start, slot.end))) return null
    if (which === 'lower') {
      if (key === ' ') return empty && !bare ? { text, caret } : null
      if (b.upper) return { text, caret: b.upper.start }
      if (key === '^') return null
      if (bare) return splice(text, b.end, b.end, '_^', b.end + 2)
      if (text[b.sign + 1] !== '_') return splice(text, b.end, b.end, '..', b.end + 2)
      return splice(text, b.end, b.end, '^', b.end + 1)
    }
    if (key === '^') return null
    if (key === ' ') return empty ? { text, caret } : null
    // an empty upper keeps its braces, or the body would be read as the limit
    if (empty && text[slot.start - 1] !== '{') {
      const rest = text.slice(b.end).replace(/^\s+/, '')
      return { text: text.slice(0, b.end) + '{} ' + rest, caret: b.end + 3 }
    }
    const body = bodyStart(text, b)
    if (body > b.end) return { text, caret: body }
    if (body < text.length) return { text, caret: b.end }
    return splice(text, b.end, b.end, ' ', b.end + 1)
  }
  if (key === 'Backspace') {
    for (const b of bounds) {
      const lowerEmpty = b.lower.start === b.lower.end
      const upperEmpty = !b.upper || b.upper.start === b.upper.end
      const bodyAt = bodyStart(text, b)
      if (caret === b.lower.start && b.end > b.sign + 1) {
        if (lowerEmpty && upperEmpty && bodyAt === text.length) return splice(text, b.sign + 1, text.length, '', b.sign + 1)
        return { text, caret: b.sign }
      }
      if (b.upper && caret === b.upper.start) {
        if (!upperEmpty || bodyAt < text.length) return { text, caret: b.lower.end }
        const from = text[b.lower.end] === '}' ? b.lower.end + 1 : b.lower.end
        return splice(text, from, text.length, '', b.lower.end)
      }
      if (b.upper && caret > b.upper.end && caret === bodyAt) {
        // nothing typed in the body yet: the space goes, otherwise just step back into the limit
        if (caret === text.length) return splice(text, b.end, caret, '', b.upper.end)
        return { text, caret: b.upper.end }
      }
    }
    if (hiddenChar(bounds, caret - 1)) return { text, caret: snapOffset(bounds, caret - 1, -1, text.length) }
    return null
  }
  if (key === 'Delete' && hiddenChar(bounds, caret)) return { text, caret: snapOffset(bounds, caret + 1, 1, text.length) }
  return null
}

const SIGN_WORDS: Record<string, string> = { int: '∫', sum: 'Σ', prod: 'Π' }

/** `int`, `sum` or `prod` still waiting at the end, turned into its sign so → or tab can start the limits. */
export function wordToSign(text: string, caret: number): Edit | null {
  if (caret !== text.length) return null
  const m = /(?<![\\A-Za-z_])(int|sum|prod)$/.exec(text)
  if (!m) return null
  return { text: text.slice(0, m.index) + SIGN_WORDS[m[1]!]!, caret: m.index + 1 }
}
