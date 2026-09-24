import { parseFunctionDef } from '../engine/evaluate'
import { answerAmong } from './answer'

// an operator typed first continues from the last answer, like pressing × after = on a calculator.
// a leading minus never does: `-3` and `- 3` are both negative three
const CHAIN_OP = /^\s*[*×·/÷^+!]/
const CHAIN_CONVERT = /^\s*(?:in|to)\s+\S/i
const PLAIN_UNSIGNED = /^(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$/

export type ChainAnswer = { plain: string; unit: boolean }

export function chainsFromAnswer(q: string, last: ChainAnswer | undefined): boolean {
  if (!last?.plain) return false
  if (CHAIN_OP.test(q)) return true
  return last.unit && CHAIN_CONVERT.test(q)
}

// evaluated through the engine's own `ans`, which keeps full precision and the unit
export function chainedExpr(q: string): string {
  return /^\s*(?:in|to)\b/i.test(q) ? `ans ${q.trimStart()}` : `ans${q}`
}

// what the tape keeps: the answer written out, so the row still reads right once `ans` moves on
export function chainedHistoryExpr(q: string, plain: string): string {
  const rest = q.trimStart()
  if (/^(?:in|to)\b/i.test(rest)) return `${plain} ${rest}`
  const loose = /^[+\-−]/.test(rest)
  const base = loose || PLAIN_UNSIGNED.test(plain) ? plain : `(${plain})`
  return `${base}${rest}`
}

// a typed `ans` is written out in the tape too; a function body keeps it, since it reads ans when called
export function ansWrittenOut(expr: string, plain: string): string {
  if (!/\bans\b/i.test(expr) || parseFunctionDef(expr.trim())) return expr
  return expr.replace(/\bans\b/gi, answerAmong(plain, /^\s*ans\s*$/i.test(expr)))
}
