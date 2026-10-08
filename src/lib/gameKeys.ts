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
