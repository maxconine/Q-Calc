import { nativeHandler, nativeWindow, type NativeWindow, type StoredDraft } from './bridge'
import { shouldRestoreDraft } from './draft'
import { newRowId, normalizeHistoryRow, persistableHistory, type HistoryRow } from './history'
import type { NativeEvalReply } from './nativeEval'
import { emptyOnboarding, mergeOnboarding, sanitizeOnboarding, type Onboarding } from './onboarding'
import { defaultSettings, mergeSettings, type NativeInfo, type Settings } from './settings'
import { applyTheme } from './theme'

export type CalcWindow = NativeWindow & {
  __QCALC_SETTINGS?: Partial<Settings> & NativeInfo
  __QCALC_ONBOARDING?: unknown
  __QCALC_FIRST_RUN?: boolean
  __qcalcApplySettings?: (s: Partial<Settings> & NativeInfo) => void
  __qcalcFirstRun?: () => void
  __qcalcShowTips?: () => void
  __qcalcEscape?: () => boolean
  __qcalcNativeResult?: (reply: NativeEvalReply) => void
}

export function calcWindow(): CalcWindow {
  return nativeWindow() ?? (window as CalcWindow)
}

const HISTORY_KEY = 'qcalc-history'
const SETTINGS_KEY = 'qcalc-settings'
const DRAFT_KEY = 'qcalc-draft'
const ONBOARDING_KEY = 'qcalc-onboarding'
const LEGACY_HISTORY_KEY = 'instant-solver-history'
const LEGACY_SETTINGS_KEY = 'instant-solver-settings'
const LEGACY_DRAFT_KEY = 'instant-solver-draft'

function readStorage(key: string, legacy: string): string | null {
  try {
    const cur = localStorage.getItem(key)
    if (cur != null) return cur
    const old = localStorage.getItem(legacy)
    if (old == null) return null
    localStorage.setItem(key, old)
    localStorage.removeItem(legacy)
    return old
  } catch {
    return null
  }
}

export function loadHistory(): HistoryRow[] {
  try {
    const raw = readStorage(HISTORY_KEY, LEGACY_HISTORY_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw) as Array<Partial<HistoryRow> & { latex?: string }>
    if (!Array.isArray(parsed)) return []
    return persistableHistory(
      parsed
        .map((row) => normalizeHistoryRow(row, newRowId()))
        .filter((row): row is HistoryRow => row != null),
    )
  } catch {
    return []
  }
}

export function saveHistory(history: HistoryRow[]): void {
  try {
    localStorage.setItem(HISTORY_KEY, JSON.stringify(persistableHistory(history)))
  } catch {
    // the mac app's non-persistent web store can reject writes
  }
}

export function loadSettings(): Settings {
  const defaults = defaultSettings()
  let saved = defaults
  try {
    const raw = readStorage(SETTINGS_KEY, LEGACY_SETTINGS_KEY)
    if (raw) saved = mergeSettings(JSON.parse(raw) as Partial<Settings>, defaults)
  } catch {
    saved = defaults
  }
  const injected = calcWindow().__QCALC_SETTINGS
  const next = injected ? mergeSettings(injected, saved) : saved
  applyTheme(next.theme)
  return next
}

export function saveSettings(settings: Settings): void {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings))
  } catch {
    // same as history: the mac app's web store can reject writes
  }
}

// localStorage can be rejected in the mac app, so drafts also live in memory and on the window
let memoryDraft: StoredDraft | null = null

function draftFromUnknown(raw: unknown): StoredDraft | null {
  if (!raw || typeof raw !== 'object') return null
  const parsed = raw as Partial<StoredDraft>
  const expr = typeof parsed.expr === 'string' ? parsed.expr : ''
  const savedAt = typeof parsed.savedAt === 'number' ? parsed.savedAt : 0
  if (!expr.trim() || savedAt <= 0) return null
  return { expr, savedAt }
}

export function readStoredDraft(draftSeconds: number, now = Date.now()): StoredDraft | null {
  let stored: StoredDraft | null = memoryDraft ?? draftFromUnknown(calcWindow().__QCALC_DRAFT)
  try {
    const raw = readStorage(DRAFT_KEY, LEGACY_DRAFT_KEY)
    stored = draftFromUnknown(raw ? JSON.parse(raw) : null) ?? stored
  } catch {
    // keep the in-memory copy
  }
  if (!stored || !shouldRestoreDraft(stored.savedAt, now, draftSeconds)) {
    clearStoredDraft()
    return null
  }
  return stored
}

export function writeStoredDraft(expr: string, savedAt = Date.now()): void {
  if (!expr.trim()) {
    clearStoredDraft()
    return
  }
  const draft = { expr, savedAt } satisfies StoredDraft
  memoryDraft = draft
  calcWindow().__QCALC_DRAFT = draft
  try {
    localStorage.setItem(DRAFT_KEY, JSON.stringify(draft))
  } catch {
    // the in-memory copy still restores
  }
}

export function clearStoredDraft(): void {
  memoryDraft = null
  calcWindow().__QCALC_DRAFT = null
  try {
    localStorage.removeItem(DRAFT_KEY)
  } catch {
    // nothing to clear
  }
}

// onboarding also lives on the swift side; the mac app's web storage is wiped every launch
export function loadOnboarding(): Onboarding {
  let stored = emptyOnboarding()
  try {
    const raw = localStorage.getItem(ONBOARDING_KEY)
    if (raw) stored = sanitizeOnboarding(JSON.parse(raw))
  } catch {
    // start fresh
  }
  return mergeOnboarding(stored, sanitizeOnboarding(calcWindow().__QCALC_ONBOARDING))
}

export function saveOnboarding(s: Onboarding): void {
  try {
    localStorage.setItem(ONBOARDING_KEY, JSON.stringify(s))
  } catch {
    // the native copy still keeps it
  }
  nativeHandler()?.postMessage({ type: 'onboarding', ...s })
}
