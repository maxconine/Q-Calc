import type { MathNode } from 'mathjs'
import { math } from './math'

// whether a plain-number answer is an angle, so it can say deg or rad: `asin(0.5)` is 30 deg in deg mode,
// `2 asin(0.5)` is 60 deg, but `asin(0.5) / acos(0.5)` and `sin(asin(0.5))` are plain numbers

const INVERSE = new Set(['asin', 'acos', 'atan', 'atan2', 'acsc', 'asec', 'acot'])
const TRIG = new Set(['sin', 'cos', 'tan', 'csc', 'sec', 'cot'])
// the answer is an angle when its arguments are
const SAME = new Set(['abs', 'round', 'floor', 'ceil', 'min', 'max', 'mean', 'median', 'sum', 'mod'])

const MENTIONS_INVERSE = /\b(?:a|arc)(?:sin|cos|tan|csc|sec|cot)\b|\barctan2\b|\batan2\b/i

type N = { type: string; name?: string; op?: string; fn?: string | { name?: string }; args?: N[]; content?: N; value?: unknown }

/**
 * How many times an angle the expression is: 1 for an angle, 0 for a plain number, null when it can't tell.
 * Only expressions with an inverse trig function are looked at; anything else is a plain number.
 */
export function angleExponent(expr: string): number | null {
  if (!MENTIONS_INVERSE.test(expr)) return 0
  let node: MathNode
  try {
    const text = expr
      .replace(/\barc(sin|cos|tan|csc|sec|cot)\b/gi, 'a$1')
      .replace(/\barctan2\b/gi, 'atan2')
      .replace(/[×·]/g, '*')
      .replace(/÷/g, '/')
      .replace(/−/g, '-')
    node = math.parse(text)
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
