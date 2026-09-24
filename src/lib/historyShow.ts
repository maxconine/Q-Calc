import type { HistoryRow } from './history'

// recent: the last few calculations sit above the bar on their own; always: the full tape is open; arrow: nothing until ↑
export type HistoryShow = 'recent' | 'always' | 'arrow'

export const DEFAULT_HISTORY_SHOW: HistoryShow = 'recent'

export const RECENT_MS = 5 * 60 * 1000
export const RECENT_ROWS = 5

export function normalizeHistoryShow(value: unknown): HistoryShow {
  return value === 'always' || value === 'arrow' ? value : DEFAULT_HISTORY_SHOW
}

function isRecent(row: HistoryRow, now: number): boolean {
  return row.at != null && row.at <= now && now - row.at <= RECENT_MS
}

// index of the first row in the recent view; history.length when there is none.
// always a tail of the tape, so row indices mean the same thing in both
export function recentStart(history: HistoryRow[], now: number): number {
  let i = history.length
  while (i > 0 && history.length - i < RECENT_ROWS && isRecent(history[i - 1]!, now)) i--
  return i
}

// variables and functions only count from rows you can see: the whole tape when it's always open,
// otherwise the recent rows, even when ↑ has opened the rest
export function scopeStart(history: HistoryRow[], show: HistoryShow, now: number): number {
  return show === 'always' ? 0 : recentStart(history, now)
}

// when the recent view next changes on its own, or null if it won't
export function nextRecentExpiry(history: HistoryRow[], now: number): number | null {
  const ends = history.slice(recentStart(history, now)).map((row) => row.at! + RECENT_MS + 1)
  return ends.length ? Math.min(...ends) : null
}
