import { nativeWindow, type NativeWindow } from './bridge'
import { HOTKEY_FAILED_HINT, type CheatRow } from './onboarding'

type Modifiers = Pick<KeyboardEvent, 'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey' | 'key'>

// set by the windows shell's shim; the mac app and the web page leave it unset
export function isWindowsHost(): boolean {
  return (nativeWindow() as (NativeWindow & { __QCALC_PLATFORM?: string }) | undefined)?.__QCALC_PLATFORM === 'windows'
}

// the mac's ⌘ shortcuts; windows has only ctrl for them
export function commandHeld(e: Modifiers, windows = isWindowsHost()): boolean {
  return windows ? e.ctrlKey && !e.metaKey : e.metaKey && !e.ctrlKey
}

// a working fallback is named, so the user learns the shortcut that does open it; windows words its own
export function hotkeyFailedText(active?: string, windows = isWindowsHost()): string {
  return active && !windows ? `your shortcut is taken, so ${active} opens Q Calc — change it in the menu` : HOTKEY_FAILED_HINT
}

const WINDOWS_TEXT = new Map([[HOTKEY_FAILED_HINT, 'shortcut in use by another app · change it from the tray']])
const WINDOWS_MODIFIERS: Array<[glyphs: string, name: string]> = [
  ['⌘⌃', 'Ctrl'],
  ['⌥', 'Alt'],
  ['⇧', 'Shift'],
]

function windowsChord(glyphs: string): string {
  return WINDOWS_MODIFIERS.filter(([g]) => [...g].some((c) => glyphs.includes(c)))
    .map(([, name]) => `${name}+`)
    .join('')
}

// hints and cheat rows are written with mac glyphs; the windows shell reads them its own way
export function hostKeys(text: string, windows = isWindowsHost()): string {
  if (!windows) return text
  return (
    WINDOWS_TEXT.get(text) ??
    text.replace(/⌃C(?![A-Za-z])/g, 'Ctrl+Shift+⌫').replace(/[⌘⌃⌥⇧]+/g, windowsChord)
  )
}

export function hostCheats(columns: CheatRow[][], windows = isWindowsHost()): CheatRow[][] {
  return columns.map((rows) => rows.map(([key, label]): CheatRow => [hostKeys(key, windows), label]))
}
