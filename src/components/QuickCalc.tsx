import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { evaluateSheet, parseFunctionDef } from '../engine/evaluate'
import { formatValue } from '../engine/format'
import { isGraphCommand, parseGraphIntent } from '../engine/graph'
import { hasPlusMinus } from '../engine/measure'
import { inLadderUnit, isImproperUnitConversion, stepPrefix } from '../engine/units'
import type { UserFunction, Value } from '../engine/types'
import { applyTheme } from '../lib/theme'
import { AppearanceSettings } from './AppearanceSettings'
import { EdgeTools } from './EdgeTools'
import { GraphPanel } from './GraphPanel'
import { CheatSheet, HistoryTape } from './HistoryTape'
import { HistoryInsertSettings } from './HistoryInsertSettings'
import { LiveAnswer } from './LiveAnswer'
import { RationalizeSettings } from './RationalizeSettings'
import { SixtySevenArms } from './SixtySevenArms'
import { UnitSettings } from './UnitSettings'
import { useSixtySevenArms } from './useSixtySevenArms'
import { useTapeWheel } from './useTapeWheel'
import {
  insertableAnswer,
  insertableHistoryAnswer,
  insertableHistoryReuse,
  visibleAnswer,
  type AnswerForm,
} from '../lib/answer'
import { hideAction, shouldRestoreDraft } from '../lib/draft'
import { nativeHandler } from '../lib/bridge'
import { copyText, highlightedText, inputHighlight } from '../lib/dom'
import {
  evaluateNative,
  hasNativeEval,
  looksLikeNaturalLanguage,
  mergeLiveAnswer,
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
  pickHint,
  recordCommit,
  recordOpen,
  ROTATION_OFF,
  rotationItem,
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
  newRowId,
  persistableHistory,
  slimHistoryRow,
  type HistoryRow,
} from '../lib/history'
import {
  mergeNativeInfo,
  mergeSettings,
  settingsEqual,
  toggleAngleMode,
  toggleFractionMode,
  toggleSigFigMode,
  type Settings,
} from '../lib/settings'
import {
  calcWindow,
  clearStoredDraft,
  loadHistory,
  loadOnboarding,
  loadSettings,
  readStoredDraft,
  saveHistory,
  saveOnboarding,
  saveSettings,
  writeStoredDraft,
} from '../lib/storage'

type FunctionDef = { name: string; params: string[]; body: string }

// the live answer as commit() needs it, readable from stable callbacks
type LiveSnapshot = {
  display: string
  exact?: string
  n?: number
  meas?: HistoryRow['meas']
  quantity?: string
  fnDef: FunctionDef | null
  facts: Omit<CommitFacts, 'expr'>
}

const EMPTY_LIVE: LiveSnapshot = { display: '', fnDef: null, facts: {} }

const MODIFIER_KEYS = new Set(['Shift', 'Meta', 'Control', 'Alt', 'CapsLock', 'Fn'])

const CTRL_SETTING_KEYS = new Map<string, (s: Settings) => Settings>([
  ['d', toggleAngleMode],
  ['f', toggleFractionMode],
  ['s', toggleSigFigMode],
])

// browser page loads count as one open, even under StrictMode's double mount
let pageOpenCounted = false

function dismissNative(): void {
  nativeHandler()?.postMessage({ type: 'dismiss' })
}

// anchorTop lets the panel grow history upward and graphs downward around the composer
function reportNativeHeight(el: HTMLElement | null): void {
  if (!el) return
  const box = el.getBoundingClientRect()
  const height = Math.ceil(box.height)
  const composer = el.querySelector('.composer')
  const composerBox = composer instanceof HTMLElement ? composer.getBoundingClientRect() : null
  const anchorTop = composerBox ? Math.max(0, Math.round(composerBox.top - box.top)) : 0
  nativeHandler()?.postMessage({ type: 'size', height, anchorTop })
}

function fnDefText(fn: FunctionDef): string {
  return `${fn.name}(${fn.params.join(', ')}) = ${fn.body}`
}

function rowCopyText(row: HistoryRow, answerForm: AnswerForm): string {
  return row.kind === 'definition' ? row.expr : visibleAnswer(row, answerForm)
}

function lastAnswerRow(history: HistoryRow[]): HistoryRow | undefined {
  for (let i = history.length - 1; i >= 0; i--) {
    const row = history[i]
    if (!row || row.kind === 'definition' || row.kind === 'function') continue
    if ((row.n != null && Number.isFinite(row.n)) || row.display.trim()) return row
  }
  return undefined
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
  // unit picked with ⌥↑/⌥↓ for the live answer
  const [prefixUnit, setPrefixUnit] = useState<string | null>(null)
  const [nativeInfo, setNativeInfo] = useState(() =>
    mergeNativeInfo(calcWindow().__QCALC_SETTINGS, { hotkey: '', hotkeyFailed: false }),
  )
  const [rotation, setRotation] = useState<Rotation>(ROTATION_OFF)
  const [firstRun, setFirstRun] = useState(false)
  // one muted line under the composer, cleared by the next keystroke
  const [hint, setHint] = useState<string | null>(null)
  const [helpOpen, setHelpOpen] = useState(false)

  const onboardingRef = useRef<Onboarding | null>(null)
  if (!onboardingRef.current) onboardingRef.current = loadOnboarding()
  const nativeInfoRef = useRef(nativeInfo)
  nativeInfoRef.current = nativeInfo
  const mathRef = useRef<QuickInputHandle | null>(null)
  const caretRef = useRef<{ start: number; end: number } | null>(null)
  const rootRef = useRef<HTMLDivElement>(null)
  const tapeRef = useRef<HTMLDivElement>(null)
  const liveRef = useRef<LiveSnapshot>(EMPTY_LIVE)
  const steppableRef = useRef<Value | undefined>(undefined)
  const copiedTimer = useRef(0)
  const qRef = useRef(q)
  qRef.current = q
  const settingsRef = useRef(settings)
  settingsRef.current = settings
  const draftAtRef = useRef(readStoredDraft(settings.draftSeconds)?.savedAt ?? 0)
  const draftTimer = useRef(0)
  const evalIdRef = useRef(0)

  const updateOnboarding = useCallback((step: (s: Onboarding) => Onboarding) => {
    const next = step(onboardingRef.current ?? emptyOnboarding())
    onboardingRef.current = next
    saveOnboarding(next)
  }, [])

  // each time the overlay opens: count it, maybe start the examples, and repeat a hotkey failure
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

  const lastAnswer = useMemo(() => lastAnswerRow(history), [history])
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
  const liveFns = useMemo((): Record<string, UserFunction> => {
    const d = graphIntent?.functionDef
    return d ? { ...nativeFns, [d.name]: { params: d.params, body: d.body } } : nativeFns
  }, [nativeFns, graphIntent])

  const { angleMode, fractionMode, rationalize, sigFigs, sigFigMode, defaultUnits } = settings
  const evalSettings = useMemo(
    () => ({ angleMode, fractionMode, rationalize, sigFigs, sigFigMode, defaultUnits }),
    [angleMode, fractionMode, rationalize, sigFigs, sigFigMode, defaultUnits],
  )

  const sheet = useMemo(() => {
    if (graphCmd) return []
    return evaluateSheet([q], {
      ...evalSettings,
      ans: lastAns,
      variables: nativeVars,
      measures: nativeMeas,
      quantities: nativeQty,
      functions: liveFns,
    })
  }, [q, graphCmd, lastAns, nativeVars, nativeMeas, nativeQty, liveFns, evalSettings])

  const live = sheet[sheet.length - 1]
  let jsDisplay = ''
  if (graphCmd) jsDisplay = graphIntent?.label ? `graph ${graphIntent.label}` : ''
  else if (q.trim()) jsDisplay = live?.display ?? ''
  const jsN = graphCmd ? undefined : live?.value?.kind === 'number' ? live.value.n : undefined
  const merged = mergeLiveAnswer(q, jsDisplay, jsN, graphCmd ? null : nativeLive)
  // ⌥↑/⌥↓ re-expresses the js answer on its si prefix ladder; what's shown is what's copied and saved
  const jsValue = !graphCmd && jsDisplay && merged.display === jsDisplay ? live?.value : undefined
  const stepped = prefixUnit && jsValue ? inLadderUnit(jsValue, prefixUnit) : null
  steppableRef.current = stepped ?? jsValue
  const display = stepped ? formatValue(stepped, settings.sigFigs) : merged.display
  const liveN = stepped ? stepped.n : merged.n
  const liveExact = graphCmd || stepped ? undefined : q.trim() && jsDisplay ? live?.exact : undefined
  const shownLive = visibleAnswer({ display, exact: liveExact }, settings.answerForm)
  const fromJs = Boolean(display) && display === jsDisplay
  liveRef.current = {
    display,
    exact: liveExact,
    n: liveN,
    meas: fromJs ? live?.meas : undefined,
    quantity: jsValue ? live?.quantity : undefined,
    fnDef: graphIntent?.functionDef ?? null,
    facts: {
      variable: fromJs && live?.kind === 'assignment' ? live.variable : undefined,
      unit: Boolean(steppableRef.current?.unit),
    },
  }

  const examples = useMemo(
    () => exampleList(firstRun && nativeInfo.hotkey ? nativeInfo.hotkey : undefined),
    [firstRun, nativeInfo.hotkey],
  )
  const example = !q && !helpShown ? rotationItem(rotation, examples) : undefined
  const exampleAnswer = useMemo(() => {
    if (!example || example.plain) return ''
    const shown = evaluateSheet([prettyTokens(example.expr)], evalSettings)[0]?.display ?? ''
    return shown && example.note ? `${shown} · ${example.note}` : shown
  }, [example, evalSettings])

  const { shaking: armsShaking, settle: settleArms } = useSixtySevenArms(liveN)

  useEffect(() => {
    if (!rotation.on) return
    const t = window.setInterval(() => setRotation(advanceRotation), EXAMPLE_MS)
    return () => window.clearInterval(t)
  }, [rotation.on])

  useEffect(() => saveHistory(history), [history])

  useEffect(() => {
    saveSettings(settings)
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
    if (selected == null) {
      el.scrollTop = el.scrollHeight
      return
    }
    const row = el.querySelector(`[data-hist="${selected}"]`) as HTMLElement | null
    if (!row) return
    const parentBox = el.getBoundingClientRect()
    const rowBox = row.getBoundingClientRect()
    if (rowBox.top < parentBox.top) el.scrollTop -= parentBox.top - rowBox.top
    else if (rowBox.bottom > parentBox.bottom) el.scrollTop += rowBox.bottom - parentBox.bottom
  }, [history.length, selected, tapeOpen])

  const flashCopied = useCallback(() => {
    setCopied(true)
    window.clearTimeout(copiedTimer.current)
    copiedTimer.current = window.setTimeout(() => setCopied(false), 1200)
  }, [])

  const copyValue = useCallback(
    (text: string) => {
      if (!text || isImproperUnitConversion(text)) return
      copyText(text)
      flashCopied()
    },
    [flashCopied],
  )

  const copyOutput = useCallback(() => {
    const highlighted = mathRef.current?.highlighted() || highlightedText()
    if (highlighted) {
      copyText(highlighted)
      return
    }
    const row = selected != null ? history[selected] : null
    copyValue(row ? rowCopyText(row, settings.answerForm) : shownLive)
  }, [copyValue, history, selected, settings.answerForm, shownLive])

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
      if (row?.expr) insertPlain(row.expr)
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

  // `quiet` is the commit-on-hide path: nobody is looking, so no hint is spent on it
  const commit = useCallback((quiet = false) => {
    const expr = qRef.current
    const { display: liveDisplay, exact, n, meas, quantity, fnDef: graphFn, facts } = liveRef.current
    const fnDef = graphFn ?? parseFunctionDef(expr.trim())
    const isGraph = isGraphCommand(expr)
    const shown = liveDisplay || (fnDef ? fnDefText(fnDef) : '')
    if (!expr.trim() || !shown || isImproperUnitConversion(shown)) return
    const nextHint = quiet ? null : pickHint(onboardingRef.current?.hints ?? 0, { expr, ...facts })
    updateOnboarding((s) => ({ ...recordCommit(s), hints: s.hints | (nextHint?.bit ?? 0) }))
    setHint(nextHint?.text ?? null)
    setHelpOpen(false)
    // enter before the answer settled still gets its one firing
    settleArms(n)
    setHistory((prev) => {
      // inline `graph f(x)=...` registers the function; plain `graph expr` is not saved
      if (isGraph && !fnDef) return prev
      const nextRow = slimHistoryRow(
        fnDef
          ? {
              id: newRowId(),
              expr: isGraph ? fnDefText(fnDef) : expr,
              display: fnDefText(fnDef),
              kind: 'function',
              fnName: fnDef.name,
              fnParams: fnDef.params,
              fnBody: fnDef.body,
            }
          : {
              id: newRowId(),
              expr,
              display: shown,
              exact: exact && exact !== shown ? exact : undefined,
              n: Number.isFinite(n) ? n : undefined,
              meas,
              quantity,
            },
      )
      if (!nextRow) return prev
      const last = prev[prev.length - 1]
      if (last && last.expr === nextRow.expr && last.display === nextRow.display) return prev
      return persistableHistory([...prev, nextRow])
    })
    qRef.current = ''
    liveRef.current = EMPTY_LIVE
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
  }, [settleArms, stopDraftTimer, updateOnboarding])

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
    const action = hideAction(expr, liveRef.current.display, ttl)
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
        if (draftAtRef.current === savedAt) resetToCalculate()
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
    if (isHelpCommand(qRef.current)) resetToCalculate()
    else if (selected != null) insertHistoryAnswer(selected)
    else commit()
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
      const key = e.key.toLowerCase()
      const ctrlOnly = e.ctrlKey && !e.metaKey && !e.shiftKey && !e.altKey
      const cmdOnly = e.metaKey && !e.ctrlKey && !e.shiftKey && !e.altKey
      const toggle = ctrlOnly ? CTRL_SETTING_KEYS.get(key) : undefined
      let action: (() => void) | undefined
      if (toggle) action = () => setSettings(toggle)
      else if (ctrlOnly && key === 'c') action = clearHistory
      else if (cmdOnly && key === 'c') action = copyOutput
      if (!action) return
      e.preventDefault()
      e.stopPropagation()
      action()
    }
    const putOnClipboard = (e: ClipboardEvent, text: string) => {
      e.preventDefault()
      e.clipboardData?.setData('text/plain', text)
      nativeHandler()?.postMessage({ type: 'copy', text })
    }
    const onCopy = (e: ClipboardEvent) => {
      const highlighted = inputHighlight(e.target) || mathRef.current?.highlighted() || highlightedText()
      if (highlighted) {
        putOnClipboard(e, highlighted)
        return
      }
      const row = selected != null ? history[selected] : null
      const text = row ? rowCopyText(row, settings.answerForm) : shownLive
      if (!text || isImproperUnitConversion(text)) return
      putOnClipboard(e, text)
      flashCopied()
    }
    window.addEventListener('keydown', onKey, true)
    window.addEventListener('copy', onCopy, true)
    return () => {
      window.removeEventListener('keydown', onKey, true)
      window.removeEventListener('copy', onCopy, true)
    }
  }, [clearHistory, copyOutput, embedded, flashCopied, history, onClose, onWillHide, resetToCalculate, selected, settings.answerForm, shownLive])

  useEffect(() => () => stopDraftTimer(), [stopDraftTimer])

  // warm the engine (mathjs, unit tables) off the first keystroke's critical path
  useEffect(() => {
    const t = window.setTimeout(() => evaluateSheet(['1+1']), 0)
    return () => window.clearTimeout(t)
  }, [])

  useEffect(() => {
    if (!q.trim() || !hasNativeEval() || isGraphCommand(q) || isHelpCommand(q)) return
    // plain math is already answered in js; soulvercore is only needed for natural language
    if (jsDisplay && !looksLikeNaturalLanguage(q)) return
    // soulvercore has no ± (it answers `5 ± 2 * 3 ± 1` with 6); a blank beats that
    if (hasPlusMinus(q)) return
    const id = ++evalIdRef.current
    let cancelled = false
    void evaluateNative({
      id,
      expr: q,
      ans: lastAns,
      sigFigs: settings.sigFigs,
      variables: nativeVars,
    }).then((reply) => {
      if (cancelled || evalIdRef.current !== id || !reply) return
      setNativeLive(nativeReplyToLive(reply, id, qRef.current))
    })
    return () => {
      cancelled = true
    }
  }, [q, jsDisplay, lastAns, settings.sigFigs, nativeVars])

  useEffect(() => {
    const w = calcWindow()
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
      if (next) setNativeLive(next)
      else if (reply.id === evalIdRef.current && reply.expr === qRef.current) setNativeLive(null)
    }
    const onSoulver = (event: Event) => {
      const reply = (event as CustomEvent<NativeEvalReply>).detail
      if (reply) w.__qcalcNativeResult?.(reply)
    }
    window.addEventListener('qcalc-soulver', onSoulver)
    const el = rootRef.current
    size()
    const ro = el ? new ResizeObserver(() => reportNativeHeight(el)) : null
    if (el && ro) ro.observe(el)
    const focusTimers = [0, 50, 120].map((ms) => window.setTimeout(() => mathRef.current?.focus(), ms))
    return () => {
      window.removeEventListener('qcalc-soulver', onSoulver)
      for (const t of focusTimers) window.clearTimeout(t)
      ro?.disconnect()
    }
  }, [beginShowing, onPrepare, onWillHide])

  useTapeWheel(rootRef, tapeRef, {
    isOpen: () => tapeOpen,
    hasHistory: () => history.length > 0,
    open: () => {
      const input = mathRef.current?.element()
      if (input) {
        const start = input.selectionStart ?? input.value.length
        caretRef.current = { start, end: input.selectionEnd ?? start }
      }
      setTapeOpen(true)
    },
    close: () => {
      setTapeOpen(false)
      setSelected(null)
      restoreCaret()
    },
  })

  const onInputChange = (raw: string) => {
    const text = afterHelpInput(qRef.current, raw)
    setHelpOpen(false)
    // input without a keydown (dictation, ime, tests) still counts as a keystroke
    if (text) {
      setRotation(dismissRotation)
      setHint(null)
    }
    caretRef.current = null
    qRef.current = text
    setQ(text)
    if (!text.trim()) setPrefixUnit(null)
    if (selected != null && history[selected]?.expr !== text) setSelected(null)
  }

  return (
    <div className={embedded ? undefined : 'quick-wrap'}>
      <div
        ref={rootRef}
        className="spotlight-slot"
        onMouseDown={(e) => {
          const t = e.target as HTMLElement
          if (t.closest('input, button, .tape, .graph, .quick-plain, .quick-field, .edge-tools, .unit-settings, .live-dual')) return
          nativeHandler()?.postMessage({ type: 'drag' })
        }}
      >
        <div className={`spotlight ${embedded ? 'spotlight-embedded' : ''}`}>
          {helpShown ? (
            <CheatSheet cheats={cheats} />
          ) : tapeOpen && history.length > 0 ? (
            <HistoryTape
              history={history}
              selected={selected}
              answerForm={settings.answerForm}
              sigFigs={settings.sigFigs}
              tapeRef={tapeRef}
              onInsert={insertPlain}
              onInsertExpr={insertHistoryExpr}
              onInsertAnswer={insertHistoryAnswer}
            />
          ) : null}

          <div className="composer">
            <EdgeTools
              settings={settings}
              onToggle={setSettings}
              onClear={() => {
                clearHistory()
                mathRef.current?.focus()
              }}
            />
            <QuickInput
              value={q}
              ansPlain={ansPlain}
              handleRef={mathRef}
              example={example ? { text: example.expr, id: rotation.tick } : null}
              onChange={onInputChange}
              onEnter={onEnter}
              onUp={onUp}
              onDown={onDown}
              onPrefixStep={onPrefixStep}
            />
            <LiveAnswer
              copied={copied}
              example={example ? { tick: rotation.tick, answer: exampleAnswer } : null}
              display={display}
              exact={liveExact}
              shown={shownLive}
              onCopy={() => copyValue(shownLive)}
              onCopyExact={() => {
                if (liveExact) copyValue(insertableAnswer(liveExact))
              }}
              onCopyApprox={() => {
                if (!isImproperUnitConversion(display)) copyValue(insertableAnswer(display, liveN, settings.sigFigs))
              }}
            />
          </div>
          {hint ? (
            <div className="composer-hint" role="status">
              {hint}
            </div>
          ) : null}
          {graphCmd ? (
            <GraphPanel
              key={`${q.trim().toLowerCase()}|${settings.angleMode}`}
              input={q}
              functions={liveFns}
              variables={nativeVars}
              ans={lastAns}
              angleMode={settings.angleMode}
              onSelectX={copyValue}
            />
          ) : null}
        </div>
        <SixtySevenArms shaking={armsShaking} />
      </div>
      {!embedded ? (
        <>
          <AppearanceSettings value={settings.theme} onChange={(theme) => setSettings((s) => ({ ...s, theme }))} />
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
