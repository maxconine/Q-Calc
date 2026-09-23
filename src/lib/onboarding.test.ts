import { describe, expect, it } from 'vitest'
import { evaluateSheet } from '../engine/evaluate'
import { prettyTokens } from '../components/QuickInput'
import {
  advanceRotation,
  afterHelpInput,
  cheatSheet,
  dismissRotation,
  emptyOnboarding,
  EXAMPLE_COMMITS,
  EXAMPLE_OPENS,
  EXAMPLES,
  exampleList,
  examplesActive,
  HINT,
  isHelpCommand,
  mergeOnboarding,
  pickHint,
  recordCommit,
  recordOpen,
  rotationItem,
  sanitizeOnboarding,
  startRotation,
  type Onboarding,
} from './onboarding'

function times(n: number, s: Onboarding, step: (s: Onboarding) => Onboarding): Onboarding {
  for (let i = 0; i < n; i++) s = step(s)
  return s
}

describe('example lifetime', () => {
  it('runs for the first opens, then retires for good', () => {
    const s = times(EXAMPLE_OPENS, emptyOnboarding(), recordOpen)
    expect(examplesActive(s)).toBe(true)
    const after = recordOpen(s)
    expect(examplesActive(after)).toBe(false)
    expect(after.done).toBe(true)
  })

  it('stops early after enough commits', () => {
    const s = times(EXAMPLE_COMMITS, recordOpen(emptyOnboarding()), recordCommit)
    expect(examplesActive(s)).toBe(false)
    expect(s.done).toBe(true)
  })

  it('stays retired even if counters go backwards', () => {
    const done = { ...emptyOnboarding(), done: true }
    expect(examplesActive(done)).toBe(false)
    expect(examplesActive(mergeOnboarding(emptyOnboarding(), done))).toBe(false)
  })
})

describe('rotation', () => {
  const items = exampleList()

  it('cycles through the examples while on', () => {
    let r = startRotation(true)
    const seen: string[] = []
    for (let i = 0; i < items.length + 1; i++) {
      seen.push(rotationItem(r, items)!.expr)
      r = advanceRotation(r)
    }
    expect(seen.slice(0, items.length)).toEqual(items.map((e) => e.expr))
    expect(seen[items.length]).toBe(items[0]!.expr)
  })

  it('a keystroke dismisses it and ticks cannot revive it', () => {
    const r = dismissRotation(advanceRotation(startRotation(true)))
    expect(r.on).toBe(false)
    expect(rotationItem(advanceRotation(r), items)).toBeUndefined()
  })

  it('never starts when examples are retired', () => {
    expect(rotationItem(startRotation(false), items)).toBeUndefined()
  })

  it('first run leads with the hotkey', () => {
    expect(exampleList('⌘⌥Space')[0]!.expr).toBe('Press ⌘⌥Space anytime')
    expect(exampleList().length).toBe(EXAMPLES.length)
  })

  it('every math example gets a real answer from the engine', () => {
    for (const ex of EXAMPLES) {
      if (ex.plain) continue
      const line = evaluateSheet([prettyTokens(ex.expr)])[0]
      expect(line?.display, ex.expr).toBeTruthy()
    }
    expect(evaluateSheet(['2 in to cm'])[0]?.display).toBe('5.08 cm')
    expect(evaluateSheet(['sin(90)'])[0]?.display).toBe('1')
  })
})

describe('hints', () => {
  it('shows the basic hint first, once', () => {
    const first = pickHint(0, { expr: 'sin(30)' })
    expect(first?.bit).toBe(HINT.answer)
    const next = pickHint(HINT.answer, { expr: 'sin(30)' })
    expect(next?.bit).toBe(HINT.trig)
    expect(pickHint(HINT.answer | HINT.trig, { expr: 'sin(30)' })).toBeNull()
  })

  it('names the variable that was assigned', () => {
    expect(pickHint(HINT.answer, { expr: 'r = 5', variable: 'r' })?.text).toBe('use r in later calculations')
  })

  it('unit answers teach prefix stepping', () => {
    expect(pickHint(HINT.answer, { expr: '100 W * 2 hr', unit: true })?.bit).toBe(HINT.units)
  })

  it('does not mistake words for trig', () => {
    expect(pickHint(HINT.answer, { expr: 'cosine(1)' })).toBeNull()
    expect(pickHint(HINT.answer, { expr: 'asin(1)' })?.bit).toBe(HINT.trig)
  })

  it('bitmask survives storage and merges by union', () => {
    const a = sanitizeOnboarding({ opens: 2, commits: 1, hints: HINT.answer, done: false })
    const b = sanitizeOnboarding({ opens: 1, commits: 3, hints: HINT.units | 1024 })
    expect(mergeOnboarding(a, b)).toEqual({ opens: 2, commits: 3, hints: HINT.answer | HINT.units, done: false })
    expect(sanitizeOnboarding('junk')).toEqual(emptyOnboarding())
    expect(sanitizeOnboarding({ opens: -4, commits: 'x' })).toEqual(emptyOnboarding())
  })
})

describe('? sheet', () => {
  it('recognises ? and help alone', () => {
    expect(isHelpCommand('?')).toBe(true)
    expect(isHelpCommand(' ? ')).toBe(true)
    expect(isHelpCommand('Help')).toBe(true)
    expect(isHelpCommand('2?')).toBe(false)
    expect(isHelpCommand('helper')).toBe(false)
    expect(isHelpCommand('')).toBe(false)
  })

  it('typing past the command starts fresh', () => {
    expect(afterHelpInput('?', '?2')).toBe('2')
    expect(afterHelpInput('help', 'help ')).toBe(' ')
    expect(afterHelpInput('?', '')).toBe('')
    expect(afterHelpInput('2', '2?')).toBe('2?')
  })

  it('lists the hotkey only when there is one', () => {
    expect(cheatSheet('⌃⌥Space')[0]).toEqual(['⌃⌥Space', 'show / hide'])
    expect(cheatSheet().some(([k]) => k.includes('Space'))).toBe(false)
    expect(cheatSheet().at(-1)?.[0]).toBe('?')
  })
})
