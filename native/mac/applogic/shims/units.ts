// engine/units for the app-logic bundle: answered by the Rust core (units/, WP-UNITS)
import { engine } from './host'

export const isUnitName = (word: string): boolean => engine('engine/units', 'isUnitName', [word], false)
export const readsAsUnit = (text: string): boolean => engine('engine/units', 'readsAsUnit', [text], false)
export const isImproperUnitConversion = (text: string): boolean =>
  engine('engine/units', 'isImproperUnitConversion', [text], /improper unit conversion/i.test(text))
