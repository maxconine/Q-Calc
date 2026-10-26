// The app logic the native bar needs that the Rust core doesn't answer yet (WP-APP-LOGIC, WP-NOTATION), run as
// the TS itself inside JavaScriptCore (no web view, no WebContent process): src/lib's blankReason, missingUnit,
// knownWords, bounds, undo, onboarding, tour, analytics, completion and typstMath. Engine pieces they import are
// shims (applogic/shims) that call back into the Rust engine through `__host`.
//
// Swift calls `QCalcLogic.call(module, fn, argsJson)`, the same shape as Engine.callJson, and uses this only when
// the engine answers CoreError.Unported, so each function moves to Rust without a change here.
import * as analytics from '../../../src/lib/analytics'
import * as blank from '../../../src/lib/blankReason'
import * as bounds from '../../../src/lib/bounds'
import * as completion from '../../../src/lib/completion'
import * as knownWords from '../../../src/lib/knownWords'
import * as missing from '../../../src/lib/missingUnit'
import * as onboarding from '../../../src/lib/onboarding'
import * as tour from '../../../src/lib/tour'
import * as typst from '../../../src/lib/typstMath'
import * as undo from '../../../src/lib/undo'

declare const __host: { ok(text: string): boolean }

// functions whose `ok` argument is a callback: Swift passes the rest, the host answers `ok`
const ok = (text: string) => __host.ok(text)

const table: Record<string, Record<string, (...args: never[]) => unknown>> = {
  'lib/blankReason': {
    blankReason: (expr: string, names?: { variables?: string[]; functions?: string[] }) => blank.blankReason(expr, ok, names),
    enterHint: blank.enterHint as never,
  },
  'lib/missingUnit': { missingUnit: (expr: string) => missing.missingUnit(expr, ok) },
  'lib/knownWords': { knownWordSpans: knownWords.knownWordSpans as never },
  'lib/bounds': {
    boundsIn: bounds.boundsIn as never,
    isHidden: bounds.isHidden as never,
    snapOffset: bounds.snapOffset as never,
    afterTyping: bounds.afterTyping as never,
    boundKey: bounds.boundKey as never,
    wordToSign: bounds.wordToSign as never,
  },
  'lib/undo': {
    undoStart: undo.undoStart as never,
    editKind: undo.editKind as never,
    recordEdit: undo.recordEdit as never,
    breakRun: undo.breakRun as never,
    undo: undo.undo as never,
    redo: undo.redo as never,
  },
  'lib/onboarding': {
    sanitizeOnboarding: onboarding.sanitizeOnboarding as never,
    examplesActive: onboarding.examplesActive as never,
    recordOpen: onboarding.recordOpen as never,
    recordCommit: onboarding.recordCommit as never,
    exampleList: onboarding.exampleList as never,
    pickHint: onboarding.pickHint as never,
    cheatSheet: onboarding.cheatSheet as never,
  },
  'lib/tour': {
    tourSteps: tour.tourSteps as never,
    enrolTour: tour.enrolTour as never,
    tourRunning: tour.tourRunning as never,
    tourStep: tour.tourStep as never,
    tourProgress: tour.tourProgress as never,
    tourExtra: tour.tourExtra as never,
    tourUsed: tour.tourUsed as never,
    tourLineFits: tour.tourLineFits as never,
    tourBit: tour.tourBit as never,
    isTutorialCommand: tour.isTutorialCommand as never,
    isSkipCommand: tour.isSkipCommand as never,
  },
  'lib/analytics': {
    localDay: analytics.localDay as never,
    newInstallId: analytics.newInstallId as never,
    sanitizeStash: analytics.sanitizeStash as never,
    bump: analytics.bump as never,
    dueToSend: analytics.dueToSend as never,
    afterSend: analytics.afterSend as never,
    usagePayload: analytics.usagePayload as never,
    calcKind: analytics.calcKind as never,
  },
  'lib/completion': { completionFor: completion.completionFor as never },
  'lib/typstMath': {
    typstDocument: typst.typstDocument as never,
    typstPreviewUseful: typst.typstPreviewUseful as never,
    copiedEquation: typst.copiedEquation as never,
    toTypstMath: typst.toTypstMath as never,
  },
}

const constants = {
  'lib/onboarding': { EXAMPLE_MS: onboarding.EXAMPLE_MS, HINT_PAUSE_MS: onboarding.HINT_PAUSE_MS },
  'lib/tour': { TOUR_DONE_HINT: tour.TOUR_DONE_HINT, TUTORIAL_HINT: tour.TUTORIAL_HINT },
  'lib/undo': { UNDO_RUN_MS: undo.UNDO_RUN_MS },
  'lib/analytics': { ANALYTICS_URL: analytics.ANALYTICS_URL, USAGE_EVENTS: analytics.USAGE_EVENTS },
}

/** JSON in, JSON out; a thrown error comes back as {"error": "…"}; an unknown name as {"unported": true}. */
function call(module: string, fn: string, argsJson: string): string {
  const f = table[module]?.[fn]
  if (!f) {
    const c = (constants as Record<string, Record<string, unknown>>)[module]?.[fn]
    return c === undefined ? JSON.stringify({ unported: true }) : JSON.stringify({ value: c })
  }
  try {
    const args = JSON.parse(argsJson) as never[]
    const value = f(...args)
    return JSON.stringify({ value: value === undefined ? null : value })
  } catch (e) {
    return JSON.stringify({ error: String(e) })
  }
}

;(globalThis as Record<string, unknown>).QCalcLogic = { call }
