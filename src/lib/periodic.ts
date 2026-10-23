import { nativeHandler } from './bridge'
import { ELEMENTS, gridCell, insertableMass } from './elements'
import { isWindowsHost } from './platform'

export const PERIODIC_HINT = 'periodic table'

export function isPeriodicCommand(text: string): boolean {
  return /^periodic(\s+table)?$/i.test(text.trim())
}

// a plain ↵, not one finishing an input method's text: the table's formula goes in rather than the bar submitting
export function takesFormulaEnter(e: Pick<KeyboardEvent, 'key' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey' | 'isComposing' | 'repeat'>): boolean {
  return e.key === 'Enter' && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey && !e.isComposing && !e.repeat
}

// the mac app draws the table in its own window from this, so the data lives in one place; false means no native window.
// the windows shell opens its own window on sheet.html, which draws the page's table itself, so it needs no data
export function openNativePeriodicTable(): boolean {
  const native = nativeHandler()
  if (!native) return false
  if (isWindowsHost()) {
    native.postMessage({ type: 'periodic' })
    return true
  }
  const elements = ELEMENTS.map((e) => {
    const { row, col } = gridCell(e)
    return { n: e.n, symbol: e.symbol, name: e.name, mass: e.mass, insert: insertableMass(e), row, col, category: e.category }
  })
  native.postMessage({ type: 'periodic', elements })
  return true
}
