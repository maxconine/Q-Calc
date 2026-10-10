import type { MathNode } from 'mathjs'
import { math } from './math'
import { normalizeMathText } from './plainMath'
import { preprocessAscii, SCIENTIFIC_NAMES } from './scientific'

// whether a plain-number answer is an angle, so it can say deg or rad: `asin(0.5)` is 30 deg in deg mode,
// `2 asin(0.5)` is 60 deg, but `asin(0.5) / acos(0.5)` and `sin(asin(0.5))` are plain numbers

const INVERSE = new Set(['asin', 'acos', 'atan', 'atan2', 'acsc', 'asec', 'acot'])
const TRIG = new Set(['sin', 'cos', 'tan', 'csc', 'sec', 'cot'])
// the answer is an angle when its arguments are
const SAME = new Set(['abs', 'round', 'floor', 'ceil', 'min', 'max', 'mean', 'median', 'sum', 'mod'])

// after the engine's own rewrite, every way of writing one (tan^-1, tan^(-1), tan⁻¹, arctan, \arctan) reads atan
const MENTIONS_INVERSE = /\ba(?:sin|cos|tan|csc|sec|cot|tan2)\b/i

type N = { type: string; name?: string; op?: string; fn?: string | { name?: string }; args?: N[]; content?: N; value?: unknown }

/**
 * How many times an angle the expression is: 1 for an angle, 0 for a plain number, null when it can't tell.
 * Only expressions with an inverse trig function are looked at; anything else is a plain number.
 */
export function angleExponent(expr: string): number | null {
  let text: string
  try {
    text = preprocessAscii(normalizeMathText(expr))
  } catch {
    return null
  }
  if (!MENTIONS_INVERSE.test(text)) return 0
  let node: MathNode
  try {
    node = math.parse(text.replace(/[×·]/g, '*'))
  } catch {
    return null
  }
  return power(node as unknown as N)
}

function power(n: N): number | null {
  switch (n.type) {
    case 'ConstantNode':
      return 0
    case 'SymbolNode':
      return 0
    case 'ParenthesisNode':
      return n.content ? power(n.content) : null
    case 'FunctionNode': {
      const name = (typeof n.fn === 'string' ? n.fn : n.fn?.name ?? n.name ?? '').toLowerCase()
      const args = (n.args ?? []).map(power)
      if (args.some((a) => a == null)) return null
      if (INVERSE.has(name)) return args.every((a) => a === 0) ? 1 : null
      if (TRIG.has(name)) return 0
      // round's second argument is how many digits, not an angle
      if (name === 'round') return args[0] ?? 0
      if (SAME.has(name)) return args.every((a) => a === args[0]) ? (args[0] ?? 0) : null
      return args.every((a) => a === 0) ? 0 : null
    }
    case 'OperatorNode': {
      const args = (n.args ?? []).map(power)
      if (args.some((a) => a == null)) return null
      const [a, b] = args as number[]
      switch (n.fn) {
        case 'unaryMinus':
        case 'unaryPlus':
          return a ?? null
        case 'add':
        case 'subtract':
          // `asin(0.5) + 10` is 40 degrees: a plain number added to an angle is in its unit
          if (a === b) return a!
          return a === 0 ? b! : b === 0 ? a! : null
        case 'multiply':
          return a! + b!
        case 'divide':
          return a! - b!
        case 'pow': {
          if (a === 0) return 0
          const exp = n.args?.[1]
          return exp?.type === 'ConstantNode' && typeof exp.value === 'number' ? a! * exp.value : null
        }
        default:
          return args.every((x) => x === 0) ? 0 : null
      }
    }
    default:
      return null
  }
}

// a call to a built-in, however it's written: sqrt(, SIN(, \arcsin(, sin^-1(, sin^(-1)(, sin^{-1}(, sin⁻¹(
// and with a power on the name: sin^2(, cos²(
const CALL_RE = /(?<![A-Za-z_])\\?([A-Za-z][A-Za-z0-9]*?)\s*(?:\^\s*(?:-?\d+|\(\s*-?\d+\s*\)|\{\s*-?\d+\s*\})|[⁻⁰¹²³⁴⁵⁶⁷⁸⁹]+)?\s*\(/g
const BUILT_IN = new Set(SCIENTIFIC_NAMES.split('|').map((n) => n.toLowerCase()))
// a root of a quantity is a power of it: sqrt(16 m^2) is (16 m^2)^0.5
const ROOT_POWER: Record<string, string> = { sqrt: '0.5', cbrt: '(1/3)' }

function closing(text: string, open: number): number {
  let depth = 0
  for (let i = open; i < text.length; i++) {
    if (text[i] === '(') depth++
    else if (text[i] === ')' && --depth === 0) return i
  }
  return -1
}

/**
 * The expression with its function calls worked out, so they can sit among typed units, which the unit reader
 * can't call functions in: `10 N * cos(30)` → `10 N * (0.866…)`, `180 deg - asin(0.5)` → `180 deg - (30 deg)`,
 * `sqrt((3 m)^2 + (4 m)^2)` → `((3 m)^2 + (4 m)^2)^0.5`. An inverse trig answer goes in as an angle in the mode's
 * unit; a call with units inside is left for its inner calls. Null when nothing changed.
 */
export function callsWorkedOut(expr: string, mode: 'deg' | 'rad', evaluate: (call: string) => number | null): string | null {
  let changed = false
  const work = (text: string): string => {
    let out = ''
    let from = 0
    const re = new RegExp(CALL_RE.source, 'g')
    for (let m = re.exec(text); m; m = re.exec(text)) {
      const name = m[1]!.toLowerCase()
      if (!BUILT_IN.has(name)) continue
      const open = m.index + m[0].length - 1
      const close = closing(text, open)
      if (close < 0) break
      const call = text.slice(m.index, close + 1)
      const before = text.slice(from, m.index)
      // `10 N cos(30)`: a call right after a quantity multiplies it
      const glue = /[A-Za-z0-9.)²³]\s*$/.test(out + before) ? '*' : ''
      const n = evaluate(call)
      let put: string
      if (n != null && Number.isFinite(n)) put = angleExponent(call) === 1 ? `(${n} ${mode})` : `(${n})`
      else {
        const inner = work(text.slice(open + 1, close))
        const power = /[\^⁻⁰¹²³⁴⁵⁶⁷⁸⁹]/.test(m[0]) ? undefined : ROOT_POWER[name]
        if (power) put = `(${inner})^${power}`
        else {
          // nothing to do with the call itself; its inner calls may still have been worked out
          out += `${before}${text.slice(m.index, open + 1)}${inner})`
          from = close + 1
          re.lastIndex = from
          continue
        }
      }
      changed = true
      out += `${before}${glue}${put}`
      from = close + 1
      re.lastIndex = from
    }
    return out + text.slice(from)
  }
  // a root sign with brackets is the same call: √(16 m^2), ∛(8 m^3). a bare one (√2π°) is left to the engine's own
  // reading of how far it reaches, which the bar drawn over it in the input follows
  if (/[√∛](?!\s*\()/.test(expr)) return null
  const spelled = expr.replace(/√\s*\(/g, 'sqrt(').replace(/∛\s*\(/g, 'cbrt(')
  const result = work(spelled)
  return changed ? result : null
}
