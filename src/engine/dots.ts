// every way of writing a dotted variable, as the one form the time derivative reads (`thetadot`, `xddot`):
// LaTeX `\dot\theta` / `\ddot{x}`, Typst `dot(theta)` / `dot.double(x)`, and the dotted letters themselves (θ̇, ẍ)

const MARK_ORDER: Record<string, number> = { '̇': 1, '̈': 2, '⃛': 3 }
const TYPST_ORDER: Record<string, number> = { double: 2, triple: 3 }

// Greek as its ASCII name, so `thetadot` is one word (a θ beside `dot` reads as Typst's multiplication dot)
const GREEK_NAMES: Record<string, string> = { θ: 'theta', φ: 'phi', ψ: 'psi', ω: 'omega', α: 'alpha', β: 'beta', γ: 'gamma' }

function joined(name: string, order: number): string {
  return ` ${GREEK_NAMES[name] ?? name}${'d'.repeat(order)}ot `
}

export function joinDots(text: string): string {
  if (!/dot|[̇̈⃛]|[ẋẏżṙṡẍÿ]/.test(text.normalize('NFD')) && !/[ẋẏżṙṡẍÿ]/.test(text)) return text
  let s = text.replace(/\\(d{1,3})ot\s*(?:\{\s*)?\\?([A-Za-z]+)(?:\s*\})?/g, (_, ds: string, name: string) => joined(name, ds.length))
  s = s.replace(/(?<![A-Za-z])dot(?:\.(double|triple))?\(\s*([A-Za-zθφψωαβγ]+)\s*\)/g, (_, kind: string | undefined, name: string) =>
    joined(name, kind ? TYPST_ORDER[kind]! : 1),
  )
  // a letter with a combining dot or two, precomposed (ẋ) or not (θ + U+0307)
  let out = ''
  const chars = [...s.normalize('NFD')]
  const letter = (c: string | undefined) => c != null && /\p{L}/u.test(c)
  for (let i = 0; i < chars.length; i++) {
    const ch = chars[i]!
    const before = chars[i - 1]
    let order = 0
    let marks = ''
    while (chars[i + 1] && MARK_ORDER[chars[i + 1]!]) {
      marks += chars[++i]!
      order += MARK_ORDER[chars[i]!]!
    }
    const umlaut = marks.includes('\u0308')
    // ö in Ångström and ï in naïve are letters of a word, not a second derivative; a lone ẍ or θ̈ is
    const inWord = umlaut && /[aeiouy]/i.test(ch) && (letter(before) || letter(chars[i + 1]))
    out += order && !inWord && /[A-Za-zα-ωΑ-Ω]/.test(ch) ? joined(ch, Math.min(order, 3)) : ch + marks
  }
  // the rest back as it was typed (Å and é stay single characters)
  return out.normalize('NFC')
}
