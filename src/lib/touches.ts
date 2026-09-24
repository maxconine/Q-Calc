import { hasDualAnswer, visibleAnswer, type AnswerForm } from './answer'

type LineAnswer = {
  display: string
  exact?: string
  solve?: { variable: string; outcome: string }
}

// what ⌘⇧C copies: the line and its answer as they read on screen, `√8 = 2√2 ≈ 2.828`
export function lineCopyText(expr: string, answer: LineAnswer, form: AnswerForm): string {
  const line = expr.trim()
  const shown = answer.exact && hasDualAnswer(answer) ? `${answer.exact} ≈ ${answer.display}` : visibleAnswer(answer, form)
  if (!line || !shown.trim()) return shown.trim()
  if (answer.solve) return `${line}, ${answer.solve.outcome === 'roots' ? `${answer.solve.variable} = ${shown}` : shown}`
  // `x = 5` already says its answer
  const rhs = line.slice(line.lastIndexOf('=') + 1).trim()
  if (line.includes('=') && rhs === shown.trim()) return line
  return `${line} = ${shown}`
}
