// engine/parens for the app-logic bundle: answered by the Rust core (parens.rs)
import { engine } from './host'

export const fillParens = (text: string): string => engine('engine/parens', 'fillParens', [text], text)
export const inferParens = (text: string): unknown => engine('engine/parens', 'inferParens', [text], null)
export const autofillParens = (text: string): string => engine('engine/parens', 'autofillParens', [text], text)
