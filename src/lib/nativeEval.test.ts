import { describe, expect, it } from 'vitest'
import {
  looksLikeDictionaryQuery,
  looksLikeNaturalLanguage,
  mergeLiveAnswer,
  nativeDefinition,
  nativeEvalPayload,
  nativeReplyToLive,
  usableNativeDisplay,
  soulverAngleSafe,
} from './nativeEval'

describe('mergeLiveAnswer', () => {
  it('prefers the JS engine for ordinary arithmetic', () => {
    expect(mergeLiveAnswer('2+2', '4', 4, { expr: '2+2', display: '4.00', n: 4 })).toEqual({
      display: '4',
      n: 4,
    })
  })

  it('prefers SoulverCore for natural-language phrases even if JS also answered', () => {
    expect(
      mergeLiveAnswer('what is 40% of 90', '36', 36, {
        expr: 'what is 40% of 90',
        display: '36',
        n: 36,
      }),
    ).toEqual({ display: '36', n: 36 })
  })

  it('keeps the JS engine for unit products even if SoulverCore also answered', () => {
    expect(
      mergeLiveAnswer('50 W * 1 day', '4320000 J', 4.32e6, {
        expr: '50 W * 1 day',
        display: '1.2 kWh',
        n: 1.2,
      }),
    ).toEqual({ display: '4320000 J', n: 4.32e6 })
  })

  it('uses SoulverCore when the JS engine has no result', () => {
    expect(
      mergeLiveAnswer('$10 for lunch + 15% tip', '', undefined, {
        expr: '$10 for lunch + 15% tip',
        display: '$11.50',
        n: 11.5,
      }),
    ).toEqual({ display: '$11.50', n: 11.5 })
  })

  it('ignores a stale SoulverCore answer for a different expression', () => {
    expect(mergeLiveAnswer('2 in to cm', '', undefined, { expr: '65 kg in lb', display: '143.3 lb', n: 143.3 })).toEqual(
      { display: '' },
    )
  })

  it('shows a unit conversion failure from SoulverCore', () => {
    expect(
      mergeLiveAnswer('10 meters to kilograms', '', undefined, {
        expr: '10 meters to kilograms',
        display: 'Error: incompatible units',
      }),
    ).toEqual({ display: 'improper unit conversion' })
  })

  it('clears the answer when the field is empty', () => {
    expect(mergeLiveAnswer('  ', '', undefined, { expr: '2+2', display: '4', n: 4 })).toEqual({ display: '' })
  })
})

describe('nativeReplyToLive', () => {
  it('accepts a matching reply', () => {
    expect(
      nativeReplyToLive({ id: 3, expr: '3:45pm + 4 hr', display: '7:55 pm', n: null }, 3, '3:45pm + 4 hr'),
    ).toEqual({
      expr: '3:45pm + 4 hr',
      display: '7:55 pm',
    })
  })

  it('drops stale or empty replies', () => {
    expect(nativeReplyToLive({ id: 1, expr: '2+2', display: '4', n: 4 }, 2, '2+2')).toBeNull()
    expect(nativeReplyToLive({ id: 1, expr: '2+2', display: '4', n: 4 }, 1, '2+3')).toBeNull()
    expect(nativeReplyToLive({ id: 1, expr: '2+2', display: '  ', n: 4 }, 1, '2+2')).toBeNull()
  })

  it('maps SoulverCore incompatible units to a friendly message', () => {
    expect(
      nativeReplyToLive(
        { id: 1, expr: '10 meters to kilograms', display: 'Error: incompatible units', n: null },
        1,
        '10 meters to kilograms',
      ),
    ).toEqual({
      expr: '10 meters to kilograms',
      display: 'improper unit conversion',
    })
    expect(
      nativeReplyToLive({ id: 1, expr: '10^1000 m to nm', display: 'Error: ∞', n: null }, 1, '10^1000 m to nm'),
    ).toBeNull()
  })
})

describe('usableNativeDisplay', () => {
  it('maps incompatible-unit engine errors and rejects other errors', () => {
    expect(usableNativeDisplay('Error: incompatible units')).toBe('improper unit conversion')
    expect(usableNativeDisplay('improper unit conversion')).toBe('improper unit conversion')
    expect(usableNativeDisplay('error: divide by zero')).toBe('')
    expect(usableNativeDisplay('  Error: ∞/m  ')).toBe('')
    expect(usableNativeDisplay('25.4 cm')).toBe('25.4 cm')
  })
})

describe('looksLikeNaturalLanguage', () => {
  it('detects Soulver-style phrases', () => {
    expect(looksLikeNaturalLanguage('what is 40% of 90')).toBe(true)
    expect(looksLikeNaturalLanguage('$10 for lunch + 15% tip')).toBe(true)
    expect(looksLikeNaturalLanguage('3:45pm + 4 hr 10 min')).toBe(true)
  })

  it('leaves scientific calculator input to the JS engine', () => {
    expect(looksLikeNaturalLanguage('sin(90)')).toBe(false)
    expect(looksLikeNaturalLanguage('2+2')).toBe(false)
    expect(looksLikeNaturalLanguage('sqrt(2)')).toBe(false)
    expect(looksLikeNaturalLanguage('50 W * 1 day')).toBe(false)
    expect(looksLikeNaturalLanguage('1 Therm / 1 day')).toBe(false)
    expect(looksLikeNaturalLanguage('20 m * 2 in')).toBe(false)
    expect(looksLikeNaturalLanguage('1 kW * 2 hr')).toBe(false)
  })
})

describe('nativeEvalPayload', () => {
  it('omits undefined ans so WKWebView can serialize the message', () => {
    const payload = nativeEvalPayload({ id: 1, expr: 'what is 40% of 90', ans: undefined, sigFigs: 12 })
    expect(payload).toEqual({ type: 'eval', id: 1, expr: 'what is 40% of 90', sigFigs: 12 })
    expect(Object.values(payload).some((v) => v === undefined)).toBe(false)
  })

  it('includes a finite previous answer', () => {
    expect(nativeEvalPayload({ id: 2, expr: 'ans + 1', ans: 36, sigFigs: 12 }).ans).toBe(36)
  })
})

describe('mergeLiveAnswer', () => {
  it.each(['2+2', 'sin(90)', 'sqrt(2)', '10^3', '5!', 'pi*2', '1/3', '3(4+5)', 'log(100)', '2^8'])(
    'prefers JS for %s',
    (expr) => {
      expect(mergeLiveAnswer(expr, '1', 1, { expr, display: '9', n: 9 })).toEqual({ display: '1', n: 1 })
    },
  )
  it.each(['what is 10% of 50', 'what is 20% of 80', "what's 15% of 40", '40 is what % of 90', '$5 for lunch + 10% tip'])(
    'prefers native NLP for %s',
    (expr) => {
      expect(mergeLiveAnswer(expr, '1', 1, { expr, display: '9', n: 9 })).toEqual({ display: '9', n: 9 })
    },
  )
  it.each(['50 W * 1 day', '1 kW * 2 hr', '20 m * 2 in', '10 N * 5 m', '1 Therm / 1 day'])('keeps JS unit product for %s', (expr) => {
    expect(mergeLiveAnswer(expr, '4 J', 4, { expr, display: '9 kWh', n: 9 })).toEqual({ display: '4 J', n: 4 })
  })
  it.each(['$10 + 15%', '3:45pm + 1 hr', '20% off 50'])('uses native when JS empty for %s', (expr) => {
    expect(mergeLiveAnswer(expr, '', undefined, { expr, display: 'ok', n: 1 })).toEqual({ display: 'ok', n: 1 })
  })
  it.each(['2+2', '1 m', 'sin(30)', '10 kg'])('ignores stale native for %s', (expr) => {
    expect(mergeLiveAnswer(expr, '', undefined, { expr: 'other', display: '9', n: 9 })).toEqual({ display: '' })
  })
  it.each(['', ' ', '   ', '\t'])('clears blank %j', (expr) => {
    expect(mergeLiveAnswer(expr, '4', 4, { expr: '2+2', display: '4', n: 4 })).toEqual({ display: '' })
  })
  it.each(['10 m to kg', '2 kg to m', '1 J to m'])('maps native unit error for %s', (expr) => {
    expect(mergeLiveAnswer(expr, '', undefined, { expr, display: 'Error: incompatible units' })).toEqual({
      display: 'improper unit conversion',
    })
  })
  it.each(Array.from({ length: 60 }, (_, i) => `2+${i}`))('js wins arithmetic %s', (expr) => {
    expect(mergeLiveAnswer(expr, String(iFrom(expr)), iFrom(expr), { expr, display: '9', n: 9 })).toEqual({
      display: String(iFrom(expr)),
      n: iFrom(expr),
    })
  })
})

function iFrom(expr: string): number {
  return Number(expr.slice(2))
}

describe('nativeReplyToLive', () => {
  it.each([1, 2, 3, 4, 5, 10, 99, 100, 1000])('accepts matching id %i', (id) => {
    expect(nativeReplyToLive({ id, expr: 'x', display: '1', n: 1 }, id, 'x')).toEqual({ expr: 'x', display: '1', n: 1 })
  })
  it.each([1, 2, 3, 4, 5, 8, 9, 10])('drops stale id %i', (id) => {
    expect(nativeReplyToLive({ id, expr: 'x', display: '1', n: 1 }, id + 1, 'x')).toBeNull()
  })
  it.each(['a', 'b', '2+2', 'sin(90)', '1 m'])('drops mismatched expr %s', (expr) => {
    expect(nativeReplyToLive({ id: 1, expr, display: '1', n: 1 }, 1, expr + '!')).toBeNull()
  })
  it.each(['', ' ', '  '])('drops empty display %j', (display) => {
    expect(nativeReplyToLive({ id: 1, expr: 'x', display, n: 1 }, 1, 'x')).toBeNull()
  })
  it.each(['10 m to kg', '2 kg to s', '1 J to m'])('maps incompatible units for %s', (expr) => {
    expect(nativeReplyToLive({ id: 1, expr, display: 'Error: incompatible units', n: null }, 1, expr)).toEqual({
      expr,
      display: 'improper unit conversion',
    })
  })
  it.each(['Error: ∞', 'Error: boom', 'error: overflow'])('drops other errors %s', (display) => {
    expect(nativeReplyToLive({ id: 1, expr: 'x', display, n: null }, 1, 'x')).toBeNull()
  })
  it.each(Array.from({ length: 70 }, (_, i) => ({ id: i, expr: `e${i}`, display: `${i}` })))(
    'roundtrip $expr',
    ({ id, expr, display }) => {
      expect(nativeReplyToLive({ id, expr, display, n: id }, id, expr)).toEqual({
        expr,
        display,
        n: id,
      })
    },
  )
})

describe('usableNativeDisplay', () => {
  it.each([
    ['Error: incompatible units', 'improper unit conversion'],
    ['error: incompatible units', 'improper unit conversion'],
    ['improper unit conversion', 'improper unit conversion'],
    ['  improper unit conversion  ', 'improper unit conversion'],
    ['error: divide by zero', ''],
    ['Error: ∞', ''],
    ['  Error: ∞/m  ', ''],
    ['error: overflow', ''],
    ['25.4 cm', '25.4 cm'],
    ['4', '4'],
    ['$11.50', '$11.50'],
    ['7:55 pm', '7:55 pm'],
    ['1.2 m', '1.2 m'],
    ['3.14', '3.14'],
    ['', ''],
    ['   ', ''],
  ])('%j → %j', (input, out) => {
    expect(usableNativeDisplay(input)).toBe(out)
  })
  it.each(Array.from({ length: 80 }, (_, i) => `${i} m`))('keeps %s', (d) => {
    expect(usableNativeDisplay(d)).toBe(d)
  })
  it.each(['12 ft', '3 kg', '9.8 m/s^2', '1 A', '5 V'])('keeps extra %s', (d) => {
    expect(usableNativeDisplay(d)).toBe(d)
  })
})

describe('looksLikeNaturalLanguage', () => {
  it.each([
    'what is 40% of 90',
    'what is 10% of 20',
    "what's 5% of 80",
    'whats 12% of 50',
    '$10 for lunch + 15% tip',
    '$20 lunch + 20% tip',
    '3:45pm + 4 hr',
    '9:00am + 1 hr',
    '10:30 pm - 15 min',
    '20% off 40',
    '15% of 200',
    'tip 18%',
    'lunch 12',
    'today + 1 day',
    'tomorrow - 2 hours',
    'yesterday + 1',
    'percent of 50',
    '3 people * $20',
    '2 nights * $100',
    'from 3 to 5',
    'until 6pm',
    'between 1 and 2',
    '5 per person',
    '3 hours ago',
    '€20 + tax',
    '£5 + tip',
    '¥1000',
    '₹50 + 10%',
  ])('detects %s', (expr) => {
    expect(looksLikeNaturalLanguage(expr)).toBe(true)
  })
  it.each([
    'sin(90)',
    'cos(0)',
    'tan(45)',
    '2+2',
    '3*4',
    'sqrt(2)',
    'log(100)',
    '5!',
    'pi*2',
    '50 W * 1 day',
    '1 Therm / 1 day',
    '20 m * 2 in',
    '1 kW * 2 hr',
    '10 N * 5 m',
    '1 kg * 2 m/s^2',
    '100 J / 2 s',
    '12 V * 2 A',
    '1 m + 2 m',
    '3 ft to m',
    '2 in',
    'abs(-3)',
    'mean(1,2,3)',
    'nCr(5,2)',
    '2^8',
    '10^-3',
    '1.5e4',
    '(1+2)*3',
    'x = 4',
    'ans * 2',
    '',
    '   ',
  ])('scientific %s', (expr) => {
    expect(looksLikeNaturalLanguage(expr)).toBe(false)
  })
  it.each(Array.from({ length: 20 }, (_, i) => `what is ${i}% of 100`))('detects %s', (expr) => {
    expect(looksLikeNaturalLanguage(expr)).toBe(true)
  })
  it.each(Array.from({ length: 20 }, (_, i) => `sqrt(${i})`))('scientific extra %s', (expr) => {
    expect(looksLikeNaturalLanguage(expr)).toBe(false)
  })
})

describe('nativeEvalPayload', () => {
  it.each(Array.from({ length: 40 }, (_, i) => i + 1))('omits undefined ans id=%i', (id) => {
    const payload = nativeEvalPayload({ id, expr: 'x', ans: undefined, sigFigs: 12 })
    expect(payload).toEqual({ type: 'eval', id, expr: 'x', sigFigs: 12 })
    expect(Object.values(payload).some((v) => v === undefined)).toBe(false)
  })
  it.each([0, 1, 2, 3.5, 10, 36, 99, -4, 1e6, Math.PI])('includes ans %s', (ans) => {
    expect(nativeEvalPayload({ id: 1, expr: 'ans', ans, sigFigs: 8 }).ans).toBe(ans)
  })
  it.each([2, 3, 4, 6, 8, 12, 16])('includes sigFigs %i', (sigFigs) => {
    expect(nativeEvalPayload({ id: 1, expr: 'x', sigFigs }).sigFigs).toBe(sigFigs)
  })
  it.each(['a', 'b', '2+2', 'what is 40% of 90'])('keeps expr %s', (expr) => {
    expect(nativeEvalPayload({ id: 7, expr }).expr).toBe(expr)
  })
  it.each(Array.from({ length: 40 }, (_, i) => ({ id: i, expr: `e${i}`, ans: i, sigFigs: 4 })))(
    'payload $expr',
    ({ id, expr, ans, sigFigs }) => {
      const p = nativeEvalPayload({ id, expr, ans, sigFigs })
      expect(p).toEqual({ type: 'eval', id, expr, ans, sigFigs })
    },
  )
})

describe('dictionary replies', () => {
  it('does not treat a definition as a calculator answer', () => {
    expect(
      mergeLiveAnswer('serendipity', '', undefined, {
        expr: 'serendipity',
        display: 'serendipity\nnoun\nthe occurrence of events by chance',
        kind: 'definition',
        term: 'serendipity',
        pos: 'noun',
      }),
    ).toEqual({ display: '' })
  })

  it('keeps math even if a definition is also present', () => {
    expect(
      mergeLiveAnswer('pi', '3.14159', Math.PI, {
        expr: 'pi',
        display: 'pi\nnoun\nthe sixteenth letter',
        kind: 'definition',
        term: 'pi',
        pos: 'noun',
      }),
    ).toEqual({ display: '3.14159', n: Math.PI })
  })

  it('does not let natural-language words leak a definition into the live answer', () => {
    expect(
      mergeLiveAnswer('lunch', '', undefined, {
        expr: 'lunch',
        display: 'lunch\nnoun\na meal eaten in the middle of the day',
        kind: 'definition',
        term: 'lunch',
        pos: 'noun',
      }),
    ).toEqual({ display: '' })
  })

  it('exposes a matching definition for the overlay', () => {
    const native = {
      expr: 'serendipity',
      display: 'serendipity\nnoun\nthe occurrence of events by chance',
      kind: 'definition' as const,
      term: 'serendipity',
      pos: 'noun',
      body: 'the occurrence of events by chance',
    }
    expect(nativeDefinition(native, 'serendipity')).toEqual(native)
    expect(nativeDefinition(native, 'apple')).toBeNull()
    expect(nativeDefinition({ expr: '2+2', display: '4', n: 4 }, '2+2')).toBeNull()
  })

  it('roundtrips definition fields on a matching reply', () => {
    expect(
      nativeReplyToLive(
        {
          id: 4,
          expr: 'define apple',
          display: 'apple\nnoun\na fruit',
          n: null,
          kind: 'definition',
          term: 'apple',
          pos: 'noun',
          pronunciation: 'ˈap(ə)l',
          body: 'a fruit',
        },
        4,
        'define apple',
      ),
    ).toEqual({
      expr: 'define apple',
      display: 'apple\nnoun\na fruit',
      kind: 'definition',
      term: 'apple',
      pos: 'noun',
      pronunciation: 'ˈap(ə)l',
      body: 'a fruit',
    })
  })

  it('treats a reply with dictionary fields as a definition even without kind', () => {
    expect(
      nativeReplyToLive(
        { id: 1, expr: 'ingenious', display: 'ingenious\nadjective\nclever', term: 'ingenious', pos: 'adjective' },
        1,
        'ingenious',
      ),
    ).toEqual({
      expr: 'ingenious',
      display: 'ingenious\nadjective\nclever',
      kind: 'definition',
      term: 'ingenious',
      pos: 'adjective',
    })
  })
})

describe('looksLikeDictionaryQuery', () => {
  it.each(['ingenious', 'serendipity', 'hello', "it's", 'well-being', 'Apple', 'define ingenious', 'Define: apple', 'definition of New York', 'what does pi mean'])(
    'defines %s',
    (expr) => {
      expect(looksLikeDictionaryQuery(expr)).toBe(true)
    },
  )
  it.each(['2+2', 'sin(90)', '72 f', 'today', 'tomorrow', '$10 for lunch + 15% tip', 'what is 40% of 90', 'a', '', '  '])(
    'leaves %j to the calculator',
    (expr) => {
      expect(looksLikeDictionaryQuery(expr)).toBe(false)
    },
  )
})

describe('soulverAngleSafe', () => {
  it('keeps soulvercore, which works in radians, off trig in degree mode', () => {
    expect(soulverAngleSafe('what is sin(30)', 'deg')).toBe(false)
    expect(soulverAngleSafe('arctan(1) per day', 'deg')).toBe(false)
    expect(soulverAngleSafe('what is sin(30)', 'rad')).toBe(true)
    expect(soulverAngleSafe('sinh(1) per day', 'deg')).toBe(true)
    expect(soulverAngleSafe('tip of 15% on 80', 'deg')).toBe(true)
  })
})

describe('calculus stays in the js engine', () => {
  it('is never natural language, even with from/to or half typed', () => {
    expect(looksLikeNaturalLanguage('integral of x^2 from 0 to 1')).toBe(false)
    expect(looksLikeNaturalLanguage('integral of x^2 from 0')).toBe(false)
    expect(looksLikeNaturalLanguage('∫ x from 0 to 1')).toBe(false)
    expect(looksLikeNaturalLanguage('from 2 to 3')).toBe(true)
  })

  it('a blank calculus answer is not filled in by soulvercore', () => {
    const native = { expr: '∫0..1 1/x', display: '0', n: 0 }
    expect(mergeLiveAnswer('∫0..1 1/x', '', undefined, native)).toEqual({ display: '', n: undefined })
    expect(mergeLiveAnswer('∫0..1 x', '0.5', 0.5, { ...native, expr: '∫0..1 x' })).toEqual({ display: '0.5', n: 0.5 })
  })
})
