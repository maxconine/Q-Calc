import { latexToAscii } from '../engine/plainMath'
import { boundsIn } from './bounds'

/** Macros available to the typeset preview. */
export const TYPST_PREAMBLE = `#let choose = "choose"
#let multichoose = "multichoose"
#let multibinom(upper, ..lower) = {
  let helper = math.binom(upper, ..lower)
  $(helper)$
}
#let amat(..rows) = {
  math.mat(..rows, augment: -1)
}

#let emat(..rows) = {
  math.mat(..rows, delim: "(")
}

#let dmat(..rows) = {
  math.mat(..rows, delim: "|")
}

#let tan1 = $tan^(-1)$

#let dt = $space d t$
#let dtau = $space d tau$

#let bu = $bold(u)$
#let bv = $bold(v)$
#let bw = $bold(w)$
#let b0 = $bold(0)$
#let bx = $bold(x)$
#let bP = $bold(P)$
#let bF = $bold(F)$
#let bz = $bold(z)$
#let ba = $bold(a)$
#let bc = $bold(c)$

#let lp = $cal(L)$

#let pm = $plus.minus$

#let proj = $text("proj")$
#let span = $op("span")$
#let null = $op("null")$
#let row = $op("row")$
#let col = $op("col")$
#let rank = $op("rank")$
#let range = $op("range")$
#let nullity = $op("nullity")$
#let cov = $op("cov")$

#let sa = $stretch(->)$
#let ssa = $stretch(=>)$
#let se = $stretch(=)$

#let dd = math.dot.double

#let dx = $space d x$

#let ibar(content) = $lr(#content space |)$

#let tand = $space.quad text("and") space.quad$

#let imod(n) = $space (mod #n)$
#let ctimes = $times.o$
#let cplus = $plus.o$
#let cminus = $minus.o$

#let circ = math.op($circle.small$)

#let cC = $cal(C)$
#let cD = $cal(D)$
#let cB = $cal(B)$
#let cP = $cal(P)$

#let ip(x, y) = $lr(chevron.l #x, #y chevron.r)$
#let ict = math.op("ict")
#let cross = $times$
`

const CONTENT = new Set([
  'choose', 'multichoose', 'tan1', 'dt', 'dtau', 'bu', 'bv', 'bw', 'b0', 'bx', 'bP', 'bF', 'bz', 'ba', 'bc',
  'lp', 'pm', 'proj', 'span', 'null', 'row', 'col', 'rank', 'range', 'nullity', 'cov', 'sa', 'ssa', 'se',
  'dd', 'dx', 'tand', 'ctimes', 'cplus', 'cminus', 'circ', 'cC', 'cD', 'cB', 'cP', 'ict', 'cross',
])

const CALLS = new Set(['multibinom', 'amat', 'emat', 'dmat', 'ibar', 'imod', 'ip'])

const INVERSE: Record<string, string> = {
  asin: 'arcsin',
  acos: 'arccos',
  atan: 'tan1',
  arcsin: 'arcsin',
  arccos: 'arccos',
  arctan: 'tan1',
  arccsc: 'arccsc',
  arcsec: 'arcsec',
  arccot: 'arccot',
}

function hexFill(fill: string): string {
  return /^#[0-9a-fA-F]{6}$/.test(fill) ? fill : '#1d1d1f'
}

/** Exact form when the engine has one, otherwise the displayed answer. */
export function typstAnswer(exact: string | undefined, display: string): string {
  const pretty = exact?.trim() ?? ''
  const shown = display.trim()
  if (pretty && shown && pretty !== shown) return `${pretty} ≈ ${shown}`
  return pretty || shown
}

export function typstDocument(expr: string, fill: string, answer = ''): string | null {
  const math = toTypstMath(expr)
  if (!math) return null
  const ans = toTypstMath(answer)
  const body = ans ? `${math} = ${ans}` : math
  return `#import "/macros.typ": *
#set page(width: auto, height: auto, margin: 2pt, fill: none)
#set text(size: 18pt, fill: rgb("${hexFill(fill)}"))
$ ${body} $
`
}

/** Search-bar copy: Typst math when the setting is on, otherwise the typed text. */
export function copiedEquation(text: string, typstCopy: boolean): string {
  if (!typstCopy) return text
  return toTypstMath(text) || text
}

export function toTypstMath(expr: string): string {
  let s = expr.trim()
  if (!s) return ''
  if (/\\[a-zA-Z]+/.test(s)) {
    try {
      s = latexToAscii(s)
    } catch {
      // keep the typed text
    }
  }
  const out = emit(s, 0, s.length).trim()
  return out
}

function emit(s: string, from: number, to: number): string {
  let i = from
  const parts: string[] = []
  const skip = () => {
    while (i < to && /\s/.test(s[i]!)) i++
  }
  while (i < to) {
    skip()
    if (i >= to) break
    const atom = readPiece(s, i, to)
    if (!atom || atom.end <= i) break
    i = atom.end
    if (!atom.text) continue
    if (atom.text.startsWith('_(') && parts.length) parts[parts.length - 1] += atom.text
    else parts.push(atom.text)
  }
  return parts.join(' ')
}

type Piece = { text: string; end: number }

function readPiece(s: string, i: number, to: number): Piece | null {
  const ch = s[i]!
  if (ch === '(' || ch === '[' || ch === '{') {
    if (s.startsWith('[[', i)) return readMatrix(s, i, to)
    const close = matchClose(s, i, to)
    if (close < 0) return { text: ch, end: i + 1 }
    const inner = emit(s, i + 1, close)
    const wrap = ch === '(' ? `(${inner})` : ch === '[' ? `lr(\\[${inner}\\])` : `lr(\\{${inner}\\})`
    return { text: wrap, end: close + 1 }
  }
  if (ch === ')' || ch === ']' || ch === '}') return null
  if (ch === ',') return { text: ',', end: i + 1 }
  if (ch === '_') return readSubscript(s, i, to)
  if (ch === '√' || ch === '∛') return readRadical(s, i, to)
  if (isIdentStart(ch) || 'Σ∑Π∏∫'.includes(ch)) return readWord(s, i, to)
  if (isDigit(ch) || (ch === '.' && isDigit(s[i + 1] ?? ''))) return readNumber(s, i, to)
  return readSymbol(s, i, to)
}

function readWord(s: string, i: number, to: number): Piece {
  const glyph = s[i]!
  if ('Σ∑Π∏∫'.includes(glyph) || /^(?:sum|prod|product|int|integral)(?![A-Za-z])/i.test(s.slice(i, to))) {
    const big = readBigOp(s, i, to)
    if (big) return big
  }
  let j = i
  if (isIdentStart(s[j]!)) {
    j++
    while (j < to && isIdentPart(s[j]!)) j++
  } else {
    j++
  }
  const word = s.slice(i, j)
  let k = j
  while (k < to && /\s/.test(s[k]!)) k++
  if (s[k] === '(' && CALLS.has(word)) {
    const close = matchClose(s, k, to)
    if (close > 0) return { text: callMacro(word, s.slice(k + 1, close)), end: close + 1 }
  }
  if (s[k] === '(') {
    const close = matchClose(s, k, to)
    if (close > 0) return { text: callFn(word, s.slice(k + 1, close)), end: close + 1 }
  }
  return { text: bareName(word), end: j }
}

function callMacro(name: string, inner: string): string {
  if (name === 'amat' || name === 'emat' || name === 'dmat' || name === 'multibinom') {
    const rows = splitTop(inner, ';').map((row) => splitTop(row, ',').map((cell) => emit(cell, 0, cell.length) || ' ').join(', '))
    return `#${name}(${rows.join('; ')})`
  }
  const args = splitTop(inner, ',').map((arg) => {
    const math = emit(arg, 0, arg.length)
    return math ? `$${math}$` : '$ $'
  })
  return `#${name}(${args.join(', ')})`
}

function callFn(word: string, inner: string): string {
  const key = word.toLowerCase()
  const args = splitTop(inner, ',').map((arg) => emit(arg, 0, arg.length))
  if (key === 'sqrt') return `sqrt(${args[0] ?? ''})`
  if (key === 'cbrt') return `root(3, ${args[0] ?? ''})`
  if (key === 'nthroot' && args.length >= 2) return `root(${args[1]}, ${args[0]})`
  if (key === 'ncr' || key === 'combinations') return `binom(${args[0] ?? ''}, ${args[1] ?? ''})`
  if (key === 'npr' || key === 'permutations') return `attach(P, bl: ${args[0] ?? ''}, br: ${args[1] ?? ''})`
  if (key === 'log2') return `log(${args[0] ?? ''}, base: 2)`
  if (key === 'log10' || key === 'log') return `log(${args[0] ?? ''})`
  if (key === 'ln') return `ln(${args[0] ?? ''})`
  if (key === 'abs') return `abs(${args[0] ?? ''})`
  const inv = INVERSE[key]
  if (inv === 'tan1') return `(#tan1)(${args.join(', ')})`
  if (inv) return `${inv}(${args.join(', ')})`
  const name = mathName(word)
  return `${name}(${args.join(', ')})`
}

function bareName(word: string): string {
  if (CONTENT.has(word)) return `#${word}`
  const key = word.toLowerCase()
  if (CONTENT.has(key) && key === word) return `#${word}`
  if (key === 'dot') return 'dot.op'
  if (key === 'pi' || word === 'π') return 'pi'
  if (key === 'theta' || word === 'θ') return 'theta'
  if (key === 'tau') return 'tau'
  if (key === 'infinity' || key === 'infty' || key === 'inf' || word === '∞') return 'infinity'
  if (INVERSE[key] === 'tan1') return '(#tan1)'
  if (INVERSE[key]) return INVERSE[key]
  if (key === 'sqrt') return 'sqrt'
  return mathName(word)
}

function mathName(word: string): string {
  if (/^[A-Za-z][A-Za-z0-9]*$/.test(word)) return word
  return `"${word.replaceAll('"', '')}"`
}

function readRadical(s: string, i: number, to: number): Piece {
  const cube = s[i] === '∛'
  let k = i + 1
  while (k < to && /\s/.test(s[k]!)) k++
  if (s[k] === '(') {
    const close = matchClose(s, k, to)
    if (close > 0) {
      const inner = emit(s, k + 1, close)
      return { text: cube ? `root(3, ${inner})` : `sqrt(${inner})`, end: close + 1 }
    }
  }
  const atom = k < to ? readPiece(s, k, to) : null
  if (!atom) return { text: cube ? 'root(3, )' : 'sqrt', end: i + 1 }
  return { text: cube ? `root(3, ${atom.text})` : `sqrt(${atom.text})`, end: atom.end }
}

function readNumber(s: string, i: number, to: number): Piece {
  let j = i
  while (j < to && /[\d.]/.test(s[j]!)) j++
  if ((s[j] === 'e' || s[j] === 'E') && j + 1 < to) {
    let k = j + 1
    if (s[k] === '+' || s[k] === '-' || s[k] === '−') k++
    const exp = k
    while (k < to && isDigit(s[k]!)) k++
    if (k > exp) j = k
  }
  return { text: s.slice(i, j).replaceAll('−', '-'), end: j }
}

function readSymbol(s: string, i: number, to: number): Piece {
  const two = s.slice(i, Math.min(to, i + 3))
  if (two.startsWith('...')) return { text: 'dots.h', end: i + 3 }
  if (two.startsWith('..')) return { text: 'dots.h', end: i + 2 }
  if (two.startsWith('->') || two.startsWith('→')) return { text: 'arrow.r', end: i + (two.startsWith('->') ? 2 : 1) }
  if (two.startsWith('=>')) return { text: 'arrow.r.double', end: i + 2 }
  if (two.startsWith('<=') || two.startsWith('≤')) return { text: '<=', end: i + (two.startsWith('<=') ? 2 : 1) }
  if (two.startsWith('>=') || two.startsWith('≥')) return { text: '>=', end: i + (two.startsWith('>=') ? 2 : 1) }
  if (two.startsWith('!=' ) || two.startsWith('≠')) return { text: '!=', end: i + (two.startsWith('!=') ? 2 : 1) }
  if (two.startsWith('+-') || two.startsWith('±')) return { text: '#pm', end: i + (two.startsWith('+-') ? 2 : 1) }
  const ch = s[i]!
  const one: Record<string, string> = {
    '+': '+',
    '-': '-',
    '−': '-',
    '–': '-',
    '*': 'ast',
    '×': 'times',
    '·': 'dot.op',
    '÷': 'div',
    '/': '/',
    '^': '^',
    '=': '=',
    '<': '<',
    '>': '>',
    '!': '!',
    '%': '%',
    '.': '.',
    'π': 'pi',
    'θ': 'theta',
    '∞': 'infinity',
    '±': '#pm',
    '∓': 'minus.plus',
    '≈': 'approx',
    '°': 'degree',
    '…': 'dots.h',
    '|': '|',
    '&': '&',
  }
  if (one[ch]) return { text: one[ch], end: i + 1 }
  if (ch === '$' || ch === '#') return { text: `\\${ch}`, end: i + 1 }
  if (ch === '\\') return { text: '', end: i + 1 }
  return { text: '', end: i + 1 }
}

function readMatrix(s: string, i: number, to: number): Piece | null {
  if (!s.startsWith('[[', i)) return null
  const close = matchClose(s, i, to)
  if (close < 0 || s[close - 1] !== ']') return null
  const inner = s.slice(i + 1, close)
  const rows = splitTop(inner.slice(1, -1), ',').map((row) => {
    const cell = row.trim()
    const body = cell.startsWith('[') && cell.endsWith(']') ? cell.slice(1, -1) : cell
    return splitTop(body, ',').map((c) => emit(c, 0, c.length) || ' ').join(', ')
  })
  return { text: `mat(${rows.join('; ')})`, end: close + 1 }
}

function readBigOp(s: string, i: number, to: number): Piece | null {
  let j = i
  let op: 'sum' | 'product' | 'integral' = 'sum'
  if ('Σ∑'.includes(s[j]!)) {
    op = 'sum'
    j++
  } else if ('Π∏'.includes(s[j]!)) {
    op = 'product'
    j++
  } else if (s[j] === '∫') {
    op = 'integral'
    j++
  } else {
    const m = /^(sum|prod|product|int|integral)(?![A-Za-z])/i.exec(s.slice(j, to))
    if (!m) return null
    op = /^prod/i.test(m[1]!) ? 'product' : /^int/i.test(m[1]!) ? 'integral' : 'sum'
    j += m[0].length
  }
  while (j < to && /\s/.test(s[j]!)) j++
  if (s[j] === '(') {
    const close = matchClose(s, j, to)
    if (close < 0) return null
    const args = splitTop(s.slice(j + 1, close), ',').map((a) => a.trim())
    const rendered = renderCallOp(op, args)
    if (!rendered) return null
    return { text: rendered, end: close + 1 }
  }
  if (op === 'integral' || op === 'sum' || op === 'product') {
    const bound = readLimitOp(s, i, to, op)
    if (bound) return bound
  }
  const comma = topIndex(s, j, to, ',')
  if (comma < 0 || op === 'integral') return null
  const body = emit(s, j, comma)
  const range = parseRange(s.slice(comma + 1, to).trim())
  if (!range) return null
  const head = op === 'product' ? 'product' : 'sum'
  return { text: `${head}_(${range.index} = ${range.from})^(${range.to}) ${body}`, end: to }
}

function mathOf(src: string): string {
  return emit(src, 0, src.length)
}

function differential(v: string): string {
  return `dif ${v}`
}

// `x^2 dx` names the variable; the differential is Typst's `dif`, not part of the integrand.
function peelDifferential(src: string): { src: string; dif: string } {
  const m = src.match(/^(.*?)(?:\s*\*)?\s*\bd([A-Za-z])\s*$/s)
  if (!m) return { src: src.trim(), dif: '' }
  return { src: m[1]!.trim(), dif: `dif ${m[2]}` }
}

function renderCallOp(op: 'sum' | 'product' | 'integral', args: string[]): string | null {
  if (op === 'integral') {
    if (args.length === 3) {
      const peeled = peelDifferential(args[0]!)
      const body = mathOf(peeled.src)
      return `integral_(${mathOf(args[1]!)})^(${mathOf(args[2]!)}) ${body}${peeled.dif ? ` ${peeled.dif}` : ''}`.trim()
    }
    if (args.length === 4) return `integral_(${mathOf(args[2]!)})^(${mathOf(args[3]!)}) ${mathOf(args[0]!)} ${differential(args[1]!.trim())}`
    return null
  }
  const head = op === 'product' ? 'product' : 'sum'
  if (args.length === 2) {
    const range = parseRange(args[1]!)
    if (!range) return null
    return `${head}_(${range.index} = ${range.from})^(${range.to}) ${mathOf(args[0]!)}`
  }
  if (args.length === 3) {
    const named = args[1]!.match(/^([A-Za-z]|theta)\s*=\s*(.+)$/i)
    if (named) return `${head}_(${named[1]} = ${mathOf(named[2]!)})^(${mathOf(args[2]!)}) ${mathOf(args[0]!)}`
    return `${head}_(n = ${mathOf(args[1]!)})^(${mathOf(args[2]!)}) ${mathOf(args[0]!)}`
  }
  if (args.length === 4) return `${head}_(${args[1]!.trim()} = ${mathOf(args[2]!)})^(${mathOf(args[3]!)}) ${mathOf(args[0]!)}`
  return null
}

function readSubscript(s: string, i: number, to: number): Piece {
  let j = i + 1
  while (j < to && /\s/.test(s[j]!)) j++
  if (j >= to) return { text: '', end: i + 1 }
  if (s[j] === '{') {
    const close = matchClose(s, j, to)
    if (close < 0) return { text: `_(${emit(s, j + 1, to)})`, end: to }
    return { text: `_(${emit(s, j + 1, close)})`, end: close + 1 }
  }
  const atom = readPiece(s, j, to)
  if (!atom?.text) return { text: '', end: j }
  return { text: `_(${atom.text})`, end: atom.end }
}

// `sum` and `int` are the same limits as Σ and ∫. boundsIn only sees the sign, so a word is rewritten as one.
function limitSource(s: string, i: number, to: number): string | null {
  if ('Σ∑Π∏∫'.includes(s[i]!)) return s.slice(i, to)
  const word = /^(sum|prod|product|int|integral)(?![A-Za-z])/i.exec(s.slice(i, to))
  if (!word) return null
  const glyph = /^prod/i.test(word[1]!) ? 'Π' : /^int/i.test(word[1]!) ? '∫' : 'Σ'
  return glyph + s.slice(i + word[0].length, to)
}

// The calculator stores a built-in integral as ∫_lower^upper. The underscore is the limit marker, not a character to print.
function readLimitOp(s: string, i: number, to: number, head: 'integral' | 'sum' | 'product'): Piece | null {
  const src = limitSource(s, i, to)
  if (!src) return null
  const found = boundsIn(src).find((bound) => bound.sign === 0)
  if (!found) return null
  const lowerSrc = src.slice(found.lower.start, found.lower.end)
  const upperSrc = found.upper ? src.slice(found.upper.start, found.upper.end) : ''
  const lower = emit(lowerSrc, 0, lowerSrc.length)
  const upper = emit(upperSrc, 0, upperSrc.length)
  const peeled = head === 'integral' ? peelDifferential(src.slice(found.end)) : { src: src.slice(found.end).trim(), dif: '' }
  const body = emit(peeled.src, 0, peeled.src.length).trim()
  const limits = `${lower ? `_(${lower})` : ''}${upper ? `^(${upper})` : ''}`
  const tail = `${body ? ` ${body}` : ''}${peeled.dif ? ` ${peeled.dif}` : ''}`
  return { text: `${head}${limits}${tail}`, end: to }
}

function parseRange(raw: string): { index: string; from: string; to: string } | null {
  const m = raw.match(/^(?:([A-Za-z]|theta)\s*(?:=|from|in)\s*)?(.+?)\s*(?:\.\.\.?|…|\bto\b)\s*(.+)$/i)
  if (!m) return null
  const from = m[2]!.trim()
  const upper = m[3]!.trim()
  if (!from || !upper || /\s/.test(from) || /\s/.test(upper)) return null
  return { index: m[1] || 'n', from: emit(from, 0, from.length), to: emit(upper, 0, upper.length) }
}

function splitTop(s: string, sep: ',' | ';'): string[] {
  const out: string[] = []
  let depth = 0
  let start = 0
  for (let i = 0; i < s.length; i++) {
    const ch = s[i]!
    if ('([{'.includes(ch)) depth++
    else if (')]}'.includes(ch)) depth--
    else if (ch === sep && depth === 0) {
      out.push(s.slice(start, i))
      start = i + 1
    }
  }
  out.push(s.slice(start))
  return out
}

function topIndex(s: string, from: number, to: number, sep: string): number {
  let depth = 0
  for (let i = from; i < to; i++) {
    const ch = s[i]!
    if ('([{'.includes(ch)) depth++
    else if (')]}'.includes(ch)) depth--
    else if (ch === sep && depth === 0) return i
  }
  return -1
}

function matchClose(s: string, open: number, to: number): number {
  const pairs: Record<string, string> = { '(': ')', '[': ']', '{': '}' }
  const want = pairs[s[open]!]
  if (!want) return -1
  let depth = 0
  for (let i = open; i < to; i++) {
    if (s[i] === s[open]) depth++
    else if (s[i] === want && --depth === 0) return i
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
