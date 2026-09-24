const FIGURE_SPACE = ' '
const PUNCTUATION_SPACE = ' '
const PLAIN = /^−?[\d,]+(?:\.(\d+))?$/

// pads plain numbers on the right so a column of them lines up on the decimal point;
// null entries (duals, units, scientific) are left alone
export function alignDecimals(answers: Array<string | null>, maxChars = 22): Array<string | null> {
  const fracs = answers.map((a) => (a == null ? null : PLAIN.exec(a)))
  const widest = Math.max(0, ...fracs.map((m) => (m ? (m[1]?.length ?? 0) : 0)))
  if (!widest) return answers
  return answers.map((a, i) => {
    const m = fracs[i]
    if (a == null || !m) return a
    const frac = m[1]?.length ?? 0
    const pad = (frac ? '' : PUNCTUATION_SPACE) + FIGURE_SPACE.repeat(widest - frac)
    return a.length + pad.length > maxChars ? a : a + pad
  })
}
