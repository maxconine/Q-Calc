// the overlay's own shortcuts, chosen by the user. a chord is "ctrl+alt+shift+cmd+key" in that order, key names
// from the physical key, so the mac settings window, the windows shell and the page all spell them the same way
export type KeyAction = 'show' | 'settings' | 'copyAnswer' | 'copyLine' | 'angle' | 'fraction' | 'sigFigs' | 'clear'

// only the overrides are stored; '' means the action has no key
export type Keybinds = Partial<Record<KeyAction, string>>

// `show` is the host's global shortcut; the page lists it but the host registers it
export const KEY_ACTIONS: ReadonlyArray<{ id: KeyAction; label: string }> = [
  { id: 'show', label: 'Show / hide Q Calc' },
  { id: 'settings', label: 'Open settings' },
  { id: 'copyAnswer', label: 'Copy answer' },
  { id: 'copyLine', label: 'Copy whole line' },
  { id: 'angle', label: 'Degrees / radians' },
  { id: 'fraction', label: 'Fractions' },
  { id: 'sigFigs', label: 'Sig figs from input' },
  { id: 'clear', label: 'Clear history' },
]

// on while a settings row waits for keys, so the overlay doesn't act on them too
export const keyRecorder = { active: false }

const ACTION_IDS = new Set<string>(KEY_ACTIONS.map((a) => a.id))

const MAC_DEFAULTS: Record<KeyAction, string> = {
  show: 'ctrl+alt+space',
  settings: 'cmd+,',
  copyAnswer: 'cmd+c',
  copyLine: 'shift+cmd+c',
  angle: 'ctrl+d',
  fraction: 'ctrl+f',
  sigFigs: 'ctrl+s',
  clear: 'ctrl+c',
}

// windows keeps ctrl+c for copy, so clearing moves to ctrl+shift+backspace
const WINDOWS_DEFAULTS: Record<KeyAction, string> = {
  show: 'alt+space',
  settings: 'ctrl+,',
  copyAnswer: 'ctrl+c',
  copyLine: 'ctrl+shift+c',
  angle: 'ctrl+d',
  fraction: 'ctrl+f',
  sigFigs: 'ctrl+s',
  clear: 'ctrl+shift+backspace',
}

export function defaultKeybind(action: KeyAction, windows: boolean): string {
  return (windows ? WINDOWS_DEFAULTS : MAC_DEFAULTS)[action]
}

export function keybindFor(binds: Keybinds, action: KeyAction, windows: boolean): string {
  const own = binds[action]
  return typeof own === 'string' ? own : defaultKeybind(action, windows)
}

const MODIFIERS = ['ctrl', 'alt', 'shift', 'cmd'] as const

const CODE_KEYS: Record<string, string> = {
  Comma: ',',
  Period: '.',
  Slash: '/',
  Backslash: '\\',
  Semicolon: ';',
  Quote: "'",
  BracketLeft: '[',
  BracketRight: ']',
  Minus: '-',
  Equal: '=',
  Backquote: '`',
  Space: 'space',
  Backspace: 'backspace',
  Delete: 'delete',
  Enter: 'enter',
  NumpadEnter: 'enter',
  Tab: 'tab',
  Escape: 'escape',
  ArrowUp: 'up',
  ArrowDown: 'down',
  ArrowLeft: 'left',
  ArrowRight: 'right',
  Home: 'home',
  End: 'end',
  PageUp: 'pageup',
  PageDown: 'pagedown',
}

const KEY_NAMES: Record<string, string> = {
  ' ': 'space',
  '+': 'plus',
  arrowup: 'up',
  arrowdown: 'down',
  arrowleft: 'left',
  arrowright: 'right',
  esc: 'escape',
  del: 'delete',
  return: 'enter',
}

const MODIFIER_KEYS = new Set(['shift', 'meta', 'control', 'alt', 'altgraph', 'capslock', 'fn', 'os', 'hyper', 'super'])

type KeyEventLike = Pick<KeyboardEvent, 'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey' | 'key'> & { code?: string }

// the physical key when the browser says, so ⌥D reads as d and not ∂
export function eventKeyName(e: KeyEventLike): string | null {
  const code = e.code ?? ''
  if (/^Key[A-Z]$/.test(code)) return code.slice(3).toLowerCase()
  if (/^Digit[0-9]$/.test(code)) return code.slice(5)
  if (/^F[0-9]{1,2}$/.test(code)) return code.toLowerCase()
  if (CODE_KEYS[code]) return CODE_KEYS[code]
  const key = (e.key ?? '').toLowerCase()
  if (!key || key === 'unidentified' || MODIFIER_KEYS.has(key)) return null
  return KEY_NAMES[key] ?? key
}

// null while only modifiers are down
export function chordFromEvent(e: KeyEventLike): string | null {
  const key = eventKeyName(e)
  if (!key) return null
  const held = { ctrl: e.ctrlKey, alt: e.altKey, shift: e.shiftKey, cmd: e.metaKey }
  return [...MODIFIERS.filter((m) => held[m]), key].join('+')
}

function splitChord(chord: string): { mods: Set<string>; key: string } {
  const parts = chord.split('+')
  // "ctrl++" can't happen: + is spelled plus
  const key = parts.pop() ?? ''
  return { mods: new Set(parts), key }
}

export function isChord(text: string): boolean {
  if (!text) return false
  const { mods, key } = splitChord(text)
  return Boolean(key) && !MODIFIERS.includes(key as never) && [...mods].every((m) => MODIFIERS.includes(m as never))
}

// the stored order, whatever order it came in
export function normalizeChord(text: string): string {
  const { mods, key } = splitChord(text.toLowerCase())
  return [...MODIFIERS.filter((m) => mods.has(m)), key].join('+')
}

// the page reads a keydown as one of its actions; `show` never, the host owns it
export function actionForEvent(e: KeyEventLike, binds: Keybinds, windows: boolean): KeyAction | null {
  const chord = chordFromEvent(e)
  if (!chord) return null
  for (const { id } of KEY_ACTIONS) {
    if (id !== 'show' && keybindFor(binds, id, windows) === chord) return id
  }
  return null
}

const MAC_GLYPHS: Record<string, string> = { ctrl: '⌃', alt: '⌥', shift: '⇧', cmd: '⌘' }
const WINDOWS_NAMES: Record<string, string> = { ctrl: 'Ctrl', alt: 'Alt', shift: 'Shift', cmd: 'Win' }
const MAC_KEY_LABELS: Record<string, string> = {
  space: 'Space',
  backspace: '⌫',
  delete: '⌦',
  enter: '↵',
  tab: '⇥',
  escape: 'esc',
  up: '↑',
  down: '↓',
  left: '←',
  right: '→',
  home: '↖',
  end: '↘',
  pageup: '⇞',
  pagedown: '⇟',
  plus: '+',
}
const WINDOWS_KEY_LABELS: Record<string, string> = {
  space: 'Space',
  backspace: 'Backspace',
  delete: 'Delete',
  enter: 'Enter',
  tab: 'Tab',
  escape: 'Esc',
  up: '↑',
  down: '↓',
  left: '←',
  right: '→',
  home: 'Home',
  end: 'End',
  pageup: 'PgUp',
  pagedown: 'PgDn',
  plus: '+',
}

// ⌃⇧C on the mac, Ctrl+Shift+C on windows
export function chordLabel(chord: string, windows: boolean): string {
  if (!chord) return ''
  const { mods, key } = splitChord(chord)
  const keyText = (windows ? WINDOWS_KEY_LABELS : MAC_KEY_LABELS)[key] ?? key.toUpperCase()
  // ⌃⌥⇧⌘ is also the mac's own order
  const held = MODIFIERS.filter((m) => mods.has(m))
  if (windows) return [...held.map((m) => WINDOWS_NAMES[m]), keyText].join('+')
  return held.map((m) => MAC_GLYPHS[m]).join('') + keyText
}

// the text box's own keys, which a binding would take away
function reserved(chord: string, windows: boolean): string | null {
  const { mods, key } = splitChord(chord)
  const cmd = windows ? 'ctrl' : 'cmd'
  const only = (...want: string[]) => mods.size === want.length && want.every((m) => mods.has(m))
  if (only(cmd)) {
    const names: Record<string, string> = { v: 'paste', x: 'cut', a: 'select all', z: 'undo' }
    if (names[key]) return names[key]
  }
  if (only(cmd, 'shift') && key === 'z') return 'redo'
  if (mods.size === 0 && ['escape', 'enter', 'tab', 'up', 'down', 'left', 'right', 'backspace', 'delete', 'home', 'end'].includes(key)) {
    return 'editing'
  }
  return null
}

// why a chord can't be used, or null when it can. typing keys need ctrl, alt or ⌘ so they don't eat the input
export function chordProblem(chord: string, action: KeyAction, binds: Keybinds, windows: boolean): string | null {
  if (!isChord(chord)) return 'Press a key'
  const { mods, key } = splitChord(chord)
  const fkey = /^f[0-9]{1,2}$/.test(key)
  if (!fkey && !mods.has('ctrl') && !mods.has('alt') && !mods.has('cmd')) {
    return `Add ${windows ? 'Ctrl or Alt' : '⌘, ⌃ or ⌥'} so typing still works`
  }
  const kept = reserved(chord, windows)
  if (kept) return `${chordLabel(chord, windows)} is kept for ${kept}`
  for (const other of KEY_ACTIONS) {
    if (other.id !== action && keybindFor(binds, other.id, windows) === chord) {
      return `${chordLabel(chord, windows)} already does ${other.label.toLowerCase()}`
    }
  }
  return null
}

// a default choice is stored as no override, so a later default change still reaches it
export function setKeybind(binds: Keybinds, action: KeyAction, chord: string, windows: boolean): Keybinds {
  const next = { ...binds }
  if (chord === defaultKeybind(action, windows)) delete next[action]
  else next[action] = chord
  return next
}

export function sanitizeKeybinds(raw: unknown): Keybinds {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return {}
  const out: Keybinds = {}
  for (const [id, value] of Object.entries(raw as Record<string, unknown>)) {
    if (!ACTION_IDS.has(id) || id === 'show' || typeof value !== 'string') continue
    const chord = value.toLowerCase()
    if (chord === '' || isChord(chord)) out[id as KeyAction] = chord && normalizeChord(chord)
  }
  return out
}

export function keybindsEqual(a: Keybinds, b: Keybinds): boolean {
  const keys = new Set([...Object.keys(a), ...Object.keys(b)]) as Set<KeyAction>
  for (const k of keys) if (a[k] !== b[k]) return false
  return true
}
