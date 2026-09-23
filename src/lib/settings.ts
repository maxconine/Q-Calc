import { clampSigFigs, DEFAULT_SIG_FIGS } from '../engine/format'
import { defaultUnitsEqual, sanitizeDefaultUnits, type DefaultUnits } from '../engine/units'
import { normalizeHistoryInsert, type AnswerForm, type HistoryInsert } from './answer'
import { clampDraftSeconds, DEFAULT_DRAFT_SECONDS } from './draft'
import { normalizeTheme, type Theme } from './theme'

export type AngleMode = 'deg' | 'rad'

export type Settings = {
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

// pushed by the mac app alongside settings; never stored by the web view
export type NativeInfo = { hotkey?: string; hotkeyFailed?: boolean }

export function defaultSettings(): Settings {
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

export function mergeSettings(partial: Partial<Settings> | undefined, base: Settings): Settings {
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

export function settingsEqual(a: Settings, b: Settings): boolean {
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

export function mergeNativeInfo(partial: NativeInfo | undefined, base: Required<NativeInfo>): Required<NativeInfo> {
  return {
    hotkey: typeof partial?.hotkey === 'string' ? partial.hotkey : base.hotkey,
    hotkeyFailed: typeof partial?.hotkeyFailed === 'boolean' ? partial.hotkeyFailed : base.hotkeyFailed,
  }
}

export const toggleAngleMode = (s: Settings): Settings => ({ ...s, angleMode: s.angleMode === 'deg' ? 'rad' : 'deg' })
export const toggleFractionMode = (s: Settings): Settings => ({ ...s, fractionMode: !s.fractionMode })
export const toggleSigFigMode = (s: Settings): Settings => ({ ...s, sigFigMode: !s.sigFigMode })
