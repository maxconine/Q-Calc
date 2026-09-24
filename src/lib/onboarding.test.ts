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
  exampleAnswer,
  HINTS,
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
    expect(evaluateSheet(['5 ft 10 in to cm'])[0]?.display).toBe('177.8 cm')
    expect(evaluateSheet(['80 + 15%'])[0]?.display).toBe('92')
    expect(evaluateSheet([prettyTokens('(5.0 +- 0.2) * 3')])[0]?.display).toBe('15.0 ± 0.6')
  })

  it('example answers show exact ≈ approx when there are two', () => {
    expect(exampleAnswer(evaluateSheet(['sqrt(8)'])[0])).toMatch(/^\S+ ≈ 2\.828/)
    expect(exampleAnswer(evaluateSheet(['100 W * 2 hr to kWh'])[0])).toBe('0.2 kWh')
    expect(exampleAnswer(undefined)).toBe('')
    expect(exampleAnswer(evaluateSheet(['x^2 = 2'])[0])).toBe('x = ±sqrt(2) ≈ ±1.41421356237')
  })
})

describe('hints', () => {
  const bit = (text: string) => HINTS.find((h) => typeof h.text === 'string' && h.text === text)!.bit
  const COPY = bit('⌘C copies the answer')
  const BACK = bit('↑ brings it back')
  const trig = HINTS.find((h) => h.on === 'pause' && h.when?.({ expr: 'sin(30)' }))!.bit
  const UNIT = bit('⌥↑ ⌥↓ step the unit prefix')
  const FRAC = bit('⌃F shows fractions')

  it('every hint is one short line', () => {
    for (const h of HINTS) {
      const text = typeof h.text === 'string' ? h.text : h.text({ expr: 'x = 5', variable: 'x' })
      expect(text.length, text).toBeLessThanOrEqual(34)
    }
  })

  it('bits are unique single bits, and old bits stay retired', () => {
    const bits = HINTS.map((h) => h.bit)
    expect(new Set(bits).size).toBe(bits.length)
    for (const b of bits) expect(b & (b - 1)).toBe(0)
    for (const retired of [1, 4, 8]) expect(bits).not.toContain(retired)
  })

  it('a paused answer teaches copy first, once', () => {
    expect(pickHint(0, { expr: '2+2', answer: '4' }, 'pause')?.text).toBe('⌘C copies the answer')
    expect(pickHint(COPY, { expr: '2+2', answer: '4' }, 'pause')).toBeNull()
  })

  it('enter teaches history first, once', () => {
    expect(pickHint(0, { expr: '2+2' })?.bit).toBe(BACK)
    expect(pickHint(BACK, { expr: '2+2' })).toBeNull()
  })

  it('names the variable that was assigned', () => {
    expect(pickHint(BACK, { expr: 'r = 5', variable: 'r' })?.text).toBe('use r in later calculations')
  })

  it('trig says which mode you are in', () => {
    expect(pickHint(COPY, { expr: 'sin(30)', angleMode: 'deg' }, 'pause')?.text).toBe('degrees · ⌃D for radians')
    expect(pickHint(COPY, { expr: 'sin(30)', angleMode: 'rad' }, 'pause')?.text).toBe('radians · ⌃D for degrees')
    expect(pickHint(COPY, { expr: 'cosine(1)' }, 'pause')).toBeNull()
    expect(pickHint(COPY, { expr: 'asin(1)' }, 'pause')?.bit).toBe(trig)
  })

  it('unit answers teach prefix stepping while they are on screen', () => {
    expect(pickHint(COPY, { expr: '100 W * 2 hr', unit: true }, 'pause')?.bit).toBe(UNIT)
    expect(pickHint(BACK, { expr: '100 W * 2 hr', unit: true })).toBeNull()
  })

  it('a decimal from a division suggests fractions unless they are on', () => {
    const facts = { expr: '1/3 + 1/4', answer: '0.583333333333' }
    expect(pickHint(COPY, facts, 'pause')?.bit).toBe(FRAC)
    expect(pickHint(COPY, { ...facts, fractionMode: true }, 'pause')).toBeNull()
    expect(pickHint(COPY, { expr: '8/2', answer: '4' }, 'pause')).toBeNull()
  })

  it('the mac app points at settings once, from the second open', () => {
    const MENU = bit('⌘, opens settings')
    expect(pickHint(0, { expr: '', native: true, opens: 1 }, 'open')).toBeNull()
    expect(pickHint(0, { expr: '', native: true, opens: 2 }, 'open')?.bit).toBe(MENU)
    expect(pickHint(MENU, { expr: '', native: true, opens: 3 }, 'open')).toBeNull()
    expect(pickHint(0, { expr: '', native: false, opens: 2 }, 'open')).toBeNull()
  })

  it('bitmask survives storage and merges by union', () => {
    const a = sanitizeOnboarding({ opens: 2, commits: 1, hints: COPY, done: false })
    const b = sanitizeOnboarding({ opens: 1, commits: 3, hints: UNIT | 1 | 1 << 20 })
    expect(mergeOnboarding(a, b)).toEqual({ opens: 2, commits: 3, hints: COPY | UNIT, done: false })
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
    expect(cheatSheet('⌃⌥Space')[0]![0]).toEqual(['⌃⌥Space', 'show / hide'])
    expect(cheatSheet().flat().some(([k]) => k.includes('Space'))).toBe(false)
    expect(cheatSheet()[1]!.at(-1)?.[0]).toBe('?')
  })

  it('names ⌘, only in the mac app', () => {
    const keys = (native: boolean) => cheatSheet('⌃⌥Space', native)[0]!.map(([k]) => k)
    expect(keys(true)).toContain('⌘,')
    expect(keys(true).at(-1)).toBe('esc')
    expect(keys(false)).not.toContain('⌘,')
  })

  it('is two short columns: keys, then things to type', () => {
    const [keys, type] = cheatSheet('⌃⌥Space', true)
    expect(keys!.length).toBeLessThanOrEqual(11)
    expect(type!.length).toBeLessThanOrEqual(keys!.length)
    for (const k of ['⌥↑ ⌥↓', '⌃D', '⌃F', '⌃S', '⌃C']) expect(keys!.map(([key]) => key)).toContain(k)
    expect(type!.map(([key]) => key).join(' ')).toMatch(/graph.*gcf/)
  })

  it('every thing to type works', () => {
    for (const expr of ['5 ft to cm', '80 + 15%', '5 +- 0.2', '5 -+ 0.2', 'x = 5', 'gcf(12, 18)', 'lcm(4, 6)']) {
      expect(evaluateSheet([prettyTokens(expr)])[0]?.display, expr).toBeTruthy()
    }
  })
})
