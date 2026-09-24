export type ParenFill = {
  leading: number
  trailing: number
  filled: string
}

/** Count unmatched parens and the grey ( / ) needed to balance them. */
export function inferParens(expr: string): ParenFill {
  let depth = 0
  let minDepth = 0
  for (const ch of expr) {
    if (ch === '(') depth++
    else if (ch === ')') minDepth = Math.min(minDepth, --depth)
  }
  const leading = -minDepth
  const trailing = depth + leading
  return { leading, trailing, filled: `${'('.repeat(leading)}${expr}${')'.repeat(trailing)}` }
}

export function fillParens(expr: string): string {
  return inferParens(expr).filled
}

/** Commit inferred parens when the caret is at the end with no selection. */
export function autofillParens(expr: string, caret: number, selectionEnd = caret): string | null {
  if (caret !== selectionEnd || caret !== expr.length) return null
  const fill = inferParens(expr)
  return fill.leading || fill.trailing ? fill.filled : null
}
