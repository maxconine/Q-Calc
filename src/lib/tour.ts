import type { Onboarding } from './onboarding'

// the walkthrough keeps its progress in the onboarding `hints` bitmask, which both native shells already
// persist and merge by union. bits stay below 1 << 30 so js bit operations keep them. never reuse or renumber one
export const TOUR_ON = 1 << 12
// skipped, or the install was past onboarding when the tour arrived
export const TOUR_OFF = 1 << 13
// the tour goes quiet after this many opens however far it got, so it never outstays its welcome
export const TOUR_OPENS = 15

export type TourStepId = 'hotkey' | 'arithmetic' | 'units' | 'chain' | 'copy'

export type TourStep = {
  id: TourStepId
  bit: number
  // shown faintly in the empty bar; `plain` has no answer beside it
  expr: string
  plain?: boolean
  // the line under the bar
  text: string
}

export type TourExtra = { id: 'solve' | 'graph' | 'periodic' | 'help'; bit: number; expr: string; plain?: boolean; text: string }

// in order: each line waits for the ones before it
export function tourSteps(hotkey: string): TourStep[] {
  return [
    {
      id: 'hotkey',
      bit: 1 << 14,
      expr: `${hotkey} opens Q Calc from anywhere`,
      plain: true,
      text: `try it: ${hotkey} hides me, ${hotkey} again brings me back`,
    },
    { id: 'arithmetic', bit: 1 << 15, expr: '12 * 7', text: 'type 12 * 7 and press ↵' },
    { id: 'units', bit: 1 << 16, expr: '5 ft to cm', text: 'units convert too: type 5 ft to cm, then ↵' },
    { id: 'chain', bit: 1 << 17, expr: '* 2', plain: true, text: 'carry on from the last answer: type * 2, then ↵' },
    { id: 'copy', bit: 1 << 18, expr: '2^10', text: '⌘C copies the answer: type 2^10, then ⌘C' },
  ]
}

// one per open once the basics are done, and only for what hasn't been used already
export const TOUR_EXTRAS: readonly TourExtra[] = [
  { id: 'solve', bit: 1 << 19, expr: 'x^2 = 2', text: 'you can also solve: type x^2 = 2' },
  { id: 'graph', bit: 1 << 20, expr: 'graph sin(x)', plain: true, text: 'or plot one: graph sin(x), then ↵' },
  { id: 'periodic', bit: 1 << 21, expr: 'periodic', plain: true, text: 'periodic ↵ opens the periodic table' },
  { id: 'help', bit: 1 << 22, expr: '?', plain: true, text: '? lists everything else, any time' },
]

const STEP_BITS = tourSteps('').reduce((all, s) => all | s.bit, 0)
const EXTRA_BITS = TOUR_EXTRAS.reduce((all, e) => all | e.bit, 0)
export const TOUR_ALL = TOUR_ON | TOUR_OFF | STEP_BITS | EXTRA_BITS

export const TOUR_DONE_HINT = 'that’s the basics · ? lists everything'

export function tourBit(id: TourStepId | TourExtra['id']): number {
  return tourSteps('').find((s) => s.id === id)?.bit ?? TOUR_EXTRAS.find((e) => e.id === id)?.bit ?? 0
}

/**
 * Decided once per install, on the first open that sees it: a new install takes the tour, one already past
 * onboarding never does. Whatever it has clearly done already counts as done.
 */
export function enrolTour(s: Onboarding, native: boolean): Onboarding {
  if (s.hints & (TOUR_ON | TOUR_OFF)) return s
  if (s.done || !native) return { ...s, hints: s.hints | TOUR_OFF }
  const known = (s.opens >= 2 ? tourBit('hotkey') : 0) | (s.commits > 0 ? tourBit('arithmetic') : 0)
  return { ...s, hints: s.hints | TOUR_ON | known }
}

export function tourRunning(s: Onboarding): boolean {
  return (s.hints & TOUR_ON) !== 0 && (s.hints & TOUR_OFF) === 0 && s.opens <= TOUR_OPENS
}

/** The first step not done yet. The hotkey step is left out when there's no shortcut to teach. */
export function tourStep(done: number, hotkey: string): TourStep | null {
  const steps = tourSteps(hotkey).filter((s) => hotkey || s.id !== 'hotkey')
  return steps.find((s) => (done & s.bit) === 0) ?? null
}

export function tourProgress(done: number, hotkey: string): { at: number; of: number } {
  const steps = tourSteps(hotkey).filter((s) => hotkey || s.id !== 'hotkey')
  const at = steps.findIndex((s) => (done & s.bit) === 0)
  return { at: at < 0 ? steps.length : at + 1, of: steps.length }
}

export function tourExtra(done: number, hotkey: string): TourExtra | null {
  if (tourStep(done, hotkey)) return null
  return TOUR_EXTRAS.find((e) => (done & e.bit) === 0) ?? null
}

export type TourFacts = {
  expr: string
  unit?: boolean
  chained?: boolean
  solve?: boolean
  graph?: boolean
}

/** What a saved calculation counts toward, steps and extras alike. */
export function tourUsed(f: TourFacts): number {
  let bits = 0
  if (f.graph) bits |= tourBit('graph')
  else bits |= tourBit('arithmetic')
  if (f.unit) bits |= tourBit('units')
  if (f.chained) bits |= tourBit('chain')
  if (f.solve) bits |= tourBit('solve')
  return bits
}

/** Whether the line under the bar still fits what's typed: an empty bar, or the start of the step's own text. */
export function tourLineFits(step: TourStep, typed: string, answer: boolean): boolean {
  if (!typed.trim()) return true
  if (step.id === 'hotkey') return false
  // ⌘C works on any answer, so the copy line stays while there is one
  if (step.id === 'copy' && answer) return true
  const squash = (t: string) => t.replace(/\s+/g, '').toLowerCase()
  return squash(step.expr).startsWith(squash(typed))
}

export function isTutorialCommand(text: string): boolean {
  return /^(?:tutorial|tour)$/i.test(text.trim())
}

export function isSkipCommand(text: string): boolean {
  return /^skip(?:\s+(?:tutorial|tour))?$/i.test(text.trim())
}

export const TUTORIAL_HINT = 'start the tutorial'
