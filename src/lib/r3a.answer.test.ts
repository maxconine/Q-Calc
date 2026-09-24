import { describe, expect, it } from 'vitest'
import { evaluateLine } from '../engine/evaluate'
import { formatNumber } from '../engine/format'
import { answerAmong, hasDualAnswer, insertableAnswer, insertableHistoryAnswer, prettyRoots, visibleAnswer } from './answer'

describe('insertableAnswer round trips through the real engine', () => {
  // insertableAnswer receives whatever the engine's own display/n produced; feeding its
  // output straight back into evaluateLine should land on the same numeric value
  it.each(['sqrt(2)', 'cos(30)', '1/3', 'pi/6', '-sqrt(2)', '2sqrt(3)', '-1/3'])(
    '%s',
    (expr) => {
      const r = evaluateLine(expr)
      expect(r.value).not.toBeNull()
      const inserted = insertableAnswer(r.display, r.value!.n)
      const back = evaluateLine(inserted)
      expect(back.value?.n).toBeCloseTo(r.value!.n, 6)
    },
  )

  it('a unit answer round-trips to the same physical quantity, even if the display flips systems', () => {
    // "10 ft to m" answers "3.048 m"; re-typing "3.048 m" flips back to the imperial display
    // ("10 ft") the app shows metric input in by default, but it is still the same length
    const r = evaluateLine('10 ft to m')
    const inserted = insertableAnswer(r.display, r.value?.n)
    expect(inserted).toBe('3.048 m')
    const back = evaluateLine(inserted)
    expect(back.display).toBe('10 ft')
  })

  it('a plain negative number from the engine is what String(n) would already give', () => {
    // formatValue/evaluateLine use a plain ascii hyphen for display and insertion; the pretty
    // unicode minus is only applied separately, for on-screen display (prettyAnswer)
    const r = evaluateLine('0-5')
    expect(r.display).toBe('-5')
    expect(insertableAnswer(r.display, r.value?.n)).toBe('-5')
  })
})

describe('insertableAnswer at the edges of its numeric fallback', () => {
  it('a bare zero is not mistaken for "no display"', () => {
    expect(insertableAnswer('0', 0)).toBe('0')
  })

  it('an explicit sig fig count is honored even when it rounds to a whole number', () => {
    expect(insertableAnswer('2.999999', 3, 2)).toBe(formatNumber(3, 2))
  })
})

describe('prettyRoots on mixed-sign and unit lists', () => {
  it('groups digits and swaps the minus per part, independently of neighboring parts', () => {
    expect(prettyRoots('-1000000, 2000000')).toBe('−1,000,000, 2,000,000')
  })

  it('a lone positive root is untouched apart from grouping', () => {
    expect(prettyRoots('1234567')).toBe('1,234,567')
  })
})

describe('answerAmong with function-shaped answers', () => {
  it('a bare function call is already atomic-looking and left alone', () => {
    expect(answerAmong('sqrt(2)', false)).toBe('sqrt(2)')
    expect(answerAmong('sin(30)', false)).toBe('sin(30)')
  })

  it('a sum of two atoms is wrapped, since it is not one atom', () => {
    expect(answerAmong('2 + 3', false)).toBe('(2 + 3)')
  })

  it('always leaves the answer alone when it stands on its own', () => {
    expect(answerAmong('2 + 3', true)).toBe('2 + 3')
  })
})

describe('visibleAnswer only looks at form and exact, independently of hasDualAnswer', () => {
  it('falls back to display when there is no exact form at all, or it repeats the display', () => {
    for (const row of [{ display: '4' }, { display: '4', exact: '4' }]) {
      expect(visibleAnswer(row, 'exact')).toBe(row.display)
      expect(hasDualAnswer(row)).toBe(false)
    }
  })

  it('still returns the exact form even with a blank display, where hasDualAnswer disagrees', () => {
    const row = { display: '', exact: 'sqrt(2)' }
    expect(visibleAnswer(row, 'exact')).toBe('sqrt(2)')
    expect(hasDualAnswer(row)).toBe(false)
  })
})

describe('insertableHistoryAnswer with a measured value keeps the shown precision', () => {
  it('inserts the sig-fig display, not a re-rounded n', () => {
    const row = { display: '3.140', n: Math.PI, meas: { sig: 4, dp: 3 } }
    expect(insertableHistoryAnswer(row, 'approx')).toBe('3.140')
    expect(insertableHistoryAnswer(row, 'approx')).not.toBe(formatNumber(Math.PI))
  })
})
