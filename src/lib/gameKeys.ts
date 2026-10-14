import { nativeWindow } from './bridge'

// keyboard helpers the game panels share

export const plainKey = (e: KeyboardEvent) => !e.metaKey && !e.ctrlKey && !e.altKey

// keys a game takes for itself; everything else (⌘C, ⌃D…) still reaches the bar. the mac app's boot script
// has already buffered it for the hidden input, so drop it from there too
export function swallow(e: KeyboardEvent): void {
  e.preventDefault()
  e.stopPropagation()
  const w = nativeWindow()
  if (w?.__QCALC_KEYS?.length) w.__QCALC_KEYS = []
}

// one keydown listener that runs before every game's own: it's added when this module loads, ahead of any
// panel's. the open game's shell sets the gate; a gate that returns true takes the key from everyone else.
// both live on the window, so a reloaded module (vite's hot reload) finds the same ones
type Gate = (e: KeyboardEvent) => boolean
type GateWindow = Window & { __qcalcKeyGate?: Gate | null; __qcalcKeyGateOn?: boolean }

export function setKeyGate(fn: Gate): () => void {
  const w = nativeWindow() as GateWindow | undefined
  if (w) w.__qcalcKeyGate = fn
  return () => {
    if (w?.__qcalcKeyGate === fn) w.__qcalcKeyGate = null
  }
}

const gw = nativeWindow() as GateWindow | undefined
if (gw && !gw.__qcalcKeyGateOn) {
  gw.__qcalcKeyGateOn = true
  gw.addEventListener(
    'keydown',
    (e) => {
      if (!gw.__qcalcKeyGate?.(e)) return
      swallow(e)
      e.stopImmediatePropagation()
    },
    true,
  )
}
