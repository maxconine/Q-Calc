// WP-SYMBOLIC: records the engine's calls into mathjs's symbolic layer (simplify, derivative, rationalize,
// Node.toString, and parse from the symbolic readers) as `symbolic.*` fixtures. Loaded by setup.ts only.
//
// A node is written as a small tree:
//   {"C": value}                      ConstantNode (value in the recorder's encoding)
//   {"S": name}                       SymbolNode
//   {"O": fn, "op": op, "a": [...]}   OperatorNode, plus "i": true when implicit, "p": true for a percentage
//   {"F": name, "a": [...]}           FunctionNode called by name ({"F": {tree}} for any other callee)
//   {"P": tree}                       ParenthesisNode
//   {"X": type, "s": text}            any other node (not replayable)
// A multiply whose args hold the very same node object twice gets "same": true (derivative's product rule
// compares args by identity, so such a tree differentiates differently from a copy of it).
import { record } from './recorder'
import { encode } from './recorder'

type AnyNode = {
  type: string
  value?: unknown
  name?: string
  op?: string
  fn?: unknown
  args?: AnyNode[]
  content?: AnyNode
  implicit?: boolean
  isPercentage?: boolean
  toString: (o?: unknown) => string
}

export function tree(n: AnyNode): unknown {
  switch (n.type) {
    case 'ConstantNode':
      return { C: encode(n.value) }
    case 'SymbolNode':
      return { S: n.name }
    case 'ParenthesisNode':
      return { P: tree(n.content!) }
    case 'OperatorNode': {
      const out: Record<string, unknown> = { O: n.fn, op: n.op, a: n.args!.map(tree) }
      if (n.implicit) out.i = true
      if (n.isPercentage) out.p = true
      const args = n.args!
      if (args.some((x, i) => args.indexOf(x) !== i)) out.same = true
      return out
    }
    case 'FunctionNode': {
      const fn = n.fn as AnyNode
      return { F: fn.type === 'SymbolNode' ? fn.name : tree(fn), a: n.args!.map(tree) }
    }
    default:
      return { X: n.type, s: guarded(() => n.toString()) }
  }
}

let inside = 0
function guarded<T>(f: () => T): T {
  inside++
  try {
    return f()
  } finally {
    inside--
  }
}

function isNode(v: unknown): v is AnyNode {
  return typeof v === 'object' && v !== null && (v as { isNode?: boolean }).isNode === true
}

function encodeArg(v: unknown): unknown {
  if (isNode(v)) return tree(v)
  return encode(v)
}

function encodeOut(v: unknown): unknown {
  if (isNode(v)) return { text: guarded(() => v.toString()), tree: tree(v) }
  return encode(v)
}

/** Callers whose parses feed the symbolic layer (the solve, isolate, system and calculus readers). */
const PARSE_CALLERS = /\/src\/engine\/(calculus|isolate|system|solve)\.ts/

function callerIsSymbolic(): boolean {
  const stack = new Error().stack ?? ''
  // frame 0 is this function, 1 the wrapper; the first engine frame after them is the caller
  const lines = stack.split('\n').slice(3)
  const first = lines.find((l) => l.includes('/src/'))
  return first ? PARSE_CALLERS.test(first) : false
}

function wrapCall(fnName: string, orig: (...a: unknown[]) => unknown, onlyIf?: () => boolean) {
  const wrapped = function (this: unknown, ...args: unknown[]) {
    if (inside > 0 || (onlyIf && !onlyIf())) return orig.apply(this, args)
    let encoded: unknown
    try {
      encoded = args.map(encodeArg)
    } catch {
      return orig.apply(this, args)
    }
    let out: unknown
    let threw: unknown = null
    inside++
    try {
      out = orig.apply(this, args)
    } catch (e) {
      threw = e ?? new Error('threw')
    } finally {
      inside--
    }
    try {
      record(`symbolic/${fnName}`, fnName, encoded, threw ? { throws: String((threw as Error)?.message ?? threw) } : { out: encodeOut(out) })
    } catch {
      // unserializable: not recorded
    }
    if (threw) throw threw
    return out
  }
  Object.assign(wrapped, orig)
  return wrapped
}

/** Instruments a mathjs instance in place. */
export function instrumentMath(math: Record<string, unknown>): void {
  for (const name of ['simplify', 'derivative', 'rationalize']) {
    math[name] = wrapCall(name, math[name] as (...a: unknown[]) => unknown)
  }
  math.parse = wrapCall('parse', math.parse as (...a: unknown[]) => unknown, callerIsSymbolic)
  const proto = (math.Node as { prototype: { toString: (o?: unknown) => string } }).prototype
  const toString = proto.toString
  proto.toString = function (this: AnyNode, options?: unknown) {
    if (inside > 0) return toString.call(this, options)
    let encoded: unknown
    try {
      encoded = options === undefined ? [tree(this)] : [tree(this), encode(options)]
    } catch {
      return toString.call(this, options)
    }
    inside++
    let out: string
    try {
      out = toString.call(this, options)
    } finally {
      inside--
    }
    try {
      record('symbolic/toString', 'toString', encoded, { out })
    } catch {
      // not recorded
    }
    return out
  }
}
