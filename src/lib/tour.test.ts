import { describe, expect, it } from 'vitest'
import { evaluateSheet } from '../engine/evaluate'
import { prettyTokens } from '../components/QuickInput'
import { emptyOnboarding, HINTS, mergeOnboarding, recordCommit, recordOpen, sanitizeOnboarding, type Onboarding } from './onboarding'
import {
  enrolTour,
  isSkipCommand,
  isTutorialCommand,
  TOUR_ALL,
  TOUR_EXTRAS,
  TOUR_OFF,
  TOUR_ON,
  TOUR_OPENS,
  tourBit,
  tourExtra,
  tourLineFits,
  tourProgress,
  tourRunning,
  tourStep,
  tourSteps,
  tourUsed,
} from './tour'

const KEY = '⌥Space'
const fresh = (): Onboarding => enrolTour(recordOpen(emptyOnboarding()), true)

describe('who takes the tour', () => {
  it('a new install does, starting with the shortcut', () => {
    const s = fresh()
    expect(tourRunning(s)).toBe(true)
    expect(tourStep(s.hints, KEY)?.id).toBe('hotkey')
  })

  it('an install already past onboarding never does', () => {
    const s = enrolTour({ opens: 40, commits: 90, hints: 16, done: true }, true)
    expect(s.hints & TOUR_OFF).toBeTruthy()
    expect(tourRunning(s)).toBe(false)
    // and a later open can't enrol it after all
    expect(tourRunning(enrolTour(recordOpen(s), true))).toBe(false)
  })

  it('the web page keeps its rotating examples instead', () => {
    expect(tourRunning(enrolTour(recordOpen(emptyOnboarding()), false))).toBe(false)
  })

  it('a light user mid-onboarding gets it, minus what they clearly know', () => {
    const s = enrolTour({ opens: 3, commits: 2, hints: 0, done: false }, true)
    expect(tourRunning(s)).toBe(true)
    expect(tourStep(s.hints, KEY)?.id).toBe('units')
  })

  it('enrolment is decided once', () => {
    const s = fresh()
    expect(enrolTour(s, true)).toBe(s)
    expect(enrolTour({ ...s, done: true }, true).hints & TOUR_OFF).toBe(0)
  })

  it('outlasts the examples but goes quiet after enough opens', () => {
    let s = fresh()
    for (let i = 0; i < 6; i++) s = recordCommit(recordOpen(s))
    expect(s.done).toBe(true)
    expect(tourRunning(s)).toBe(true)
    while (s.opens <= TOUR_OPENS) s = recordOpen(s)
    expect(tourRunning(s)).toBe(false)
  })

  it('skipping is for good', () => {
    const s = fresh()
    expect(tourRunning({ ...s, hints: s.hints | TOUR_OFF })).toBe(false)
  })
})

describe('steps', () => {
  it('go in order, one at a time', () => {
    let done = 0
    const seen: string[] = []
    for (let step = tourStep(done, KEY); step; step = tourStep(done, KEY)) {
      seen.push(step.id)
      done |= step.bit
    }
    expect(seen).toEqual(['hotkey', 'arithmetic', 'units', 'chain', 'copy'])
  })

  it('skip the shortcut when there is none to teach', () => {
    expect(tourStep(0, '')?.id).toBe('arithmetic')
    expect(tourProgress(0, '')).toEqual({ at: 1, of: 4 })
    expect(tourProgress(0, KEY)).toEqual({ at: 1, of: 5 })
    expect(tourProgress(tourBit('hotkey') | tourBit('arithmetic'), KEY)).toEqual({ at: 3, of: 5 })
  })

  it('later lines wait for earlier ones, even when used out of order', () => {
    const done = tourBit('hotkey') | tourBit('chain') | tourBit('copy')
    expect(tourStep(done, KEY)?.id).toBe('arithmetic')
    expect(tourStep(done | tourBit('arithmetic'), KEY)?.id).toBe('units')
  })

  it('a saved calculation counts toward what it used', () => {
    expect(tourUsed({ expr: '12*7' })).toBe(tourBit('arithmetic'))
    expect(tourUsed({ expr: '5 ft to cm', unit: true }) & tourBit('units')).toBeTruthy()
    expect(tourUsed({ expr: '*2', chained: true }) & tourBit('chain')).toBeTruthy()
    expect(tourUsed({ expr: 'x^2=2', solve: true }) & tourBit('solve')).toBeTruthy()
    expect(tourUsed({ expr: 'graph x', graph: true })).toBe(tourBit('graph'))
  })

  it('every example works as written', () => {
    const answer = (expr: string) => evaluateSheet([prettyTokens(expr)])[0]?.display
    expect(answer('12 * 7')).toBe('84')
    expect(answer('5 ft to cm')).toBe('152.4 cm')
    expect(answer('2^10')).toBe('1024')
    expect(answer('x^2 = 2')).toBeTruthy()
    const chained = evaluateSheet(['ans * 2'], { ans: 84 })[0]?.display
    expect(chained).toBe('168')
  })

  it('lines name the shortcut and stay short', () => {
    expect(tourSteps(KEY)[0]!.text).toContain(KEY)
    for (const s of [...tourSteps('⌃⌥Space'), ...TOUR_EXTRAS]) expect(s.text.length, s.text).toBeLessThanOrEqual(56)
  })
})

describe('the line under the bar', () => {
  const step = (id: string) => tourSteps(KEY).find((s) => s.id === id)!

  it('shows on an empty bar and while the example is being typed', () => {
    expect(tourLineFits(step('arithmetic'), '', false)).toBe(true)
    expect(tourLineFits(step('arithmetic'), '12 *', true)).toBe(true)
    expect(tourLineFits(step('arithmetic'), '12*7', true)).toBe(true)
    expect(tourLineFits(step('units'), '5 FT', false)).toBe(true)
  })

  it('steps aside for anything else', () => {
    expect(tourLineFits(step('arithmetic'), '3 + 4', true)).toBe(false)
    expect(tourLineFits(step('hotkey'), '1', true)).toBe(false)
  })

  it('the copy line stays while any answer is up', () => {
    expect(tourLineFits(step('copy'), '3 + 4', true)).toBe(true)
    expect(tourLineFits(step('copy'), '3 +', false)).toBe(false)
  })
})

describe('you can also', () => {
  const basics = tourSteps(KEY).reduce((all, s) => all | s.bit, 0)

  it('waits for the basics', () => {
    expect(tourExtra(0, KEY)).toBeNull()
    expect(tourExtra(basics & ~tourBit('copy'), KEY)).toBeNull()
    expect(tourExtra(basics, KEY)?.id).toBe('solve')
  })

  it('one at a time, skipping what was already used', () => {
    expect(tourExtra(basics | tourBit('solve'), KEY)?.id).toBe('graph')
    expect(tourExtra(basics | tourBit('solve') | tourBit('graph') | tourBit('periodic'), KEY)?.id).toBe('help')
    expect(tourExtra(basics | TOUR_EXTRAS.reduce((all, e) => all | e.bit, 0), KEY)).toBeNull()
  })
})

describe('storage', () => {
  it('tour bits never collide with hint bits and fit in 32-bit js', () => {
    for (const h of HINTS) expect(h.bit & TOUR_ALL).toBe(0)
    expect(TOUR_ALL).toBeLessThan(2 ** 30)
    expect(TOUR_ALL).toBeGreaterThan(0)
  })

  it('survive sanitising and merge by union, like the native copies do', () => {
    const a = sanitizeOnboarding({ opens: 2, commits: 1, hints: TOUR_ON | tourBit('hotkey') })
    const b = sanitizeOnboarding({ opens: 1, commits: 3, hints: TOUR_ON | tourBit('units') })
    expect(mergeOnboarding(a, b).hints).toBe(TOUR_ON | tourBit('hotkey') | tourBit('units'))
  })
})

describe('commands', () => {
  it('tutorial or tour replays it', () => {
    expect(isTutorialCommand(' Tutorial ')).toBe(true)
    expect(isTutorialCommand('tour')).toBe(true)
    expect(isTutorialCommand('tutorials')).toBe(false)
    expect(isTutorialCommand('')).toBe(false)
  })

  it('skip ends it', () => {
    expect(isSkipCommand('skip')).toBe(true)
    expect(isSkipCommand('skip tutorial')).toBe(true)
    expect(isSkipCommand('skipper')).toBe(false)
  })

  it('neither is something the engine answers', () => {
    for (const word of ['tutorial', 'tour', 'skip']) expect(evaluateSheet([word])[0]?.display ?? '').toBe('')
  })
})
