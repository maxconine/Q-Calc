type NativeHandler = { postMessage: (m: string | Record<string, unknown>) => void }
type SoulverHandler = { postMessage: (m: Record<string, unknown> | string) => Promise<unknown> }

export type StoredDraft = { expr: string; savedAt: number }

export type NativeWindow = Window & {
  webkit?: { messageHandlers?: { qcalc?: NativeHandler; soulver?: SoulverHandler } }
  __QCALC_NATIVE?: boolean
  __QCALC_KEYS?: string[]
  __QCALC_SETTINGS?: Record<string, unknown>
  __QCALC_DRAFT?: StoredDraft | null
  __qcalcFocus?: () => void
  __qcalcReset?: () => void
  __qcalcWillHide?: () => void
  __qcalcSize?: () => void
  __qcalcPaste?: (text: string) => void
}

export function nativeWindow(): NativeWindow | undefined {
  return typeof window === 'undefined' ? undefined : (window as NativeWindow)
}

export function nativeHandler(): NativeHandler | undefined {
  return nativeWindow()?.webkit?.messageHandlers?.qcalc
}
