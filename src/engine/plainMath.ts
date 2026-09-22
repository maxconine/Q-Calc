import type { UserFunction, Value } from './types'
import { fillParens } from './parens'
import { evalScientific, rewriteTypesetMul, stitchConstants, wrapBareFunctions, type AngleMode } from './scientific'
import { tryConvert, type DefaultUnits } from './units'

export type { AngleMode }

const NLP_WORDS =
  /\b(of|off|from|today|tomorrow|yesterday|tax|tip|people|nights|was|until|between|per|earnings|lunch|miles|weeks?|days?|hours?|months?)\b/i

function looksLikeLatex(s: string): boolean {
  return /\\[a-zA-Z]+|[\^_]\{|\\frac|\\sqrt/.test(s)
}

/** Drop leading "what is" / trailing "?" so "what is 40% of 90" can use the existing percent-of path. */
function unwrapQuestion(text: string): string {
  const src = text.trim()
  if (!src) return src
  const stripped = src
    .replace(/\s*\?+\s*$/g, '')
    .replace(/^(?:what(?:'s|\s+is|s)|how\s+much\s+is|calculate|compute)\s+/i, '')
    .trim()
  return stripped || src
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
  s = s.replace(/\\pm/g, '+')
  s = s.replace(/\\pi\b/g, '(pi)')
  s = s.replace(/\\tau\b/g, '(tau)')
  s = s.replace(/\\infty\b/g, 'Infinity')
  s = s.replace(/\\exponentialE\b/g, '(e)')
  s = s.replace(/\\mathrm\{e\}/g, '(e)')
  s = s.replace(/\\imaginaryI\b|\\imaginaryJ\b/g, 'i')
  s = stitchConstants(s)
  s = s.replace(/\\theta\b/g, 'theta')
  s = s.replace(/\\degree\b/g, '')
  s = s.replace(/\\text\{([^}]*)\}/g, ' $1 ')
  s = s.replace(/\\operatorname\{([^}]+)\}/g, '$1')
  s = s.replace(/\\mathrm\{([^}]+)\}/g, '$1')
  s = replaceBraced(s, '\\frac', 2, (a, b) => `((${a})/(${b}))`)
  s = replaceBraced(s, '\\binom', 2, (a, b) => `combinations(${a},${b})`)
  s = convertSqrts(s)
  s = replaceBraced(s, '\\abs', 1, (a) => `abs(${a})`)
  s = s.replace(/\\lfloor\s*/g, 'floor(').replace(/\\rfloor/g, ')')
  s = s.replace(/\\lceil\s*/g, 'ceil(').replace(/\\rceil/g, ')')
  s = convertLogs(s)
  s = s.replace(/\\operatorname\{arsinh\}|\\arsinh/g, 'asinh')
  s = s.replace(/\\operatorname\{arcosh\}|\\arcosh/g, 'acosh')
  s = s.replace(/\\operatorname\{artanh\}|\\artanh/g, 'atanh')
  s = s.replace(/\\sinh\b/g, 'sinh').replace(/\\cosh\b/g, 'cosh').replace(/\\tanh\b/g, 'tanh')
  s = s.replace(/\\csch\b/g, 'csch').replace(/\\sech\b/g, 'sech').replace(/\\coth\b/g, 'coth')
  s = s.replace(/\\arcsin\b/g, 'asin').replace(/\\arccos\b/g, 'acos').replace(/\\arctan\b/g, 'atan')
  s = s.replace(/\\arccsc\b/g, 'acsc').replace(/\\arcsec\b/g, 'asec').replace(/\\arccot\b/g, 'acot')
  s = s.replace(/\\sin\s*\^\s*\{-1\}/g, 'asin')
  s = s.replace(/\\cos\s*\^\s*\{-1\}/g, 'acos')
  s = s.replace(/\\tan\s*\^\s*\{-1\}/g, 'atan')
  s = s.replace(/\\csc\s*\^\s*\{-1\}/g, 'acsc')
  s = s.replace(/\\sec\s*\^\s*\{-1\}/g, 'asec')
  s = s.replace(/\\cot\s*\^\s*\{-1\}/g, 'acot')
  s = s.replace(/\\sin\b/g, 'sin').replace(/\\cos\b/g, 'cos').replace(/\\tan\b/g, 'tan')
  s = s.replace(/\\csc\b/g, 'csc').replace(/\\sec\b/g, 'sec').replace(/\\cot\b/g, 'cot')
  s = s.replace(/\\exp\b/g, 'exp')
  s = s.replace(/\\overline\{([^}]+)\}/g, 'conj($1)')
  s = s.replace(/\|([^|]+)\|/g, 'abs($1)')
  s = s.replace(/\^{([^{}]+)}/g, '^($1)')
  s = s.replace(/_{([^{}]+)}/g, '')
  s = s.replace(/\\%/g, '%')
  s = s.replace(/\\\$/g, '$')
  s = s.replace(/\\/g, '')
  s = s.replace(/[{}]/g, '')
  s = wrapBareFunctions(s.replace(/\s+/g, ' ').trim())
  return s
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
    i = at + put.length
  }
  return s
}

function grabBrace(s: string, open: number): { inner: string; end: number } | null {
  if (s[open] !== '{') return null
  let depth = 0
  for (let i = open; i < s.length; i++) {
    if (s[i] === '{') depth++
    else if (s[i] === '}') {
      depth--
      if (depth === 0) return { inner: s.slice(open + 1, i), end: i + 1 }
    }
  }
  return null
}

function grabParen(s: string, open: number): { inner: string; end: number } | null {
  if (s[open] !== '(') return null
  let depth = 0
  for (let i = open; i < s.length; i++) {
    if (s[i] === '(') depth++
    else if (s[i] === ')') {
      depth--
      if (depth === 0) return { inner: s.slice(open + 1, i), end: i + 1 }
    }
  }
  return null
}

/** True when `expr` mentions a known variable as a bare identifier (unit symbols must not steal them). */
function mentionsVariable(expr: string, variables?: Record<string, number>): boolean {
  const names = Object.keys(variables ?? {})
  if (!names.length) return false
  const escaped = names
    .sort((a, b) => b.length - a.length)
    .map((n) => n.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
  return new RegExp(`\\b(?:${escaped.join('|')})\\b`).test(expr)
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
  const ascii = looksLikeLatex(src) ? latexToAscii(src) : src
  const filled = fillParens(rewriteTypesetMul(ascii))
  // Prefer scientific eval when a token is a known variable (e.g. `n=5` then `n*2`),
  // so unit symbols like N/m/s do not steal the name.
  if (!mentionsVariable(filled, ctx.variables)) {
    const converted = tryConvert(filled, ctx.defaultUnits)
    if (converted) return converted
  }
  const cleaned = src.replace(/\d+(?:\.\d+)?\s*%\s*of\b/gi, (m) => m.replace(/\s*of\b/i, ''))
  if (hasNlpWords(cleaned) && !looksLikeLatex(src)) return null
  return evalScientific(filled, ctx)
}
