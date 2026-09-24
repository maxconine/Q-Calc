// shared checks for the audit.*.test.ts files. expected values in those files are worked out by hand
import { expect } from 'vitest'
import { evaluateLine, evaluateSheet } from './evaluate'
import type { EvaluateOptions, LineResult } from './types'

export function line(text: string, opts: EvaluateOptions = {}): LineResult {
  return evaluateLine(text, opts)
}

export function last(lines: string[], opts: EvaluateOptions = {}): LineResult {
  const out = evaluateSheet(lines, opts)
  return out[out.length - 1]!
}

export function shown(text: string, opts: EvaluateOptions = {}): string {
  return evaluateLine(text, opts).display
}

function numberOf(r: LineResult): number | undefined {
  return r.value?.kind === 'number' ? r.value.n : undefined
}

function near(a: number, b: number, rel: number): boolean {
  if (a === b) return true
  return Math.abs(a - b) <= rel * Math.max(1, Math.abs(b))
}

/** The answer is a number within `rel` (relative, or absolute below 1) of `want`. */
export function expectValue(r: LineResult, want: number, rel = 1e-9): void {
  const n = numberOf(r)
  const label = `${r.raw} → "${r.display}" (n=${n}), want ${want}`
  expect(n, label).toBeTypeOf('number')
  expect(near(n!, want, rel), label).toBe(true)
}

export function expectNum(text: string, want: number, opts: EvaluateOptions = {}, rel = 1e-9): void {
  expectValue(line(text, opts), want, rel)
}

/** A number in `unit` (the display's unit label). */
export function expectQty(text: string, want: number, unit: string, opts: EvaluateOptions = {}, rel = 1e-9): void {
  const r = line(text, opts)
  expectValue(r, want, rel)
  expect(r.value?.unit, `${text} → "${r.display}" unit`).toBe(unit)
}

export function expectBlank(text: string, opts: EvaluateOptions = {}): void {
  expect(shown(text, opts), text).toBe('')
}

export function expectUndefined(text: string, opts: EvaluateOptions = {}): void {
  expect(shown(text, opts), text).toBe('undefined')
}

/** Either blank or one of the allowed answers; anything else is a wrong answer. */
export function expectBlankOr(text: string, allowed: string[], opts: EvaluateOptions = {}): void {
  const d = shown(text, opts)
  expect([...allowed, ''], `${text} → "${d}"`).toContain(d)
}
