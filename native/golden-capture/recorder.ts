// Records top-level calls into the TS engine while vitest runs, for the Rust golden fixtures.
// Loaded only by native/golden-capture/vitest.config.ts; `npm test` never sees it.
import { appendFileSync, mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { expect } from 'vitest'

const RAW_DIR = process.env.QCALC_GOLDEN_RAW ?? join(process.cwd(), 'native/core/golden/.raw')
/** Records whose JSON is larger than this are dropped (counted in the merge summary). */
const MAX_RECORD = 256 * 1024
const FLUSH_AT = 500

/** A callback the replayer understands: the corpus tags its callbacks with `__golden`. */
export type TaggedFn = ((...args: never[]) => unknown) & { __golden?: unknown }

class Unserializable extends Error {}

/**
 * JSON with the values plain JSON loses: NaN, ±Infinity and -0 become {"$f": "..."}, a Date {"$date": iso},
 * a tagged callback {"$fn": tag}, any other function {"$fn": "anon"}. Classes, maps and cycles can't be replayed.
 */
export function encode(value: unknown, seen = new Set<object>()): unknown {
  if (value === null) return null
  switch (typeof value) {
    case 'undefined':
      return { $undef: true }
    case 'number':
      if (Number.isNaN(value)) return { $f: 'NaN' }
      if (value === Infinity) return { $f: 'Infinity' }
      if (value === -Infinity) return { $f: '-Infinity' }
      if (Object.is(value, -0)) return { $f: '-0' }
      return value
    case 'string':
    case 'boolean':
      return value
    case 'bigint':
      return { $bigint: value.toString() }
    case 'function': {
      const tag = (value as TaggedFn).__golden
      return { $fn: tag ?? 'anon' }
    }
    case 'object':
      break
    default:
      throw new Unserializable(typeof value)
  }
  const obj = value as object
  if (seen.has(obj)) throw new Unserializable('cycle')
  if (obj instanceof Date) return { $date: obj.toISOString() }
  seen.add(obj)
  try {
    if (Array.isArray(obj)) return obj.map((v) => (v === undefined ? { $undef: true } : encode(v, seen)))
    const proto = Object.getPrototypeOf(obj)
    if (proto !== Object.prototype && proto !== null) throw new Unserializable(proto?.constructor?.name ?? 'class')
    const out: Record<string, unknown> = {}
    for (const [k, v] of Object.entries(obj)) {
      // a missing field and an undefined one read the same everywhere in the engine
      if (v !== undefined) out[k] = encode(v, seen)
    }
    return out
  } finally {
    seen.delete(obj)
  }
}

let depth = 0
let buffer: string[] = []
let fileName: string | null = null
const stats = { recorded: 0, unserializable: 0, oversize: 0 }

function outFile(): string {
  if (!fileName) {
    mkdirSync(RAW_DIR, { recursive: true })
    fileName = join(RAW_DIR, `${process.pid}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}.jsonl`)
  }
  return fileName
}

export function flush(): void {
  if (!buffer.length) return
  appendFileSync(outFile(), buffer.join(''))
  buffer = []
}

function testPath(): string {
  try {
    const p = expect.getState().testPath ?? ''
    const at = p.indexOf('/src/')
    if (at >= 0) return p.slice(at + 1)
    const native = p.indexOf('/native/')
    return native >= 0 ? p.slice(native + 1) : p
  } catch {
    return ''
  }
}

// trace of the inner engine calls that answered, for sorting evaluate calls into areas
let trace: Set<string> | null = null

function answered(v: unknown): boolean {
  if (v == null || v === false || v === '') return false
  if (Array.isArray(v)) return v.length > 0
  return true
}

function sameJson(a: unknown, b: unknown): boolean {
  try {
    return JSON.stringify(encode(a)) === JSON.stringify(encode(b))
  } catch {
    return false
  }
}

/** Wraps one exported function so calls from test code (not engine-internal ones) are recorded. */
export function wrap<F extends (...args: never[]) => unknown>(module: string, name: string, fn: F): F {
  const wrapped = function (this: unknown, ...args: never[]) {
    if (depth > 0) {
      depth++
      try {
        const out = fn.apply(this, args)
        if (trace && answered(out)) trace.add(`${module}.${name}`)
        return out
      } finally {
        depth--
      }
    }
    let encodedArgs: unknown
    try {
      encodedArgs = encode(args)
    } catch {
      stats.unserializable++
      return fn.apply(this, args)
    }
    const now = Date.now()
    trace = new Set()
    depth = 1
    let out: unknown
    let threw: unknown = null
    let second: unknown
    let secondThrew = false
    try {
      try {
        out = fn.apply(this, args)
      } catch (e) {
        threw = e
      }
      // run it again: anything that answers differently (random, the clock, a cache) is marked nondeterministic
      try {
        second = fn.apply(this, args)
      } catch {
        secondThrew = true
      }
    } finally {
      depth = 0
    }
    const touched = [...trace].sort()
    trace = null
    try {
      const record: Record<string, unknown> = {
        module,
        fn: name,
        args: encodedArgs,
        src: testPath(),
        now,
        trace: touched,
      }
      if (threw) record.throws = String((threw as Error)?.message ?? threw)
      else record.out = encode(out)
      if (Boolean(threw) !== secondThrew || (!threw && !sameJson(out, second))) record.nondet = true
      const line = `${JSON.stringify(record)}\n`
      if (line.length > MAX_RECORD) stats.oversize++
      else {
        buffer.push(line)
        stats.recorded++
        if (buffer.length >= FLUSH_AT) flush()
      }
    } catch {
      stats.unserializable++
    }
    if (threw) throw threw
    return out
  }
  Object.defineProperty(wrapped, 'length', { value: fn.length })
  Object.defineProperty(wrapped, 'name', { value: fn.name })
  return wrapped as unknown as F
}

// WP-SYMBOLIC: a record made outside `wrap`, for calls into mathjs (see symbolic.ts); args come pre-encoded
export function record(module: string, name: string, args: unknown, result: { out?: unknown; throws?: string }): void {
  const rec: Record<string, unknown> = { module, fn: name, args, src: testPath(), now: Date.now(), trace: [] }
  if (result.throws != null) rec.throws = result.throws
  else rec.out = result.out
  const line = `${JSON.stringify(rec)}\n`
  if (line.length > MAX_RECORD) stats.oversize++
  else {
    buffer.push(line)
    stats.recorded++
    if (buffer.length >= FLUSH_AT) flush()
  }
}

function isPlainFunction(v: unknown): v is (...args: never[]) => unknown {
  if (typeof v !== 'function') return false
  return !/^class[\s{]/.test(Function.prototype.toString.call(v))
}

/** The module with every exported plain function wrapped; constants, types and classes pass through. */
export function wrapModule<M extends Record<string, unknown>>(module: string, mod: M): M {
  const out: Record<string, unknown> = { ...mod }
  for (const [k, v] of Object.entries(mod)) {
    if (isPlainFunction(v)) out[k] = wrap(module, k, v)
  }
  return out as M
}

export function recorderStats(): typeof stats {
  return stats
}
