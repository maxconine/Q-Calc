import { describe, expect, it } from 'vitest'
import {
  ellipsize,
  historyVariables,
  lastHistoryNumber,
  MAX_HISTORY,
  MAX_HISTORY_DISPLAY,
  MAX_HISTORY_EXACT,
  MAX_HISTORY_EXPR,
  MAX_HISTORY_JSON,
  normalizeHistoryRow,
  persistableHistory,
  slimHistoryRow,
  type HistoryRow,
} from './history'

function row(partial: Partial<HistoryRow> & { expr?: string; display?: string }): HistoryRow {
  return {
    id: partial.id ?? '1',
    expr: partial.expr ?? '2+2',
    display: partial.display ?? '4',
    exact: partial.exact,
    n: partial.n,
    kind: partial.kind,
  }
}

describe('ellipsize', () => {
  it('leaves short strings alone', () => {
    expect(ellipsize('cos(30)', 20)).toBe('cos(30)')
  })

  it('caps long strings with an ellipsis', () => {
    expect(ellipsize('abcdefghij', 6)).toBe('abcde…')
    expect(ellipsize('ab', 1)).toBe('…')
  })
})

describe('slimHistoryRow', () => {
  it('keeps ordinary calculations intact', () => {
    const kept = slimHistoryRow(row({ expr: 'sin(90)', display: '1', exact: '1', n: 1 }))
    expect(kept).toMatchObject({ expr: 'sin(90)', display: '1', n: 1 })
  })

  it('truncates oversized expressions and answers', () => {
    const expr = `mean(${Array.from({ length: 400 }, (_, i) => i).join(',')})`
    const display = `[${Array.from({ length: 200 }, (_, i) => i).join(', ')}]`
    const exact = '1'.repeat(200)
    const slim = slimHistoryRow(row({ expr, display, exact, n: 199.5 }))
    expect(slim).not.toBeNull()
    expect(slim!.expr.length).toBeLessThanOrEqual(MAX_HISTORY_EXPR)
    expect(slim!.display.length).toBeLessThanOrEqual(MAX_HISTORY_DISPLAY)
    expect(slim!.exact!.length).toBeLessThanOrEqual(MAX_HISTORY_EXACT)
    expect(slim!.n).toBe(199.5)
    expect(slim!.expr.endsWith('…')).toBe(true)
  })

  it('keeps the assignment name when the right-hand side is huge', () => {
    const slim = slimHistoryRow(row({ expr: `x = ${'1+'.repeat(300)}1`, display: '301', n: 301 }))
    expect(slim?.expr.startsWith('x = ')).toBe(true)
    expect(slim?.expr.length).toBeLessThanOrEqual(MAX_HISTORY_EXPR)
    expect(historyVariables([slim!])).toEqual({ x: 301 })
  })

  it('drops empty rows and non-finite numbers', () => {
    expect(slimHistoryRow(row({ expr: '', display: '' }))).toBeNull()
    expect(slimHistoryRow(row({ expr: '170!', display: 'Infinity', n: Infinity }))?.n).toBeUndefined()
  })
})

describe('normalizeHistoryRow', () => {
  it('migrates legacy latex fields and slims on load', () => {
    const loaded = normalizeHistoryRow({ latex: '2+2', display: '4', n: 4 }, 'id')
    expect(loaded).toMatchObject({ id: 'id', expr: '2+2', display: '4', n: 4 })
  })
})

describe('historyVariables and lastHistoryNumber', () => {
  it('reads numeric assignments without the original expression', () => {
    const rows = [
      row({ id: 'a', expr: 'x = 170!', display: '∞', n: Infinity }),
      row({ id: 'b', expr: 'x = 4', display: '4', n: 4 }),
      row({ id: 'c', expr: 'y = x + 3', display: '7', n: 7 }),
      row({ id: 'd', expr: 'random(80)', display: '[…]' }),
    ]
    expect(historyVariables(rows)).toEqual({ x: 4, y: 7 })
    expect(lastHistoryNumber(rows)).toBe(7)
  })

  it('skips dictionary rows', () => {
    expect(historyVariables([row({ expr: 'pi', display: 'noun', n: 3, kind: 'definition' })])).toEqual({})
  })
})

describe('persistableHistory', () => {
  it('keeps only the newest slimmed rows', () => {
    const rows = Array.from({ length: 15 }, (_, i) => row({ id: String(i), expr: `${i}`, display: `${i}`, n: i }))
    const saved = persistableHistory(rows)
    expect(saved).toHaveLength(MAX_HISTORY)
    expect(saved[0]?.n).toBe(5)
    expect(saved[saved.length - 1]?.n).toBe(14)
  })

  it('stays under the json budget after slimming oversized rows', () => {
    const bulky = Array.from({ length: MAX_HISTORY }, (_, i) =>
      row({
        id: String(i),
        expr: 'e'.repeat(MAX_HISTORY_EXPR * 4),
        display: 'd'.repeat(MAX_HISTORY_DISPLAY * 4),
        exact: 'x'.repeat(400),
        n: i,
      }),
    )
    const saved = persistableHistory(bulky)
    expect(saved).toHaveLength(MAX_HISTORY)
    expect(JSON.stringify(saved).length).toBeLessThanOrEqual(MAX_HISTORY_JSON)
    expect(saved.every((r) => r.expr.length <= MAX_HISTORY_EXPR)).toBe(true)
    expect(saved.every((r) => r.display.length <= MAX_HISTORY_DISPLAY)).toBe(true)
  })
})
