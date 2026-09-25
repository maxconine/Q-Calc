import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { convertCurrency, formatMoney, parseMoney } from './currency'
import { answerPhrase } from './phrases'
import { formatClock, formatDuration, parseClock, parseSpan } from './phraseTime'

// thursday 24 september 2026, 3:12:40 pm local time
const NOW = new Date(2026, 8, 24, 15, 12, 40)
const RATES = { base: 'EUR', rates: { USD: 1.25, GBP: 0.8, JPY: 160, CAD: 1.5 } }

function shown(expr: string, now = NOW, rates: typeof RATES | null = RATES): string {
  return answerPhrase(expr, { now, rates })?.display ?? ''
}

describe('percentages', () => {
  it.each([
    ['20% off 80', '64'],
    ['80 - 20%', '64'],
    ['80 + 15%', '92'],
    ['15% of 80', '12'],
    ['20% on 50', '60'],
    ['what is 15% of 80', '12'],
    ['20 percent of 80', '16'],
    ['20 per cent off 80', '64'],
    ['what % of 80 is 20', '25%'],
    ['what percent of 80 is 20', '25%'],
    ['20 is what % of 80', '25%'],
    ['20 is what percent of 80?', '25%'],
    ['20 as a % of 80', '25%'],
    ['20 as % of 80', '25%'],
    ['1 as a % of 3', '33.3333333333%'],
    ['% change from 80 to 100', '25%'],
    ['percent change from 100 to 80', '-20%'],
    ['percentage change from 50 to 75', '50%'],
    ['$80 - 20%', '$64.00'],
    ['20% off $80', '$64.00'],
    ['15% of £80', '£12.00'],
    ['what % of $80 is $20', '25%'],
    ['100% off 80', '0'],
  ])('%s → %s', (expr, out) => {
    expect(shown(expr)).toBe(out)
  })

  it('leaves percentages of nothing and nonsense blank', () => {
    for (const expr of ['what % of 0 is 5', '% change from 0 to 5', '120% off 80', '20% off', '% of 80', 'what % of $80 is €20', '20%', '% change from 80']) {
      expect(shown(expr), expr).toBe('')
    }
  })

  it('does not attach a number to a percentage', () => {
    expect(answerPhrase('20 is what % of 80')?.n).toBeUndefined()
    expect(answerPhrase('15% of 80')?.n).toBe(12)
  })
})

describe('tips and splitting', () => {
  it.each([
    ['$60 + 18% tip', '$70.80'],
    ['$10 for lunch + 15% tip', '$11.50'],
    ['$20 lunch + 20% tip', '$24.00'],
    ['60 with a 20% tip', '72'],
    ['60 plus 20% tip', '72'],
    ['$1,000 + 18% tip', '$1180.00'],
    ['60 split 3 ways', '20'],
    ['$60 split 3 ways', '$20.00'],
    ['$100 split between 3 people', '$33.33'],
    ['60 split among 4', '15'],
    ['60 / 3 people', '20'],
    ['60 divided by 4 people', '15'],
    ['$60 / 3 per person', '$20.00'],
    ['$90 for 3 people per person', '$30.00'],
    ['$60 + 18% tip split 3 ways', '$23.60'],
    ['€50 / 2 people', '€25.00'],
  ])('%s → %s', (expr, out) => {
    expect(shown(expr)).toBe(out)
  })

  it('rounds the money number to the cents shown', () => {
    expect(answerPhrase('$60 + 18% tip split 3 ways')?.n).toBe(23.6)
    expect(answerPhrase('$100 split 3 ways')?.n).toBe(33.33)
  })

  it('leaves loose or impossible splits blank', () => {
    for (const expr of ['60 split 0 ways', '60 split 3', '60 per person', '$60 for 3 people', 'tip of 15% on 80', 'split 3 ways', '60 / 3.5 people', '60 split -3 ways', '15% tip']) {
      expect(shown(expr), expr).toBe('')
    }
  })
})

describe('clock times', () => {
  it.each([
    ['3:45pm + 4 hr', '7:45 pm'],
    ['3:45pm + 4 hr 10 min', '7:55 pm'],
    ['3:45 pm + 4 hours and 10 minutes', '7:55 pm'],
    ['9:00am + 1 hr', '10:00 am'],
    ['10:30 pm - 15 min', '10:15 pm'],
    ['11pm + 3 hr', '2:00 am'],
    ['noon + 90 min', '1:30 pm'],
    ['15:30 + 2 hr', '5:30 pm'],
    ['12am + 1 hr', '1:00 am'],
    ['12pm - 1 hr', '11:00 am'],
    ['10 am', '10:00 am'],
    ['9am to 5pm', '8 hr'],
    ['from 9:30am to 5pm', '7 hr 30 min'],
    ['between 9am and 9:45am', '45 min'],
    ['10pm to 2am', '4 hr'],
    ['until 6pm', '2 hr 47 min'],
    ['time until 5pm', '1 hr 47 min'],
    ['until 3pm', '23 hr 47 min'],
  ])('%s → %s', (expr, out) => {
    expect(shown(expr)).toBe(out)
  })

  it('leaves times that could be durations blank', () => {
    for (const expr of ['3:45 + 1 hr', '12:30 + 1', '1:30:00', '9am - 5pm', '9am to 9am', '13pm', '3:75pm', '3:45pm + 4', '3:45pm + 2 days', '25:00 + 1 hr']) {
      expect(shown(expr), expr).toBe('')
    }
  })

  it('formats clock times and spans', () => {
    expect(formatClock(0)).toBe('12:00 am')
    expect(formatClock(720)).toBe('12:00 pm')
    expect(formatClock(-60)).toBe('11:00 pm')
    expect(formatDuration(45)).toBe('45 min')
    expect(formatDuration(120)).toBe('2 hr')
    expect(parseClock('7:05 p.m.')).toBe(19 * 60 + 5)
    expect(parseSpan('1.5 hr')).toEqual({ minutes: 90, days: 0, months: 0 })
    expect(parseSpan('1.5 days')).toBeNull()
    expect(parseSpan('2 days 3 hr')).toBeNull()
  })
})

describe('dates', () => {
  it.each([
    ['today', 'Thu 24 Sep 2026'],
    ['tomorrow', 'Fri 25 Sep 2026'],
    ['yesterday', 'Wed 23 Sep 2026'],
    ['today + 3 weeks', 'Thu 15 Oct 2026'],
    ['tomorrow - 2 days', 'Wed 23 Sep 2026'],
    ['3 weeks from today', 'Thu 15 Oct 2026'],
    ['3 days from now', 'Sun 27 Sep 2026'],
    ['in 3 days', 'Sun 27 Sep 2026'],
    ['2 weeks after tomorrow', 'Fri 9 Oct 2026'],
    ['3 days before dec 25', 'Tue 22 Dec 2026'],
    ['3 days ago', 'Mon 21 Sep 2026'],
    ['a week ago', 'Thu 17 Sep 2026'],
    ['1 year ago', 'Wed 24 Sep 2025'],
    ['3 hours ago', '12:12 pm'],
    ['in 90 min', '4:42 pm'],
    ['16 hours ago', 'Wed 23 Sep 2026, 11:12 pm'],
    ['next friday', 'Fri 25 Sep 2026'],
    ['next thursday', 'Thu 1 Oct 2026'],
    ['next fri', 'Fri 25 Sep 2026'],
    ['last friday', 'Fri 18 Sep 2026'],
    ['last thursday', 'Thu 17 Sep 2026'],
    ['thursday', 'Thu 24 Sep 2026'],
    ['monday', 'Mon 28 Sep 2026'],
    ['monday + 10 days', 'Thu 8 Oct 2026'],
    ['dec 25', 'Fri 25 Dec 2026'],
    ['25 december 2027', 'Sat 25 Dec 2027'],
    ['march 3rd', 'Tue 3 Mar 2026'],
    ['jan 31 2026 + 1 month', 'Sat 28 Feb 2026'],
    ['feb 29 2028 + 1 year', 'Wed 28 Feb 2029'],
    ['days until dec 25', '92 days'],
    ['how many days until christmas', ''],
    ['days until 25th december', '92 days'],
    ['days until sep 1', '342 days'],
    ['days until tomorrow', '1 day'],
    ['days until today', '0 days'],
    ['days since jan 1', '266 days'],
    ['days since dec 25', '273 days'],
    ['days between dec 1 and dec 25', '24 days'],
    ['days from dec 25 2026 to jan 1 2027', '7 days'],
  ])('%s → %s', (expr, out) => {
    expect(shown(expr)).toBe(out)
  })

  it('leaves ambiguous dates blank', () => {
    for (const expr of ['may', 'march', 'next week', 'this friday', 'fri', 'sat', 'feb 30', 'feb 29', 'dec 32', '12/25', 'tomorrow - 2 hours', 'days until feb 30 2026', 'days until jan 1 2020', 'days from dec 25 to dec 1', 'today + 1.5 days', 'today + 3', 'now']) {
      expect(shown(expr), expr).toBe('')
    }
  })

  it('follows the injected clock', () => {
    const newYearsEve = new Date(2026, 11, 31, 23, 30)
    expect(shown('tomorrow', newYearsEve)).toBe('Fri 1 Jan 2027')
    expect(shown('days until dec 25', newYearsEve)).toBe('359 days')
    expect(shown('in 1 hr', newYearsEve)).toBe('Fri 1 Jan 2027, 12:30 am')
    expect(shown('feb 29 + 1 day', new Date(2028, 0, 5))).toBe('Wed 1 Mar 2028')
  })

  describe('across a daylight saving change', () => {
    const tz = process.env.TZ
    beforeAll(() => {
      process.env.TZ = 'America/New_York'
    })
    afterAll(() => {
      if (tz === undefined) delete process.env.TZ
      else process.env.TZ = tz
    })
    it('counts calendar days, not 24 hour blocks', () => {
      const now = new Date(2026, 9, 31, 23, 0)
      expect(shown('days until nov 2', now)).toBe('2 days')
      expect(shown('tomorrow + 1 day', now)).toBe('Mon 2 Nov 2026')
      expect(shown('days between mar 7 and mar 9', now)).toBe('2 days')
    })
  })
})

describe('currency', () => {
  it.each([
    ['$20 in euros', '€16.00'],
    ['20 usd to gbp', '£12.80'],
    ['20 USD to GBP', '£12.80'],
    ['€10 in dollars', '$12.50'],
    ['£5 as yen', '¥1000'],
    ['¥1000 in dollars', '$7.81'],
    ['20€ in usd', '$25.00'],
    ['5 pounds in euros', '€6.25'],
    ['$20 in pounds', '£12.80'],
    ['10 cad to usd', '$8.33'],
    ['$20 in usd', '$20.00'],
    ['€20', '€20.00'],
    ['$5', '$5.00'],
    ['20 dollars', '$20.00'],
    ['30 cad', '30.00 CAD'],
  ])('%s → %s', (expr, out) => {
    expect(shown(expr)).toBe(out)
  })

  it('stays blank without rates', () => {
    for (const expr of ['$20 in euros', '20 usd to gbp', '£5 as yen']) {
      expect(shown(expr, NOW, null), expr).toBe('')
    }
  })

  it('stays blank for a currency the table lacks', () => {
    expect(shown('20 usd to chf')).toBe('')
  })

  it('never reads pounds as money on its own', () => {
    expect(shown('5 pounds in kg')).toBe('')
    expect(shown('5 pounds')).toBe('')
    expect(parseMoney('5 pounds')).toBeNull()
  })

  it('converts through the base currency', () => {
    expect(convertCurrency(100, 'USD', 'GBP', RATES)).toBeCloseTo(64)
    expect(convertCurrency(100, 'EUR', 'EUR', RATES)).toBe(100)
    expect(convertCurrency(1, 'usd', 'jpy', RATES)).toBeCloseTo(128)
    expect(convertCurrency(1, 'USD', 'XYZ', RATES)).toBeNull()
    expect(convertCurrency(1, 'USD', 'GBP', null)).toBeNull()
    expect(convertCurrency(1, 'USD', 'GBP', { base: 'EUR', rates: { USD: 0, GBP: 0.8 } })).toBeNull()
  })

  it('formats money', () => {
    expect(formatMoney(-5, 'USD')).toBe('-$5.00')
    expect(formatMoney(-0.001, 'USD')).toBe('$0.00')
    expect(formatMoney(1234.5, 'JPY')).toBe('¥1235')
    expect(formatMoney(3, 'CHF')).toBe('3.00 CHF')
  })
})

describe('ordinary maths is not a phrase', () => {
  it.each([
    '20%', '2+2', '10 mod 3', '5 % 3', '5 min + 3 s', '3 days', '2 weeks', '1 hr to min', '3 weeks in days', 'sin(90)',
    'sqrt(2)', '1/3', 'x = 4', 'ans * 2', '10^3', '5!', 'pi*2', '50 W * 1 day', '10 m to kg', '2 in to cm', '65 kg in lb',
    '1 kW * 2 hr', '20 m * 2 in', '3 ft to m', 'between 1 and 2', 'lunch', 'tip', '5 people', 'derivative of x^2', '∫0..1 x dx',
    'Σ(n, 1, 10)', 'graph sin(x)', 'x^2 = 4', '2H2 + O2 -> 2H2O', '5 ± 2', '12', '1,000', '', '   ', '80', 'in', 'to', 'ago',
    'mon', 'sun', 'sat', 'dec', '2 hr + 30 min', '10 - 3 days', '3 - 20', 'days', 'today tomorrow',
  ])('%j stays blank', (expr) => {
    expect(shown(expr)).toBe('')
  })
})
