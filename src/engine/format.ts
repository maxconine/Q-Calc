import type { Value } from './types'

export const DEFAULT_SIG_FIGS = 12
export const MIN_SIG_FIGS = 2
export const MAX_SIG_FIGS = 16

export function clampSigFigs(n: number): number {
  if (!Number.isFinite(n)) return DEFAULT_SIG_FIGS
  return Math.min(MAX_SIG_FIGS, Math.max(MIN_SIG_FIGS, Math.round(n)))
}

function scientific(n: number, sigFigs: number): string {
  return n
    .toExponential(Math.max(0, sigFigs - 1))
    .replace(/(\.\d*?)0+(e[+-]?\d+)$/, '$1$2')
    .replace(/\.e/, 'e')
}

export function formatNumber(n: number, sigFigs = DEFAULT_SIG_FIGS): string {
  if (n === Infinity) return '∞'
  if (n === -Infinity) return '-∞'
  if (!Number.isFinite(n)) return 'undefined'
  if (n === 0) return '0'
  const figs = clampSigFigs(sigFigs)
  const mag = Math.floor(Math.log10(Math.abs(n)))
  if (mag < -6 || mag >= 12) return scientific(n, figs)
  const rounded = Number(n.toPrecision(figs))
  if (!Number.isFinite(rounded)) return scientific(n, figs)
  let s = String(rounded)
  if (s.includes('e')) return scientific(rounded, figs)
  if (s.includes('.')) {
    const decimals = Math.min(100, Math.max(0, figs - mag - 1))
    const frac = s.split('.')[1] ?? ''
    if (frac.length > decimals) s = Number(rounded.toFixed(decimals)).toString()
  }
  return s
}

export function formatValue(value: Value, sigFigs = DEFAULT_SIG_FIGS): string {
  if (value.kind === 'text') return value.text ?? ''
  const n = formatNumber(value.n, sigFigs)
  return value.unit ? `${n} ${value.unit}` : n
}

export function num(n: number): Value {
  return { kind: 'number', n }
}

export function textVal(text: string): Value {
  return { kind: 'text', n: 0, text }
}
