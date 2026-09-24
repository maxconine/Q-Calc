import { describe, expect, it } from 'vitest'
import { isEditOf } from './steady'

describe('isEditOf', () => {
  it('holds while typing on or backspacing', () => {
    expect(isEditOf('sqrt(2)*3+', 'sqrt(2)*3')).toBe(true)
    expect(isEditOf('2+', '2')).toBe(true)
    expect(isEditOf('12 kg in l', '12 kg in lb')).toBe(true)
  })

  it('holds through an edit in the middle', () => {
    expect(isEditOf('12 * (3 + ) * 4', '12 * (3 + 5) * 4')).toBe(true)
  })

  it('lets go of unrelated input', () => {
    expect(isEditOf('x', '12')).toBe(false)
    expect(isEditOf('9^', '2+2')).toBe(false)
    expect(isEditOf('', '2')).toBe(false)
    expect(isEditOf('2', '2')).toBe(false)
  })
})
