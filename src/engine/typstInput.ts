/**
 * Typst math (`$frac(1, 2)$`, `root(3, 8)`, `integral_0^1 x^2 dif x`) rewritten
 * into the ascii the rest of the engine already evaluates. Ordinary calculator
 * text is returned unchanged.
 */

const MARK =
  /^\$(?:\$[\s\S]+\$|[^$\n]+)\$\s*$|(?:^|[^A-Za-z])(?:frac|binom|root|attach|lr|overline|underline|limits|scripts|upright|bold|italic|bb|cal|frak)\s*\(|\b(?:plus\.minus|minus\.plus|plusminus|arrow\.[a-z]|dot\.(?:c|op|circle)|dots(?:\.[a-z]+)?|eq\.(?:not|triple)|gt\.eq|lt\.eq|times(?:\.circle)?)\b|\bdiv\b|\bdif\b|\bintegral_|\bbase\s*:/

export function looksLikeTypst(text: string): boolean {
  const s = text.trim()
  if (!s || /\\[a-zA-Z]+/.test(s)) return false
  return MARK.test(s)
}

const OPS: Record<string, string> = {
  'plus.minus': '±',
  plusminus: '±',
  pm: '±',
  'minus.plus': '∓',
  times: '*',
  'times.circle': '*',
  cdot: '*',
  dot: '*',
  'dot.c': '*',
  'dot.op': '*',
  'dot.circle': '*',
  ast: '*',
  'ast.op': '*',
  star: '*',
  cross: '*',
  div: '/',
  'arrow.r': '->',
  'arrow.r.long': '->',
  'arrow.r.double': '->',
  'arrow.r.double.long': '->',
  'arrow.r.hook': '->',
  'arrow.l.r': '<->',
  'eq.not': '!=',
  neq: '!=',
  'gt.eq': '>=',
  geq: '>=',
  'lt.eq': '<=',
  leq: '<=',
  approx: '≈',
  dots: '...',
  'dots.h': '...',
  'dots.c': '...',
  'dots.v': '...',
  'dots.down': '...',
  'dots.up': '...',
  ldots: '...',
  cdots: '...',
  tan1: 'arctan',
  prime: "'",
}

const GAP = new Set([
  'thin',
  'med',
  'thick',
  'quad',
  'wide',
  'space',
  'space.quad',
  'space.en',
  'space.thin',
  'space.med',
  'space.thick',
  'space.nobreak',
  'neg',
  'nothing',
  'degree',
  'zws',
])

const UNWRAP = new Set([
  'bold',
  'italic',
  'upright',
  'bb',
  'cal',
  'frak',
  'serif',
  'mono',
  'display',
  'inline',
  'limits',
  'scripts',
  'underline',
])

const OPAQUE = new Set(['mat', 'vec', 'cases'])

export function typstToAscii(text: string): string {
  if (!looksLikeTypst(text)) return text
  let s = text.trim()
  if (s.startsWith('$$') && s.endsWith('$$') && s.length > 4) s = s.slice(2, -2).trim()
  else if (s.startsWith('$') && s.endsWith('$') && s.length > 2) s = s.slice(1, -1).trim()
  return tidy(leibniz(emit(s, 0, s.length)))
}

function tidy(s: string): string {
  return s
    .replace(/\s+/g, ' ')
    .replace(/\s+\(/g, '(')
    .replace(/\(\s+/g, '(')
    .replace(/\s+\)/g, ')')
    .replace(/\s*([_^])\s*/g, '$1')
    .trim()
}

/** `frac(dif, dif x)` is the `d/dx` the derivative parser already reads. */
function leibniz(s: string): string {
  return s.replace(/\(\(\s*d\s*\)\s*\/\s*\(\s*d([A-Za-z])\s*\)\s*\)/g, 'd/d$1').replace(/\(\s*d\s*\)\s*\/\s*\(\s*d([A-Za-z])\s*\)/g, 'd/d$1')
}

function emit(s: string, from: number, to: number): string {
  let i = from
  let out = ''
  let gap = false
  const skip = () => {
    const start = i
    while (i < to && /\s/.test(s[i]!)) i++
    if (i > start) gap = true
  }
  while (i < to) {
    skip()
    if (i >= to) break
    if (s[i] === '#') {
      i++
      continue
    }
    if (s[i] === '\\' && i + 1 < to) {
      i++
      continue
    }
    const piece = readPiece(s, i, to)
    if (!piece) break
    if (piece.text) {
      if (gap && out && /[\w)]$/.test(out) && /^[\w(]/.test(piece.text)) out += ' '
      out += piece.text
      gap = false
    } else gap = true
    i = piece.end
  }
  return out
}

type Piece = { text: string; end: number }

function readPiece(s: string, i: number, to: number): Piece | null {
  const ch = s[i]!
  if (ch === '"' ) {
    const end = s.indexOf('"', i + 1)
    const stop = end < 0 || end >= to ? to : end
    return { text: s.slice(i + 1, stop), end: end < 0 ? to : end + 1 }
  }
  if (ch === '(' || ch === '[' || ch === '{') {
    const close = matchClose(s, i, to)
    if (close < 0) return { text: ch, end: i + 1 }
    const inner = emit(s, i + 1, close)
    return { text: `(${inner})`, end: close + 1 }
  }
  if (ch === ')' || ch === ']' || ch === '}') return null
  if (isDigit(ch) || (ch === '.' && isDigit(s[i + 1] ?? ''))) return readNumber(s, i, to)
  if (isIdentStart(ch)) return readWord(s, i, to)
  return readSymbol(s, i, to)
}

function readWord(s: string, i: number, to: number): Piece {
  let j = i + 1
  while (j < to && isIdentPart(s[j]!)) j++
  while (j < to && s[j] === '.' && isIdentStart(s[j + 1] ?? '')) {
    j += 2
    while (j < to && isIdentPart(s[j]!)) j++
  }
  const word = s.slice(i, j)
  let k = j
  while (k < to && /\s/.test(s[k]!)) k++
  if (s[k] === '(') {
    const close = matchClose(s, k, to)
    if (close > 0) return { text: call(word, s.slice(k + 1, close)), end: close + 1 }
  }
  if (word === 'dif') {
    const letter = singleLetter(s, k, to)
    if (letter) return { text: `d${letter.text}`, end: letter.end }
    return { text: 'd', end: j }
  }
  if ((word === 'integral' || word === 'integrate') && s[k] === '_') return { text: 'int', end: j }
  if (GAP.has(word)) return { text: '', end: j }
  if (OPS[word]) return { text: OPS[word]!, end: j }
  return { text: word, end: j }
}

function singleLetter(s: string, i: number, to: number): { text: string; end: number } | null {
  if (!/^[A-Za-z]$/.test(s[i] ?? '')) return null
  const next = s[i + 1] ?? ''
  if (i + 1 < to && (isIdentPart(next) || next === '.')) return null
  return { text: s[i]!, end: i + 1 }
}

function call(name: string, inner: string): string {
  if (OPAQUE.has(name)) return `${name}(${inner})`
  const args = splitArgs(inner)
  if (name === 'frac' && args.length === 2) return `((${emitArg(args[0]!)})/(${emitArg(args[1]!)}))`
  if (name === 'binom' && args.length === 2) return `combinations(${emitArg(args[0]!)},${emitArg(args[1]!)})`
  if (name === 'root') {
    if (args.length >= 2) return `nthroot(${emitArg(args[1]!)},${emitArg(args[0]!)})`
    return `sqrt(${emitArg(args[0] ?? '')})`
  }
  if (name === 'lr') return unwrapLr(inner)
  if (name === 'attach') return attach(args)
  if (name === 'overline') return overline(args)
  if (UNWRAP.has(name)) return args.map(emitArg).join(', ')
  if (name === 'op' || name === 'text') return quoted(args[0] ?? '')
  const rendered = args.map(emitArgNamed)
  return `${name}(${rendered.join(', ')})`
}

function emitArg(raw: string): string {
  return emit(raw, 0, raw.length).trim()
}

function emitArgNamed(raw: string): string {
  const named = raw.trim().match(/^([A-Za-z][A-Za-z0-9]*)\s*:\s*([\s\S]+)$/)
  if (named?.[1] === 'base') return emitArg(named[2]!)
  return emitArg(raw)
}

function unwrapLr(inner: string): string {
  const t = inner.trim()
  if (t.startsWith('|') && t.endsWith('|') && t.length >= 2) return `abs(${emitArg(t.slice(1, -1))})`
  if ((t.startsWith('(') && t.endsWith(')')) || (t.startsWith('[') && t.endsWith(']')) || (t.startsWith('\\{') && t.endsWith('\\}')) || (t.startsWith('{') && t.endsWith('}'))) {
    const body = t.startsWith('\\{') ? t.slice(2, -2) : t.slice(1, -1)
    return `(${emitArg(body)})`
  }
  return `(${emitArg(t)})`
}

function attach(args: string[]): string {
  const base = emitArg(args[0] ?? '')
  let top = ''
  let bot = ''
  for (const arg of args.slice(1)) {
    const m = arg.trim().match(/^(t|b|tl|tr|bl|br)\s*:\s*([\s\S]+)$/)
    if (!m) continue
    const v = emitArg(m[2]!)
    if (m[1] === 't' || m[1] === 'tr' || m[1] === 'tl') top = v
    if (m[1] === 'b' || m[1] === 'br' || m[1] === 'bl') bot = v
  }
  return `${base}${top ? `^(${top})` : ''}${bot ? `_(${bot})` : ''}`
}

function overline(args: string[]): string {
  const body = emitArg(args[0] ?? '')
  if (/^\d+$/.test(body)) return `\\overline{${body}}`
  return `conj(${body})`
}

function quoted(raw: string): string {
  const m = raw.trim().match(/^"([^"]*)"$/)
  return m ? m[1]! : emitArg(raw)
}

function readNumber(s: string, i: number, to: number): Piece {
  let j = i
  while (j < to && /[\d.]/.test(s[j]!)) j++
  if ((s[j] === 'e' || s[j] === 'E') && j + 1 < to) {
    let k = j + 1
    if (s[k] === '+' || s[k] === '-') k++
    const exp = k
    while (k < to && isDigit(s[k]!)) k++
    if (k > exp) j = k
  }
  return { text: s.slice(i, j), end: j }
}

function readSymbol(s: string, i: number, to: number): Piece {
  const two = s.slice(i, Math.min(to, i + 3))
  if (two.startsWith('...')) return { text: '...', end: i + 3 }
  if (two.startsWith('->')) return { text: '->', end: i + 2 }
  if (two.startsWith('=>')) return { text: '->', end: i + 2 }
  if (two.startsWith('<->')) return { text: '<->', end: i + 3 }
  if (two.startsWith('!=')) return { text: '!=', end: i + 2 }
  if (two.startsWith('<=')) return { text: '<=', end: i + 2 }
  if (two.startsWith('>=')) return { text: '>=', end: i + 2 }
  const ch = s[i]!
  if ('+-*/^=<>!.,%|&'.includes(ch)) return { text: ch, end: i + 1 }
  if (ch === '×' || ch === '·' || ch === '⋅') return { text: '*', end: i + 1 }
  if (ch === '÷') return { text: '/', end: i + 1 }
  if (ch === '|') return { text: '|', end: i + 1 }
  return { text: ch, end: i + 1 }
}

function splitArgs(s: string): string[] {
  const out: string[] = []
  let depth = 0
  let start = 0
  let quote = false
  for (let i = 0; i < s.length; i++) {
    const ch = s[i]!
    if (ch === '"' && s[i - 1] !== '\\') quote = !quote
    else if (quote) continue
    else if ('([{'.includes(ch)) depth++
    else if (')]}'.includes(ch)) depth--
    else if (ch === ',' && depth === 0) {
      out.push(s.slice(start, i))
      start = i + 1
    }
  }
  out.push(s.slice(start))
  return out
}

function matchClose(s: string, open: number, to: number): number {
  const pairs: Record<string, string> = { '(': ')', '[': ']', '{': '}' }
  const want = pairs[s[open]!]
  if (!want) return -1
  let depth = 0
  let quote = false
  for (let i = open; i < to; i++) {
    const ch = s[i]!
    if (ch === '"' && s[i - 1] !== '\\') quote = !quote
    else if (quote) continue
    else if (ch === s[open]) depth++
    else if (ch === want && --depth === 0) return i
  }
  return -1
}

function isIdentStart(ch: string): boolean {
  return /[A-Za-z_]/.test(ch)
}
function isIdentPart(ch: string): boolean {
  return /[A-Za-z0-9]/.test(ch)
}
function isDigit(ch: string): boolean {
  return ch >= '0' && ch <= '9'
}
