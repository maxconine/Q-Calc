import { describe, expect, it } from 'vitest'
import { ellipsize, historyFunctions, historyVariables, MAX_HISTORY, persistableHistory, slimHistoryRow, type HistoryRow } from './history'

function row(partial: Partial<HistoryRow> & { expr?: string; display?: string }): HistoryRow {
  return {
    id: partial.id ?? '1',
    expr: partial.expr ?? '2+2',
    display: partial.display ?? '4',
    exact: partial.exact,
    n: partial.n,
    quantity: partial.quantity,
    kind: partial.kind,
    fnName: partial.fnName,
    fnParams: partial.fnParams,
    fnBody: partial.fnBody,
  }
}

describe('audit2: ellipsize at the zero boundary', () => {
  it('max <= 0 is always the empty string, even for empty input', () => {
    expect(ellipsize('hello', 0)).toBe('')
    expect(ellipsize('hello', -3)).toBe('')
    expect(ellipsize('', 0)).toBe('')
  })
  it('max exactly matching the length keeps the string whole', () => {
    expect(ellipsize('hello', 5)).toBe('hello')
  })
})

describe('audit2: an exact form identical to the display is dropped, not stored twice', () => {
  it('slimHistoryRow clears exact when it equals the (possibly truncated) display', () => {
    const slim = slimHistoryRow(row({ expr: '2+2', display: '4', exact: '4', n: 4 }))
    expect(slim?.exact).toBeUndefined()
  })
  it('keeps a genuinely different exact form', () => {
    const slim = slimHistoryRow(row({ expr: 'cos(30)', display: '0.866025403784', exact: 'sqrt(3)/2', n: Math.sqrt(3) / 2 }))
    expect(slim?.exact).toBe('sqrt(3)/2')
  })
})

describe('audit2: a variable and a function of the same name live in separate sticky namespaces', () => {
  it('redefining the variable x does not evict an older function also called x, and vice versa', () => {
    const rows: HistoryRow[] = [
      row({ id: 'fx', expr: 'x(t) = t^2', display: 'x(t) = t^2', kind: 'function', fnName: 'x', fnParams: ['t'], fnBody: 't^2' }),
      row({ id: 'vx', expr: 'x = 5', display: '5', n: 5 }),
      ...Array.from({ length: MAX_HISTORY }, (_, i) => row({ id: `c${i}`, expr: `${i}+0`, display: `${i}`, n: i })),
    ]
    const saved = persistableHistory(rows)
    // both the old function def and the old variable def survive as sticky rows, since
    // definedName keys them "fn:x" and "var:x" separately
    expect(saved.some((r) => r.id === 'fx')).toBe(true)
    expect(saved.some((r) => r.id === 'vx')).toBe(true)
    expect(historyVariables(saved)).toEqual({ x: 5 })
    expect(historyFunctions(saved)).toEqual({ x: { params: ['t'], body: 't^2' } })
  })
})

describe('audit2: historyFunctions falls back to parsing expr when kind is missing', () => {
  it('a plain f(x) = ... row with no explicit kind is still picked up', () => {
    const rows = [row({ expr: 'double(n) = 2*n', display: 'double(n) = 2*n' })]
    expect(historyFunctions(rows)).toEqual({ double: { params: ['n'], body: '2*n' } })
  })
})
