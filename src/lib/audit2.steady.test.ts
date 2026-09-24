// adversarial coverage for the steady (dimmed) answer edit-detection, on top of steady.test.ts
import { describe, expect, it } from 'vitest'
import { isEditOf } from './steady'

describe('isEditOf: continuations', () => {
  it('appending several characters at once (a fast paste-like keystroke) still counts as typing on', () => {
    expect(isEditOf('12 + 345', '12 + ')).toBe(true)
  })
  it('backspacing to nothing but one character is still an edit of a longer string', () => {
    expect(isEditOf('1', '123456789')).toBe(false)
  })
  it('deleting from the middle keeps the shared head and tail', () => {
    expect(isEditOf('sqrt(4)', 'sqrt(144)')).toBe(true)
  })
  it('selecting all and retyping the same length is not an edit', () => {
    expect(isEditOf('5+5', '3+3')).toBe(false)
  })
})

describe('isEditOf: boundary on the half-length rule', () => {
  it('exactly half shared (rounded up) still counts', () => {
    // prev length 4, ceil(4/2)=2 shared chars needed; head 'ab' shared, tail none
    expect(isEditOf('abxy', 'abcd')).toBe(true)
  })
  it('less than half shared does not count', () => {
    expect(isEditOf('aZZZ', 'abcd')).toBe(false)
  })
})

describe('isEditOf: symmetry is not assumed', () => {
  it('growing from short to long can hold even when shrinking back would not', () => {
    // prev='2', very short; any next starting with '2' shares 100% of prev's one character
    expect(isEditOf('2+2', '2')).toBe(true)
  })
})

describe('isEditOf: real editing scenarios from the input field', () => {
  it('changing the angle mode word mid-expression', () => {
    expect(isEditOf('sin(30) deg', 'sin(30) rad')).toBe(true)
  })
  it('fixing a typo near the start', () => {
    expect(isEditOf('12 kg in lb', '1 kg in lb')).toBe(true)
  })
  it('an entirely different line typed over a short previous one', () => {
    expect(isEditOf('12 kg in lb', '2+2')).toBe(false)
  })
})
