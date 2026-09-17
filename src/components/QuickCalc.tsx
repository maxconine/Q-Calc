import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { evaluateSheet } from '../engine/evaluate'
import { clampSigFigs, DEFAULT_SIG_FIGS } from '../engine/format'
import { defaultUnitsEqual, isImproperUnitConversion, sanitizeDefaultUnits, type DefaultUnits } from '../engine/units'
import { applyTheme, normalizeTheme, type Theme } from '../lib/theme'
import { AppearanceSettings } from './AppearanceSettings'
import { HistoryInsertSettings } from './HistoryInsertSettings'
import { UnitSettings } from './UnitSettings'
import {
  hasDualAnswer,
  insertableAnswer,
  insertableHistoryAnswer,
  insertableHistoryReuse,
  normalizeHistoryInsert,
  visibleAnswer,
  type AnswerForm,
  type HistoryInsert,
} from '../lib/answer'
import {
  clampDraftSeconds,
  DEFAULT_DRAFT_SECONDS,
  hideAction,
  shouldRestoreDraft,
} from '../lib/draft'
import { nativeHandler, nativeWindow, type NativeWindow, type StoredDraft } from '../lib/bridge'
import {
  evaluateNative,
  hasNativeEval,
  mergeLiveAnswer,
  // nativeDefinition, // Apple Dictionary — uncomment to restore
  nativeReplyToLive,
  type NativeEvalReply,
  type NativeLive,
} from '../lib/nativeEval'
import { QuickInput, flattenPastedText, type QuickInputHandle } from './QuickInput'

export type AngleMode = 'deg' | 'rad'

type Settings = {
  angleMode: AngleMode
  fractionMode: boolean
  answerForm: AnswerForm
  historyInsert: HistoryInsert
  sigFigs: number
  draftSeconds: number
  defaultUnits: DefaultUnits
  theme: Theme
}

type CalcWindow = NativeWindow & {
  __QCALC_SETTINGS?: Partial<Settings>
  __qcalcApplySettings?: (s: Partial<Settings>) => void
  __qcalcNativeResult?: (reply: NativeEvalReply) => void
}

export type HistoryRow = {
  id: string
  expr: string
  display: string
  exact?: string
  n?: number
  kind?: 'definition'
}

const HISTORY_KEY = 'qcalc-history'
const SETTINGS_KEY = 'qcalc-settings'
const DRAFT_KEY = 'qcalc-draft'
const LEGACY_HISTORY_KEY = 'instant-solver-history'
const LEGACY_SETTINGS_KEY = 'instant-solver-settings'
const LEGACY_DRAFT_KEY = 'instant-solver-draft'
const MAX_HISTORY = 10

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

function defaultSettings(): Settings {
  return {
    angleMode: 'deg',
    fractionMode: false,
    answerForm: 'exact',
    historyInsert: 'expr',
    sigFigs: DEFAULT_SIG_FIGS,
    draftSeconds: DEFAULT_DRAFT_SECONDS,
    defaultUnits: {},
    theme: 'system',
  }
}

function dismissNative(): void {
  nativeHandler()?.postMessage({ type: 'dismiss' })
}

function reportNativeHeight(el: HTMLElement | null): void {
  if (!el) return
  const box = el.getBoundingClientRect()
  const height = Math.ceil(box.height)
  const composer = el.querySelector('.composer')
  const composerBox = composer instanceof HTMLElement ? composer.getBoundingClientRect() : null
  const anchorTop = composerBox ? Math.max(0, Math.round(composerBox.top - box.top)) : 0
  nativeHandler()?.postMessage({ type: 'size', height, anchorTop })
}

function loadHistory(): HistoryRow[] {
  try {
    const raw = readStorage(HISTORY_KEY, LEGACY_HISTORY_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw) as Array<Partial<HistoryRow> & { latex?: string }>
    if (!Array.isArray(parsed)) return []
    return parsed
      .map((row) => ({
        id: typeof row.id === 'string' ? row.id : uid(),
        expr: row.expr ?? row.latex ?? '',
        display: row.display ?? '',
        exact: typeof row.exact === 'string' ? row.exact : undefined,
        n: typeof row.n === 'number' ? row.n : undefined,
        kind: row.kind === 'definition' ? ('definition' as const) : undefined,
      }))
      .filter((row) => row.expr || row.display)
      .slice(-MAX_HISTORY)
  } catch {
    return []
  }
}

function mergeSettings(partial: Partial<Settings> | undefined, base: Settings): Settings {
  return {
    angleMode: partial?.angleMode === 'rad' ? 'rad' : partial?.angleMode === 'deg' ? 'deg' : base.angleMode,
    fractionMode: partial?.fractionMode == null ? base.fractionMode : Boolean(partial.fractionMode),
    answerForm: partial?.answerForm === 'approx' ? 'approx' : partial?.answerForm === 'exact' ? 'exact' : base.answerForm,
    historyInsert: partial?.historyInsert == null ? base.historyInsert : normalizeHistoryInsert(partial.historyInsert),
    sigFigs: partial?.sigFigs == null ? base.sigFigs : clampSigFigs(partial.sigFigs),
    draftSeconds: partial?.draftSeconds == null ? base.draftSeconds : clampDraftSeconds(partial.draftSeconds),
    defaultUnits: partial?.defaultUnits == null ? base.defaultUnits : sanitizeDefaultUnits(partial.defaultUnits),
    theme: partial?.theme == null ? base.theme : normalizeTheme(partial.theme),
  }
}

let memoryDraft: StoredDraft | null = null

function windowDraft(): CalcWindow {
  return (nativeWindow() ?? (window as CalcWindow))
}

function draftFromUnknown(raw: unknown): StoredDraft | null {
  if (!raw || typeof raw !== 'object') return null
  const parsed = raw as Partial<StoredDraft>
  const expr = typeof parsed.expr === 'string' ? parsed.expr : ''
  const savedAt = typeof parsed.savedAt === 'number' ? parsed.savedAt : 0
  if (!expr.trim() || savedAt <= 0) return null
  return { expr, savedAt }
}

function readStoredDraft(draftSeconds: number, now = Date.now()): StoredDraft | null {
  let stored: StoredDraft | null = memoryDraft ?? draftFromUnknown(windowDraft().__QCALC_DRAFT)
  try {
    const raw = readStorage(DRAFT_KEY, LEGACY_DRAFT_KEY)
    stored = draftFromUnknown(raw ? JSON.parse(raw) : null) ?? stored
  } catch {
    /* use memory */
  }
  if (!stored || !shouldRestoreDraft(stored.savedAt, now, draftSeconds)) {
    clearStoredDraft()
    return null
  }
  return stored
}

function writeStoredDraft(expr: string, savedAt = Date.now()): void {
  if (!expr.trim()) {
    clearStoredDraft()
    return
  }
  const draft = { expr, savedAt } satisfies StoredDraft
  memoryDraft = draft
  windowDraft().__QCALC_DRAFT = draft
  try {
    localStorage.setItem(DRAFT_KEY, JSON.stringify(draft))
  } catch {
    /* WKWebView private stores can reject localStorage; memory still restores. */
  }
}

function clearStoredDraft(): void {
  memoryDraft = null
  windowDraft().__QCALC_DRAFT = null
  try {
    localStorage.removeItem(DRAFT_KEY)
  } catch {
    /* ignore */
  }
}

function settingsEqual(a: Settings, b: Settings): boolean {
  return (
    a.angleMode === b.angleMode &&
    a.fractionMode === b.fractionMode &&
    a.answerForm === b.answerForm &&
    a.historyInsert === b.historyInsert &&
    a.sigFigs === b.sigFigs &&
    a.draftSeconds === b.draftSeconds &&
    a.theme === b.theme &&
    defaultUnitsEqual(a.defaultUnits, b.defaultUnits)
  )
}

function loadSettings(): Settings {
  const fallback = defaultSettings()
  let stored = fallback
  try {
    const raw = readStorage(SETTINGS_KEY, LEGACY_SETTINGS_KEY)
    if (raw) stored = mergeSettings(JSON.parse(raw) as Partial<Settings>, fallback)
  } catch {
    stored = fallback
  }
  const injected = windowDraft().__QCALC_SETTINGS
  const next = injected ? mergeSettings(injected, stored) : stored
  applyTheme(next.theme)
  return next
}

function uid(): string {
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

function historyExprTitle(row: HistoryRow): string {
  if (row.kind === 'definition') return 'Insert word at the cursor'
  return 'Insert expression at the cursor'
}

function copyText(text: string): void {
  if (!text) return
  nativeHandler()?.postMessage({ type: 'copy', text })
  void navigator.clipboard.writeText(text).catch(() => {
    const el = document.createElement('textarea')
    el.value = text
    document.body.appendChild(el)
    el.select()
    document.execCommand('copy')
    el.remove()
  })
}

function inputHighlight(el: EventTarget | null): string {
  if (!(el instanceof HTMLInputElement) && !(el instanceof HTMLTextAreaElement)) return ''
  const start = el.selectionStart ?? 0
  const end = el.selectionEnd ?? 0
  return end > start ? el.value.slice(start, end) : ''
}

function highlightedText(): string {
  return (
    inputHighlight(document.querySelector('.quick-plain')) ||
    inputHighlight(document.activeElement) ||
    window.getSelection()?.toString() ||
    ''
  )
}

export function QuickCalcPage() {
  return (
    <div className="quick-app">
      <QuickCalc onClose={dismissNative} embedded />
    </div>
  )
}

export function QuickCalc({ onClose, embedded = false }: { onClose: () => void; embedded?: boolean }) {
  const [history, setHistory] = useState<HistoryRow[]>(loadHistory)
  const [settings, setSettings] = useState<Settings>(loadSettings)
  const [q, setQ] = useState(() => readStoredDraft(loadSettings().draftSeconds)?.expr ?? '')
  const [copied, setCopied] = useState(false)
  const [selected, setSelected] = useState<number | null>(null)
  const [tapeOpen, setTapeOpen] = useState(false)
  const [nativeLive, setNativeLive] = useState<NativeLive | null>(null)
  const mathRef = useRef<QuickInputHandle | null>(null)
  const caretRef = useRef<{ start: number; end: number } | null>(null)
  const rootRef = useRef<HTMLDivElement>(null)
  const tapeRef = useRef<HTMLDivElement>(null)
  const definitionRef = useRef<HTMLDivElement>(null)
  const defLiveRef = useRef<NativeLive | null>(null)
  const copiedTimer = useRef(0)
  const tapeOpenRef = useRef(tapeOpen)
  const historyLenRef = useRef(history.length)
  const qRef = useRef(q)
  const displayRef = useRef('')
  const exactRef = useRef<string | undefined>(undefined)
  const liveNRef = useRef<number | undefined>(undefined)
  const settingsRef = useRef(settings)
  const draftAtRef = useRef(readStoredDraft(settings.draftSeconds)?.savedAt ?? 0)
  const draftTimer = useRef(0)
  const evalIdRef = useRef(0)
  tapeOpenRef.current = tapeOpen
  historyLenRef.current = history.length
  qRef.current = q
  settingsRef.current = settings

  const stopDraftTimer = useCallback(() => {
    window.clearTimeout(draftTimer.current)
    draftTimer.current = 0
  }, [])

  const resetToCalculate = useCallback(() => {
    qRef.current = ''
    draftAtRef.current = 0
    stopDraftTimer()
    clearStoredDraft()
    setQ('')
    setSelected(null)
    setTapeOpen(false)
    setCopied(false)
    setNativeLive(null)
    mathRef.current?.setValue('')
    mathRef.current?.focus()
  }, [stopDraftTimer])

  const sheet = useMemo(() => {
    const lines = [...history.map((h) => h.expr), q]
    return evaluateSheet(lines, {
      angleMode: settings.angleMode,
      fractionMode: settings.fractionMode,
      sigFigs: settings.sigFigs,
      defaultUnits: settings.defaultUnits,
    })
  }, [history, q, settings.angleMode, settings.fractionMode, settings.sigFigs, settings.defaultUnits])

  const live = sheet[sheet.length - 1]
  const jsDisplay = q.trim() ? (live?.display ?? '') : ''
  const jsN = live?.value?.kind === 'number' ? live.value.n : undefined
  const merged = mergeLiveAnswer(q, jsDisplay, jsN, nativeLive)
  const display = merged.display
  const liveN = merged.n
  const liveExact = q.trim() && jsDisplay ? live?.exact : undefined
  const shownLive = visibleAnswer({ display, exact: liveExact }, settings.answerForm)
  // Apple Dictionary — uncomment to restore lookups:
  // const definition = !display ? nativeDefinition(nativeLive, q) : null
  const definition = null as NativeLive | null
  displayRef.current = display
  exactRef.current = liveExact
  liveNRef.current = liveN
  defLiveRef.current = definition

  const nativeVars = useMemo(() => {
    const vars: Record<string, number> = {}
    for (const row of sheet.slice(0, -1)) {
      if (row.variable && row.value?.kind === 'number' && Number.isFinite(row.value.n)) {
        vars[row.variable] = row.value.n
      }
    }
    return vars
  }, [sheet])

  const lastAnswer = useMemo(() => {
    for (let i = history.length - 1; i >= 0; i--) {
      const row = history[i]
      if (!row || row.kind === 'definition') continue
      if (row.n != null && Number.isFinite(row.n)) return row
      if (row.display.trim()) return row
    }
    return undefined
  }, [history])

  const lastAns = lastAnswer?.n
  const ansPlain = lastAnswer ? insertableHistoryAnswer(lastAnswer, settings.answerForm) : undefined

  useEffect(() => {
    localStorage.setItem(HISTORY_KEY, JSON.stringify(history.slice(-MAX_HISTORY)))
  }, [history])

  useEffect(() => {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings))
    nativeHandler()?.postMessage({ type: 'settings', sigFigs: settings.sigFigs })
  }, [settings])

  useEffect(() => {
    applyTheme(settings.theme)
    const mq = window.matchMedia('(prefers-color-scheme: dark)')
    const onChange = () => {
      if (settingsRef.current.theme === 'system') applyTheme('system')
    }
    mq.addEventListener('change', onChange)
    return () => mq.removeEventListener('change', onChange)
  }, [settings.theme])

  useEffect(() => {
    const el = tapeRef.current
    if (!el) return
    if (selected == null) el.scrollTop = el.scrollHeight
    else {
      const row = el.querySelector(`[data-hist="${selected}"]`) as HTMLElement | null
      if (!row) return
      const parentBox = el.getBoundingClientRect()
      const rowBox = row.getBoundingClientRect()
      if (rowBox.top < parentBox.top) el.scrollTop -= parentBox.top - rowBox.top
      else if (rowBox.bottom > parentBox.bottom) el.scrollTop += rowBox.bottom - parentBox.bottom
    }
  }, [history.length, selected, tapeOpen])

  const flashCopied = useCallback(() => {
    setCopied(true)
    window.clearTimeout(copiedTimer.current)
    copiedTimer.current = window.setTimeout(() => setCopied(false), 1200)
  }, [])

  const copyDefinition = useCallback(() => {
    const text = definition?.display
    if (!text) return
    copyText(text)
    flashCopied()
  }, [definition, flashCopied])

  const copyOutput = useCallback(() => {
    const highlighted = mathRef.current?.highlighted() || highlightedText()
    if (highlighted) {
      copyText(highlighted)
      return
    }
    const row = selected != null ? history[selected] : null
    if (row) {
      const text = row.kind === 'definition' ? row.expr : visibleAnswer(row, settings.answerForm)
      if (!text || isImproperUnitConversion(text)) return
      copyText(text)
      flashCopied()
      return
    }
    if (definition?.display) {
      copyDefinition()
      return
    }
    if (!shownLive || isImproperUnitConversion(shownLive)) return
    copyText(shownLive)
    flashCopied()
  }, [copyDefinition, definition, flashCopied, history, selected, settings.answerForm, shownLive])

  const snapshotCaret = useCallback(() => {
    const tracked = mathRef.current?.caret()
    if (tracked) {
      caretRef.current = tracked
      return
    }
    const el = mathRef.current?.element()
    if (!el) return
    const start = el.selectionStart ?? el.value.length
    const end = el.selectionEnd ?? start
    caretRef.current = { start, end }
  }, [])

  const restoreCaret = useCallback(() => {
    const el = mathRef.current?.element()
    const caret = caretRef.current
    if (!el || !caret) return
    const max = el.value.length
    el.focus()
    el.setSelectionRange(Math.min(caret.start, max), Math.min(caret.end, max))
  }, [])

  const insertPlain = useCallback((chunk: string) => {
    if (!chunk) return
    const el = mathRef.current?.element()
    const tracked = mathRef.current?.caret()
    const fromEl =
      el && document.activeElement === el
        ? {
            start: el.selectionStart ?? tracked?.start ?? el.value.length,
            end: el.selectionEnd ?? tracked?.end ?? el.selectionStart ?? el.value.length,
          }
        : undefined
    const at = fromEl ?? tracked ?? caretRef.current ?? undefined
    mathRef.current?.insert(chunk, at)
    const dest = (at?.start ?? 0) + chunk.length
    caretRef.current = { start: dest, end: dest }
    mathRef.current?.focus()
    setSelected(null)
    setTapeOpen(false)
  }, [])

  const insertHistoryExpr = useCallback(
    (index: number) => {
      const row = history[index]
      if (!row?.expr) return
      insertPlain(row.expr)
    },
    [history, insertPlain],
  )

  const insertHistoryAnswer = useCallback(
    (index: number) => {
      const row = history[index]
      if (!row) return
      insertPlain(insertableHistoryReuse(row, settings.answerForm, settings.historyInsert))
    },
    [history, insertPlain, settings.answerForm, settings.historyInsert],
  )

  const copyValue = useCallback(
    (text: string) => {
      if (!text || isImproperUnitConversion(text)) return
      copyText(text)
      flashCopied()
    },
    [flashCopied],
  )

  const copyLive = useCallback(() => {
    copyValue(shownLive)
  }, [copyValue, shownLive])

  const copyLiveExact = useCallback(() => {
    if (!liveExact) return
    copyValue(insertableAnswer(liveExact))
  }, [copyValue, liveExact])

  const copyLiveApprox = useCallback(() => {
    if (!display || isImproperUnitConversion(display)) return
    copyValue(insertableAnswer(display, liveN))
  }, [copyValue, display, liveN])

  const commit = useCallback(() => {
    const expr = qRef.current
    const def = defLiveRef.current
    const shown = displayRef.current || (def ? def.pos || def.term || def.display.split('\n')[0] || '' : '')
    const exact = exactRef.current
    const n = liveNRef.current
    if (!expr.trim() || !shown || isImproperUnitConversion(shown)) return
    setHistory((prev) => {
      const last = prev[prev.length - 1]
      if (last && last.expr === expr && last.display === shown) return prev
      return [
        ...prev,
        {
          id: uid(),
          expr,
          display: shown,
          exact: def ? undefined : exact && exact !== shown ? exact : undefined,
          n: def ? undefined : Number.isFinite(n) ? n : undefined,
          kind: def ? ('definition' as const) : undefined,
        },
      ].slice(-MAX_HISTORY)
    })
    qRef.current = ''
    displayRef.current = ''
    exactRef.current = undefined
    liveNRef.current = undefined
    defLiveRef.current = null
    draftAtRef.current = 0
    stopDraftTimer()
    clearStoredDraft()
    setQ('')
    setNativeLive(null)
    mathRef.current?.setValue('')
    mathRef.current?.focus()
    setSelected(null)
    setTapeOpen(false)
  }, [stopDraftTimer])

  const restoreDraft = useCallback((expr: string, savedAt: number) => {
    stopDraftTimer()
    qRef.current = expr
    draftAtRef.current = savedAt
    setQ(expr)
    mathRef.current?.setValue(expr)
    setSelected(null)
    setTapeOpen(false)
    setCopied(false)
    mathRef.current?.focus()
  }, [stopDraftTimer])

  const onWillHide = useCallback(() => {
    const expr = qRef.current
    const ttl = settingsRef.current.draftSeconds
    const action = hideAction(expr, displayRef.current, ttl)
    if (action === 'commit') {
      commit()
      return
    }
    setSelected(null)
    setTapeOpen(false)
    setCopied(false)
    if (action === 'keep') {
      const savedAt = Date.now()
      draftAtRef.current = savedAt
      writeStoredDraft(expr, savedAt)
      stopDraftTimer()
      draftTimer.current = window.setTimeout(() => {
        if (draftAtRef.current !== savedAt) return
        resetToCalculate()
      }, ttl * 1000)
      return
    }
    resetToCalculate()
  }, [commit, resetToCalculate, stopDraftTimer])

  const onPrepare = useCallback(() => {
    stopDraftTimer()
    const ttl = settingsRef.current.draftSeconds
    const now = Date.now()
    const stored = readStoredDraft(ttl, now)
    const expr = qRef.current
    const draftLive = shouldRestoreDraft(draftAtRef.current, now, ttl)

    if (ttl <= 0) {
      if (expr.trim() || stored) resetToCalculate()
      return
    }

    if (expr.trim()) {
      if (draftLive || stored || !draftAtRef.current) return
      resetToCalculate()
      return
    }

    if (stored) {
      restoreDraft(stored.expr, stored.savedAt)
      return
    }
    resetToCalculate()
  }, [resetToCalculate, restoreDraft, stopDraftTimer])

  const onUp = useCallback((): boolean => {
    if (!history.length) return false
    if (selected == null) snapshotCaret()
    if (!tapeOpen) {
      setTapeOpen(true)
      setSelected(history.length - 1)
      return true
    }
    setSelected((cur) => (cur == null ? history.length - 1 : Math.max(0, cur - 1)))
    return true
  }, [history.length, selected, snapshotCaret, tapeOpen])

  const onDown = useCallback((): boolean => {
    if (!tapeOpen) return false
    if (selected == null || selected >= history.length - 1) {
      setSelected(null)
      restoreCaret()
      return true
    }
    setSelected(selected + 1)
    return true
  }, [history.length, restoreCaret, selected, tapeOpen])

  const onEnter = useCallback(() => {
    if (selected != null) {
      insertHistoryAnswer(selected)
      return
    }
    commit()
  }, [commit, selected, insertHistoryAnswer])

  useEffect(() => {
    if (!caretRef.current) return
    restoreCaret()
    const timers = [0, 40, 120].map((ms) => window.setTimeout(restoreCaret, ms))
    return () => {
      for (const t of timers) window.clearTimeout(t)
    }
  }, [restoreCaret, selected, tapeOpen])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' || e.key === 'Esc') {
        e.preventDefault()
        e.stopPropagation()
        if (embedded) onWillHide()
        else resetToCalculate()
        onClose()
        return
      }
      const ctrlOnly = e.ctrlKey && !e.metaKey && !e.shiftKey && !e.altKey
      const key = e.key.toLowerCase()
      if (ctrlOnly && key === 'd') {
        e.preventDefault()
        e.stopPropagation()
        setSettings((s) => ({ ...s, angleMode: s.angleMode === 'deg' ? 'rad' : 'deg' }))
        return
      }
      if (ctrlOnly && key === 'f') {
        e.preventDefault()
        e.stopPropagation()
        setSettings((s) => ({ ...s, fractionMode: !s.fractionMode }))
        return
      }
      if ((e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey && key === 'c') {
        e.preventDefault()
        e.stopPropagation()
        copyOutput()
      }
    }
    const onCopy = (e: ClipboardEvent) => {
      const highlighted =
        inputHighlight(e.target) || mathRef.current?.highlighted() || highlightedText()
      if (highlighted) {
        e.preventDefault()
        e.clipboardData?.setData('text/plain', highlighted)
        nativeHandler()?.postMessage({ type: 'copy', text: highlighted })
        return
      }
      const row = selected != null ? history[selected] : null
      const text = row
        ? row.kind === 'definition'
          ? row.expr
          : visibleAnswer(row, settings.answerForm)
        : definition?.display || shownLive
      if (!text || isImproperUnitConversion(text)) return
      e.preventDefault()
      e.clipboardData?.setData('text/plain', text)
      nativeHandler()?.postMessage({ type: 'copy', text })
      flashCopied()
    }
    window.addEventListener('keydown', onKey, true)
    window.addEventListener('copy', onCopy, true)
    return () => {
      window.removeEventListener('keydown', onKey, true)
      window.removeEventListener('copy', onCopy, true)
    }
  }, [copyOutput, definition, embedded, flashCopied, history, onClose, onWillHide, resetToCalculate, selected, settings.answerForm, shownLive])

  useEffect(() => () => stopDraftTimer(), [stopDraftTimer])

  useEffect(() => {
    if (!q.trim() || !hasNativeEval()) return
    const id = ++evalIdRef.current
    const expr = q
    let cancelled = false
    void evaluateNative({
      id,
      expr,
      ans: lastAns,
      sigFigs: settings.sigFigs,
      variables: nativeVars,
    }).then((reply) => {
      if (cancelled || evalIdRef.current !== id) return
      if (!reply) return
      setNativeLive(nativeReplyToLive(reply, id, qRef.current))
    })
    return () => {
      cancelled = true
    }
  }, [q, lastAns, settings.sigFigs, nativeVars])

  useEffect(() => {
    const w = windowDraft()
    const size = () => reportNativeHeight(rootRef.current)
    w.__qcalcFocus = () => mathRef.current?.focus()
    w.__qcalcPaste = (text) => {
      if (typeof text !== 'string' || !text) return
      mathRef.current?.insert(flattenPastedText(text))
      mathRef.current?.focus()
    }
    w.__qcalcSize = size
    w.__qcalcWillHide = () => onWillHide()
    w.__qcalcReset = () => {
      onPrepare()
      requestAnimationFrame(size)
    }
    w.__qcalcApplySettings = (partial) => {
      setSettings((prev) => {
        const next = mergeSettings(partial, prev)
        return settingsEqual(next, prev) ? prev : next
      })
    }
    w.__qcalcNativeResult = (reply) => {
      const next = nativeReplyToLive(reply, evalIdRef.current, qRef.current)
      if (next) {
        setNativeLive(next)
        return
      }
      if (reply.id === evalIdRef.current && reply.expr === qRef.current) {
        setNativeLive(null)
      }
    }
    const onSoulver = (event: Event) => {
      const reply = (event as CustomEvent<NativeEvalReply>).detail
      if (!reply) return
      w.__qcalcNativeResult?.(reply)
    }
    window.addEventListener('qcalc-soulver', onSoulver)
    const el = rootRef.current
    size()
    const ro = el ? new ResizeObserver(() => reportNativeHeight(el)) : null
    if (el && ro) ro.observe(el)
    const t1 = window.setTimeout(() => mathRef.current?.focus(), 0)
    const t2 = window.setTimeout(() => mathRef.current?.focus(), 50)
    const t3 = window.setTimeout(() => mathRef.current?.focus(), 120)
    return () => {
      window.removeEventListener('qcalc-soulver', onSoulver)
      window.clearTimeout(t1)
      window.clearTimeout(t2)
      window.clearTimeout(t3)
      ro?.disconnect()
    }
  }, [onPrepare, onWillHide])

  useEffect(() => {
    const root = rootRef.current
    if (!root) return
    const onWheel = (e: WheelEvent) => {
      const tape = tapeRef.current
      const definitionEl = definitionRef.current
      const overTape = Boolean(tape && e.target instanceof Node && tape.contains(e.target))
      const overDefinition = Boolean(
        definitionEl && e.target instanceof Node && definitionEl.contains(e.target),
      )
      if (overDefinition) return
      const open = tapeOpenRef.current
      const hasHistory = historyLenRef.current > 0

      if (e.deltaY < 0) {
        if (!hasHistory) {
          if (!overTape) e.preventDefault()
          return
        }
        if (!open) {
          e.preventDefault()
          const input = mathRef.current?.element()
          if (input) {
            const start = input.selectionStart ?? input.value.length
            caretRef.current = { start, end: input.selectionEnd ?? start }
          }
          setTapeOpen(true)
          return
        }
        if (!overTape) {
          e.preventDefault()
          tape?.scrollBy({ top: e.deltaY })
        }
        return
      }

      if (!open) {
        e.preventDefault()
        return
      }
      if (!tape) {
        e.preventDefault()
        return
      }
      const atBottom = tape.scrollTop + tape.clientHeight >= tape.scrollHeight - 1
      if (atBottom) {
        e.preventDefault()
        setTapeOpen(false)
        setSelected(null)
        const input = mathRef.current?.element()
        const caret = caretRef.current
        if (input && caret) {
          const max = input.value.length
          input.focus()
          input.setSelectionRange(Math.min(caret.start, max), Math.min(caret.end, max))
        }
        return
      }
      if (!overTape) {
        e.preventDefault()
        tape.scrollTop += e.deltaY
      }
    }
    root.addEventListener('wheel', onWheel, { passive: false })
    return () => root.removeEventListener('wheel', onWheel)
  }, [])

  return (
    <div className={embedded ? undefined : 'quick-wrap'}>
    <div
      ref={rootRef}
      className={`spotlight ${embedded ? 'spotlight-embedded' : ''}`}
      onMouseDown={(e) => {
        const t = e.target as HTMLElement
        if (t.closest('input, button, .tape, .definition, .quick-plain, .quick-field, .modes, .unit-settings, .live-dual')) return
        nativeHandler()?.postMessage({ type: 'drag' })
      }}
    >
      {tapeOpen && history.length > 0 ? (
        <div className="tape" ref={tapeRef} aria-label="Calculation history">
          {history.map((row, i) => (
            <div
              className={`tape-row ${selected === i ? 'selected' : ''}`}
              data-hist={i}
              key={row.id}
            >
              <button
                type="button"
                className="tape-q"
                title={historyExprTitle(row)}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => insertHistoryExpr(i)}
              >
                {row.expr}
              </button>
              {hasDualAnswer(row) ? (
                <div className="tape-a-dual" role="group" aria-label="History answer">
                  <button
                    type="button"
                    className="tape-a"
                    title="Insert exact value at the cursor"
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={() => insertPlain(insertableAnswer(row.exact!))}
                  >
                    {row.exact}
                  </button>
                  <span className="tape-eq" aria-hidden>
                    ≈
                  </span>
                  <button
                    type="button"
                    className="tape-a"
                    title="Insert approximation at the cursor"
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={() => insertPlain(insertableAnswer(row.display, row.n))}
                  >
                    {row.display}
                  </button>
                </div>
              ) : (
                <button
                  type="button"
                  className="tape-a"
                  title={
                    row.kind === 'definition'
                      ? 'Insert word at the cursor'
                      : settings.answerForm === 'exact' && row.exact
                        ? 'Insert exact value at the cursor'
                        : 'Insert approximation at the cursor'
                  }
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => {
                    if (row.kind === 'definition') insertHistoryAnswer(i)
                    else insertPlain(insertableHistoryAnswer(row, settings.answerForm))
                  }}
                >
                  {visibleAnswer(row, settings.answerForm)}
                </button>
              )}
            </div>
          ))}
        </div>
      ) : null}

      <div className="composer">
        <div className="modes">
          <button
            type="button"
            className={settings.angleMode === 'deg' ? 'active' : ''}
            title="Degrees · ⌃D"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => setSettings((s) => ({ ...s, angleMode: 'deg' }))}
          >
            deg
          </button>
          <button
            type="button"
            className={settings.angleMode === 'rad' ? 'active' : ''}
            title="Radians · ⌃D"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => setSettings((s) => ({ ...s, angleMode: 'rad' }))}
          >
            rad
          </button>
          <button
            type="button"
            className={settings.fractionMode ? 'active' : ''}
            title="Fraction results · ⌃F"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => setSettings((s) => ({ ...s, fractionMode: !s.fractionMode }))}
          >
            a/b
          </button>
        </div>
        <QuickInput
          value={q}
          ansPlain={ansPlain}
          handleRef={mathRef}
          onChange={(text) => {
            caretRef.current = null
            setQ(text)
            if (selected != null && history[selected]?.expr !== text) setSelected(null)
          }}
          onEnter={onEnter}
          onUp={onUp}
          onDown={onDown}
        />
        {copied ? (
          <button type="button" className="live copied" disabled>
            copied
          </button>
        ) : definition ? (
          <button
            type="button"
            className={`live message ${definition.pos ? '' : 'empty'}`}
            title="Copy definition · ⌘C also copies"
            disabled={!definition.display}
            onMouseDown={(e) => e.preventDefault()}
            onClick={copyDefinition}
          >
            {definition.pos ?? ''}
          </button>
        ) : isImproperUnitConversion(display) ? (
          <button type="button" className="live message" disabled>
            {display}
          </button>
        ) : hasDualAnswer({ display, exact: liveExact }) && liveExact ? (
          <div className="live-dual" role="group" aria-label="Answer">
            <button
              type="button"
              className="live live-part"
              title="Copy exact value"
              onMouseDown={(e) => e.preventDefault()}
              onClick={copyLiveExact}
            >
              {liveExact}
            </button>
            <span className="live-eq" aria-hidden>
              ≈
            </span>
            <button
              type="button"
              className="live live-part"
              title="Copy approximation"
              onMouseDown={(e) => e.preventDefault()}
              onClick={copyLiveApprox}
            >
              {display}
            </button>
          </div>
        ) : (
          <button
            type="button"
            className={`live ${shownLive ? '' : 'empty'}`}
            title={shownLive ? 'Copy to clipboard · ⌘C also copies' : undefined}
            disabled={!shownLive}
            onMouseDown={(e) => e.preventDefault()}
            onClick={copyLive}
          >
            {shownLive}
          </button>
        )}
      </div>
      {definition ? (
        <div
          ref={definitionRef}
          className="definition"
          role="region"
          aria-label={definition.term ? `Definition of ${definition.term}` : 'Definition'}
        >
          <div className="definition-head">
            <span className="definition-term">{definition.term || q.trim()}</span>
            {definition.pronunciation ? (
              <span className="definition-pron">{definition.pronunciation}</span>
            ) : null}
          </div>
          <div
            className="definition-body"
            title="Copy definition · ⌘C also copies"
            onClick={() => {
              if (window.getSelection()?.toString()) return
              copyDefinition()
            }}
          >
            {definition.body || definition.display}
          </div>
        </div>
      ) : null}
    </div>
    {!embedded ? (
      <>
        <AppearanceSettings
          value={settings.theme}
          onChange={(theme) => setSettings((s) => ({ ...s, theme }))}
        />
        <HistoryInsertSettings
          value={settings.historyInsert}
          onChange={(historyInsert) => setSettings((s) => ({ ...s, historyInsert }))}
        />
        <UnitSettings
          value={settings.defaultUnits}
          onChange={(defaultUnits) => setSettings((s) => ({ ...s, defaultUnits }))}
        />
      </>
    ) : null}
    </div>
  )
}
