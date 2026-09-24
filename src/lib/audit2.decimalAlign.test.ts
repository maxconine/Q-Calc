// new coverage for decimal-column alignment in the history tape; there was no dedicated test
// file for this yet. FIGURE_SPACE/PUNCTUATION_SPACE are both U+2007/U+2008-style fixed spaces,
// so widths below are counted in characters, not measured pixels.
import { describe, expect, it } from 'vitest'
import { alignDecimals } from './decimalAlign'

const FIGURE_SPACE = ' '
const PUNCTUATION_SPACE = ' '

describe('alignDecimals: basic padding', () => {
  it('pads shorter fractional parts to match the widest', () => {
    const out = alignDecimals(['1.5', '2.25', '3'])
    // widest fraction is 2 digits ("25"); "1.5" needs 1 more, "3" needs a point-space plus 2
    expect(out[0]).toBe(`1.5${FIGURE_SPACE}`)
    expect(out[1]).toBe('2.25')
    expect(out[2]).toBe(`3${PUNCTUATION_SPACE}${FIGURE_SPACE}${FIGURE_SPACE}`)
  })

  it('does nothing when every entry is already a whole number', () => {
    expect(alignDecimals(['1', '22', '333'])).toEqual(['1', '22', '333'])
  })

  it('does nothing to a single-entry list', () => {
    expect(alignDecimals(['1.5'])).toEqual([`1.5`])
  })

  it('leaves an all-null list alone', () => {
    expect(alignDecimals([null, null])).toEqual([null, null])
  })
})

describe('alignDecimals: entries that are left alone', () => {
  it('a null entry (dual answer, unit, scientific) is skipped but does not block others', () => {
    const out = alignDecimals(['1.2', null, '3.45'])
    expect(out[0]).toBe(`1.2${FIGURE_SPACE}`)
    expect(out[1]).toBe(null)
    expect(out[2]).toBe('3.45')
  })

  it('a non-plain entry like scientific notation or a unit is left untouched, and does not count toward the widest fraction', () => {
    const out = alignDecimals(['1.2', '1e+10', '3.45 kg'])
    // neither '1e+10' nor '3.45 kg' matches the plain-number pattern, so the only plain
    // entry ('1.2') sets the widest fraction to itself and needs no padding
    expect(out[0]).toBe('1.2')
    expect(out[1]).toBe('1e+10')
    expect(out[2]).toBe('3.45 kg')
  })

  it('a non-plain entry does not block padding among the plain ones alongside it', () => {
    const out = alignDecimals(['1.2', '1e+10', '3.456'])
    expect(out[0]).toBe(`1.2${FIGURE_SPACE}${FIGURE_SPACE}`)
    expect(out[1]).toBe('1e+10')
    expect(out[2]).toBe('3.456')
  })

  it('a minus sign (the U+2212 form) is still read as plain', () => {
    const out = alignDecimals(['−1.2', '3.456'])
    expect(out[0]).toBe(`−1.2${FIGURE_SPACE}${FIGURE_SPACE}`)
    expect(out[1]).toBe('3.456')
  })

  it('grouped thousands with commas are still read as plain', () => {
    const out = alignDecimals(['1,234.5', '6.78'])
    expect(out[0]).toBe(`1,234.5${FIGURE_SPACE}`)
    expect(out[1]).toBe('6.78')
  })
})

describe('alignDecimals: the maxChars guard', () => {
  it('skips padding an entry that would cross maxChars', () => {
    // "12345678901234567890" is 20 chars; padding to match a 1-digit-longer fraction
    // would push it past a maxChars of 20, so it is left alone
    const out = alignDecimals(['12345678901234567890.1', '2.22'], 20)
    expect(out[0]).toBe('12345678901234567890.1')
  })

  it('still pads others under the limit even when the long one is skipped', () => {
    const long = '12345678901234567890.1' // 22 chars; padding it by 1 would reach 23 > maxChars 22
    const out = alignDecimals(['1.1', '2.22', long], 22)
    expect(out[2]).toBe(long)
    expect(out[0]).toBe(`1.1${FIGURE_SPACE}`)
    expect(out[1]).toBe('2.22')
  })
})

describe('alignDecimals: empty input', () => {
  it('returns an empty array for an empty array', () => {
    expect(alignDecimals([])).toEqual([])
  })
})
