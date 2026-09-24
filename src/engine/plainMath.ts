import type { UserFunction, Value } from './types'
import { fillParens } from './parens'
import { looksLikeTypst, typstToAscii } from './typstInput'
import { truthful } from './precise'
import { evalScientific, rewriteTypesetMul, stitchConstants, wrapBareFunctions, type AngleMode } from './scientific'
import { tryConvert, type DefaultUnits } from './units'

export type { AngleMode }

const NLP_WORDS =
  /\b(of|off|from|today|tomorrow|yesterday|tax|tip|people|nights|was|until|between|per|earnings|lunch|miles|weeks?|days?|hours?|months?)\b/i

export function looksLikeLatex(s: string): boolean {
  return /\\[a-zA-Z%]+|[\^_]\{|\\frac|\\sqrt/.test(s)
}

/** "what is 40% of 90?" becomes "40% of 90". */
function unwrapQuestion(text: string): string {
  const src = text.trim()
  if (!src) return src
  const bare = src
    .replace(/\s*\?+\s*$/g, '')
    .replace(/^(?:what(?:'s|\s+is|s)|how\s+much\s+is|calculate|compute)\s+/i, '')
    .trim()
  return bare || src
}

function hasNlpWords(s: string): boolean {
  const t = s
    .replace(/\\text\{([^}]*)\}/g, ' $1 ')
    .replace(/\\mathrm\{([^}]*)\}/g, ' $1 ')
    .replace(/\\[a-zA-Z]+/g, ' ')
  return NLP_WORDS.test(t) || /\bin\b/i.test(t.replace(/\bsin\b|\bmin\b|\binfinity\b/gi, ''))
}

export function latexToAscii(latex: string): string {
  let s = latex.trim()
  if (!s) return s
  s = s.replace(/\\left/g, '').replace(/\\right/g, '')
  s = s.replace(/\\,|\\;|\\!|\\:|\\ /g, '')
  s = s.replace(/\\ldots|\\cdots|\\dots/g, '...')
  s = rewriteTypesetMul(s.replace(/\\cdot|\\times/g, '*'))
  s = s.replace(/\\div/g, '/')
  // ± stays ±: the engine reads it as an uncertainty, or leaves the line blank
  s = s.replace(/\\pm\b/g, '±').replace(/\\mp\b/g, '∓')
  s = s.replace(/\\pi\b/g, '(pi)')
  s = s.replace(/\\tau\b/g, '(tau)')
  s = s.replace(/\\infty\b/g, 'Infinity')
  s = s.replace(/\\to\b/g, '->')
  s = s.replace(/\\exponentialE\b/g, '(e)')
  s = s.replace(/\\mathrm\{e\}/g, '(e)')
  s = s.replace(/\\imaginaryI\b|\\imaginaryJ\b/g, 'i')
  s = stitchConstants(s)
  s = s.replace(/\\theta\b/g, 'theta')
  s = s.replace(/\\degree\b/g, '')
  s = s.replace(/\\text\{([^}]*)\}/g, ' $1 ')
  s = s.replace(/\\operatorname\{([^}]+)\}/g, '$1')
  s = s.replace(/\\mathrm\{([^}]+)\}/g, '$1')
  s = s.replace(/\\[dtc]frac\b/g, '\\frac')
  s = replaceBraced(s, '\\frac', 2, (a, b) => `((${a})/(${b}))`)
  s = replaceBraced(s, '\\binom', 2, (a, b) => `combinations(${a},${b})`)
  s = convertSqrts(s)
  s = replaceBraced(s, '\\abs', 1, (a) => `abs(${a})`)
  s = s.replace(/\\lfloor\s*/g, 'floor(').replace(/\\rfloor/g, ')')
  s = s.replace(/\\lceil\s*/g, 'ceil(').replace(/\\rceil/g, ')')
  s = convertLogs(s)
  s = s.replace(/\\ar(sinh|cosh|tanh)/g, 'a$1')
  s = s.replace(/\\(sinh|cosh|tanh|csch|sech|coth)\b/g, '$1')
  s = s.replace(/\\arc(sin|cos|tan|csc|sec|cot)\b/g, 'a$1')
  s = s.replace(/\\(sin|cos|tan|csc|sec|cot)\s*\^\s*\{-1\}/g, 'a$1')
  s = s.replace(/\\(sin|cos|tan|csc|sec|cot)\b/g, '$1')
  s = s.replace(/\\exp\b/g, 'exp')
  s = repeatingDecimals(s)
  s = s.replace(/\\overline\{([^}]+)\}/g, 'conj($1)')
  s = s.replace(/\|([^|]+)\|/g, 'abs($1)')
  s = s.replace(/\^{([^{}]+)}/g, '^($1)')
  // a subscript names a different thing (`x_{1}`, `10_{2}`), so it stays and the line goes blank
  s = s.replace(/_{([^{}]+)}/g, '_$1')
  s = s.replace(/\\%/g, '%')
  s = s.replace(/\\\$/g, '$')
  s = s.replace(/\\/g, '')
  s = s.replace(/[{}]/g, '')
  s = wrapBareFunctions(s.replace(/\s+/g, ' ').trim())
  return s
}

/** `0.1\overline{6}` is 0.1666…, as a fraction so no digits are lost. */
function repeatingDecimals(s: string): string {
  return s.replace(/(?<![\d.])(\d*)\.(\d*)\\overline\{(\d+)\}/g, (_, whole: string, fixed: string, rep: string) => {
    const top = BigInt(fixed + rep) - BigInt(fixed || '0')
    const bottom = '9'.repeat(rep.length) + '0'.repeat(fixed.length)
    return `(${whole || '0'}+${top}/${bottom})`
  })
}

function convertSqrts(src: string): string {
  let s = src
  let i = 0
  while (true) {
    const at = s.indexOf('\\sqrt', i)
    if (at < 0) break
    let p = at + 5
    while (p < s.length && s[p] === ' ') p++
    let n: string | null = null
    if (s[p] === '[') {
      const close = s.indexOf(']', p)
      if (close < 0) {
        i = at + 1
        continue
      }
      n = s.slice(p + 1, close)
      p = close + 1
    }
    while (p < s.length && s[p] === ' ') p++
    if (s[p] !== '{') {
      i = at + 1
      continue
    }
    const grabbed = grabBrace(s, p)
    if (!grabbed) {
      i = at + 1
      continue
    }
    const put = n ? `nthRoot(${grabbed.inner},${n})` : `sqrt(${grabbed.inner})`
    s = s.slice(0, at) + put + s.slice(grabbed.end)
    i = at + put.length
  }
  return s
}

function convertLogs(src: string): string {
  let s = replaceBraced(src, '\\ln', 1, (a) => `ln(${a})`)
  s = s.replace(/\\ln\s*\(([^)]*)\)/g, 'ln($1)')
  s = s.replace(/\\ln\b/g, 'ln')
  let i = 0
  while (true) {
    const at = s.indexOf('\\log', i)
    if (at < 0) break
    let p = at + 4
    while (p < s.length && s[p] === ' ') p++
    let base = '10'
    if (s[p] === '_') {
      p++
      if (s[p] === '{') {
        const grabbed = grabBrace(s, p)
        if (grabbed) {
          base = grabbed.inner
          p = grabbed.end
        }
      } else if (p < s.length) {
        base = s[p]
        p++
      }
    }
    while (p < s.length && s[p] === ' ') p++
    let arg: string | null = null
    if (s[p] === '{') {
      const grabbed = grabBrace(s, p)
      if (grabbed) {
        arg = grabbed.inner
        p = grabbed.end
      }
    } else if (s[p] === '(') {
      const grabbed = grabParen(s, p)
      if (grabbed) {
        arg = grabbed.inner
        p = grabbed.end
      }
    }
    const put = arg == null ? (base === '10' ? 'log10' : `log(_,${base})`) : base === '10' ? `log10(${arg})` : `log(${arg},${base})`
    s = s.slice(0, at) + put + s.slice(p)
    i = at + put.length
  }
  return s
}

function replaceBraced(src: string, cmd: string, arity: number, build: (...args: string[]) => string): string {
  let s = src
  let i = 0
  while (true) {
    const at = s.indexOf(cmd, i)
    if (at < 0) break
    let p = at + cmd.length
    const args: string[] = []
    let ok = true
    for (let n = 0; n < arity; n++) {
      while (p < s.length && s[p] === ' ') p++
      if (s[p] !== '{') {
        ok = false
        break
      }
      const grabbed = grabBrace(s, p)
      if (!grabbed) {
        ok = false
        break
      }
      args.push(grabbed.inner)
      p = grabbed.end
    }
    if (!ok) {
      i = at + 1
      continue
    }
    const put = build(...args)
    s = s.slice(0, at) + put + s.slice(p)
    // an argument can hold the same command (`\frac{\frac{1}{2}}{2}`)
    i = at
  }
  return s
}

function grabGroup(s: string, open: number, left: string, right: string): { inner: string; end: number } | null {
  if (s[open] !== left) return null
  let depth = 0
  for (let i = open; i < s.length; i++) {
    if (s[i] === left) depth++
    else if (s[i] === right && --depth === 0) return { inner: s.slice(open + 1, i), end: i + 1 }
  }
  return null
}

function grabBrace(s: string, open: number): { inner: string; end: number } | null {
  return grabGroup(s, open, '{', '}')
}

function grabParen(s: string, open: number): { inner: string; end: number } | null {
  return grabGroup(s, open, '(', ')')
}

/** `1,000,000` becomes `1000000`; commas inside a call's arguments (`max(1,200)`) and lists stay. */
function stripThousands(s: string): string {
  return s.replace(/(?<![A-Za-z_][A-Za-z0-9_]*\([^()]*|\[[^\][]*)\b\d{1,3}(?:,\d{3})+\b(?!,?\d)/g, (m) => m.replace(/,/g, ''))
}

const SUPERSCRIPT = '⁰¹²³⁴⁵⁶⁷⁸⁹'

/**
 * `3.00 × 10^8 m/s` becomes `3.00e8 m/s`, so the unit reads. only where nothing binds tighter on
 * either side: `1/2 × 10^3`, `2^-3 × 10^2` and `3 × 10^2!` are left alone.
 */
function foldPowersOfTen(s: string): string {
  return s.replace(
    /(?<=(?:^|[-+*(,=−])\s*)(?<!\^\s*[-+−]\s*)(?<![\d.][eE][-+−])(\d+(?:\.\d*)?|\.\d+)\s*[×*·⋅]\s*10\s*(?:\^\s*(?:\(\s*([-−]?\d+)\s*\)|([-−]?\d+))|([⁻]?[⁰¹²³⁴⁵⁶⁷⁸⁹]+))(?![\d.^!(%⁰¹²³⁴⁵⁶⁷⁸⁹⁻])/g,
    (_, mantissa: string, paren?: string, bare?: string, sup?: string) => {
      const power = paren ?? bare ?? [...sup!].map((c) => (c === '⁻' ? '-' : SUPERSCRIPT.indexOf(c))).join('')
      return `${mantissa}e${power.replace('−', '-')}`
    },
  )
}

/** LaTeX to ascii, typeset operators, thousands separators and missing parens. */
export function normalizeMathText(text: string): string {
  const typst = looksLikeTypst(text) ? typstToAscii(text) : text
  const ascii = looksLikeLatex(typst) ? latexToAscii(typst) : typst
  return fillParens(foldPowersOfTen(stripThousands(rewriteTypesetMul(ascii))))
}

export function tryPlainMath(
  text: string,
  ctx: {
    ans?: number
    angleMode?: AngleMode
    variables?: Record<string, number>
    functions?: Record<string, UserFunction>
    defaultUnits?: DefaultUnits
  } = {},
): Value | null {
  const src = unwrapQuestion(text)
  if (!src) return null
  const filled = normalizeMathText(src)
  // a known variable (`n = 5`, then `n*2`) is never read as a unit symbol like N, m or s
  const converted = tryConvert(filled, ctx.defaultUnits, ctx.variables)
  if (converted) return converted
  const cleaned = src.replace(/\d+(?:\.\d+)?\s*%\s*of\b/gi, (m) => m.replace(/\s*of\b/i, ''))
  if (hasNlpWords(cleaned) && !looksLikeLatex(src)) return null
  const value = evalScientific(filled, ctx)
  return value && truthful(filled, value, ctx)
}
