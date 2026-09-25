import { afterEach, describe, expect, it, vi } from 'vitest'
import { evaluateSheet } from '../engine'
import { answerPhrase } from '../engine/phrases'
import { mergeLiveAnswer } from './nativeEval'
import { hasSoulver, hostRates, withPhraseAnswer } from './phraseLive'

const NOW = new Date(2026, 8, 24, 15, 12, 40)
const RATES = { base: 'EUR', rates: { USD: 1.25, GBP: 0.8 } }

function jsOf(expr: string): { display: string; n?: number } {
  const live = evaluateSheet([expr])[0]
  const display = live?.display ?? ''
  return { display, n: live?.value?.kind === 'number' ? live.value.n : undefined }
}

// what the page shows with no native host at all
function pageShows(expr: string, enabled = true): string {
  const js = jsOf(expr)
  const merged = mergeLiveAnswer(expr, js.display, js.n, null)
  return withPhraseAnswer(expr, js.display, merged, { enabled, now: NOW, rates: RATES }).display
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('gating', () => {
  it('sees soulvercore only when its handler is there', () => {
    expect(hasSoulver()).toBe(false)
    vi.stubGlobal('window', { __QCALC_NATIVE: true })
    expect(hasSoulver()).toBe(false)
    vi.stubGlobal('window', { __QCALC_NATIVE: true, webkit: { messageHandlers: { soulver: { postMessage: () => Promise.resolve(null) } } } })
    expect(hasSoulver()).toBe(true)
  })

  it('reads host rates only in the expected shape', () => {
    expect(hostRates()).toBeNull()
    vi.stubGlobal('window', { __QCALC_RATES: RATES })
    expect(hostRates()).toEqual(RATES)
    vi.stubGlobal('window', { __QCALC_RATES: { USD: 1 } })
    expect(hostRates()).toBeNull()
  })

  it('does nothing when switched off, as on the mac', () => {
    for (const expr of ['20% off 80', '$60 + 18% tip', 'tomorrow', '60 split 3 ways', '9am to 5pm']) {
      const js = jsOf(expr)
      const merged = mergeLiveAnswer(expr, js.display, js.n, null)
      expect(withPhraseAnswer(expr, js.display, merged, { enabled: false, now: NOW })).toBe(merged)
    }
  })

  it('never replaces a native reply', () => {
    const merged = mergeLiveAnswer('20% off 80', '', undefined, { expr: '20% off 80', display: '64.00', n: 64 })
    expect(withPhraseAnswer('20% off 80', '', merged, { enabled: true, now: NOW })).toEqual({ display: '64.00', n: 64 })
  })
})

describe('the page without soulvercore', () => {
  it.each([
    ['20% off 80', '64'],
    ['% change from 80 to 100', '25%'],
    ['60 split 3 ways', '20'],
    ['$60 + 18% tip', '$70.80'],
    ['60 / 3 people', '20'],
    ['9am to 5pm', '8 hr'],
    ['10 am', '10:00 am'],
    ['3:45pm + 4 hr', '7:45 pm'],
    ['next friday', 'Fri 25 Sep 2026'],
    ['$20 in euros', '€16.00'],
  ])('%s → %s', (expr, out) => {
    expect(pageShows(expr)).toBe(out)
  })

  it('shows the same blank as before when switched off', () => {
    expect(pageShows('20% off 80', false)).toBe('')
    expect(pageShows('9am to 5pm', false)).toBe(jsOf('9am to 5pm').display)
  })

  it.each([
    '20%', '10 mod 3', '5 min + 3 s', '80 - 20%', '80 + 15%', '15% of 80', 'what is 40% of 90', '3 days', '3 weeks in days',
    '1 hr to min', '2+2', 'sin(90)', '50 W * 1 day', '1 kW * 2 hr', '20 m * 2 in', '3 ft to m', '80 * 20%', '80 - 20% - 10%',
    '50 - 10% of 20', '5! / 3', 'sqrt(16)', '2^10', '100 kg to lb', '1e3', '0.5 + 0.25',
  ])('leaves js in charge of %s', (expr) => {
    const js = jsOf(expr)
    expect(pageShows(expr)).toBe(js.display)
  })

  it('agrees with js wherever both answer', () => {
    for (const expr of ['80 - 20%', '80 + 15%', '15% of 80', '20% of 80', '12.5% of 64', '200 - 12.5%', '1000 + 7%']) {
      const js = jsOf(expr)
      const phrase = answerPhrase(expr, { now: NOW })
      expect(js.display, expr).not.toBe('')
      expect(phrase?.display, expr).toBe(js.display)
    }
  })

  it('stays blank on loose phrases', () => {
    for (const expr of ['between 1 and 2', 'lunch', '5 people', 'tip 18%', 'tip of 15% on 80', '$20 in euros and change', 'until', 'days until', 'next week']) {
      expect(pageShows(expr), expr).toBe('')
    }
  })
})
