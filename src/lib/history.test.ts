import { describe, expect, it } from 'vitest'
import { evaluateLine } from '../engine/evaluate'
import {
  ellipsize,
  historyFunctions,
  historyMeasures,
  historyQuantities,
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
    quantity: partial.quantity,
    kind: partial.kind,
    fnName: partial.fnName,
    fnParams: partial.fnParams,
    fnBody: partial.fnBody,
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

  it('keeps function metadata and drops n', () => {
    const slim = slimHistoryRow(
      row({
        expr: 'f(x) = x^2',
        display: 'f(x) = x^2',
        n: 99,
        kind: 'function',
        fnName: 'f',
        fnParams: ['x'],
        fnBody: 'x^2',
      }),
    )
    expect(slim).toMatchObject({
      kind: 'function',
      fnName: 'f',
      fnParams: ['x'],
      fnBody: 'x^2',
      display: 'f(x) = x^2',
    })
    expect(slim?.n).toBeUndefined()
  })

  it('keeps the function name when the body is huge', () => {
    const body = `${'x+'.repeat(200)}1`
    const slim = slimHistoryRow(
      row({
        expr: `f(x) = ${body}`,
        display: `f(x) = ${body}`,
        kind: 'function',
        fnName: 'f',
        fnParams: ['x'],
        fnBody: body,
      }),
    )
    expect(slim?.expr.startsWith('f(x) = ')).toBe(true)
    expect(slim?.expr.length).toBeLessThanOrEqual(MAX_HISTORY_EXPR)
    expect(slim?.fnName).toBe('f')
    expect(slim?.fnBody?.length).toBeLessThanOrEqual(MAX_HISTORY_EXPR)
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

  it('preserves function kind and fields on load', () => {
    const loaded = normalizeHistoryRow(
      {
        expr: 'g(a, b) = a + b',
        display: 'g(a, b) = a + b',
        kind: 'function',
        fnName: 'g',
        fnParams: ['a', 'b'],
        fnBody: 'a + b',
      },
      'id',
    )
    expect(loaded).toMatchObject({
      kind: 'function',
      fnName: 'g',
      fnParams: ['a', 'b'],
      fnBody: 'a + b',
    })
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

  it('recovers legacy assignment rows missing n', () => {
    const rows = [
      row({ id: 'a', expr: 'x = 5', display: '5' }),
      row({ id: 'b', expr: 'y = x * 2', display: '10' }),
      row({ id: 'c', expr: 'z = y + 1', display: '' }),
    ]
    expect(historyVariables(rows)).toEqual({ x: 5, y: 10, z: 11 })
  })

  it('skips dictionary rows', () => {
    expect(historyVariables([row({ expr: 'pi', display: 'noun', n: 3, kind: 'definition' })])).toEqual({})
  })

  it('skips function rows when reading variables', () => {
    expect(
      historyVariables([
        row({
          expr: 'f(x) = x^2',
          display: 'f(x) = x^2',
          kind: 'function',
          fnName: 'f',
          fnParams: ['x'],
          fnBody: 'x^2',
        }),
      ]),
    ).toEqual({})
  })

  it('skips definition rows when finding the last number', () => {
    const rows = [
      row({ id: 'a', expr: '2+2', display: '4', n: 4 }),
      row({ id: 'b', expr: 'pi', display: 'noun', n: 3, kind: 'definition' }),
    ]
    expect(lastHistoryNumber(rows)).toBe(4)
  })

  it('skips function rows when finding the last number', () => {
    const rows = [
      row({ id: 'a', expr: '2+2', display: '4', n: 4 }),
      row({
        id: 'b',
        expr: 'f(x) = x',
        display: 'f(x) = x',
        kind: 'function',
        fnName: 'f',
        fnParams: ['x'],
        fnBody: 'x',
        n: 99,
      }),
    ]
    expect(lastHistoryNumber(rows)).toBe(4)
  })
})

describe('historyFunctions', () => {
  it('reads function defs with newest winning', () => {
    const rows = [
      row({
        id: 'a',
        expr: 'f(x) = x',
        display: 'f(x) = x',
        kind: 'function',
        fnName: 'f',
        fnParams: ['x'],
        fnBody: 'x',
      }),
      row({
        id: 'b',
        expr: 'g(a, b) = a + b',
        display: 'g(a, b) = a + b',
        kind: 'function',
        fnName: 'g',
        fnParams: ['a', 'b'],
        fnBody: 'a + b',
      }),
      row({
        id: 'c',
        expr: 'f(x) = x^2',
        display: 'f(x) = x^2',
        kind: 'function',
        fnName: 'f',
        fnParams: ['x'],
        fnBody: 'x^2',
      }),
      row({ id: 'd', expr: 'x = 3', display: '3', n: 3 }),
    ]
    expect(historyFunctions(rows)).toEqual({
      f: { params: ['x'], body: 'x^2' },
      g: { params: ['a', 'b'], body: 'a + b' },
    })
  })

  it('recovers functions from expr when metadata is missing', () => {
    expect(historyFunctions([row({ expr: 'h() = 42', display: 'h() = 42' })])).toEqual({
      h: { params: [], body: '42' },
    })
  })

  it('prefers stored body over a truncated expr', () => {
    const rows = [
      row({
        expr: 'f(x) = x+x+…',
        display: 'f(x) = …',
        kind: 'function',
        fnName: 'f',
        fnParams: ['x'],
        fnBody: 'x + x + 1',
      }),
    ]
    expect(historyFunctions(rows)).toEqual({ f: { params: ['x'], body: 'x + x + 1' } })
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

describe('sticky definitions', () => {
  const calcs = (count: number, from = 0) =>
    Array.from({ length: count }, (_, i) => row({ id: `c${from + i}`, expr: `${from + i}+0`, display: `${from + i}`, n: from + i }))

  it('keeps an assignment older than the newest rows', () => {
    const rows = [row({ id: 'x', expr: 'x = 5', display: '5', n: 5 }), ...calcs(12)]
    const saved = persistableHistory(rows)
    expect(saved).toHaveLength(MAX_HISTORY + 1)
    expect(saved[0]?.expr).toBe('x = 5')
    expect(historyVariables(saved)).toEqual({ x: 5 })
  })

  it('drops the old definition once a recent row redefines it', () => {
    const rows = [row({ id: 'x5', expr: 'x = 5', display: '5', n: 5 }), ...calcs(6), row({ id: 'x7', expr: 'x = 7', display: '7', n: 7 }), ...calcs(6, 6)]
    const saved = persistableHistory(rows)
    expect(saved).toHaveLength(MAX_HISTORY)
    expect(saved.some((r) => r.expr === 'x = 5')).toBe(false)
    expect(historyVariables(saved)).toEqual({ x: 7 })
  })

  it('keeps only the newest older definition of each function', () => {
    const rows = [
      row({ id: 'f1', expr: 'f(x) = x', display: 'f(x) = x', kind: 'function', fnName: 'f', fnParams: ['x'], fnBody: 'x' }),
      row({ id: 'f2', expr: 'f(x) = 2x', display: 'f(x) = 2x', kind: 'function', fnName: 'f', fnParams: ['x'], fnBody: '2x' }),
      ...calcs(MAX_HISTORY),
    ]
    const saved = persistableHistory(rows)
    expect(saved.map((r) => r.id)).toEqual(['f2', ...calcs(MAX_HISTORY).map((r) => r.id)])
    expect(historyFunctions(saved)).toEqual({ f: { params: ['x'], body: '2x' } })
  })
})

describe('measured answers', () => {
  it('keeps sig figs and uncertainty through slimming and storage', () => {
    const r = slimHistoryRow({ id: '1', expr: 'x = 2.50', display: '2.50', n: 2.5, meas: { sig: 3, dp: 2 } })
    expect(r?.meas).toEqual({ sig: 3, dp: 2 })
    const stored = JSON.parse(JSON.stringify([r])) as HistoryRow[]
    expect(normalizeHistoryRow(stored[0]!, 'x')?.meas).toEqual({ sig: 3, dp: 2 })
  })

  it('drops malformed metadata', () => {
    const bad = { id: '1', expr: '1', display: '1', n: 1, meas: { sig: 'x', unc: -1 } } as unknown as HistoryRow
    expect(normalizeHistoryRow(bad, 'a')?.meas).toBeUndefined()
  })

  it('maps variables and the last answer to their metadata, newest wins', () => {
    const rows: HistoryRow[] = [
      { id: '1', expr: 'x = 2.50', display: '2.50', n: 2.5, meas: { sig: 3, dp: 2 } },
      { id: '2', expr: 'y = 10 ± 0.7', display: '10.0 ± 0.7', n: 10, meas: { unc: 0.7 } },
      { id: '3', expr: 'x = 4', display: '4', n: 4 },
      { id: '4', expr: '1.5 * 2', display: '3.0', n: 3, meas: { sig: 2, dp: 1 } },
    ]
    expect(historyMeasures(rows)).toEqual({ y: { unc: 0.7 }, ans: { sig: 2, dp: 1 } })
  })

  it('feeds a later line so x keeps its sig figs', () => {
    const rows: HistoryRow[] = [{ id: '1', expr: 'x = 2.50', display: '2.50', n: 2.5, meas: { sig: 3, dp: 2 } }]
    const r = evaluateLine('x * 3.1', { sigFigMode: true, variables: historyVariables(rows), measures: historyMeasures(rows) })
    expect(r.display).toBe('7.8')
  })
})

describe('unit-valued history answers', () => {
  const rows = [
    row({ id: '1', expr: 'k = 2', display: '2', n: 2 }),
    row({ id: '2', expr: 'd = 5 cm', display: '1.97 in', n: 1.9685, quantity: '1.96850393701 in' }),
    row({ id: '3', expr: '3 m', display: '9.84 ft', n: 9.84, quantity: '9.84251968504 ft' }),
  ]
  it('keeps them out of the numeric variables and ans', () => {
    expect(historyVariables(rows)).toEqual({ k: 2 })
    expect(lastHistoryNumber(rows)).toBeUndefined()
  })
  it('hands them back as quantities, ans included', () => {
    expect(historyQuantities(rows)).toEqual({ d: '1.96850393701 in', ans: '9.84251968504 ft' })
    expect(historyQuantities([...rows, row({ id: '4', expr: 'd = 3', display: '3', n: 3 })])).toEqual({})
  })
  it('survives slimming, and drops a malformed quantity', () => {
    expect(slimHistoryRow(rows[1]!)?.quantity).toBe('1.96850393701 in')
    expect(normalizeHistoryRow({ expr: '3 m', display: '9.84 ft', quantity: 5 as unknown as string }, 'x')?.quantity).toBeUndefined()
  })
})

describe('solved equations in history', () => {
  const solved = (over: Partial<HistoryRow> = {}): HistoryRow => ({
    id: 's',
    expr: 'x = 2x - 5',
    display: '5',
    n: 5,
    solve: { variable: 'x', roots: [5], outcome: 'roots' },
    ...over,
  })

  it('94: never read back as an assignment', () => {
    expect(historyVariables([solved()])).toEqual({})
    expect(historyVariables([{ id: 'a', expr: 'x = 2', display: '2', n: 2 }, solved()])).toEqual({ x: 2 })
  })

  it('95: nor as a measure, quantity or sticky definition', () => {
    const measured: HistoryRow = { id: 'm', expr: 'x = 5.0 ± 0.2', display: '5.0 ± 0.2', n: 5, meas: { unc: 0.2 } }
    const out = historyMeasures([measured, solved()])
    expect(out.x).toEqual({ unc: 0.2 })
    // ans is the solved root now, which carries no uncertainty
    expect(out.ans).toBeUndefined()
    expect(historyQuantities([{ id: 'q', expr: 'x = 3 cm', display: '3 cm', n: 3, quantity: '3 cm' }, solved()])).toEqual({ x: '3 cm' })
    const rows = [solved({ id: 'old' }), ...Array.from({ length: MAX_HISTORY }, (_, i) => ({ id: `r${i}`, expr: `${i}+1`, display: `${i + 1}`, n: i + 1 }))]
    expect(persistableHistory(rows).map((r) => r.id)).not.toContain('old')
  })

  it('96: keeps a valid solve through slimming and drops a malformed one', () => {
    const pair = solved({ expr: 'x^2 = 4', display: '±2', n: undefined, solve: { variable: 'x', roots: [-2, 2], outcome: 'roots' } })
    expect(slimHistoryRow(pair)?.solve).toEqual({ variable: 'x', roots: [-2, 2], outcome: 'roots' })
    expect(normalizeHistoryRow(JSON.parse(JSON.stringify(pair)), 'id')?.solve).toEqual(pair.solve)
    const bad = (solve: unknown) => normalizeHistoryRow({ ...pair, solve } as Partial<HistoryRow>, 'id')?.solve
    expect(bad({ variable: 'xy', roots: [1], outcome: 'roots' })).toBeUndefined()
    expect(bad({ variable: 'x', roots: ['a'], outcome: 'roots' })).toBeUndefined()
    expect(bad({ variable: 'x', roots: [1], outcome: 'maybe' })).toBeUndefined()
    expect(bad({ variable: 'θ', roots: [], outcome: 'none' })).toEqual({ variable: 'θ', roots: [], outcome: 'none' })
  })

  it('gives ans the single root', () => {
    expect(lastHistoryNumber([solved()])).toBe(5)
  })

  it('keeps a system as sys and restores its equations', () => {
    const saved = slimHistoryRow({
      id: 's',
      expr: 'x + y = 5; x - y = 1',
      display: 'x = 3, y = 2',
      kind: 'system',
      equations: ['x + y = 5', 'x - y = 1', ''],
    })
    expect(saved?.expr).toBe('sys')
    expect(saved?.kind).toBe('system')
    expect(saved?.equations).toEqual(['x + y = 5', 'x - y = 1', ''])
    expect(normalizeHistoryRow(JSON.parse(JSON.stringify(saved)), 'id')?.equations).toEqual(saved?.equations)
    expect(lastHistoryNumber([saved!])).toBeUndefined()
  })
})
