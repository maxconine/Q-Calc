import { hasDualAnswer } from './answer'

export type Onboarding = {
  opens: number
  commits: number
  // bitmask of HINTS bits already shown
  hints: number
  // examples retired for good
  done: boolean
}

export const EXAMPLE_OPENS = 5
export const EXAMPLE_COMMITS = 5
export const EXAMPLE_MS = 2500

export function emptyOnboarding(): Onboarding {
  return { opens: 0, commits: 0, hints: 0, done: false }
}

function count(raw: unknown): number {
  const n = typeof raw === 'number' ? raw : Number(raw)
  return Number.isFinite(n) && n > 0 ? Math.floor(n) : 0
}

export function sanitizeOnboarding(raw: unknown): Onboarding {
  if (!raw || typeof raw !== 'object') return emptyOnboarding()
  const o = raw as Record<string, unknown>
  return {
    opens: count(o.opens),
    commits: count(o.commits),
    hints: count(o.hints) & HINT_ALL,
    done: o.done === true || o.done === 1,
  }
}

// the web and native copies only move forward, so take the furthest of each
export function mergeOnboarding(a: Onboarding, b: Onboarding): Onboarding {
  return {
    opens: Math.max(a.opens, b.opens),
    commits: Math.max(a.commits, b.commits),
    hints: a.hints | b.hints,
    done: a.done || b.done,
  }
}

export function examplesActive(s: Onboarding): boolean {
  return !s.done && s.opens <= EXAMPLE_OPENS && s.commits < EXAMPLE_COMMITS
}

function settle(s: Onboarding): Onboarding {
  return !s.done && !examplesActive(s) ? { ...s, done: true } : s
}

export function recordOpen(s: Onboarding): Onboarding {
  return settle({ ...s, opens: s.opens + 1 })
}

export function recordCommit(s: Onboarding): Onboarding {
  return settle({ ...s, commits: s.commits + 1 })
}

// answers are computed at display time so they follow the settings; `plain` has no answer.
// lead with what no other calculator does: the first three are seen within 8 seconds
type Example = { expr: string; plain?: boolean }

export const EXAMPLES: readonly Example[] = [
  { expr: '5 ft 10 in to cm' },
  { expr: '80 + 15%' },
  { expr: '(5.0 +- 0.2) * 3' },
  { expr: 'graph sin(x)', plain: true },
  { expr: 'sqrt(8)' },
  { expr: '100 W * 2 hr to kWh' },
  { expr: 'x^2 = 2' },
]

// the example answer reads like the live one: exact ≈ approx when there are two, after `x =` for a solve
export function exampleAnswer(
  line: { display?: string; exact?: string; solve?: { variable: string; outcome: string } } | undefined,
): string {
  const display = line?.display ?? ''
  const answer = line?.exact && hasDualAnswer({ display, exact: line.exact }) ? `${line.exact} ≈ ${display}` : display
  return line?.solve?.outcome === 'roots' ? `${line.solve.variable} = ${answer}` : answer
}

export function exampleList(firstRunHotkey?: string): Example[] {
  const lead: Example[] = firstRunHotkey ? [{ expr: `Press ${firstRunHotkey} anytime`, plain: true }] : []
  return [...lead, ...EXAMPLES]
}

// examples run until the first keystroke of a showing
export type Rotation = { on: boolean; tick: number }

export const ROTATION_OFF: Rotation = { on: false, tick: 0 }

export function startRotation(active: boolean): Rotation {
  return { on: active, tick: 0 }
}

export function advanceRotation(r: Rotation): Rotation {
  return r.on ? { on: true, tick: r.tick + 1 } : r
}

export function dismissRotation(r: Rotation): Rotation {
  return r.on ? ROTATION_OFF : r
}

export function rotationItem<T>(r: Rotation, items: readonly T[]): T | undefined {
  if (!r.on || !items.length) return undefined
  return items[r.tick % items.length]
}

export type HintFacts = {
  expr: string
  // the live answer as shown
  answer?: string
  variable?: string
  unit?: boolean
  angleMode?: 'deg' | 'rad'
  fractionMode?: boolean
  native?: boolean
  opens?: number
}

// `pause`: the answer sat still for HINT_PAUSE_MS while typing, so the hint is about the answer on screen.
// `commit`: enter was pressed, so the hint is about what happens next.
// `open`: the overlay just appeared
type HintRule = {
  bit: number
  on: 'pause' | 'commit' | 'open'
  when?: (f: HintFacts) => boolean
  text: string | ((f: HintFacts) => string)
}

export const HINT_PAUSE_MS = 1500

const TRIG = /(?<![A-Za-z])(?:a|arc)?(?:sin|cos|tan)\s*\(/i
const DIVISION = /\d\s*\/\s*\d/
const DECIMAL = /^-?\d*\.\d+$/

// one line each, shown once ever, first match wins. bits are persisted: never reuse or renumber one.
// retired bits: 1 (the old "⌘C copies · Enter saved it"), 4 and 8 (units and trig after enter)
export const HINTS: readonly HintRule[] = [
  { bit: 16, on: 'pause', text: '⌘C copies the answer' },
  {
    bit: 32,
    on: 'pause',
    when: (f) => TRIG.test(f.expr),
    text: (f) => (f.angleMode === 'rad' ? 'radians · ⌃D for degrees' : 'degrees · ⌃D for radians'),
  },
  { bit: 64, on: 'pause', when: (f) => Boolean(f.unit), text: '⌥↑ ⌥↓ step the unit prefix' },
  {
    bit: 128,
    on: 'pause',
    when: (f) => !f.fractionMode && DIVISION.test(f.expr) && DECIMAL.test(f.answer ?? ''),
    text: '⌃F shows fractions',
  },
  { bit: 256, on: 'commit', text: '↑ brings it back' },
  { bit: 2, on: 'commit', when: (f) => Boolean(f.variable), text: (f) => `use ${f.variable} in later calculations` },
  // not on first run, which already has the hotkey to teach
  { bit: 512, on: 'open', when: (f) => Boolean(f.native) && (f.opens ?? 0) >= 2, text: '⌘, opens settings' },
]

const HINT_ALL = HINTS.reduce((all, h) => all | h.bit, 0)

type Hint = { bit: number; text: string }

export function pickHint(seen: number, facts: HintFacts, on: HintRule['on'] = 'commit'): Hint | null {
  const rule = HINTS.find((h) => h.on === on && (seen & h.bit) === 0 && (h.when?.(facts) ?? true))
  if (!rule) return null
  return { bit: rule.bit, text: typeof rule.text === 'string' ? rule.text : rule.text(facts) }
}

export const HOTKEY_FAILED_HINT = 'shortcut in use by macOS — change it in the menu'

export function isHelpCommand(text: string): boolean {
  const t = text.trim().toLowerCase()
  return t === '?' || t === 'help'
}

// typing past `?` or `help` keeps just the new text
export function afterHelpInput(prev: string, next: string): string {
  if (!isHelpCommand(prev) || next.length <= prev.length || !next.startsWith(prev)) return next
  return next.slice(prev.length)
}

export type CheatRow = [key: string, label: string]

// left column: keys. right column: things to type
export const CHEATS: { keys: readonly CheatRow[]; type: readonly CheatRow[] } = {
  keys: [
    ['↵', 'save'],
    ['↑', 'history'],
    ['⌘C', 'copy answer'],
    ['⌥↑ ⌥↓', 'unit prefix'],
    ['⌃D', 'degrees / radians'],
    ['⌃F', 'fractions'],
    ['⌃S', 'sig figs from input'],
    ['⌃C', 'clear history'],
    ['esc', 'hide'],
  ],
  type: [
    ['ft to cm', 'units'],
    ['80 + 15%', 'percent'],
    ['+-  -+', '± ∓ uncertainty'],
    ['x = 5', 'variables'],
    ['x^2 = 2', 'solve'],
    ['graph x^2', 'plot'],
    ['gcf  lcm', 'factors'],
    ['sum n, n=1..9', 'Σ sums'],
    ['?', 'this list'],
  ],
}

// only the mac app has the settings window; the web page shows them inline
const MENU_BAR: CheatRow = ['⌘,', 'settings']

export function cheatSheet(hotkey?: string, native = false): CheatRow[][] {
  const keys = [...(hotkey ? [[hotkey, 'show / hide'] as CheatRow] : []), ...CHEATS.keys]
  if (native) keys.splice(keys.length - 1, 0, MENU_BAR)
  return [keys, [...CHEATS.type]]
}
