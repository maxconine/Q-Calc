// Seeded generators for the golden corpus. Seeds are fixed so a regenerated corpus only changes when the engine does.

export type Rng = () => number

/** mulberry32: small, fast, and the same on every machine. */
export function rng(seed: number): Rng {
  let a = seed >>> 0
  return () => {
    a = (a + 0x6d2b79f5) >>> 0
    let t = a
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

export function pick<T>(r: Rng, xs: readonly T[]): T {
  return xs[Math.floor(r() * xs.length)]!
}

export function int(r: Rng, lo: number, hi: number): number {
  return lo + Math.floor(r() * (hi - lo + 1))
}

/** A number as a person types it: integers, decimals, a leading point, e-notation, the odd big one. */
export function typedNumber(r: Rng): string {
  const k = r()
  if (k < 0.35) return String(int(r, 0, 12))
  if (k < 0.55) return String(int(r, 13, 9999))
  if (k < 0.7) return `${int(r, 0, 99)}.${String(int(r, 0, 999)).padStart(int(r, 1, 3), '0')}`
  if (k < 0.75) return `.${int(r, 1, 99)}`
  if (k < 0.82) return `${int(r, 1, 9)}${r() < 0.5 ? '.' + int(r, 0, 99) : ''}e${r() < 0.4 ? '-' : ''}${int(r, 0, 30)}`
  if (k < 0.86) return String(int(r, 1, 9) * 10 ** int(r, 6, 18))
  if (k < 0.9) return `0.${'0'.repeat(int(r, 1, 8))}${int(r, 1, 999)}`
  if (k < 0.94) return pick(r, ['0.1', '0.2', '0.3', '0.7', '1.1', '2.675', '1.005', '0.29', '4.35', '9.995'])
  return String(int(r, 1, 9) * 111111)
}

const BINARY = ['+', '-', '*', '/', '^', '+', '-', '*', '/']
const SPACED = [' + ', ' - ', ' * ', ' / ', '+', '-', '*', '/']

/** A random arithmetic expression over typed numbers: + - * / ^, unary minus, brackets, factorial, percent. */
export function arithmetic(r: Rng, depth = 0): string {
  const k = r()
  if (depth > 2 || k < 0.3) {
    const n = typedNumber(r)
    const t = r()
    if (t < 0.05 && Number(n) <= 20 && /^\d+$/.test(n)) return `${n}!`
    if (t < 0.08) return `${n}%`
    return n
  }
  if (k < 0.4) return `(${arithmetic(r, depth + 1)})`
  if (k < 0.45) return `-${arithmetic(r, depth + 1)}`
  if (k < 0.5) return `${typedNumber(r)}(${arithmetic(r, depth + 1)})`
  const op = r() < 0.7 ? pick(r, BINARY) : pick(r, SPACED)
  const right = op.trim() === '^' ? String(int(r, -3, 8)) : arithmetic(r, depth + 1)
  return `${arithmetic(r, depth + 1)}${op}${right}`
}

/** Doubles across the whole range, with the awkward ones (ties, near-integers, tiny, huge) over-represented. */
export function double(r: Rng): number {
  const k = r()
  const sign = r() < 0.2 ? -1 : 1
  if (k < 0.2) return sign * int(r, 0, 1e6)
  if (k < 0.4) return sign * r() * 10 ** int(r, -12, 16)
  if (k < 0.5) return sign * (int(r, 0, 1e5) + 0.5) / 10 ** int(r, 0, 6)
  if (k < 0.6) return sign * int(r, 1, 999) / int(r, 1, 999)
  if (k < 0.7) return sign * (1 + Number.EPSILON * int(r, -4, 4)) * int(r, 1, 1000)
  if (k < 0.8) return sign * Number(`${int(r, 1, 9)}e${int(r, -320, 308)}`)
  if (k < 0.9) return sign * (0.1 * int(r, 1, 30) + 0.2 * int(r, 0, 5))
  return sign * Math.PI * 10 ** int(r, -8, 14)
}
