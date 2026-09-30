import { describe, expect, it } from 'vitest'
import { afterSend, bump, calcKind, dueToSend, localDay, newInstallId, sanitizeStash, usagePayload, type Stash } from './analytics'

const ID = 'a'.repeat(32)
const empty = (): Stash => ({ id: ID, version: '2.0.3', sentAt: 0, days: {} })

describe('usage stash', () => {
  it('makes 32 hex ids', () => {
    expect(newInstallId()).toMatch(/^[0-9a-f]{32}$/)
    expect(newInstallId()).not.toBe(newInstallId())
  })

  it('reads local days', () => {
    expect(localDay(new Date(2026, 0, 5, 23, 59).getTime())).toBe('2026-01-05')
  })

  it('keeps only known events, whole counts and real days', () => {
    const s = sanitizeStash({
      id: ID,
      version: '2.0.3',
      sentAt: 5,
      days: { '2026-09-29': { open: 3, 'x = 1+1': 1, 'calc.arith': 1.5, copy: 2 }, nope: { open: 1 } },
    })
    expect(s).toEqual({ id: ID, version: '2.0.3', sentAt: 5, days: { '2026-09-29': { open: 3 } } })
    expect(sanitizeStash({ id: 'short' })).toBeNull()
    expect(sanitizeStash(null)).toBeNull()
  })

  it('counts per day and keeps a month', () => {
    let s = empty()
    for (let d = 1; d <= 40; d++) s = bump(s, 'open', `2026-08-${String(d).padStart(2, '0')}`)
    s = bump(s, 'open', '2026-08-40')
    expect(Object.keys(s.days)).toHaveLength(30)
    expect(s.days['2026-08-40']).toEqual({ open: 2 })
  })

  it('sends every six hours when there is something to send', () => {
    const h = 60 * 60 * 1000
    expect(dueToSend(empty(), 10 * h)).toBe(false)
    const s = bump(empty(), 'launch', '2026-09-29')
    expect(dueToSend(s, 10 * h)).toBe(true)
    expect(dueToSend({ ...s, sentAt: 5 * h }, 10 * h)).toBe(false)
  })

  it('drops sent past days but keeps today for its final count', () => {
    let s = bump(bump(empty(), 'open', '2026-09-28'), 'open', '2026-09-29')
    const sent = s.days
    s = bump(s, 'copy.line', '2026-09-29')
    const after = afterSend(s, sent, '2026-09-29', 99)
    expect(after.sentAt).toBe(99)
    expect(after.days).toEqual({ '2026-09-29': { open: 1, 'copy.line': 1 } })
  })

  it('never carries anything typed', () => {
    const body = usagePayload(bump(empty(), 'calc.arith', '2026-09-29'), 'mac', 'macOS 15.5')
    expect(Object.keys(body).sort()).toEqual(['days', 'id', 'os', 'platform', 'version'])
    expect(body.days).toEqual({ '2026-09-29': { 'calc.arith': 1 } })
  })
})

describe('calcKind', () => {
  it('sorts a calculation into one bucket', () => {
    expect(calcKind({ expr: '2+2' })).toBe('calc.arith')
    expect(calcKind({ expr: 'sys', system: true })).toBe('calc.system')
    expect(calcKind({ expr: 'graph sin(x)', graph: true })).toBe('calc.graph')
    expect(calcKind({ expr: 'f(x) = x^2', fn: true })).toBe('calc.function')
    expect(calcKind({ expr: 'x^2 = 4', solve: true })).toBe('calc.solve')
    expect(calcKind({ expr: 'a = 3', assign: true })).toBe('calc.assign')
    expect(calcKind({ expr: 'd/dx x^2' })).toBe('calc.calculus')
    expect(calcKind({ expr: 'int(x, x, 0, 1)' })).toBe('calc.calculus')
    expect(calcKind({ expr: 'det([[1,2],[3,4]])' })).toBe('calc.matrix')
    expect(calcKind({ expr: '5 km to mi', unit: true })).toBe('calc.units')
    expect(calcKind({ expr: '20% of 50', native: true })).toBe('calc.words')
    expect(calcKind({ expr: '*2', chained: true })).toBe('calc.chain')
  })
})
