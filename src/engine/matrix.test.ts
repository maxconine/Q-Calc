import { describe, expect, it } from 'vitest'
import { evaluateLine, evaluateSheet } from './evaluate'
import { looksLikeMatrix } from './matrix'

const show = (text: string, opts = {}) => evaluateLine(text, opts).display

describe('matrices', () => {
  it('adds and subtracts matching sizes', () => {
    expect(show('[[1,2],[3,4]] + [[5,6],[7,8]]')).toBe('[6 8; 10 12]')
    expect(show('[[5,6],[7,8]] - [[1,2],[3,4]]')).toBe('[4 4; 4 4]')
  })

  it('refuses mismatched sizes', () => {
    expect(show('[[1,2],[3,4]] + [[1,2,3],[4,5,6]]')).toBe("can't add 2×2 and 2×3: sizes must match")
    expect(show('[[1,2]] - [[1],[2]]')).toBe("can't subtract 1×2 and 2×1: sizes must match")
    expect(show('[[1,2],[3,4]] + 1')).toBe("can't add a number and a matrix")
  })

  it('scales by a number', () => {
    expect(show('3[[1,2],[3,4]]')).toBe('[3 6; 9 12]')
    expect(show('[[1,2],[3,4]] * 2')).toBe('[2 4; 6 8]')
    expect(show('-[[1,2],[3,4]]')).toBe('[-1 -2; -3 -4]')
    expect(show('[[2,4],[6,8]] / 2')).toBe('[1 2; 3 4]')
  })

  it('multiplies rows by columns', () => {
    expect(show('[[1,2],[3,4]] * [[5,6],[7,8]]')).toBe('[19 22; 43 50]')
    expect(show('[[1,2,3],[4,5,6]] * [[7,8],[9,10],[11,12]]')).toBe('[58 64; 139 154]')
    expect(show('[[1,2,3]] * [[1],[2],[3]]')).toBe('14')
    expect(show('[[1,2],[3,4]] * [[1,2,3]]')).toBe("can't multiply 2×2 by 1×3: columns of the first must equal rows of the second")
  })

  it('transposes', () => {
    expect(show('transpose([[1,2,3],[4,5,6]])')).toBe('[1 4; 2 5; 3 6]')
    expect(show('[[1,2,3],[4,5,6]]^T')).toBe('[1 4; 2 5; 3 6]')
    expect(show('[[1,2],[3,4]]ᵀ')).toBe('[1 3; 2 4]')
    expect(show('transpose([1,2,3])')).toBe('[1; 2; 3]')
  })

  it('takes determinants and traces of square matrices', () => {
    expect(show('det([[1,2],[3,4]])')).toBe('-2')
    expect(show('det([[2,0,1],[1,3,2],[1,1,2]])')).toBe('6')
    expect(show('det([[1,2],[2,4]])')).toBe('0')
    expect(show('trace([[1,2,3],[4,5,6],[7,8,9]])')).toBe('15')
    expect(show('tr([[1,2],[3,4]])')).toBe('5')
    expect(show('det([[1,2,3],[4,5,6]])')).toBe('determinant needs a square matrix (this is 2×3)')
    expect(show('trace([[1,2]])')).toBe('trace needs a square matrix (this is 1×2)')
  })

  it('inverts a 3x3 matrix', () => {
    expect(show('inv([[1,2,3],[0,1,4],[5,6,0]])')).toBe('[-24 18 5; 20 -15 -4; -5 4 1]')
    expect(show('[[1,2,3],[0,1,4],[5,6,0]]^-1')).toBe('[-24 18 5; 20 -15 -4; -5 4 1]')
    expect(show('[[1,2,3],[0,1,4],[5,6,0]]⁻¹')).toBe('[-24 18 5; 20 -15 -4; -5 4 1]')
  })

  it('gives fractions beside a decimal inverse', () => {
    const r = evaluateLine('inv([[4,7],[2,6]])')
    expect(r.display).toBe('[0.6 -0.7; -0.2 0.4]')
    expect(r.exact).toBe('[3/5 -7/10; -1/5 2/5]')
    expect(show('inv([[4,7],[2,6]])', { fractionMode: true })).toBe('[3/5 -7/10; -1/5 2/5]')
  })

  it('refuses a singular inverse', () => {
    expect(show('inv([[1,2],[2,4]])')).toBe('no inverse (determinant is 0)')
    expect(show('inv([[1,2,3]])')).toBe('inverse needs a square matrix (this is 1×3)')
  })

  it('reads other ways of writing a matrix', () => {
    expect(show('[1,2;3,4] * 2')).toBe('[2 4; 6 8]')
    expect(show('[1 2; 3 4] + [1 -2; 0 1]')).toBe('[2 0; 3 5]')
    expect(show('[[1,2],[3,4]]')).toBe('[1 2; 3 4]')
  })

  it('stores matrices and uses them on later lines', () => {
    const rows = evaluateSheet(['A = [[1,2],[3,4]]', 'B = [[0,1],[1,0]]', 'A*B', 'AB', 'A^T', 'det(A)', 'inv(A)', 'k = 3', 'k A', 'A + [[1]]'])
    expect(rows.map((r) => r.display)).toEqual([
      '[1 2; 3 4]',
      '[0 1; 1 0]',
      '[2 1; 4 3]',
      '[2 1; 4 3]',
      '[1 3; 2 4]',
      '-2',
      '[-2 1; 1.5 -0.5]',
      '3',
      '[3 6; 9 12]',
      "can't add 2×2 and 1×1: sizes must match",
    ])
    expect(rows[0]!.kind).toBe('assignment')
    expect(rows[0]!.quantity).toBe('[[1, 2], [3, 4]]')
  })

  it('leaves plain lists alone', () => {
    expect(looksLikeMatrix('[1, 2, 3]')).toBe(false)
    expect(looksLikeMatrix('mean([1, 2, 3])')).toBe(false)
    expect(show('mean([1, 2, 3])')).toBe('2')
    expect(show('2+3')).toBe('5')
  })
})
