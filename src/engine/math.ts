import { all, create } from 'mathjs'

export const math = create(all, { number: 'number' })

/** GCD of the truncated magnitudes; 1 (not 0) when both are 0, so it is always safe to divide by. */
export function intGcd(a: number, b: number): number {
  a = Math.abs(Math.trunc(a))
  b = Math.abs(Math.trunc(b))
  while (b) {
    const t = b
    b = a % b
    a = t
  }
  return a || 1
}

/** A regex alternation matching any of `names` literally, longest first so `ab` wins over `a`. */
export function namesPattern(names: string[]): string {
  return [...names]
    .sort((a, b) => b.length - a.length)
    .map((n) => n.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
    .join('|')
}
