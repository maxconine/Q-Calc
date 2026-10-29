import { describe, expect, it } from 'vitest'
import { evaluateSheet } from './index'
import { answerPhrase } from './phrases'
import { formatClock } from './phraseTime'
import { withPhraseAnswer } from '../lib/phraseLive'

// the phrases soulvercore answered on the mac that the js side left blank or got wrong, found by running a list of
// soulver-style lines through both before soulvercore was removed. each case here is what the bar now shows instead

// thursday 24 september 2026, 3:12:40 pm local time
const NOW = new Date(2026, 8, 24, 15, 12, 40)
const RATES = { base: 'EUR', rates: { USD: 1.25, GBP: 0.8, JPY: 160, CAD: 1.5 } }

function shown(expr: string, now = NOW): string {
  return answerPhrase(expr, { now, rates: RATES })?.display ?? ''
}

// what the bar shows: the js engine first, then the phrase layer, as on windows and now the mac
function inBar(expr: string): string {
  const js = evaluateSheet([expr], { angleMode: 'deg' })[0]?.display ?? ''
  return withPhraseAnswer(expr, js, { display: js }, { enabled: true, rates: RATES, now: NOW }).display
}

describe('money arithmetic', () => {
  it.each([
    ['$45.50 + $12.25', '$57.75'],
    ['$10 * 3', '$30.00'],
    ['3 x $4.99', '$14.97'],
    ['$9.99 * 12', '$119.88'],
    ['$100 / 8', '$12.50'],
    ['$100 + 8.25% tax', '$108.25'],
    ['$100 + 10% + 5%', '$115.50'],
    ['$120 - $20 + 5%', '$105.00'],
    ['my rent is $1500 + utilities $200', '$1700.00'],
    ['€20 + €5', '€25.00'],
  ])('%s → %s', (expr, want) => {
    expect(shown(expr)).toBe(want)
  })

  it('leaves mixed currencies and money times money alone', () => {
    expect(shown('$50 + €20')).toBe('')
    expect(shown('$5 * $5')).toBe('')
    expect(shown('$5 $6')).toBe('')
  })
})

describe('a rate times an amount', () => {
  it.each([
    ['$30/hr * 40 hr', '$1200.00'],
    ['$15 per hour * 8 hours', '$120.00'],
    ['$3.50 per gallon * 12 gallons', '$42.00'],
    ['$5 per day for 30 days', '$150.00'],
    ['$1200 per month * 12', '$14400.00'],
    ['10 nights at $120', '$1200.00'],
    ['3 tickets at $25 each', '$75.00'],
  ])('%s → %s', (expr, want) => {
    expect(shown(expr)).toBe(want)
  })
})

describe('shorthand numbers', () => {
  it.each([
    ['1.5k + 500', '2000'],
    ['$50k / 12', '$4166.67'],
    ['$1.5m / 3', '$500000.00'],
    ['1 million / 1000', '1000'],
    ['2 billion / 4', '500000000'],
    ['5 dozen', '60'],
    ['$2.5k', '$2500.00'],
  ])('%s → %s', (expr, want) => {
    expect(shown(expr)).toBe(want)
  })
})

describe('reverse percentages', () => {
  it.each([
    ['20% of what is 30', '150'],
    ['30 is 20% of what', '150'],
    ['50 is 25% of what', '200'],
    ['$30 is 20% of what', '$150.00'],
    ['increase 50 by 10%', '55'],
    ['decrease 200 by 25%', '150'],
    ['increase $80 by 5%', '$84.00'],
  ])('%s → %s', (expr, want) => {
    expect(shown(expr)).toBe(want)
  })
})

describe('times and dates', () => {
  it.each([
    ['10:30 - 45 min', '9:45 am'],
    ['10:30 + 2 hours', '12:30 pm'],
    ['5pm - 9am', '8 hr'],
    ['5:30pm - 9am', '8 hr 30 min'],
    ['days until christmas', '92 days'],
    ['days until halloween', '37 days'],
    ["days until new year's", '99 days'],
    ['days until new year', '99 days'],
    ['march 1 to may 1', '61 days'],
  ])('%s → %s', (expr, want) => {
    expect(shown(expr)).toBe(want)
  })

  it('tells the time in a city or zone', () => {
    const at = (zone: string) => {
      const parts = new Intl.DateTimeFormat('en-US', { timeZone: zone, hour: 'numeric', minute: '2-digit', hourCycle: 'h23' }).formatToParts(NOW)
      const get = (t: string) => Number(parts.find((p) => p.type === t)?.value)
      return formatClock(get('hour') * 60 + get('minute'))
    }
    expect(shown('time in tokyo')).toBe(at('Asia/Tokyo'))
    expect(shown('time in London')).toBe(at('Europe/London'))
    expect(shown('what time is it in new york')).toBe(at('America/New_York'))
    expect(shown('time in pst')).toBe(at('America/Los_Angeles'))
    expect(shown('time in atlantis')).toBe('')
  })
})

describe('in the bar', () => {
  it('a clock difference beats the engine reading pm and am as lengths', () => {
    expect(inBar('5pm - 9am')).toBe('8 hr')
  })

  it.each([
    ['1.5k + 500', '2000'],
    ['$45.50 + $12.25', '$57.75'],
    ['increase 50 by 10%', '55'],
    ['days until christmas', '92 days'],
  ])('%s → %s', (expr, want) => {
    expect(inBar(expr)).toBe(want)
  })

  it('keeps the answers soulvercore got wrong right', () => {
    expect(inBar('what % of 90 is 40')).toBe('44.4444444444%')
    expect(inBar('a third of 90')).toBe('30')
    expect(inBar('twice 25')).toBe('50')
    expect(inBar('72 f in c')).toBe('22.2222222222 °C')
    expect(inBar('$50 dinner + 20% tip split 3 ways')).toBe('$20.00')
  })

  it('plain maths is still the engine’s', () => {
    expect(inBar('2 + 3 * 4')).toBe('14')
    expect(inBar('5 km to mi')).toBe(evaluateSheet(['5 km to mi'])[0]!.display)
  })
})
