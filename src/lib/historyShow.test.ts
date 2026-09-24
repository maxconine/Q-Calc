import { describe, expect, it } from 'vitest'
import type { HistoryRow } from './history'
import { normalizeHistoryShow, nextRecentExpiry, RECENT_MS, recentStart } from './historyShow'
import { normalizeHistoryRow } from './history'

const now = 10_000_000
const row = (at?: number): HistoryRow => ({ id: String(Math.random()), expr: '1+1', display: '2', at })

describe('recent calculations above the bar', () => {
  it('shows rows committed in the last two minutes', () => {
    const h = [row(now - RECENT_MS - 5), row(now - RECENT_MS), row(now - 1000)]
    expect(recentStart(h, now)).toBe(1)
  })

  it('old rows and rows without a time are not recent', () => {
    expect(recentStart([row(), row()], now)).toBe(2)
    expect(recentStart([row(now - 1000), row()], now)).toBe(2)
    expect(recentStart([], now)).toBe(0)
  })

  it('is capped at five rows', () => {
    const h = [row(now - 7), row(now - 6), row(now - 5), row(now - 4), row(now - 3), row(now - 2), row(now - 1)]
    expect(recentStart(h, now)).toBe(2)
  })

  it('a clock that went backwards does not count as recent', () => {
    expect(recentStart([row(now + 60_000)], now)).toBe(1)
  })

  it('knows when the next row ages out', () => {
    const h = [row(now - 90_000), row(now - 10_000)]
    expect(nextRecentExpiry(h, now)).toBe(now - 90_000 + RECENT_MS + 1)
    expect(recentStart(h, nextRecentExpiry(h, now)!)).toBe(1)
    expect(nextRecentExpiry([row()], now)).toBeNull()
  })

  it('the commit time survives a save and load', () => {
    expect(normalizeHistoryRow({ expr: '2+3', display: '5', at: 123 }, 'x')?.at).toBe(123)
    expect(normalizeHistoryRow({ expr: '2+3', display: '5', at: Number.NaN }, 'x')?.at).toBeUndefined()
  })

  it('unknown values fall back to recent', () => {
    expect(normalizeHistoryShow('always')).toBe('always')
    expect(normalizeHistoryShow('arrow')).toBe('arrow')
    expect(normalizeHistoryShow('sometimes')).toBe('recent')
    expect(normalizeHistoryShow(undefined)).toBe('recent')
  })
})
