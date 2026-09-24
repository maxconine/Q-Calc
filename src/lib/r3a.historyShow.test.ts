import { describe, expect, it } from 'vitest'
import type { HistoryRow } from './history'
import { nextRecentExpiry, RECENT_MS, RECENT_ROWS, recentStart } from './historyShow'

const now = 10_000_000
const row = (at?: number): HistoryRow => ({ id: String(Math.random()), expr: '1+1', display: '2', at })

describe('recentStart at the exact edges', () => {
  it('a row exactly RECENT_MS old is still recent (the check is <=)', () => {
    expect(recentStart([row(now - RECENT_MS)], now)).toBe(0)
  })

  it('a row one millisecond past RECENT_MS is not', () => {
    expect(recentStart([row(now - RECENT_MS - 1)], now)).toBe(1)
  })

  it('an old row breaks the recent run even with newer rows after it', () => {
    // recentStart only ever trims off the end, so a stale gap earlier in the tape does not
    // stop the newest few rows from counting, as long as they are within the cap
    const h = [row(now - RECENT_MS - 1), row(now - 1000), row(now - 500)]
    expect(recentStart(h, now)).toBe(1)
  })

  it('a row with no timestamp inside an otherwise-recent run still stops it', () => {
    const h = [row(now - 1000), row(), row(now - 500)]
    expect(recentStart(h, now)).toBe(2)
  })
})

describe('nextRecentExpiry across several recent rows', () => {
  it('picks the soonest of several recent rows to age out, not the oldest overall', () => {
    const h = [row(now - RECENT_MS - 5), row(now - 100), row(now - 50)]
    // only the last two are recent; the soonest to expire is the older of those two
    expect(nextRecentExpiry(h, now)).toBe(now - 100 + RECENT_MS + 1)
  })

  it('is null once nothing in the tape is recent', () => {
    expect(nextRecentExpiry([row(now - RECENT_MS - 1)], now)).toBeNull()
    expect(nextRecentExpiry([], now)).toBeNull()
  })
})

describe('the cap holds even when every row is fresh', () => {
  it('never shows more than RECENT_ROWS even if all of them just happened', () => {
    const h = Array.from({ length: 20 }, (_, i) => row(now - i))
    expect(h.length - recentStart(h, now)).toBe(RECENT_ROWS)
  })
})
