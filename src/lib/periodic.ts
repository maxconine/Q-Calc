import { nativeHandler } from './bridge'
import { ELEMENTS, gridCell, insertableMass } from './elements'

export const PERIODIC_HINT = 'periodic table'

export function isPeriodicCommand(text: string): boolean {
  return /^periodic(\s+table)?$/i.test(text.trim())
}

// the mac app draws the table in its own window from this, so the data lives in one place; false means no native window
export function openNativePeriodicTable(): boolean {
  const native = nativeHandler()
  if (!native) return false
  const elements = ELEMENTS.map((e) => {
    const { row, col } = gridCell(e)
    return { n: e.n, symbol: e.symbol, name: e.name, mass: e.mass, insert: insertableMass(e), row, col, category: e.category }
  })
  native.postMessage({ type: 'periodic', elements })
  return true
}
