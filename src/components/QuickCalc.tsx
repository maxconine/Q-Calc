import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { flushSync } from 'react-dom'
import { chemCopyText, isReactionInput } from '../engine/chem'
import { evaluateSheet, parseFunctionDef } from '../engine/evaluate'
import { formatValue } from '../engine/format'
import { isGraphCommand, parseGraphIntent } from '../engine/graph'
import { isSysCommand, parseSystemCall, solveLive, sysCommand, type SystemAnswer } from '../engine/system'
import { isIsolateCommand, isolatePrevious } from '../engine/isolate'
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
import { CheatSheet, GamesSheet, HistoryTape } from './HistoryTape'
import { HistoryInsertSettings } from './HistoryInsertSettings'
import { KeepWordsSettings } from './KeepWordsSettings'
import { LiveAnswer } from './LiveAnswer'
import { PeriodicCard } from './PeriodicCard'
import { IdentityCard } from './IdentityCard'
import { PongPanel } from './PongPanel'
import { Connect4Panel } from './Connect4Panel'
import { CrackPanel } from './CrackPanel'
import { EmberPanel } from './EmberPanel'
import { EqualPanel } from './EqualPanel'
import { NumbersPanel } from './NumbersPanel'
import { TrailsPanel } from './TrailsPanel'
import { RationalizeSettings } from './RationalizeSettings'
import { FourTwentySmoke } from './FourTwentySmoke'
import { SixtyNineFold } from './SixtyNineFold'
import { SixtySevenArms } from './SixtySevenArms'
import { typstAnswer, typstPreviewUseful } from '../lib/typstMath'
import { TypstPreview } from './TypstPreview'
import { TimeLetters } from './TimeLetters'
import { TypstCopySettings, TypstSettings } from './TypstSettings'
import { CopyUnitlessSettings } from './CopySettings'
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
  unitlessAnswer,
  visibleAnswer,
  type AnswerForm,
} from '../lib/answer'
import { blankReason, enterHint, type Span } from '../lib/blankReason'
import { ansWrittenOut, chainedExpr, chainedHistoryExpr, chainsFromAnswer } from '../lib/chain'
import { hideAction, shouldRestoreDraft } from '../lib/draft'
import { nativeHandler } from '../lib/bridge'
import { openSmokeRoom } from '../lib/smokeRoom'
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
  isMatrixQuantity,
  historyVariables,
  lastHistoryNumber,
  newRowId,
  persistableHistory,
  slimHistoryRow,
  type HistoryRow,
} from '../lib/history'
import { nextRecentExpiry, recentStart, scopeStart } from '../lib/historyShow'
import { isPeriodicCommand, openNativePeriodicTable, PERIODIC_HINT } from '../lib/periodic'
import { identitySheetFor, isIdentityCommand, openNativeIdentitySheet, type IdentitySheet } from '../lib/identities'
import { GREETING_REPLY, isGreeting } from '../lib/greeting'
import { calcKind, setUsageSharing, track } from '../lib/analytics'
import { actionForEvent, keyRecorder, type KeyAction } from '../lib/keybinds'
import { KeybindSettings } from './KeybindSettings'
import { useKeyLabels } from './useKeyLabels'
import {
  enrolTour,
  isSkipCommand,
  isTutorialCommand,
  TOUR_ALL,
  TOUR_DONE_HINT,
  TOUR_OFF,
  tourBit,
  tourExtra,
  tourLineFits,
  tourProgress,
  tourRunning,
  tourStep,
  tourUsed,
  TUTORIAL_HINT,
} from '../lib/tour'
import { hasPeerTransport, peerTransport } from '../lib/peer'
import { GAMES, GAMES_HINT, gameCommand, gameHint, gameNeedsLink, isGamesCommand, type GameKind } from '../lib/games'
import { commandHeld, hostCheats, hostKeys, hotkeyFailedText, isWindowsHost } from '../lib/platform'
import { hasSoulver, withPhraseAnswer } from '../lib/phraseLive'
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
  // the answer came from soulvercore or a phrase, not the js engine
  native?: boolean
  facts: Omit<HintFacts, 'expr'>
}

const EMPTY_LIVE: LiveSnapshot = { display: '', fnDef: null, facts: {} }

// how long typing has to pause before a blank answer points at what it couldn't read
const SQUIGGLE_IDLE_MS = 700
// recent rows wait for a pause in typing before they age out, so the panel never resizes mid keystroke
const RECENT_IDLE_MS = 1500

const MODIFIER_KEYS = new Set(['Shift', 'Meta', 'Control', 'Alt', 'CapsLock', 'Fn'])

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
  const room = openSmokeRoom(el)
  nativeHandler()?.postMessage(room ? { type: 'size', height, anchorTop, room } : { type: 'size', height, anchorTop })
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

// the walkthrough's line under the bar: what to try, how far along, and a way out
function TourLine({ text, progress, onSkip }: { text: string; progress: { at: number; of: number }; onSkip: () => void }) {
  return (
    <div className="composer-hint tour-line" role="status">
      <span className="tour-text">{text}</span>
      <span className="tour-count">
        {progress.at}/{progress.of}
      </span>
      <button type="button" className="tour-skip" onClick={onSkip} title="skip the tutorial (or type skip)">
        skip
      </button>
    </div>
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
  // a hint that also puts its example in the empty bar, for as long as that hint is up
  const [hintGhost, setHintGhost] = useState<{ hint: string; expr: string; plain?: boolean } | null>(null)
  // the walkthrough's bits, mirrored from onboarding so a step done re-renders the line
  const [tourSeen, setTourSeen] = useState(() => loadOnboarding().hints & TOUR_ALL)
  // steps done in a replay asked for with `tutorial`; null when not replaying. lives for this page only
  const [replay, setReplay] = useState<number | null>(null)
  const replayRef = useRef<number | null>(null)
  replayRef.current = replay
  const [helpOpen, setHelpOpen] = useState(false)
  const [squiggle, setSquiggle] = useState<{ q: string; span: Span | null } | null>(null)
  const [inputSel, setInputSel] = useState<{ start: number; end: number } | null>(null)
  // the browser build's periodic table; the mac app opens its own window instead
  const [periodicOpen, setPeriodicOpen] = useState(false)
  // the browser build's identity or calculus sheet, the same way
  const [identityOpen, setIdentityOpen] = useState<IdentitySheet | null>(null)
  // letters moved by hand between changing with time and constant, for d/dt; kept until Q Calc quits
  const [timeVarying, setTimeVarying] = useState<Record<string, boolean>>({})
  // equation fields for `sys N`; null until enter opens them
  const [sysLines, setSysLines] = useState<string[] | null>(null)
  // a game (pong, connect 4) takes over the bar until esc or the panel hides
  const [pongOpen, setPongOpen] = useState<GameKind | null>(null)
  const pongOpenRef = useRef<GameKind | null>(null)
  const emberEscRef = useRef<(() => boolean) | null>(null)
  pongOpenRef.current = pongOpen
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
    setTourSeen(next.hints & TOUR_ALL)
  }, [])

  // a step or extra used, in the real progress and in a replay alike
  const markTour = useCallback(
    (bits: number) => {
      if (!bits) return
      if (((onboardingRef.current?.hints ?? 0) & bits) !== bits) updateOnboarding((s) => ({ ...s, hints: s.hints | bits }))
      // the ref moves now so a commit can tell the basics just finished
      if (replayRef.current != null) replayRef.current |= bits
      setReplay((r) => (r == null ? r : r | bits))
    },
    [updateOnboarding],
  )

  const touringNow = useCallback(
    (): boolean => replayRef.current != null || tourRunning(onboardingRef.current ?? emptyOnboarding()),
    [],
  )

  // the walkthrough owns the line under the bar until its basics are done, so ordinary hints wait
  const tourBasicsLeft = useCallback((): boolean => {
    const s = onboardingRef.current ?? emptyOnboarding()
    const done = replayRef.current ?? s.hints
    return (replayRef.current != null || tourRunning(s)) && tourStep(done, nativeInfoRef.current.hotkey) != null
  }, [])

  // each time the overlay opens: count it, maybe start the examples, and repeat a hotkey failure
  const beginShowing = useCallback(() => {
    updateOnboarding((s) => enrolTour(recordOpen(s), Boolean(calcWindow().__QCALC_NATIVE)))
    setRotation(startRotation(!touringNow() && examplesActive(onboardingRef.current ?? emptyOnboarding())))
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
    setHint(nativeInfoRef.current.hotkeyFailed ? hotkeyFailedText(nativeInfoRef.current.hotkey) : (opened?.text ?? null))
  }, [updateOnboarding])

  // after beginShowing: an open after the first is the shortcut learned (or the menu used, which names it too),
  // and once the basics are done each open brings one "you can also", in place of the open's hint
  const beginTour = useCallback(() => {
    if ((onboardingRef.current?.opens ?? 0) >= 2 || replayRef.current != null) markTour(tourBit('hotkey'))
    if (!touringNow() || nativeInfoRef.current.hotkeyFailed) return
    const extra = tourExtra(replayRef.current ?? onboardingRef.current?.hints ?? 0, nativeInfoRef.current.hotkey)
    if (!extra) return
    markTour(extra.bit)
    setHint(extra.text)
    setHintGhost({ hint: extra.text, expr: extra.expr, plain: extra.plain })
  }, [markTour, touringNow])

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
    track('history.undo')
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
  const gamesShown = isGamesCommand(q)
  // the row ↑ and ↓ have picked in the `games` list, from the top; null is back in the bar
  const [gamePick, setGamePick] = useState<number | null>(null)
  if (!gamesShown && gamePick != null) setGamePick(null)
  const keyLabels = useKeyLabels(settings.keybinds)
  const cheats = useMemo(
    () => hostCheats(cheatSheet(nativeInfo.hotkey || undefined, Boolean(calcWindow().__QCALC_NATIVE), keyLabels)),
    [nativeInfo.hotkey, keyLabels],
  )
  const graphCmd = isGraphCommand(q)
  const sysCmd = isSysCommand(q)
  const greeting = isGreeting(q)
  // `periodic` and `hello` answer with a message, not a calculation
  const periodicCmd = isPeriodicCommand(q) || greeting
  const identityCmd = identitySheetFor(q)
  // pong or connect 4
  const pongCmd = gameCommand(q)
  const tutorialCmd = isTutorialCommand(q)
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
    ansPlain && lastAnswer && (Number.isFinite(lastAnswer.n) || isMatrixQuantity(lastAnswer.quantity))
      ? { plain: ansPlain, unit: lastAnswer.quantity != null && !isMatrixQuantity(lastAnswer.quantity) }
      : undefined
  // `isolate x` or `solve for x` alone works on the equation in the row before
  const isolated = isolatePrevious(q, history[history.length - 1]?.expr)
  const chained = !graphCmd && !isolated && chainsFromAnswer(q, chainAnswer)
  const tapeExpr = chained && ansPlain ? chainedHistoryExpr(q, ansPlain) : q
  chainedRef.current = isolated ?? (chainAnswer ? ansWrittenOut(tapeExpr, chainAnswer.plain) : chained ? tapeExpr : '')
  // typeset only when it shows something the bar doesn't, and never for a command's own panel
  const typstExpr = chained ? chainedExpr(q) : q
  const typstShown = useMemo(
    () => settings.typstPreview && !graphCmd && !sysCmd && !periodicCmd && !identityCmd && !pongCmd && !tutorialCmd && !helpShown && !gamesShown && typstPreviewUseful(typstExpr),
    [settings.typstPreview, graphCmd, sysCmd, periodicCmd, identityCmd, pongCmd, tutorialCmd, helpShown, gamesShown, typstExpr],
  )
  const evalOptions = useMemo(
    () => ({
      ...evalSettings,
      ans: lastAns,
      variables: nativeVars,
      measures: nativeMeas,
      quantities: nativeQty,
      functions: liveFns,
      timeVarying,
    }),
    [lastAns, nativeVars, nativeMeas, nativeQty, liveFns, evalSettings, timeVarying],
  )
  // for enter's blank hint, read from a stable callback
  const liveOptionsRef = useRef(evalOptions)
  liveOptionsRef.current = evalOptions
  const liveChainRef = useRef(chained)
  liveChainRef.current = chained
  const sheet = useMemo(() => {
    if (graphCmd || periodicCmd || identityCmd || pongCmd || tutorialCmd || sysCmd) return []
    return evaluateSheet([isolated ?? (chained ? chainedExpr(q) : q)], evalOptions)
  }, [q, isolated, chained, graphCmd, periodicCmd, identityCmd, pongCmd, tutorialCmd, sysCmd, evalOptions])

  const sysParsed = useMemo(() => (sysCmd ? sysCommand(q) : null), [sysCmd, q])
  const sysAnswer = useMemo((): SystemAnswer | null => {
    if (!sysLines || !sysParsed || !('count' in sysParsed) || sysParsed.count !== sysLines.length) return null
    return solveLive(sysLines, defaultUnits, angleMode)
  }, [sysLines, sysParsed, defaultUnits, angleMode])

  const sysShown = sysAnswer?.display ?? (sysParsed && 'hint' in sysParsed && !sysLines ? sysParsed.hint : '')
  const sysShownRef = useRef('')
  sysShownRef.current = sysShown
  const sysMessage = Boolean(sysCmd && (sysAnswer?.message || (sysParsed && 'hint' in sysParsed && !sysLines)))

  const live = sheet[sheet.length - 1]
  let jsDisplay = ''
  if (graphCmd) jsDisplay = graphIntent?.label ? `graph ${graphIntent.label}` : ''
  else if (q.trim()) jsDisplay = live?.display ?? ''
  const jsN = graphCmd || sysCmd ? undefined : live?.value?.kind === 'number' ? live.value.n : undefined
  const nativeUsable = !graphCmd && !sysCmd && !periodicCmd && !identityCmd && !pongCmd && !tutorialCmd && !chained && soulverAngleSafe(q, settings.angleMode)
  const merged = withPhraseAnswer(q, jsDisplay, mergeLiveAnswer(q, jsDisplay, jsN, nativeUsable ? nativeLive : null), {
    enabled: nativeUsable && !hasSoulver(),
    sigFigs: settings.sigFigs,
  })
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
    graphCmd || sysCmd || helpShown || gamesShown || periodicCmd || identityCmd || pongCmd || tutorialCmd
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
    native: Boolean(display) && !fromJs && !sysCmd,
    facts: {
      answer: display,
      variable: fromJs && live?.kind === 'assignment' ? live.variable : undefined,
      unit: Boolean(steppableRef.current?.unit),
      chained,
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
    if (display || !q.trim() || graphCmd || sysCmd || helpShown || gamesShown || periodicCmd || identityCmd || pongCmd || tutorialCmd || looksLikeNaturalLanguage(q)) return
    const t = window.setTimeout(() => {
      const ok = (text: string) => Boolean(evaluateSheet([chained ? chainedExpr(text) : text], evalOptions)[0]?.display)
      const names = { variables: Object.keys(nativeVars), functions: Object.keys(liveFns) }
      setSquiggle({ q, span: blankReason(q, ok, names) })
    }, SQUIGGLE_IDLE_MS)
    return () => window.clearTimeout(t)
  }, [q, display, chained, graphCmd, sysCmd, helpShown, gamesShown, periodicCmd, identityCmd, pongCmd, tutorialCmd, evalOptions, nativeVars, liveFns])

  const examples = useMemo(
    () => exampleList(firstRun && nativeInfo.hotkey ? nativeInfo.hotkey : undefined),
    [firstRun, nativeInfo.hotkey],
  )
  // the walkthrough: the step it's on, its example in the empty bar, and the line under it while that still fits
  const tourDone = replay ?? tourSeen
  const tourOn = replay != null || tourRunning({ ...(onboardingRef.current ?? emptyOnboarding()), hints: tourSeen })
  const step = tourOn ? tourStep(tourDone, nativeInfo.hotkey) : null
  const ghost = step ?? (hintGhost && hint === hintGhost.hint ? hintGhost : null)
  const example = !q && !helpShown ? (ghost ? { expr: ghost.expr, plain: ghost.plain } : rotationItem(rotation, examples)) : undefined
  const exampleId = ghost ? -1 - (step ? tourProgress(tourDone, nativeInfo.hotkey).at : 0) : rotation.tick
  const exampleExpr = example && !example.plain ? example.expr : ''
  const exampleShown = useMemo(
    () => (exampleExpr ? exampleAnswer(evaluateSheet([prettyTokens(exampleExpr)], evalSettings)[0]) : ''),
    [exampleExpr, evalSettings],
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
      if (tourBasicsLeft()) return
      const next = pickHint(onboardingRef.current?.hints ?? 0, { expr: qRef.current, ...liveRef.current.facts }, 'pause')
      if (!next) return
      updateOnboarding((s) => ({ ...s, hints: s.hints | next.bit }))
      setHint(next.text)
    }, HINT_PAUSE_MS)
    return () => window.clearTimeout(t)
  }, [pauseHintable, q, display, tourBasicsLeft, updateOnboarding])

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
    setUsageSharing(settings.shareUsage)
  }, [settings.shareUsage])

  useEffect(() => {
    if (helpShown) track('help')
  }, [helpShown])

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
    // the walkthrough's copy step, and the ⌘C hint it already taught
    const basicsLeft = tourBasicsLeft()
    markTour(tourBit('copy') | 16)
    if (basicsLeft && !tourBasicsLeft()) setHint(TOUR_DONE_HINT)
    setCopied(true)
    window.clearTimeout(copiedTimer.current)
    copiedTimer.current = window.setTimeout(() => setCopied(false), 1200)
  }, [markTour, tourBasicsLeft])

  const copyValue = useCallback(
    (text: string) => {
      if (!text || isImproperUnitConversion(text)) return
      copyText(chemCopyText(text))
      flashCopied()
    },
    [flashCopied],
  )

  // an answer, as opposed to a whole line, leaves its unit behind when the setting says so
  const answerCopy = useCallback(
    (text: string) => (settingsRef.current.copyUnitless ? unitlessAnswer(text) : text),
    [],
  )
  const copyAnswer = useCallback((text: string) => copyValue(answerCopy(text)), [answerCopy, copyValue])

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
    track('copy.answer')
    copyAnswer(row ? rowCopyText(row, settings.answerForm) : shownLive)
  }, [copyAnswer, history, selected, settings.answerForm, shownLive])

  // exact forms read with their symbols, like the line they sit next to
  const copyLine = useCallback(() => {
    const row = selected != null ? history[selected] : null
    track('copy.line')
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
      if (!row?.expr) return
      track('history.insert')
      insertPlain(row.expr)
    },
    [history, insertPlain],
  )

  const insertHistoryAnswer = useCallback(
    (index: number) => {
      const row = history[index]
      if (!row) return
      track('history.insert')
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
      else {
        track('history.insert')
        insertPlain(insertableHistoryAnswer(row, settings.answerForm, settings.sigFigs), true)
      }
    },
    [history, insertHistoryExpr, insertPlain, settings.answerForm, settings.historyInsert, settings.sigFigs],
  )

  // `quiet` is the commit-on-hide path: nobody is looking, so no hint is spent on it.
  // false when there was nothing to save
  const commit = useCallback((quiet = false): boolean => {
    const sysNow = sysLinesRef.current
    const expr = sysNow ? 'sys' : chainedRef.current || qRef.current
    const { display: liveDisplay, exact, n, meas, quantity, solve, fnDef: graphFn, native, facts } = liveRef.current
    const fnDef = graphFn ?? parseFunctionDef(expr.trim())
    const isGraph = isGraphCommand(expr)
    const written = sysNow?.map((line) => line.trim()).filter(Boolean).join('; ') ?? ''
    const shown = sysNow ? sysShownRef.current.trim() || written : liveDisplay || (fnDef ? fnDefText(fnDef) : '')
    if (!expr.trim() || !shown || isImproperUnitConversion(shown)) {
      if (expr.trim() && !quiet) track(isImproperUnitConversion(shown) ? 'error.unit' : 'error.blank')
      return false
    }
    track(
      calcKind({
        expr,
        system: Boolean(sysNow),
        graph: isGraph,
        fn: Boolean(fnDef),
        solve: Boolean(solve),
        assign: Boolean(facts.variable),
        unit: facts.unit,
        chained: Boolean(chainedRef.current) && !sysNow,
        native,
      }),
    )
    const basicsLeft = tourBasicsLeft()
    const nextHint = quiet || basicsLeft ? null : pickHint(onboardingRef.current?.hints ?? 0, { expr, ...facts })
    updateOnboarding((s) => ({ ...recordCommit(s), hints: s.hints | (nextHint?.bit ?? 0) }))
    markTour(tourUsed({ expr, unit: facts.unit || quantity != null, chained: facts.chained, solve: Boolean(solve), graph: isGraph }))
    const at = Date.now()
    setRecentNow(at)
    setHint(!quiet && basicsLeft && !tourBasicsLeft() ? TOUR_DONE_HINT : (nextHint?.text ?? null))
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
    return true
  }, [markTour, settleArms, settleSmoke, settleSixtyNine, stopDraftTimer, tourBasicsLeft, updateOnboarding])

  const stepForm = tabForm.step
  const onTab = useCallback(
    (dir: 1 | -1) => {
      if (selected != null) return
      track('form.tab')
      stepForm(dir)
    },
    [selected, stepForm],
  )

  const onPrefixStep = useCallback((dir: 1 | -1) => {
    const base = steppableRef.current
    const next = base ? stepPrefix(base, dir) : null
    if (!next?.unitId) return
    track('prefix.step')
    setPrefixUnit(next.unitId)
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
    setPongOpen(null)
    if (isHelpCommand(expr) || isPeriodicCommand(expr) || isIdentityCommand(expr) || gameCommand(expr) || isGamesCommand(expr) || isTutorialCommand(expr) || isGreeting(expr)) {
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
    // in the `games` list, ↑ starts at the game nearest the bar and climbs
    if (isGamesCommand(qRef.current)) {
      setGamePick((cur) => (cur == null ? GAMES.length - 1 : Math.max(0, cur - 1)))
      return true
    }
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
  }, [helpOpen, history.length, selected, setGamePick, snapshotCaret, tapeOpen])

  const onDown = useCallback((): boolean => {
    if (isGamesCommand(qRef.current)) {
      if (gamePick == null) return false
      setGamePick(gamePick >= GAMES.length - 1 ? null : gamePick + 1)
      return true
    }
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
  }, [gamePick, history.length, restoreCaret, selected, setGamePick, tapeOpen])

  // `tutorial` in the bar, or replay from settings: the walkthrough from the top, whatever was done before
  const startTutorial = useCallback(() => {
    replayRef.current = 0
    setReplay(0)
    setRotation(ROTATION_OFF)
    setHint(null)
    resetToCalculate()
  }, [resetToCalculate])

  const skipTutorial = useCallback(() => {
    if (replayRef.current != null) {
      replayRef.current = null
      setReplay(null)
    } else updateOnboarding((s) => ({ ...s, hints: s.hints | TOUR_OFF }))
    setHint('skipped · type tutorial any time to see it again')
    mathRef.current?.focus()
  }, [updateOnboarding])

  // enter that saved nothing says why, in the hint line; the squiggle shows where
  const blankEnterHint = useCallback((): string => {
    const expr = qRef.current
    const ok = (text: string) => Boolean(evaluateSheet([liveChainRef.current ? chainedExpr(text) : text], liveOptionsRef.current)[0]?.display)
    const names = { variables: Object.keys(liveOptionsRef.current.variables ?? {}), functions: Object.keys(liveOptionsRef.current.functions ?? {}) }
    const graph = isGraphCommand(expr)
    return enterHint({
      expr,
      span: graph || !expr.trim() ? null : blankReason(expr, ok, names),
      improper: isImproperUnitConversion(liveRef.current.display),
      naturalLanguage: looksLikeNaturalLanguage(expr),
      bareGraph: graph && /^\s*graph\s*$/i.test(expr),
      history: historyRef.current.length > 0,
    })
  }, [])

  // from typing a game's name and ↵, or a row of the `games` sheet
  const playGame = useCallback(
    (game: GameKind) => {
      if (!peerTransport() && gameNeedsLink(game)) return
      resetToCalculate()
      setTapeOpen(false)
      setPongOpen(game)
    },
    [resetToCalculate],
  )

  const onEnter = useCallback((alt = false) => {
    const opening = sysCommand(qRef.current)
    if (selected == null && opening && 'count' in opening && sysLinesRef.current?.length !== opening.count) {
      setSysLines(Array.from({ length: opening.count }, () => ''))
      return
    }
    if (opening && 'count' in opening && sysLinesRef.current?.length === opening.count) {
      // nothing typed yet: enter goes down to the first equation rather than saving an empty system
      const first = document.querySelector<HTMLInputElement>('.sys-eq')
      if (first && sysLinesRef.current.every((line) => !line.trim())) first.focus()
      else commit()
      return
    }
    if (isHelpCommand(qRef.current)) {
      markTour(tourBit('help'))
      resetToCalculate()
    } else if (isGreeting(qRef.current)) resetToCalculate()
    else if (selected == null && isTutorialCommand(qRef.current)) startTutorial()
    else if (selected == null && isSkipCommand(qRef.current) && tourBasicsLeft()) {
      skipTutorial()
      resetToCalculate()
    } else if (selected == null && isPeriodicCommand(qRef.current)) {
      markTour(tourBit('periodic'))
      track('periodic')
      if (!openNativePeriodicTable()) setPeriodicOpen(true)
      resetToCalculate()
    } else if (selected == null && isIdentityCommand(qRef.current)) {
      const sheet = identitySheetFor(qRef.current)!
      if (!openNativeIdentitySheet(sheet)) setIdentityOpen(sheet)
      resetToCalculate()
    } else if (selected == null && gameCommand(qRef.current)) playGame(gameCommand(qRef.current)!)
    else if (selected == null && isGamesCommand(qRef.current)) {
      // ↵ plays the row ↑ ↓ picked; with none picked the list stays up, for a name or a click
      if (gamePick != null) playGame(GAMES[gamePick]!.kind)
    } else if (selected != null && history[selected]?.kind === 'system') openHistorySystem(selected)
    else if (selected != null && alt) insertHistoryOther(selected)
    else if (selected != null) insertHistoryAnswer(selected)
    else if (!commit()) setHint(blankEnterHint())
  }, [blankEnterHint, commit, gamePick, markTour, resetToCalculate, selected, history, insertHistoryAnswer, insertHistoryOther, openHistorySystem, playGame, skipTutorial, startTutorial, tourBasicsLeft])

  // esc leaves the overlay, except from pong, which it closes back to the bar. false tells the mac app to hide;
  // the page does not clear the tape or the input first.
  const escapeLayer = useCallback((): boolean => {
    if (!pongOpenRef.current) return false
    // ember & frost steps back through its own screens first, and only closes from its title
    if (pongOpenRef.current === 'ember' && emberEscRef.current?.()) return true
    pongOpenRef.current = null
    setPongOpen(null)
    return true
  }, [])

  // so the mac app hands esc to the page while pong is up
  useEffect(() => {
    nativeHandler()?.postMessage({ type: 'escapeLayer', on: Boolean(pongOpen) })
    if (!pongOpen) mathRef.current?.focus()
  }, [pongOpen])

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
    const keyActions: Record<KeyAction, (() => void) | undefined> = {
      // the host's global shortcut, never seen here
      show: undefined,
      settings: nativeHandler()
        ? () => {
            track('settings.open')
            nativeHandler()?.postMessage({ type: 'openSettings' })
          }
        : undefined,
      copyAnswer: copyOutput,
      copyLine,
      angle: () => setSettings(toggleAngleMode),
      fraction: () => setSettings(toggleFractionMode),
      sigFigs: () => setSettings(toggleSigFigMode),
      clear: clearHistory,
    }
    const onKey = (e: KeyboardEvent) => {
      if (keyRecorder.active) return
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
      const cmd = commandHeld(e) && !e.altKey
      const cmdOnly = cmd && !e.shiftKey
      const mainInput = e.target instanceof HTMLInputElement && e.target.classList.contains('quick-plain')
      const bound = actionForEvent(e, settingsRef.current.keybinds, isWindowsHost())
      let action: (() => void) | undefined
      if (bound) action = keyActions[bound]
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
      putOnClipboard(e, chemCopyText(answerCopy(text)))
      flashCopied()
    }
    window.addEventListener('keydown', onKey, true)
    window.addEventListener('copy', onCopy, true)
    return () => {
      window.removeEventListener('keydown', onKey, true)
      window.removeEventListener('copy', onCopy, true)
    }
  }, [answerCopy, clearHistory, copyLine, copyOutput, embedded, escapeLayer, flashCopied, history, onClose, onWillHide, removeRow, resetToCalculate, selected, settings.answerForm, shownLive, undoTape])

  useEffect(() => () => stopDraftTimer(), [stopDraftTimer])

  // warm the engine (mathjs, unit tables) off the first keystroke's critical path
  useEffect(() => {
    const t = window.setTimeout(() => evaluateSheet(['1+1', 'x^3 = 2']), 0)
    return () => window.clearTimeout(t)
  }, [])

  useEffect(() => {
    // an equation js can't solve would come back from soulvercore as something else
    if (!q.trim() || !hasNativeEval() || isGraphCommand(q) || isSysCommand(q) || isHelpCommand(q) || isPeriodicCommand(q) || isIdentityCommand(q) || gameCommand(q) || isGamesCommand(q) || isTutorialCommand(q) || isGreeting(q) || isEquation(q) || isIsolateCommand(q) || parseSystemCall(q)) return
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
      track('open')
      flushSync(() => {
        onPrepare()
        beginShowing()
        beginTour()
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
      // the walkthrough's first step teaches the shortcut in place of the examples
      setRotation(startRotation(!touringNow()))
    }
    w.__qcalcShowTips = () => {
      setRotation(ROTATION_OFF)
      setHint(null)
      setHelpOpen(true)
      markTour(tourBit('help'))
    }
    w.__qcalcTutorial = startTutorial
    if (w.__QCALC_FIRST_RUN) {
      beginShowing()
      beginTour()
      w.__qcalcFirstRun()
    } else if (!w.__QCALC_NATIVE && !pageOpenCounted) {
      pageOpenCounted = true
      beginShowing()
      beginTour()
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
  }, [beginShowing, beginTour, insertPlain, markTour, touringNow, onPrepare, onWillHide, startTutorial])

  useTapeWheel(rootRef, tapeRef, {
    longInput: () => {
      const input = mathRef.current?.element()
      return input && input.scrollWidth > input.clientWidth + 1 ? input : null
    },
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
    const typed = text !== qRef.current
    if (typed) {
      lastKeyRef.current = Date.now()
      tapeUndoRef.current = null
    }
    qRef.current = text
    setQ(text)
    const opened = sysLinesRef.current
    const cmd = sysCommand(text)
    if (opened) {
      if (!cmd || !('count' in cmd) || cmd.count !== opened.length) setSysLines(null)
    } else if (typed && cmd && 'count' in cmd) {
      // `sys3` opens its three fields as soon as the count is typed; no enter needed
      const lines = Array.from({ length: cmd.count }, () => '')
      sysLinesRef.current = lines
      setSysLines(lines)
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
          // the smoke's room is the desktop to the eye, so a click there is a click away
          const inRoom = t === e.currentTarget || t.classList.contains('four-twenty-room')
          if (inRoom && openSmokeRoom(e.currentTarget)) return dismissNative()
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
          {pongOpen === 'pong' && peerTransport() ? (
            <PongPanel transport={peerTransport()!} onClose={escapeLayer} />
          ) : pongOpen === 'connect4' && peerTransport() ? (
            <Connect4Panel transport={peerTransport()!} onClose={escapeLayer} />
          ) : pongOpen === 'ember' ? (
            <EmberPanel transport={peerTransport()} escapeRef={emberEscRef} onClose={escapeLayer} />
          ) : pongOpen === 'crack' ? (
            <CrackPanel onClose={escapeLayer} />
          ) : pongOpen === 'equal' ? (
            <EqualPanel onClose={escapeLayer} />
          ) : pongOpen === 'numbers' ? (
            <NumbersPanel onClose={escapeLayer} />
          ) : pongOpen === 'trails' ? (
            <TrailsPanel onClose={escapeLayer} />
          ) : helpShown ? (
            <CheatSheet cheats={cheats} />
          ) : gamesShown ? (
            <GamesSheet games={GAMES} selected={gamePick} onPlay={playGame} />
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

          <div className={pongOpen ? 'composer composer-hidden' : 'composer'}>
            <SixtyNineFold active={sixtyNine} />
            <EdgeTools
              settings={settings}
              keyLabels={keyLabels}
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
              example={example ? { text: example.expr, id: exampleId } : null}
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
              example={example ? { tick: exampleId, answer: exampleShown } : null}
              display={sysCmd ? sysShown : greeting ? GREETING_REPLY : periodicCmd ? PERIODIC_HINT : identityCmd ? identityCmd.hint : tutorialCmd ? TUTORIAL_HINT : gamesShown ? GAMES_HINT : pongCmd ? gameHint(pongCmd, hasPeerTransport()) : graphCmd ? '' : display}
              exact={liveExact}
              shown={shownLive}
              steady={steady}
              formTick={tabForm.tick}
              onCopy={copyAnswer}
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
              message={sysMessage || periodicCmd || gamesShown || Boolean(identityCmd) || tutorialCmd || Boolean(pongCmd) || Boolean(liveSolve && liveSolve.outcome !== 'roots')}
            />
            )}
          </div>
          {typstShown && !pongOpen ? (
            <TypstPreview
              expr={typstExpr}
              answer={
                // a message ("no real solution") or a command label isn't an answer to typeset
                graphCmd || periodicCmd || identityCmd || pongCmd || tutorialCmd || sysMessage || (liveSolve && !rootsOf)
                  ? ''
                  : typstAnswer(liveExact, display)
              }
              solvedFor={rootsOf}
            />
          ) : null}
          {live?.time && !pongOpen && !graphCmd && !sysCmd && (live.time.varying.length || live.time.constant.length) ? (
            <TimeLetters
              letters={live.time}
              onToggle={(letter, varying) => setTimeVarying((prev) => ({ ...prev, [letter]: varying }))}
            />
          ) : null}
          {hint && !pongOpen ? (
            <div className="composer-hint" role="status">
              {hostKeys(hint)}
            </div>
          ) : step && !pongOpen && !helpShown && tourLineFits(step, q, Boolean(shownLive)) ? (
            <TourLine text={hostKeys(step.text)} progress={tourProgress(tourDone, nativeInfo.hotkey)} onSkip={skipTutorial} />
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
      {identityOpen ? <IdentityCard sheet={identityOpen} onClose={() => setIdentityOpen(null)} /> : null}
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
          <CopyUnitlessSettings
            value={settings.copyUnitless}
            onChange={(copyUnitless) => setSettings((s) => ({ ...s, copyUnitless }))}
          />
          <KeybindSettings
            value={settings.keybinds}
            windows={isWindowsHost()}
            settingsWindow={false}
            onChange={(keybinds) => setSettings((s) => ({ ...s, keybinds }))}
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
