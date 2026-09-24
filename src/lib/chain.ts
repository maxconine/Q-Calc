// an operator typed first continues from the last answer, like pressing × after = on a calculator.
// a leading minus only chains with a space after it: `-3` is negative three, `- 3` is ans minus three
const CHAIN_OP = /^\s*(?:[*×·/÷^+!]|[-−]\s)/
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
