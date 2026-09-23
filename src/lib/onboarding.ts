export type Onboarding = {
  opens: number
  commits: number
  // bitmask of HINT values already shown
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

// answers are computed at display time so they follow the settings; `plain` has no answer
type Example = { expr: string; note?: string; plain?: boolean }

export const EXAMPLES: readonly Example[] = [
  { expr: '2 in to cm' },
  { expr: 'sin(90)' },
  { expr: 'x = 5' },
  { expr: '(5.0 +- 0.2) * 3' },
  { expr: '100 W * 2 hr', note: '⌥↑' },
  { expr: 'graph x^3', plain: true },
]

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

export const HINT = {
  answer: 1,
  variable: 2,
  units: 4,
  trig: 8,
} as const

const HINT_ALL = HINT.answer | HINT.variable | HINT.units | HINT.trig

export type CommitFacts = {
  expr: string
  variable?: string
  unit?: boolean
}

type Hint = { bit: number; text: string }

const TRIG = /(?<![A-Za-z])(?:a|arc)?(?:sin|cos|tan)\s*\(/i

// each hint shows once ever, most basic first
export function pickHint(seen: number, facts: CommitFacts): Hint | null {
  const candidates: Hint[] = [
    { bit: HINT.answer, text: '⌘C copies · Enter saved it' },
    ...(facts.variable ? [{ bit: HINT.variable, text: `use ${facts.variable} in later calculations` }] : []),
    ...(TRIG.test(facts.expr) ? [{ bit: HINT.trig, text: '⌃D switches degrees/radians' }] : []),
    ...(facts.unit ? [{ bit: HINT.units, text: '⌥↑ ⌥↓ switch prefixes' }] : []),
  ]
  return candidates.find((h) => (seen & h.bit) === 0) ?? null
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

export function cheatSheet(hotkey?: string): Array<[key: string, label: string]> {
  return [
    ...(hotkey ? [[hotkey, 'show / hide'] as [string, string]] : []),
    ['esc', 'hide'],
    ['⌘C', 'copy answer'],
    ['↵', 'save to history'],
    ['↑', 'history'],
    ['⌃D', 'degrees / radians'],
    ['⌃F', 'fractions'],
    ['⌃S', 'sig figs from input'],
    ['⌥↑ ⌥↓', 'unit prefix'],
    ['⌃C', 'clear history'],
    ['+-  ~', '±'],
    ['?', 'this list'],
  ]
}
