import { isCalculusInput } from '../engine/calculus'
import {
  IMPROPER_UNIT_CONVERSION,
  isImproperUnitConversion,
} from '../engine/units'
import { nativeWindow } from './bridge'

type NativeEvalRequest = {
  id: number
  expr: string
  ans?: number
  sigFigs?: number
  variables?: Record<string, number>
}

export type NativeEvalReply = {
  id: number
  expr: string
  display: string
  n?: number | null
  kind?: string
  term?: string
  pos?: string
  pronunciation?: string
  body?: string
}

export type NativeLive = {
  expr: string
  display: string
  n?: number
  kind?: 'definition'
  term?: string
  pos?: string
  pronunciation?: string
  body?: string
}

export function hasNativeEval(): boolean {
  const w = nativeWindow()
  if (!w) return false
  return Boolean(w.__QCALC_NATIVE || w.webkit?.messageHandlers?.soulver || w.webkit?.messageHandlers?.qcalc)
}

const DEFINE_PREFIX =
  /^(?:define[:\s]+|definition of\s+|meaning of\s+|what does\s+.+\s+mean\??$)/i
const SINGLE_WORD = /^\p{L}[\p{L}'’-]*$/u

// bare words and `define ...` queries; the dictionary lookup itself is currently switched off
export function looksLikeDictionaryQuery(expr: string): boolean {
  const t = expr.trim()
  if (!t) return false
  if (DEFINE_PREFIX.test(t)) return true
  if (looksLikeNaturalLanguage(t)) return false
  if (!SINGLE_WORD.test(t)) return false
  return t.replace(/[^\p{L}]/gu, '').length >= 2
}

// wkwebview rejects objects containing `undefined`, so optional fields are left out instead
export function nativeEvalPayload(req: NativeEvalRequest): Record<string, unknown> {
  const payload: Record<string, unknown> = {
    type: 'eval',
    id: req.id,
    expr: req.expr,
  }
  if (req.sigFigs != null && Number.isFinite(req.sigFigs)) payload.sigFigs = req.sigFigs
  if (req.ans != null && Number.isFinite(req.ans)) payload.ans = req.ans
  if (req.variables && Object.keys(req.variables).length > 0) payload.variables = req.variables
  return payload
}

export function looksLikeNaturalLanguage(expr: string): boolean {
  const t = expr.trim()
  if (!t || isCalculusInput(t)) return false
  if (/[$€£¥₹]/.test(t)) return true
  if (/\b(?:am|pm)\b/i.test(t)) return true
  if (/\d{1,2}:\d{2}/.test(t)) return true
  if (/%/.test(t) && /\b(?:of|off|on|what|is)\b/i.test(t)) return true
  return /\b(?:what|what's|whats|tip|lunch|today|tomorrow|yesterday|percent|people|nights|from|until|between|per|ago)\b/i.test(
    t,
  )
}

// soulvercore reports failed conversions as "Error: ..." instead of an empty result
export function usableNativeDisplay(display: string): string {
  const t = display.trim()
  if (!t) return ''
  if (isImproperUnitConversion(t)) return IMPROPER_UNIT_CONVERSION
  if (/^error:\s*incompatible units\b/i.test(t)) return IMPROPER_UNIT_CONVERSION
  if (/^error\b/i.test(t)) return ''
  return t
}

function normalizeReply(raw: unknown, req: NativeEvalRequest): NativeEvalReply | null {
  if (!raw || typeof raw !== 'object') return null
  const o = raw as Record<string, unknown>
  const display = usableNativeDisplay(typeof o.display === 'string' ? o.display : '')
  const expr = typeof o.expr === 'string' ? o.expr : req.expr
  const id = typeof o.id === 'number' && Number.isFinite(o.id) ? o.id : req.id
  const n = typeof o.n === 'number' && Number.isFinite(o.n) ? o.n : null
  const reply: NativeEvalReply = { id, expr, display, n }
  if (typeof o.kind === 'string' && o.kind) reply.kind = o.kind
  if (typeof o.term === 'string' && o.term) reply.term = o.term
  if (typeof o.pos === 'string' && o.pos) reply.pos = o.pos
  if (typeof o.pronunciation === 'string' && o.pronunciation) reply.pronunciation = o.pronunciation
  if (typeof o.body === 'string' && o.body) reply.body = o.body
  return reply
}

// without the reply handler, the answer comes back later through __qcalcNativeResult
export async function evaluateNative(req: NativeEvalRequest): Promise<NativeEvalReply | null> {
  const w = nativeWindow()
  const payload = nativeEvalPayload(req)
  const soulver = w?.webkit?.messageHandlers?.soulver
  if (soulver) {
    try {
      return normalizeReply(await soulver.postMessage(payload), req)
    } catch {
      // fall back to the host handler
    }
  }
  try {
    w?.webkit?.messageHandlers?.qcalc?.postMessage(payload)
  } catch {
    // no native side to ask
  }
  return null
}

const TRIG_CALL = /(?<![A-Za-z])(?:a|arc)?(?:sin|cos|tan|sec|csc|cot)(?![A-Za-z])/i

// soulvercore's trig is always in radians, so in degree mode it must not answer anything with trig in it
export function soulverAngleSafe(expr: string, angleMode: 'deg' | 'rad'): boolean {
  return angleMode === 'rad' || !TRIG_CALL.test(expr)
}

const SUM_LINE = /[Σ∑Π∏]|^\s*\\?(?:sum|prod|product)(?![A-Za-z0-9])/i

export function mergeLiveAnswer(
  expr: string,
  jsDisplay: string,
  jsN: number | undefined,
  native: NativeLive | null,
): { display: string; n?: number } {
  if (!expr.trim()) return { display: '' }
  // soulver doesn't know Σ ranges; a blank from the js side is the honest answer
  if (SUM_LINE.test(expr)) return { display: jsDisplay, n: jsDisplay ? jsN : undefined }
  // soulvercore would read `∫0..1 1/x` as something; a blank calculus answer stays blank
  if (isCalculusInput(expr)) return { display: jsDisplay, n: jsN }
  const nativeDisplay =
    native && native.expr === expr && native.kind !== 'definition' ? usableNativeDisplay(native.display) : ''
  const nativeHit = native && nativeDisplay ? { ...native, display: nativeDisplay } : null
  if (nativeHit && looksLikeNaturalLanguage(expr)) {
    return { display: nativeHit.display, n: nativeHit.n }
  }
  if (jsDisplay) return { display: jsDisplay, n: jsN }
  if (nativeHit) return { display: nativeHit.display, n: nativeHit.n }
  return { display: '' }
}

export function nativeDefinition(native: NativeLive | null, expr: string): NativeLive | null {
  if (!native || native.kind !== 'definition') return null
  if (native.expr !== expr) return null
  return native.display.trim() ? native : null
}

export function nativeReplyToLive(reply: NativeEvalReply, expectedId: number, currentExpr: string): NativeLive | null {
  if (reply.id !== expectedId) return null
  if (reply.expr !== currentExpr) return null
  const display = usableNativeDisplay(typeof reply.display === 'string' ? reply.display : '')
  if (!display) return null
  const n = typeof reply.n === 'number' && Number.isFinite(reply.n) ? reply.n : undefined
  const live: NativeLive = { expr: reply.expr, display, n }
  const isDefinition = reply.kind === 'definition' || Boolean(reply.term || reply.pos || reply.body)
  if (isDefinition) {
    live.kind = 'definition'
    if (reply.term) live.term = reply.term
    if (reply.pos) live.pos = reply.pos
    if (reply.pronunciation) live.pronunciation = reply.pronunciation
    if (reply.body) live.body = reply.body
  }
  return live
}
