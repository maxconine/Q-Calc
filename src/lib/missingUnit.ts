export type UnitFix = { at: number; text: string; filled: string; hint: string }

// one SI unit per kind of quantity, so at most one of them can make a sum's terms agree
const SI_UNITS = ['V', 'A', 'ohm', 'W', 'J', 'N', 'Pa', 'C', 'F', 'H', 'Hz', 'm', 's', 'kg']
const BARE_NUMERATOR = /(?<![A-Za-z_.\d^])\d+(?:\.\d+)?(?:e[+-]?\d+)?(?=\s*\/)/gi
const QUOTED_UNIT = /\d\s*"[A-Za-zΩµμ°]/

/**
 * `10 mA + 5 / 1 kohm` doesn't add up because the 5 was owed a unit: the number over a quantity that one SI unit
 * would fix, and that unit. `ok` says whether a piece of text has a real answer. It's a guess to offer, never
 * one to act on unasked.
 */
export function missingUnit(expr: string, ok: (text: string) => boolean): UnitFix | null {
  const quoted = QUOTED_UNIT.test(expr)
  for (const m of expr.matchAll(BARE_NUMERATOR)) {
    const at = m.index + m[0].length
    for (const unit of SI_UNITS) {
      const text = quoted ? ` "${unit}"` : ` ${unit}`
      const filled = expr.slice(0, at) + text + expr.slice(at)
      if (ok(filled)) return { at, text, filled, hint: `${m[0]} has no unit · ↵ again makes it ${m[0]} ${unit}` }
    }
  }
  return null
}
