// when the history tape is already open as the overlay shows
export type HistoryShow = 'recent' | 'always' | 'arrow'

export const DEFAULT_HISTORY_SHOW: HistoryShow = 'recent'

// "recent" means a keystroke or commit within this long before the overlay shows
export const RECENT_USE_MS = 2 * 60 * 1000

export function normalizeHistoryShow(value: unknown): HistoryShow {
  return value === 'always' || value === 'arrow' ? value : DEFAULT_HISTORY_SHOW
}

export function tapeOpensOnShow(mode: HistoryShow, hasHistory: boolean, lastUsedAt: number, now: number): boolean {
  if (!hasHistory || mode === 'arrow') return false
  if (mode === 'always') return true
  return lastUsedAt > 0 && now - lastUsedAt >= 0 && now - lastUsedAt <= RECENT_USE_MS
}
