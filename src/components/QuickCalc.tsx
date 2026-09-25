import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { flushSync } from 'react-dom'
import { chemCopyText, isReactionInput } from '../engine/chem'
import { evaluateSheet, parseFunctionDef } from '../engine/evaluate'
import { formatValue } from '../engine/format'
import { isGraphCommand, parseGraphIntent } from '../engine/graph'
import { isSysCommand, solveLive, sysCommand, type SystemAnswer } from '../engine/system'
import { isEquation } from '../engine/solve'
import { hasPlusMinus } from '../engine/measure'
import { inferParens } from '../engine/parens'
import { dualLabel } from '../engine/simplify'
import { inLadderUnit, isImproperUnitConversion, stepPrefix } from '../engine/units'
import type { SolveInfo, UserFunction, Value } from '../engine/types'
import { useClosedForm } from '../lib/closedForm'
import { applyTheme } from '../lib/theme'
import { AppearanceSettings } from './AppearanceSettings'
import { EdgeTools } from './EdgeTools'
import { GraphPanel } from './GraphPanel'
import { SystemPanel } from './SystemPanel'
import { CheatSheet, HistoryTape } from './HistoryTape'
import { HistoryInsertSettings } from './HistoryInsertSettings'
import { KeepWordsSettings } from './KeepWordsSettings'
import { LiveAnswer } from './LiveAnswer'
import { PeriodicCard } from './PeriodicCard'
import { RationalizeSettings } from './RationalizeSettings'
import { FourTwentySmoke } from './FourTwentySmoke'
import { SixtyNineFold } from './SixtyNineFold'
import { SixtySevenArms } from './SixtySevenArms'
import { typstAnswer } from '../lib/typstMath'
import { TypstPreview } from './TypstPreview'
import { TypstCopySettings, TypstSettings } from './TypstSettings'
import { UnitSettings } from './UnitSettings'
import { useFourTwentySmoke } from './useFourTwentySmoke'
import { useSixtyNineFold } from './useSixtyNineFold'
import { useSixtySevenArms } from './useSixtySevenArms'
import { useTapeWheel } from './useTapeWheel'
import { useAnswerForms } from './useAnswerForms'
import { useSteadyAnswer } from './useSteadyAnswer'
import {
  answerAmong,
  insertableAnswer,
  insertableHistoryAnswer,
  insertableHistoryReuse,
  visibleAnswer,
  type AnswerForm,
} from '../lib/answer'
import { blankReason, type Span } from '../lib/blankReason'
import { ansWrittenOut, chainedExpr, chainedHistoryExpr, chainsFromAnswer } from '../lib/chain'
import { hideAction, shouldRestoreDraft } from '../lib/draft'
import { nativeHandler } from '../lib/bridge'
import { copyText, highlightedText, inputHighlight, installSearchBarCopy, searchBarCopy } from '../lib/dom'
import {
  evaluateNative,
  hasNativeEval,
  looksLikeNaturalLanguage,
  mergeLiveAnswer,
  nativeReplyToLive,
  soulverAngleSafe,
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
  exampleAnswer,
  exampleList,
  examplesActive,
  HINT_PAUSE_MS,
  HOTKEY_FAILED_HINT,
  isHelpCommand,
  pickHint,
  recordCommit,
  recordOpen,
  ROTATION_OFF,
  rotationItem,
  startRotation,
  type HintFacts,
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
import { nextRecentExpiry, recentStart, scopeStart } from '../lib/historyShow'
import { isPeriodicCommand, openNativePeriodicTable, PERIODIC_HINT } from '../lib/periodic'
import { commandHeld, hostCheats, hostKeys, isClearHistoryKey } from '../lib/platform'
import { lineCopyText } from '../lib/touches'
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
  solve?: SolveInfo
  fnDef: FunctionDef | null
  facts: Omit<HintFacts, 'expr'>
}

const EMPTY_LIVE: LiveSnapshot = { display: '', fnDef: null, facts: {} }

// how long typing has to pause before a blank answer points at what it couldn't read
const SQUIGGLE_IDLE_MS = 700
// recent rows wait for a pause in typing before they age out, so the panel never resizes mid keystroke
const RECENT_IDLE_MS = 1500

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
    if (!row || row.kind === 'definition' || row.kind === 'function' || row.kind === 'system') continue
    // `ans` never becomes "no real solution"
    if (row.solve && row.solve.outcome !== 'roots') continue
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
  // read once: reading can clear an expired draft, so it mustn't run on every render
  const [storedDraft] = useState(() => readStoredDraft(settings.draftSeconds))
  const [q, setQ] = useState(storedDraft?.expr ?? '')
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
  const [squiggle, setSquiggle] = useState<{ q: string; span: Span | null } | null>(null)
  const [inputSel, setInputSel] = useState<{ start: number; end: number } | null>(null)
  // the browser build's periodic table; the mac app opens its own window instead
  const [periodicOpen, setPeriodicOpen] = useState(false)
  // equation fields for `sys N`; null until enter opens them
  const [sysLines, setSysLines] = useState<string[] | null>(null)
  const sysLinesRef = useRef<string[] | null>(null)
  sysLinesRef.current = sysLines

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
  // the answer the ✓ is for: typing on or tabbing to another form isn't what was copied
  const copiedFor = useRef('')
  const shownRef = useRef('')
  const qRef = useRef(q)
  qRef.current = q
  const settingsRef = useRef(settings)
  settingsRef.current = settings
  const draftAtRef = useRef(storedDraft?.savedAt ?? 0)
  const draftTimer = useRef(0)
  const evalIdRef = useRef(0)
  // the expression commit() saves when the input continues from the last answer
  const chainedRef = useRef('')
  const historyRef = useRef(history)
  historyRef.current = history
  const lastKeyRef = useRef(0)
  // the clock the recent rows are read against; moves on show, commit and quiet ticks only
  const [recentNow, setRecentNow] = useState(() => Date.now())
  const overControlRef = useRef(false)
  // whether the full tape sits open this showing when nothing is being browsed
  const tapeRestRef = useRef(false)

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
    const rest = settingsRef.current.historyShow === 'always' && historyRef.current.length > 0
    tapeRestRef.current = rest
    setRecentNow(Date.now())
    setSelected(null)
    setTapeOpen(rest)
    const s = onboardingRef.current ?? emptyOnboarding()
    const opened = nativeInfoRef.current.hotkeyFailed
      ? null
      : pickHint(s.hints, { expr: '', native: Boolean(calcWindow().__QCALC_NATIVE), opens: s.opens }, 'open')
    if (opened) updateOnboarding((o) => ({ ...o, hints: o.hints | opened.bit }))
    setHint(nativeInfoRef.current.hotkeyFailed ? HOTKEY_FAILED_HINT : (opened?.text ?? null))
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
    setTapeOpen(tapeRestRef.current)
    setCopied(false)
    setNativeLive(null)
    setPrefixUnit(null)
    setHelpOpen(false)
    setSysLines(null)
    mathRef.current?.setValue('')
    mathRef.current?.focus()
  }, [stopDraftTimer])

  // ⌘Z right after clearing the tape or removing a row puts it back; any edit to the input forgets it
  const tapeUndoRef = useRef<{ history: HistoryRow[]; selected: number | null; open: boolean } | null>(null)

  const clearHistory = useCallback(() => {
    if (historyRef.current.length) tapeUndoRef.current = { history: historyRef.current, selected: null, open: false }
    setHistory([])
    setSelected(null)
    setTapeOpen(false)
  }, [])

  const removeRow = useCallback((index: number) => {
    const prev = historyRef.current
    if (!prev[index]) return
    tapeUndoRef.current = { history: prev, selected: index, open: true }
    const next = prev.filter((_, i) => i !== index)
    setHistory(next)
    if (next.length) setSelected(Math.min(index, next.length - 1))
    else {
      setSelected(null)
      setTapeOpen(false)
    }
  }, [])

  const undoTape = useCallback((): boolean => {
    const saved = tapeUndoRef.current
    if (!saved) return false
    tapeUndoRef.current = null
    setHistory(saved.history)
    setSelected(saved.selected)
    if (saved.open) setTapeOpen(true)
    return true
  }, [])

  const lastAnswer = useMemo(() => lastAnswerRow(history), [history])
  const lastAns = lastHistoryNumber(history)
  const ansPlain = lastAnswer
    ? insertableHistoryAnswer(lastAnswer, settings.answerForm, settings.sigFigs)
    : undefined
  const scoped = useMemo(
    () => history.slice(scopeStart(history, settings.historyShow, recentNow)),
    [history, settings.historyShow, recentNow],
  )
  const nativeVars = useMemo(() => historyVariables(scoped), [scoped])
  const nativeFns = useMemo(() => historyFunctions(scoped), [scoped])
  const nativeMeas = useMemo(() => historyMeasures(scoped), [scoped])
  const nativeQty = useMemo(() => historyQuantities(scoped), [scoped])
  const completionNames = useMemo(
    () => ({ variables: Object.keys(nativeVars), functions: Object.keys(nativeFns), ans: lastAns != null }),
    [nativeVars, nativeFns, lastAns],
  )

  const helpShown = helpOpen || isHelpCommand(q)
  const cheats = useMemo(() => hostCheats(cheatSheet(nativeInfo.hotkey || undefined, Boolean(calcWindow().__QCALC_NATIVE))), [nativeInfo.hotkey])
  const graphCmd = isGraphCommand(q)
  const sysCmd = isSysCommand(q)
  const periodicCmd = isPeriodicCommand(q)
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

  // a chain needs the last answer row to be the one `ans` means
  const chainAnswer =
    ansPlain && lastAnswer && Number.isFinite(lastAnswer.n) ? { plain: ansPlain, unit: lastAnswer.quantity != null } : undefined
  const chained = !graphCmd && chainsFromAnswer(q, chainAnswer)
  const tapeExpr = chained && ansPlain ? chainedHistoryExpr(q, ansPlain) : q
  chainedRef.current = chainAnswer ? ansWrittenOut(tapeExpr, chainAnswer.plain) : chained ? tapeExpr : ''
  const evalOptions = useMemo(
    () => ({
      ...evalSettings,
      ans: lastAns,
      variables: nativeVars,
      measures: nativeMeas,
      quantities: nativeQty,
      functions: liveFns,
    }),
    [lastAns, nativeVars, nativeMeas, nativeQty, liveFns, evalSettings],
  )
  const sheet = useMemo(() => {
    if (graphCmd || periodicCmd || sysCmd) return []
    return evaluateSheet([chained ? chainedExpr(q) : q], evalOptions)
  }, [q, chained, graphCmd, periodicCmd, sysCmd, evalOptions])

  const sysParsed = useMemo(() => (sysCmd ? sysCommand(q) : null), [sysCmd, q])
  const sysAnswer = useMemo((): SystemAnswer | null => {
    if (!sysLines || !sysParsed || !('count' in sysParsed) || sysParsed.count !== sysLines.length) return null
    return solveLive(sysLines)
  }, [sysLines, sysParsed])

  const sysShown = sysAnswer?.display ?? (sysParsed && 'hint' in sysParsed && !sysLines ? sysParsed.hint : '')
  const sysShownRef = useRef('')
  sysShownRef.current = sysShown
  const sysMessage = Boolean(sysCmd && (sysAnswer?.message || (sysParsed && 'hint' in sysParsed && !sysLines)))

  const live = sheet[sheet.length - 1]
  let jsDisplay = ''
  if (graphCmd) jsDisplay = graphIntent?.label ? `graph ${graphIntent.label}` : ''
  else if (q.trim()) jsDisplay = live?.display ?? ''
  const jsN = graphCmd || sysCmd ? undefined : live?.value?.kind === 'number' ? live.value.n : undefined
  const nativeUsable = !graphCmd && !sysCmd && !periodicCmd && !chained && soulverAngleSafe(q, settings.angleMode)
  const merged = mergeLiveAnswer(q, jsDisplay, jsN, nativeUsable ? nativeLive : null)
  // ⌥↑/⌥↓ re-expresses the js answer on its si prefix ladder; what's shown is what's copied and saved
  const jsValue = !graphCmd && jsDisplay && merged.display === jsDisplay ? live?.value : undefined
  const stepped = prefixUnit && jsValue ? inLadderUnit(jsValue, prefixUnit) : null
  steppableRef.current = stepped ?? jsValue
  const baseDisplay = stepped ? formatValue(stepped, settings.sigFigs) : merged.display
  const closedForm = useClosedForm(graphCmd ? undefined : live?.closedForm)
  const baseExact = graphCmd || stepped ? undefined : q.trim() && jsDisplay ? (live?.exact ?? closedForm) : undefined
  // tab re-expresses the answer; like a ⌥↑ step, the form shown is what's copied and saved
  const formValue = stepped ?? jsValue
  const tabForm = useAnswerForms(
    `${q}\n${baseDisplay}\n${baseExact ?? ''}`,
    formValue && baseDisplay
      ? { value: formValue, display: baseDisplay, exact: baseExact, meas: live?.meas, sigFigs, sigFigMode, rationalize }
      : null,
  )
  const shownForm = tabForm.form
  const display = sysCmd ? (sysMessage ? '' : sysShown) : (shownForm?.display ?? baseDisplay)
  const liveN = sysCmd ? undefined : shownForm?.value ? shownForm.value.n : stepped ? stepped.n : merged.n
  const liveExact = sysCmd ? (sysMessage ? undefined : sysAnswer?.exact) : shownForm ? undefined : baseExact
  // the graph panel already labels the curve, so the answer slot stays empty
  const shownLive = sysCmd ? (sysMessage ? '' : sysShown) : graphCmd ? '' : visibleAnswer({ display, exact: liveExact }, settings.answerForm)
  shownRef.current = shownLive
  const fromJs = Boolean(display) && display === jsDisplay
  const liveSolve = fromJs && live?.kind === 'solve' ? live.solve : undefined
  const rootsOf = liveSolve?.outcome === 'roots' ? liveSolve.variable : undefined
  const steady = useSteadyAnswer(
    q,
    graphCmd || sysCmd || helpShown || periodicCmd
      ? null
      : shownLive && !isImproperUnitConversion(display)
        ? `${rootsOf ? `${rootsOf} = ` : ''}${dualLabel(liveExact, display)}`
        : '',
    evalSettings,
  )
  liveRef.current = {
    display,
    exact: liveExact,
    n: liveN,
    meas: fromJs ? live?.meas : undefined,
    quantity: jsValue ? live?.quantity : undefined,
    solve: liveSolve,
    fnDef: graphIntent?.functionDef ?? null,
    facts: {
      answer: display,
      variable: fromJs && live?.kind === 'assignment' ? live.variable : undefined,
      unit: Boolean(steppableRef.current?.unit),
      angleMode: settings.angleMode,
      fractionMode: settings.fractionMode,
    },
  }

  const selText = inputSel && inputSel.end > inputSel.start ? q.slice(inputSel.start, inputSel.end) : ''
  const selDisplay = useMemo(() => {
    const text = selText.trim()
    if (!text || text === q.trim() || graphCmd) return ''
    const shown = evaluateSheet([text], evalOptions)[0]?.display ?? ''
    // a bare number selected reads the same as its value, so there's nothing to show
    return shown === text || isImproperUnitConversion(shown) ? '' : shown
  }, [selText, q, graphCmd, evalOptions])

  useEffect(() => {
    if (display || !q.trim() || graphCmd || sysCmd || helpShown || periodicCmd || looksLikeNaturalLanguage(q)) return
    const t = window.setTimeout(() => {
      const ok = (text: string) => Boolean(evaluateSheet([chained ? chainedExpr(text) : text], evalOptions)[0]?.display)
      const names = { variables: Object.keys(nativeVars), functions: Object.keys(liveFns) }
      setSquiggle({ q, span: blankReason(q, ok, names) })
    }, SQUIGGLE_IDLE_MS)
    return () => window.clearTimeout(t)
  }, [q, display, chained, graphCmd, sysCmd, helpShown, periodicCmd, evalOptions, nativeVars, liveFns])

  const examples = useMemo(
    () => exampleList(firstRun && nativeInfo.hotkey ? nativeInfo.hotkey : undefined),
    [firstRun, nativeInfo.hotkey],
  )
  const example = !q && !helpShown ? rotationItem(rotation, examples) : undefined
  const exampleShown = useMemo(
    () => (example && !example.plain ? exampleAnswer(evaluateSheet([prettyTokens(example.expr)], evalSettings)[0]) : ''),
    [example, evalSettings],
  )

  const { shaking: armsShaking, settle: settleArms } = useSixtySevenArms(liveN)
  const { folding: sixtyNine, settle: settleSixtyNine } = useSixtyNineFold(liveN)
  const { smoking, settle: settleSmoke } = useFourTwentySmoke(liveN)

  useEffect(() => {
    if (!rotation.on) return
    const t = window.setInterval(() => setRotation(advanceRotation), EXAMPLE_MS)
    return () => window.clearInterval(t)
  }, [rotation.on])

  // an answer left on screen for a moment gets a hint about what to do with it
  const pauseHintable = Boolean(q.trim() && display && !isImproperUnitConversion(display) && !graphCmd && !helpShown)
  useEffect(() => {
    if (!pauseHintable) return
    const t = window.setTimeout(() => {
      const next = pickHint(onboardingRef.current?.hints ?? 0, { expr: qRef.current, ...liveRef.current.facts }, 'pause')
      if (!next) return
      updateOnboarding((s) => ({ ...s, hints: s.hints | next.bit }))
      setHint(next.text)
    }, HINT_PAUSE_MS)
    return () => window.clearTimeout(t)
  }, [pauseHintable, q, display, updateOnboarding])

  useEffect(() => saveHistory(history), [history])

  const recentOn = settings.historyShow === 'recent'
  // arrow mode ticks too, since its variables expire with the recent rows
  const expires = settings.historyShow !== 'always'
  useEffect(() => {
    const due = expires ? nextRecentExpiry(history, recentNow) : null
    if (due == null) return
    let t = 0
    const tick = () => {
      const idle = Date.now() - lastKeyRef.current
      if (idle < RECENT_IDLE_MS) t = window.setTimeout(tick, RECENT_IDLE_MS - idle)
      else setRecentNow(Date.now())
    }
    t = window.setTimeout(tick, Math.max(0, due - Date.now()))
    return () => window.clearTimeout(t)
  }, [history, recentNow, expires])
  const recentFrom = recentOn ? recentStart(history, recentNow) : history.length

  useEffect(() => {
    saveSettings(settings)
    // all of it, so the mac settings window follows ⌃D/⌃F/⌃S live
    nativeHandler()?.postMessage({ type: 'settings', ...settings })
  }, [settings])

  useEffect(() => {
    installSearchBarCopy(() => settingsRef.current.typstCopy)
  }, [])

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
    copiedFor.current = shownRef.current
    setCopied(true)
    window.clearTimeout(copiedTimer.current)
    copiedTimer.current = window.setTimeout(() => setCopied(false), 1200)
  }, [])

  const copyValue = useCallback(
    (text: string) => {
      if (!text || isImproperUnitConversion(text)) return
      copyText(chemCopyText(text))
      flashCopied()
    },
    [flashCopied],
  )

  const copyOutput = useCallback(() => {
    const fromBar = mathRef.current?.highlighted()
    if (fromBar) {
      copyText(searchBarCopy(fromBar))
      return
    }
    const highlighted = highlightedText()
    if (highlighted) {
      copyText(highlighted)
      return
    }
    const row = selected != null ? history[selected] : null
    copyValue(row ? rowCopyText(row, settings.answerForm) : shownLive)
  }, [copyValue, history, selected, settings.answerForm, shownLive])

  // exact forms read with their symbols, like the line they sit next to
  const copyLine = useCallback(() => {
    const row = selected != null ? history[selected] : null
    if (row) {
      const exact = row.exact && prettyTokens(row.exact)
      copyValue(row.kind === 'definition' || row.kind === 'function' ? row.expr : lineCopyText(row.expr, { ...row, exact }, settings.answerForm))
      return
    }
    const { display: shown, exact, solve } = liveRef.current
    if (!shown || graphCmd) return
    const expr = chainedRef.current || qRef.current
    if (isReactionInput(expr)) {
      copyValue(shown)
      return
    }
    const { leading, trailing } = inferParens(expr)
    const whole = '('.repeat(Math.max(0, leading)) + expr.trim() + ')'.repeat(Math.max(0, trailing))
    copyValue(lineCopyText(whole, { display: shown, exact: exact && prettyTokens(exact), solve }, settings.answerForm))
  }, [copyValue, graphCmd, history, selected, settings.answerForm])

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

  const insertPlain = useCallback((text: string, isAnswer = false) => {
    if (!text) return
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
    const value = el?.value ?? qRef.current
    const rest = value.slice(0, at?.start ?? value.length) + value.slice(at?.end ?? value.length)
    const chunk = isAnswer ? answerAmong(text, !rest.trim()) : text
    mathRef.current?.insert(chunk, at)
    const dest = (at?.start ?? 0) + chunk.length
    caretRef.current = { start: dest, end: dest }
    mathRef.current?.focus()
    setSelected(null)
    setTapeOpen(tapeRestRef.current)
  }, [])

  const openHistorySystem = useCallback((index: number) => {
    const row = history[index]
    const lines = row?.kind === 'system' ? row.equations?.slice(0, 5) : undefined
    if (!lines?.length) return
    const text = `sys ${lines.length}`
    sysLinesRef.current = lines
    qRef.current = text
    setSysLines(lines)
    setQ(text)
    setSelected(null)
    setTapeOpen(false)
    setCopied(false)
    mathRef.current?.setValue(text)
  }, [history])

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
      const isAnswer = row.kind !== 'definition' && settings.historyInsert === 'answer'
      insertPlain(insertableHistoryReuse(row, settings.answerForm, settings.historyInsert, settings.sigFigs), isAnswer)
    },
    [history, insertPlain, settings.answerForm, settings.historyInsert, settings.sigFigs],
  )

  // whichever half of the row enter doesn't insert
  const insertHistoryOther = useCallback(
    (index: number) => {
      const row = history[index]
      if (!row) return
      if (row.kind === 'definition' || row.kind === 'function' || settings.historyInsert === 'answer') insertHistoryExpr(index)
      else insertPlain(insertableHistoryAnswer(row, settings.answerForm, settings.sigFigs), true)
    },
    [history, insertHistoryExpr, insertPlain, settings.answerForm, settings.historyInsert, settings.sigFigs],
  )

  // `quiet` is the commit-on-hide path: nobody is looking, so no hint is spent on it
  const commit = useCallback((quiet = false) => {
    const sysNow = sysLinesRef.current
    const expr = sysNow ? 'sys' : chainedRef.current || qRef.current
    const { display: liveDisplay, exact, n, meas, quantity, solve, fnDef: graphFn, facts } = liveRef.current
    const fnDef = graphFn ?? parseFunctionDef(expr.trim())
    const isGraph = isGraphCommand(expr)
    const written = sysNow?.map((line) => line.trim()).filter(Boolean).join('; ') ?? ''
    const shown = sysNow ? sysShownRef.current.trim() || written : liveDisplay || (fnDef ? fnDefText(fnDef) : '')
    if (!expr.trim() || !shown || isImproperUnitConversion(shown)) return
    const nextHint = quiet ? null : pickHint(onboardingRef.current?.hints ?? 0, { expr, ...facts })
    updateOnboarding((s) => ({ ...recordCommit(s), hints: s.hints | (nextHint?.bit ?? 0) }))
    const at = Date.now()
    setRecentNow(at)
    setHint(nextHint?.text ?? null)
    setHelpOpen(false)
    // enter before the answer settled still gets its one firing
    settleArms(n)
    settleSmoke(n)
    settleSixtyNine(n)
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
              solve,
              ...(sysNow ? { kind: 'system' as const, equations: sysNow.slice(0, 5) } : {}),
            },
      )
      if (!nextRow) return prev
      nextRow.at = at
      const last = prev[prev.length - 1]
      const sameSystem = nextRow.kind === 'system' && last?.equations?.join('\n') === nextRow.equations?.join('\n')
      const same = last && last.expr === nextRow.expr && last.display === nextRow.display && (nextRow.kind !== 'system' || sameSystem)
      if (same) return [...prev.slice(0, -1), { ...last, at }]
      return persistableHistory([...prev, nextRow])
    })
    tapeUndoRef.current = null
    qRef.current = ''
    liveRef.current = EMPTY_LIVE
    draftAtRef.current = 0
    stopDraftTimer()
    clearStoredDraft()
    setQ('')
    setSysLines(null)
    setNativeLive(null)
    setPrefixUnit(null)
    mathRef.current?.setValue('')
    mathRef.current?.focus()
    setSelected(null)
    setTapeOpen(tapeRestRef.current)
  }, [settleArms, settleSmoke, settleSixtyNine, stopDraftTimer, updateOnboarding])

  const stepForm = tabForm.step
  const onTab = useCallback(
    (dir: 1 | -1) => {
      if (selected == null) stepForm(dir)
    },
    [selected, stepForm],
  )

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
    tapeRestRef.current = false
    setRotation(ROTATION_OFF)
    setHint(null)
    if (isHelpCommand(expr) || isPeriodicCommand(expr)) {
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
    const first = document.querySelector<HTMLInputElement>('.sys-eq')
    if (sysLinesRef.current?.length && first) {
      first.focus()
      const end = first.value.length
      first.setSelectionRange(end, end)
      return true
    }
    if (!tapeOpen) return false
    if (selected == null || selected >= history.length - 1) {
      setSelected(null)
      restoreCaret()
      return true
    }
    setSelected(selected + 1)
    return true
  }, [history.length, restoreCaret, selected, tapeOpen])

  const onEnter = useCallback((alt = false) => {
    const opening = sysCommand(qRef.current)
    if (selected == null && opening && 'count' in opening && sysLinesRef.current?.length !== opening.count) {
      setSysLines(Array.from({ length: opening.count }, () => ''))
      return
    }
    if (opening && 'count' in opening && sysLinesRef.current?.length === opening.count) {
      commit()
      return
    }
    if (isHelpCommand(qRef.current)) resetToCalculate()
    else if (selected == null && isPeriodicCommand(qRef.current)) {
      if (!openNativePeriodicTable()) setPeriodicOpen(true)
      resetToCalculate()
    } else if (selected != null && history[selected]?.kind === 'system') openHistorySystem(selected)
    else if (selected != null && alt) insertHistoryOther(selected)
    else if (selected != null) insertHistoryAnswer(selected)
    else commit()
  }, [commit, resetToCalculate, selected, history, insertHistoryAnswer, insertHistoryOther, openHistorySystem])

  // esc always leaves the overlay. false tells the mac app to hide; the page does not clear the tape or the input first.
  const escapeLayer = useCallback((): boolean => false, [])

  useEffect(() => {
    calcWindow().__qcalcEscape = escapeLayer
  }, [escapeLayer])

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
        if (escapeLayer()) return
        setPrefixUnit(null)
        if (embedded) onWillHide()
        else resetToCalculate()
        onClose()
        return
      }
      const key = e.key.toLowerCase()
      const ctrlOnly = e.ctrlKey && !e.metaKey && !e.shiftKey && !e.altKey
      const cmd = commandHeld(e) && !e.altKey
      const cmdOnly = cmd && !e.shiftKey
      const toggle = ctrlOnly ? CTRL_SETTING_KEYS.get(key) : undefined
      const cmdShift = cmd && e.shiftKey
      const mainInput = e.target instanceof HTMLInputElement && e.target.classList.contains('quick-plain')
      let action: (() => void) | undefined
      if (toggle) action = () => setSettings(toggle)
      else if (isClearHistoryKey(e)) action = clearHistory
      else if (cmdOnly && key === 'c') action = copyOutput
      else if (cmdShift && key === 'c') action = copyLine
      else if (cmdOnly && (e.key === 'Backspace' || e.key === 'Delete') && mainInput && sysLinesRef.current) action = resetToCalculate
      else if (cmdOnly && e.key === 'Backspace' && selected != null) action = () => removeRow(selected)
      else if (cmdOnly && key === 'z' && tapeUndoRef.current) action = undoTape
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
      const field = document.querySelector('.quick-plain')
      const fromBar = inputHighlight(e.target instanceof Element && e.target.classList.contains('quick-plain') ? e.target : field) || mathRef.current?.highlighted()
      if (fromBar) {
        putOnClipboard(e, searchBarCopy(fromBar))
        return
      }
      const highlighted = inputHighlight(e.target) || highlightedText()
      if (highlighted) {
        putOnClipboard(e, highlighted)
        return
      }
      const row = selected != null ? history[selected] : null
      const text = row ? rowCopyText(row, settings.answerForm) : shownLive
      if (!text || isImproperUnitConversion(text)) return
      putOnClipboard(e, chemCopyText(text))
      flashCopied()
    }
    window.addEventListener('keydown', onKey, true)
    window.addEventListener('copy', onCopy, true)
    return () => {
      window.removeEventListener('keydown', onKey, true)
      window.removeEventListener('copy', onCopy, true)
    }
  }, [clearHistory, copyLine, copyOutput, embedded, escapeLayer, flashCopied, history, onClose, onWillHide, removeRow, resetToCalculate, selected, settings.answerForm, shownLive, undoTape])

  useEffect(() => () => stopDraftTimer(), [stopDraftTimer])

  // warm the engine (mathjs, unit tables) off the first keystroke's critical path
  useEffect(() => {
    const t = window.setTimeout(() => evaluateSheet(['1+1', 'x^3 = 2']), 0)
    return () => window.clearTimeout(t)
  }, [])

  useEffect(() => {
    // an equation js can't solve would come back from soulvercore as something else
    if (!q.trim() || !hasNativeEval() || isGraphCommand(q) || isSysCommand(q) || isHelpCommand(q) || isPeriodicCommand(q) || isEquation(q)) return
    // plain math is already answered in js; soulvercore is only needed for natural language
    if (chained || !looksLikeNaturalLanguage(q)) return
    // soulvercore has no ± (it answers `5 ± 2 * 3 ± 1` with 6); a blank beats that
    if (hasPlusMinus(q) || !soulverAngleSafe(q, settings.angleMode)) return
    // a reaction js couldn't balance stays blank
    if (isReactionInput(q)) return
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
  }, [q, chained, jsDisplay, lastAns, settings.sigFigs, settings.angleMode, nativeVars])

  useEffect(() => {
    const w = calcWindow()
    const size = () => reportNativeHeight(rootRef.current)
    w.__qcalcFocus = () => mathRef.current?.focus()
    w.__qcalcPaste = (text) => {
      if (typeof text !== 'string' || !text) return
      mathRef.current?.insert(flattenPastedText(text))
      mathRef.current?.focus()
    }
    w.__qcalcInsert = (text) => {
      if (typeof text === 'string') insertPlain(text)
    }
    w.__qcalcSize = size
    w.__qcalcWillHide = () => onWillHide()
    // synchronous so the height is reported before the mac app fades the panel in
    w.__qcalcReset = () => {
      flushSync(() => {
        onPrepare()
        beginShowing()
      })
      size()
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
  }, [beginShowing, insertPlain, onPrepare, onWillHide])

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
      tapeRestRef.current = false
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
    setInputSel(null)
    // resets and restored drafts come through here too, already matching qRef
    if (text !== qRef.current) {
      lastKeyRef.current = Date.now()
      tapeUndoRef.current = null
    }
    qRef.current = text
    setQ(text)
    const opened = sysLinesRef.current
    if (opened) {
      const cmd = sysCommand(text)
      if (!cmd || !('count' in cmd) || cmd.count !== opened.length) setSysLines(null)
    }
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
        onMouseOver={(e) => {
          const on = Boolean((e.target as HTMLElement).closest('button, .tape'))
          if (on === overControlRef.current) return
          overControlRef.current = on
          nativeHandler()?.postMessage({ type: 'overControl', on })
        }}
      >
        <FourTwentySmoke smoking={smoking} />
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
              onInsert={(text) => insertPlain(text, true)}
              onInsertExpr={insertHistoryExpr}
              onInsertAnswer={insertHistoryAnswer}
              onOpenSystem={openHistorySystem}
            />
          ) : recentFrom < history.length ? (
            <HistoryTape
              history={history}
              from={recentFrom}
              selected={null}
              answerForm={settings.answerForm}
              sigFigs={settings.sigFigs}
              tapeRef={tapeRef}
              onInsert={(text) => insertPlain(text, true)}
              onInsertExpr={insertHistoryExpr}
              onInsertAnswer={insertHistoryAnswer}
              onOpenSystem={openHistorySystem}
            />
          ) : null}

          <div className="composer">
            <SixtyNineFold active={sixtyNine} />
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
              keepWords={settings.keepWords}
              handleRef={mathRef}
              example={example ? { text: example.expr, id: rotation.tick } : null}
              onChange={onInputChange}
              onEnter={onEnter}
              onUp={onUp}
              onDown={onDown}
              onPrefixStep={onPrefixStep}
              chain={chained}
              squiggle={squiggle?.q === q && !display ? squiggle.span : null}
              completionNames={completionNames}
              onSelection={setInputSel}
              onTab={onTab}
            />
            {selDisplay ? (
              <span className="live live-selection" aria-live="polite">
                {selDisplay}
              </span>
            ) : (
            <LiveAnswer
              copied={copied && copiedFor.current === shownLive}
              example={example ? { tick: rotation.tick, answer: exampleShown } : null}
              display={sysCmd ? sysShown : periodicCmd ? PERIODIC_HINT : graphCmd ? '' : display}
              exact={liveExact}
              shown={shownLive}
              steady={steady}
              formTick={tabForm.tick}
              onCopy={copyValue}
              onRefocus={() => mathRef.current?.focus()}
              sides={{
                exact: liveExact ? (liveSolve ? liveExact : insertableAnswer(liveExact)) : '',
                approx: liveSolve
                  ? display
                  : isImproperUnitConversion(display)
                    ? ''
                    : insertableAnswer(display, liveN, settings.sigFigs),
              }}
              label={rootsOf}
              message={sysMessage || periodicCmd || Boolean(liveSolve && liveSolve.outcome !== 'roots')}
            />
            )}
          </div>
          {settings.typstPreview ? (
            <TypstPreview
              expr={q}
              answer={graphCmd || periodicCmd ? '' : typstAnswer(liveExact, display)}
              theme={settings.theme}
            />
          ) : null}
          {hint ? (
            <div className="composer-hint" role="status">
              {hostKeys(hint)}
            </div>
          ) : null}
          {sysLines && sysParsed && 'count' in sysParsed && sysParsed.count === sysLines.length ? (
            <SystemPanel
              lines={sysLines}
              onChange={(index, text) => {
                setSysLines((prev) => (prev ? prev.map((line, i) => (i === index ? text : line)) : prev))
              }}
              onEnter={(index) => {
                const rows = document.querySelectorAll<HTMLInputElement>('.sys-eq')
                const next = rows[index + 1]
                if (next) next.focus()
                else commit()
              }}
              onFocusMain={() => mathRef.current?.focus()}
            />
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
      {periodicOpen ? <PeriodicCard onPick={insertPlain} onClose={() => setPeriodicOpen(false)} /> : null}
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
          <KeepWordsSettings
            value={settings.keepWords}
            onChange={(keepWords) => setSettings((s) => ({ ...s, keepWords }))}
          />
          <TypstSettings
            value={settings.typstPreview}
            onChange={(typstPreview) => setSettings((s) => ({ ...s, typstPreview }))}
          />
          <TypstCopySettings
            value={settings.typstCopy}
            onChange={(typstCopy) => setSettings((s) => ({ ...s, typstCopy }))}
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
