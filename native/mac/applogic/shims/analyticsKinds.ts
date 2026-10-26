// engine/calculus and engine/matrix for the app-logic bundle: only the two checks analytics' calcKind makes, by
// their leading patterns (the engine's own once it registers isCalculusInput / looksLikeMatrix)
import { engine } from './host'

const CALCULUS_START = /^(?:∫|int(?:_|[^A-Za-z\s(]\S*\.\.)|d[²³]?\s*\/\s*d(?:[A-Za-z]|[α-ωΑ-Ω])|∂[²³]?\s*\/\s*∂|lim(?:it)?\b.*(?:->|→)|(?:the\s+)?(?:integral|integrate|(?:partial\s+)?derivative|limit)\s+of\b|[A-Za-z][A-Za-z0-9]*\s*(?:'+|′+|″)\s*\()/i
const TWO_D = /\[\s*\[|\[[^\][]*;[^\][]*\]/
const SPACED_ROW = /\[[^\][,;]*[\d.)A-Za-z]\s+[-+]?[\d.(A-Za-z][^\][,;]*\]/
const MARKS = /\^\s*\(?\s*T\s*\)?(?![A-Za-z0-9_])|ᵀ|⁻¹|[\])]\s*'|[\])]\s+(?:transpose|inverse)\b/i

export const isCalculusInput = (text: string): boolean =>
  engine('engine/calculus', 'isCalculusInput', [text], CALCULUS_START.test(text.trim()))
export const looksLikeMatrix = (expr: string, functions: string[] = []): boolean =>
  engine('engine/matrix', 'looksLikeMatrix', [expr, functions], TWO_D.test(expr) || SPACED_ROW.test(expr) || MARKS.test(expr))
