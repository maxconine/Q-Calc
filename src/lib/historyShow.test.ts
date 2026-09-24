import { describe, expect, it } from 'vitest'
import { normalizeHistoryShow, RECENT_USE_MS, tapeOpensOnShow } from './historyShow'

const now = 10_000_000

describe('history tape on show', () => {
  it('opens when used within two minutes', () => {
    expect(tapeOpensOnShow('recent', true, now - 5_000, now)).toBe(true)
    expect(tapeOpensOnShow('recent', true, now - RECENT_USE_MS, now)).toBe(true)
  })

  it('stays closed after two minutes, or before any use this run', () => {
    expect(tapeOpensOnShow('recent', true, now - RECENT_USE_MS - 1, now)).toBe(false)
    expect(tapeOpensOnShow('recent', true, 0, now)).toBe(false)
  })

  it('a clock that went backwards does not count as recent', () => {
    expect(tapeOpensOnShow('recent', true, now + 60_000, now)).toBe(false)
  })

  it('never opens with no history', () => {
    for (const mode of ['recent', 'always', 'arrow'] as const) {
      expect(tapeOpensOnShow(mode, false, now, now)).toBe(false)
    }
  })

  it('always and only on ↑ ignore the clock', () => {
    expect(tapeOpensOnShow('always', true, 0, now)).toBe(true)
    expect(tapeOpensOnShow('arrow', true, now, now)).toBe(false)
  })

  it('unknown values fall back to recent', () => {
    expect(normalizeHistoryShow('always')).toBe('always')
    expect(normalizeHistoryShow('arrow')).toBe('arrow')
    expect(normalizeHistoryShow('sometimes')).toBe('recent')
    expect(normalizeHistoryShow(undefined)).toBe('recent')
  })
})
