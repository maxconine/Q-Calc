import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { evaluateSheet, parseFunctionDef } from '../engine/evaluate'
import { clampSigFigs, DEFAULT_SIG_FIGS, formatValue } from '../engine/format'
import { isGraphCommand, parseGraphIntent } from '../engine/graph'
import { hasPlusMinus } from '../engine/measure'
import { defaultUnitsEqual, inLadderUnit, isImproperUnitConversion, sanitizeDefaultUnits, stepPrefix, type DefaultUnits } from '../engine/units'
import type { Value } from '../engine/types'
import { applyTheme, normalizeTheme, type Theme } from '../lib/theme'
import { AppearanceSettings } from './AppearanceSettings'
import { GraphPanel } from './GraphPanel'
import { HistoryInsertSettings } from './HistoryInsertSettings'
import { RationalizeSettings } from './RationalizeSettings'
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
import { SIXTY_SEVEN_SETTLE_MS, sixtySevenGate } from '../lib/sixtySeven'
import {
  evaluateNative,
  hasNativeEval,
  looksLikeNaturalLanguage,
  mergeLiveAnswer,
  // nativeDefinition, // Apple Dictionary — uncomment to restore
  nativeReplyToLive,
  type NativeEvalReply,
  type NativeLive,
} from '../lib/nativeEval'
import { QuickInput, flattenPastedText, prettyTokens, type QuickInputHandle } from './QuickInput'
import {
  advanceRotation,
  afterHelpInput,
  cheatSheet,
  dismissRotation,
  emptyOnboarding,
  EXAMPLE_MS,
  exampleList,
  examplesActive,
  HOTKEY_FAILED_HINT,
  isHelpCommand,
  mergeOnboarding,
  pickHint,
  recordCommit,
  recordOpen,
  ROTATION_OFF,
  rotationItem,
  sanitizeOnboarding,
  startRotation,
  type CommitFacts,
  type Onboarding,
  type Rotation,
} from '../lib/onboarding'
import {
  historyFunctions,
  historyMeasures,
  historyQuantities,
  historyVariables,
  lastHistoryNumber,
  normalizeHistoryRow,
  persistableHistory,
  slimHistoryRow,
  type HistoryRow,
} from '../lib/history'

export type { HistoryRow }
export type AngleMode = 'deg' | 'rad'

type Settings = {
  angleMode: AngleMode
  fractionMode: boolean
  sigFigMode: boolean
  rationalize: boolean
  answerForm: AnswerForm
  historyInsert: HistoryInsert
  sigFigs: number
  draftSeconds: number
  defaultUnits: DefaultUnits
  theme: Theme
}

/** Pushed by the Mac app with settings; never stored by the web view. */
type NativeInfo = { hotkey?: string; hotkeyFailed?: boolean }

type CalcWindow = NativeWindow & {
  __QCALC_SETTINGS?: Partial<Settings> & NativeInfo
  __QCALC_ONBOARDING?: unknown
  __QCALC_FIRST_RUN?: boolean
  __qcalcApplySettings?: (s: Partial<Settings> & NativeInfo) => void
  __qcalcFirstRun?: () => void
  __qcalcShowTips?: () => void
  __qcalcNativeResult?: (reply: NativeEvalReply) => void
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

function defaultSettings(): Settings {
  return {
    angleMode: 'deg',
    fractionMode: false,
    sigFigMode: false,
    rationalize: true,
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
    return persistableHistory(
      parsed
        .map((row) => normalizeHistoryRow(row, uid()))
        .filter((row): row is HistoryRow => row != null),
    )
  } catch {
    return []
  }
}

function mergeSettings(partial: Partial<Settings> | undefined, base: Settings): Settings {
  return {
    angleMode: partial?.angleMode === 'rad' ? 'rad' : partial?.angleMode === 'deg' ? 'deg' : base.angleMode,
    fractionMode: partial?.fractionMode == null ? base.fractionMode : Boolean(partial.fractionMode),
    sigFigMode: partial?.sigFigMode == null ? base.sigFigMode : Boolean(partial.sigFigMode),
    rationalize: partial?.rationalize == null ? base.rationalize : Boolean(partial.rationalize),
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
    a.sigFigMode === b.sigFigMode &&
    a.rationalize === b.rationalize &&
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

function mergeNativeInfo(partial: NativeInfo | undefined, base: Required<NativeInfo>): Required<NativeInfo> {
  return {
    hotkey: typeof partial?.hotkey === 'string' ? partial.hotkey : base.hotkey,
    hotkeyFailed: typeof partial?.hotkeyFailed === 'boolean' ? partial.hotkeyFailed : base.hotkeyFailed,
  }
}

/** The web view's storage is not persistent in the Mac app, so the app keeps a copy and hands it back. */
function loadOnboarding(): Onboarding {
  let stored = emptyOnboarding()
  try {
    const raw = localStorage.getItem(ONBOARDING_KEY)
    if (raw) stored = sanitizeOnboarding(JSON.parse(raw))
  } catch {
    /* start fresh */
  }
  return mergeOnboarding(stored, sanitizeOnboarding(windowDraft().__QCALC_ONBOARDING))
}

function saveOnboarding(s: Onboarding): void {
  try {
    localStorage.setItem(ONBOARDING_KEY, JSON.stringify(s))
  } catch {
    /* the native copy still keeps it */
  }
  nativeHandler()?.postMessage({ type: 'onboarding', ...s })
}

/** Browser page loads count as opens once, even under StrictMode's double mount. */
let pageOpenCounted = false

const MODIFIER_KEYS = new Set(['Shift', 'Meta', 'Control', 'Alt', 'CapsLock', 'Fn'])

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
  const [armsShake, setArmsShake] = useState(false)
  /** Unit picked with ⌥↑/⌥↓ for the live answer; cleared when the answer is committed or reset. */
  const [prefixUnit, setPrefixUnit] = useState<string | null>(null)
  const [nativeInfo, setNativeInfo] = useState(() =>
    mergeNativeInfo(windowDraft().__QCALC_SETTINGS, { hotkey: '', hotkeyFailed: false }),
  )
  const [rotation, setRotation] = useState<Rotation>(ROTATION_OFF)
  const [firstRun, setFirstRun] = useState(false)
  /** One muted line under the composer; cleared by the next keystroke. */
  const [hint, setHint] = useState<string | null>(null)
  const [helpOpen, setHelpOpen] = useState(false)
  const onboardingRef = useRef<Onboarding | null>(null)
  if (!onboardingRef.current) onboardingRef.current = loadOnboarding()
  const nativeInfoRef = useRef(nativeInfo)
  nativeInfoRef.current = nativeInfo
  const commitFactsRef = useRef<Omit<CommitFacts, 'expr'>>({})
  const mathRef = useRef<QuickInputHandle | null>(null)
  const caretRef = useRef<{ start: number; end: number } | null>(null)
  const rootRef = useRef<HTMLDivElement>(null)
  const tapeRef = useRef<HTMLDivElement>(null)
  const definitionRef = useRef<HTMLDivElement>(null)
  const defLiveRef = useRef<NativeLive | null>(null)
  const graphFnRef = useRef<{ name: string; params: string[]; body: string } | null>(null)
  const copiedTimer = useRef(0)
  const armsTimer = useRef(0)
  const sixtySevenArmedRef = useRef(true)
  const steppableRef = useRef<Value | undefined>(undefined)
  const tapeOpenRef = useRef(tapeOpen)
  const historyLenRef = useRef(history.length)
  const qRef = useRef(q)
  const displayRef = useRef('')
  const exactRef = useRef<string | undefined>(undefined)
  const liveNRef = useRef<number | undefined>(undefined)
  const liveMeasRef = useRef<HistoryRow['meas']>(undefined)
  const liveQtyRef = useRef<string | undefined>(undefined)
  const settingsRef = useRef(settings)
  const draftAtRef = useRef(readStoredDraft(settings.draftSeconds)?.savedAt ?? 0)
  const draftTimer = useRef(0)
  const evalIdRef = useRef(0)
  tapeOpenRef.current = tapeOpen
  historyLenRef.current = history.length
  qRef.current = q
  settingsRef.current = settings

  const updateOnboarding = useCallback((step: (s: Onboarding) => Onboarding) => {
    const next = step(onboardingRef.current ?? emptyOnboarding())
    onboardingRef.current = next
    saveOnboarding(next)
  }, [])

  /** Each time the overlay opens: count it, maybe start the examples, and repeat a hotkey failure. */
  const beginShowing = useCallback(() => {
    updateOnboarding(recordOpen)
    setRotation(startRotation(examplesActive(onboardingRef.current ?? emptyOnboarding())))
    setFirstRun(false)
    setHelpOpen(false)
    setHint(nativeInfoRef.current.hotkeyFailed ? HOTKEY_FAILED_HINT : null)
  }, [updateOnboarding])

  const stopDraftTimer = useCallback(() => {
    window.clearTimeout(draftTimer.current)
    draftTimer.current = 0
  }, [])

  const triggerSixtySevenArms = useCallback(() => {
    setArmsShake(false)
    window.clearTimeout(armsTimer.current)
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        setArmsShake(true)
        armsTimer.current = window.setTimeout(() => setArmsShake(false), 3000)
      })
    })
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
    setPrefixUnit(null)
    setHelpOpen(false)
    mathRef.current?.setValue('')
    mathRef.current?.focus()
  }, [stopDraftTimer])

  const clearHistory = useCallback(() => {
    setHistory([])
    setSelected(null)
    setTapeOpen(false)
  }, [])

  const lastAnswer = useMemo(() => {
    for (let i = history.length - 1; i >= 0; i--) {
      const row = history[i]
      if (!row || row.kind === 'definition' || row.kind === 'function') continue
      if (row.n != null && Number.isFinite(row.n)) return row
      if (row.display.trim()) return row
    }
    return undefined
  }, [history])

  const lastAns = lastHistoryNumber(history)
  const ansPlain = lastAnswer
    ? insertableHistoryAnswer(lastAnswer, settings.answerForm, settings.sigFigs)
    : undefined
  const nativeVars = useMemo(() => historyVariables(history), [history])
  const nativeFns = useMemo(() => historyFunctions(history), [history])
  const nativeMeas = useMemo(() => historyMeasures(history), [history])
  const nativeQty = useMemo(() => historyQuantities(history), [history])

  const helpShown = helpOpen || isHelpCommand(q)
  const cheats = useMemo(() => cheatSheet(nativeInfo.hotkey || undefined), [nativeInfo.hotkey])
  const graphCmd = isGraphCommand(q)
  const graphIntent = useMemo(
    () => (graphCmd ? parseGraphIntent(q, { functions: nativeFns }) : null),
    [graphCmd, q, nativeFns],
  )
  const liveFns = useMemo(() => {
    if (!graphIntent?.functionDef) return nativeFns
    const d = graphIntent.functionDef
    return {
      ...nativeFns,
      [d.name]: { params: d.params, body: d.body },
    }
  }, [nativeFns, graphIntent])
  graphFnRef.current = graphIntent?.functionDef ?? null

  const sheet = useMemo(() => {
    if (graphCmd) return []
    return evaluateSheet([q], {
      angleMode: settings.angleMode,
      fractionMode: settings.fractionMode,
      rationalize: settings.rationalize,
      sigFigs: settings.sigFigs,
      sigFigMode: settings.sigFigMode,
      defaultUnits: settings.defaultUnits,
      ans: lastAns,
      variables: nativeVars,
      measures: nativeMeas,
      quantities: nativeQty,
      functions: liveFns,
    })
  }, [q, graphCmd, lastAns, nativeVars, nativeMeas, nativeQty, liveFns, settings.angleMode, settings.fractionMode, settings.rationalize, settings.sigFigs, settings.sigFigMode, settings.defaultUnits])

  const live = sheet[sheet.length - 1]
  const jsDisplay = graphCmd
    ? graphIntent?.label
      ? `graph ${graphIntent.label}`
      : ''
    : q.trim()
      ? (live?.display ?? '')
      : ''
  const jsN = graphCmd ? undefined : live?.value?.kind === 'number' ? live.value.n : undefined
  const merged = mergeLiveAnswer(q, jsDisplay, jsN, graphCmd ? null : nativeLive)
  // ⌥↑/⌥↓ re-expresses the JS answer on its SI prefix ladder; what's shown is what's copied and saved.
  const jsValue = !graphCmd && jsDisplay && merged.display === jsDisplay ? live?.value : undefined
  const stepped = prefixUnit && jsValue ? inLadderUnit(jsValue, prefixUnit) : null
  steppableRef.current = stepped ?? jsValue
  const display = stepped ? formatValue(stepped, settings.sigFigs) : merged.display
  const liveN = stepped ? stepped.n : merged.n
  const liveExact = graphCmd || stepped ? undefined : q.trim() && jsDisplay ? live?.exact : undefined
  const shownLive = visibleAnswer({ display, exact: liveExact }, settings.answerForm)
  // Apple Dictionary — uncomment to restore lookups:
  // const definition = !display ? nativeDefinition(nativeLive, q) : null
  const definition = null as NativeLive | null
  displayRef.current = display
  exactRef.current = liveExact
  liveNRef.current = liveN
  liveMeasRef.current = display && display === jsDisplay ? live?.meas : undefined
  // The unit quantity behind a JS answer (⌥↑-stepped or not), so `ans` and variables keep their unit.
  liveQtyRef.current = jsValue ? live?.quantity : undefined
  commitFactsRef.current = {
    variable: display && display === jsDisplay && live?.kind === 'assignment' ? live.variable : undefined,
    unit: Boolean(steppableRef.current?.unit),
  }

  const examples = useMemo(
    () => exampleList(firstRun && nativeInfo.hotkey ? nativeInfo.hotkey : undefined),
    [firstRun, nativeInfo.hotkey],
  )
  const example = !q && !helpShown ? rotationItem(rotation, examples) : undefined
  const exampleAnswer = useMemo(() => {
    if (!example || example.plain) return ''
    const shown =
      evaluateSheet([prettyTokens(example.expr)], {
        angleMode: settings.angleMode,
        fractionMode: settings.fractionMode,
        rationalize: settings.rationalize,
        sigFigs: settings.sigFigs,
        sigFigMode: settings.sigFigMode,
        defaultUnits: settings.defaultUnits,
      })[0]?.display ?? ''
    return shown && example.note ? `${shown} · ${example.note}` : shown
  }, [example, settings.angleMode, settings.fractionMode, settings.rationalize, settings.sigFigs, settings.sigFigMode, settings.defaultUnits])

  useEffect(() => {
    if (!rotation.on) return
    const t = window.setInterval(() => setRotation(advanceRotation), EXAMPLE_MS)
    return () => window.clearInterval(t)
  }, [rotation.on])
  defLiveRef.current = definition

  // Fire once per answer, after it settles (typing `670` passes through 67 without firing).
  useEffect(() => {
    const t = window.setTimeout(() => {
      const gate = sixtySevenGate(sixtySevenArmedRef.current, liveN)
      sixtySevenArmedRef.current = gate.armed
      if (gate.fire) triggerSixtySevenArms()
    }, SIXTY_SEVEN_SETTLE_MS)
    return () => window.clearTimeout(t)
  }, [liveN, triggerSixtySevenArms])

  useEffect(() => {
    return () => window.clearTimeout(armsTimer.current)
  }, [])

  useEffect(() => {
    try {
      localStorage.setItem(HISTORY_KEY, JSON.stringify(persistableHistory(history)))
    } catch {
      /* WKWebView private stores can reject localStorage. */
    }
  }, [history])

  useEffect(() => {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings))
    nativeHandler()?.postMessage({
      type: 'settings',
      sigFigs: settings.sigFigs,
      sigFigMode: settings.sigFigMode,
      rationalize: settings.rationalize,
    })
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

  const selectGraphX = useCallback(
    (xText: string) => {
      if (!xText) return
      copyText(xText)
      flashCopied()
    },
    [flashCopied],
  )

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
      insertPlain(insertableHistoryReuse(row, settings.answerForm, settings.historyInsert, settings.sigFigs))
    },
    [history, insertPlain, settings.answerForm, settings.historyInsert, settings.sigFigs],
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
    copyValue(insertableAnswer(display, liveN, settings.sigFigs))
  }, [copyValue, display, liveN, settings.sigFigs])

  /** `quiet` is the commit-on-hide path: nobody is looking, so no hint is spent on it. */
  const commit = useCallback((quiet = false) => {
    const expr = qRef.current
    const def = defLiveRef.current
    const graphFn = graphFnRef.current
    const fnDef = graphFn ?? parseFunctionDef(expr.trim())
    const isGraph = isGraphCommand(expr)
    const shown =
      displayRef.current ||
      (fnDef ? `${fnDef.name}(${fnDef.params.join(', ')}) = ${fnDef.body}` : '') ||
      (def ? def.pos || def.term || def.display.split('\n')[0] || '' : '')
    const exact = exactRef.current
    const n = liveNRef.current
    const meas = liveMeasRef.current
    const quantity = liveQtyRef.current
    if (!expr.trim() || !shown || isImproperUnitConversion(shown)) return
    const nextHint = quiet ? null : pickHint(onboardingRef.current?.hints ?? 0, { expr, ...commitFactsRef.current })
    updateOnboarding((s) => ({ ...recordCommit(s), hints: s.hints | (nextHint?.bit ?? 0) }))
    setHint(nextHint?.text ?? null)
    setHelpOpen(false)
    // Enter before the answer settled still gets its one firing.
    const gate = sixtySevenGate(sixtySevenArmedRef.current, n)
    sixtySevenArmedRef.current = gate.armed
    if (gate.fire) triggerSixtySevenArms()
    setHistory((prev) => {
      // Inline `graph f(x)=…` registers the function; plain `graph expr` is not persisted.
      if (isGraph && !fnDef) return prev
      const historyExpr = fnDef && isGraph ? `${fnDef.name}(${fnDef.params.join(', ')}) = ${fnDef.body}` : expr
      const nextRow = slimHistoryRow({
        id: uid(),
        expr: historyExpr,
        display: fnDef ? `${fnDef.name}(${fnDef.params.join(', ')}) = ${fnDef.body}` : shown,
        exact: def || fnDef ? undefined : exact && exact !== shown ? exact : undefined,
        n: def || fnDef ? undefined : Number.isFinite(n) ? n : undefined,
        meas: def || fnDef ? undefined : meas,
        quantity: def || fnDef ? undefined : quantity,
        kind: def ? ('definition' as const) : fnDef ? ('function' as const) : undefined,
        fnName: fnDef?.name,
        fnParams: fnDef?.params,
        fnBody: fnDef?.body,
      })
      if (!nextRow) return prev
      const last = prev[prev.length - 1]
      if (last && last.expr === nextRow.expr && last.display === nextRow.display) return prev
      return persistableHistory([...prev, nextRow])
    })
    qRef.current = ''
    displayRef.current = ''
    exactRef.current = undefined
    liveNRef.current = undefined
    liveMeasRef.current = undefined
    liveQtyRef.current = undefined
    defLiveRef.current = null
    graphFnRef.current = null
    draftAtRef.current = 0
    stopDraftTimer()
    clearStoredDraft()
    setQ('')
    setNativeLive(null)
    setPrefixUnit(null)
    mathRef.current?.setValue('')
    mathRef.current?.focus()
    setSelected(null)
    setTapeOpen(false)
  }, [stopDraftTimer, triggerSixtySevenArms, updateOnboarding])

  const onPrefixStep = useCallback((dir: 1 | -1) => {
    const base = steppableRef.current
    const next = base ? stepPrefix(base, dir) : null
    if (next?.unitId) setPrefixUnit(next.unitId)
  }, [])

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
    setRotation(ROTATION_OFF)
    setHint(null)
    if (isHelpCommand(expr)) {
      resetToCalculate()
      return
    }
    const action = hideAction(expr, displayRef.current, ttl)
    if (action === 'commit') {
      commit(true)
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
    if (helpOpen || isHelpCommand(qRef.current)) {
      setHelpOpen(false)
      if (isHelpCommand(qRef.current)) mathRef.current?.setValue('')
    }
    if (!history.length) return false
    if (selected == null) snapshotCaret()
    if (!tapeOpen) {
      setTapeOpen(true)
      setSelected(history.length - 1)
      return true
    }
    setSelected((cur) => (cur == null ? history.length - 1 : Math.max(0, cur - 1)))
    return true
  }, [helpOpen, history.length, selected, snapshotCaret, tapeOpen])

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
    if (isHelpCommand(qRef.current)) {
      resetToCalculate()
      return
    }
    if (selected != null) {
      insertHistoryAnswer(selected)
      return
    }
    commit()
  }, [commit, resetToCalculate, selected, insertHistoryAnswer])

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
      if (!MODIFIER_KEYS.has(e.key)) {
        setRotation(dismissRotation)
        setHint(null)
      }
      if (e.key === 'Escape' || e.key === 'Esc') {
        e.preventDefault()
        e.stopPropagation()
        setPrefixUnit(null)
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
      if (ctrlOnly && key === 's') {
        e.preventDefault()
        e.stopPropagation()
        setSettings((s) => ({ ...s, sigFigMode: !s.sigFigMode }))
        return
      }
      if (ctrlOnly && key === 'c') {
        e.preventDefault()
        e.stopPropagation()
        clearHistory()
        return
      }
      if (e.metaKey && !e.ctrlKey && !e.shiftKey && !e.altKey && key === 'c') {
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
  }, [clearHistory, copyOutput, definition, embedded, flashCopied, history, onClose, onWillHide, resetToCalculate, selected, settings.answerForm, shownLive])

  useEffect(() => () => stopDraftTimer(), [stopDraftTimer])

  // Warm the engine (mathjs, unit tables) off the first keystroke's critical path.
  useEffect(() => {
    const t = window.setTimeout(() => evaluateSheet(['1+1']), 0)
    return () => window.clearTimeout(t)
  }, [])

  useEffect(() => {
    if (!q.trim() || !hasNativeEval() || isGraphCommand(q) || isHelpCommand(q)) return
    // Plain math is already answered in JS; SoulverCore is only needed for natural language.
    if (jsDisplay && !looksLikeNaturalLanguage(q)) return
    // SoulverCore has no ± (it answers `5 ± 2 * 3 ± 1` with 6); a blank beats that.
    if (hasPlusMinus(q)) return
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
  }, [q, jsDisplay, lastAns, settings.sigFigs, nativeVars])

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
      beginShowing()
      requestAnimationFrame(size)
    }
    w.__qcalcApplySettings = (partial) => {
      setSettings((prev) => {
        const next = mergeSettings(partial, prev)
        return settingsEqual(next, prev) ? prev : next
      })
      setNativeInfo((prev) => {
        const next = mergeNativeInfo(partial, prev)
        return next.hotkey === prev.hotkey && next.hotkeyFailed === prev.hotkeyFailed ? prev : next
      })
    }
    w.__qcalcFirstRun = () => {
      w.__QCALC_FIRST_RUN = false
      setFirstRun(true)
      setRotation(startRotation(true))
    }
    w.__qcalcShowTips = () => {
      setRotation(ROTATION_OFF)
      setHint(null)
      setHelpOpen(true)
    }
    if (w.__QCALC_FIRST_RUN) {
      beginShowing()
      w.__qcalcFirstRun()
    } else if (!w.__QCALC_NATIVE && !pageOpenCounted) {
      pageOpenCounted = true
      beginShowing()
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
  }, [beginShowing, onPrepare, onWillHide])

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
      const overGraph = Boolean(e.target instanceof Element && e.target.closest('.graph'))
      if (overDefinition || overGraph) return
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
      className="spotlight-slot"
      onMouseDown={(e) => {
        const t = e.target as HTMLElement
        if (t.closest('input, button, .tape, .definition, .graph, .quick-plain, .quick-field, .edge-tools, .unit-settings, .live-dual')) return
        nativeHandler()?.postMessage({ type: 'drag' })
      }}
    >
    <div className={`spotlight ${embedded ? 'spotlight-embedded' : ''}`}>
      {helpShown ? (
        <div
          className="tape cheats"
          aria-label="Keyboard shortcuts"
          style={{ gridTemplateRows: `repeat(${Math.ceil(cheats.length / 2)}, auto)` }}
        >
          {cheats.map(([key, label]) => (
            <div className="tape-row cheat-row" key={key}>
              <kbd className="cheat-key">{key}</kbd>
              <span className="cheat-label">{label}</span>
            </div>
          ))}
        </div>
      ) : tapeOpen && history.length > 0 ? (
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
                    onClick={() => insertPlain(insertableAnswer(row.display, row.n, settings.sigFigs))}
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
                    else insertPlain(insertableHistoryAnswer(row, settings.answerForm, settings.sigFigs))
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
        <div className="edge-tools">
          <button
            type="button"
            className="edge-tool edge-angle active"
            aria-keyshortcuts="Control+D"
            aria-label={settings.angleMode === 'deg' ? 'Degrees. Shortcut Control D' : 'Radians. Shortcut Control D'}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() =>
              setSettings((s) => ({ ...s, angleMode: s.angleMode === 'deg' ? 'rad' : 'deg' }))
            }
          >
            {settings.angleMode}
            <span className="edge-key" aria-hidden="true">⌃D</span>
          </button>
          <button
            type="button"
            className={`edge-tool edge-frac ${settings.fractionMode ? 'active' : ''}`}
            aria-keyshortcuts="Control+F"
            aria-pressed={settings.fractionMode}
            aria-label={settings.fractionMode ? 'Fraction results on. Shortcut Control F' : 'Fraction results off. Shortcut Control F'}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => setSettings((s) => ({ ...s, fractionMode: !s.fractionMode }))}
          >
            <span className="edge-frac-mark" aria-hidden="true">
              <span>a</span>
              <span className="edge-frac-bar" />
              <span>b</span>
            </span>
            <span className="edge-key" aria-hidden="true">⌃F</span>
          </button>
          <button
            type="button"
            className={`edge-tool ${settings.sigFigMode ? 'active' : ''}`}
            aria-keyshortcuts="Control+S"
            aria-pressed={settings.sigFigMode}
            aria-label={settings.sigFigMode ? 'Significant figures from input on. Shortcut Control S' : 'Significant figures from input off. Shortcut Control S'}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => setSettings((s) => ({ ...s, sigFigMode: !s.sigFigMode }))}
          >
            sf
            <span className="edge-key" aria-hidden="true">⌃S</span>
          </button>
        </div>
        <button
          type="button"
          className="edge-tool edge-clear"
          aria-keyshortcuts="Control+C"
          aria-label="Clear history and variables. Shortcut Control C"
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => {
            clearHistory()
            mathRef.current?.focus()
          }}
        >
          clear
          <span className="edge-key" aria-hidden="true">⌃C</span>
        </button>
        <QuickInput
          value={q}
          ansPlain={ansPlain}
          handleRef={mathRef}
          example={example ? { text: example.expr, id: rotation.tick } : null}
          onChange={(raw) => {
            const text = afterHelpInput(qRef.current, raw)
            setHelpOpen(false)
            // Typing that arrives without a keydown (dictation, IME, tests) also counts as a keystroke.
            if (text) {
              setRotation(dismissRotation)
              setHint(null)
            }
            caretRef.current = null
            qRef.current = text
            setQ(text)
            if (!text.trim()) setPrefixUnit(null)
            if (selected != null && history[selected]?.expr !== text) setSelected(null)
          }}
          onEnter={onEnter}
          onUp={onUp}
          onDown={onDown}
          onPrefixStep={onPrefixStep}
        />
        {copied ? (
          <button type="button" className="live copied" disabled>
            copied
          </button>
        ) : example ? (
          <span key={rotation.tick} className="live live-example" aria-hidden>
            {exampleAnswer}
          </span>
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
      {hint ? (
        <div className="composer-hint" role="status">
          {hint}
        </div>
      ) : null}
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
      ) : graphCmd ? (
        <GraphPanel
          key={q.trim().toLowerCase()}
          input={q}
          functions={liveFns}
          variables={nativeVars}
          ans={lastAns}
          onSelectX={selectGraphX}
        />
      ) : null}
      <div className={`sixty-seven-arms ${armsShake ? 'shaking' : ''}`} aria-hidden="true">
        {(['left', 'right'] as const).map((side) => (
          <svg key={side} className={`sixty-seven-arm sixty-seven-arm-${side}`} viewBox="0 0 36 52">
            <g
              fill="currentColor"
              stroke="currentColor"
              strokeLinecap="round"
              strokeWidth="4.4"
              transform={side === 'right' ? 'matrix(-1 0 0 1 36 0)' : undefined}
            >
              <rect x="11" y="32" width="14" height="24" rx="5" stroke="none" />
              <rect x="8" y="18" width="20" height="20" rx="7" stroke="none" />
              <path d="M11 22 L9.5 9 M15.5 21 L15 5 M20 21 L20.5 6.5 M24.5 22.5 L26 11 M26 31 L31.5 22" fill="none" />
            </g>
          </svg>
        ))}
      </div>
    </div>
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
        <RationalizeSettings
          value={settings.rationalize}
          onChange={(rationalize) => setSettings((s) => ({ ...s, rationalize }))}
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
