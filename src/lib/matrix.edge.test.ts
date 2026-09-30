import { describe, expect, it } from 'vitest'
import { evaluateLine, evaluateSheet } from '../engine/evaluate'
import type { EvaluateOptions } from '../engine/types'
import { historyQuantities, isMatrixQuantity, lastHistoryNumber, slimHistoryRow, type HistoryRow } from './history'

const show = (text: string, opts: EvaluateOptions = {}) => evaluateLine(text, opts).display
const sheet = (lines: string[]) => evaluateSheet(lines).map((r) => r.display)

describe('matrix edge cases', () => {
  describe('how answers read', () => {
    it('1. rows are separated by ; and entries by spaces', () => {
      expect(show('[[1,2],[3,4]]')).toBe('[1 2; 3 4]')
    })
    it('2. a row stays on one line and a column stacks with ;', () => {
      expect(show('[1 2 3] * 2')).toBe('[2 4 6]')
      expect(show('[1 2 3]^T')).toBe('[1; 2; 3]')
    })
    it('3. an answer typed back in gives the same answer', () => {
      const r = evaluateLine('inv([[4,7],[2,6]])')
      expect(show(r.display)).toBe(r.display)
      expect(show(r.exact!)).toBe(r.display)
    })
    it('4. negative entries in a spaced row are entries, not subtraction', () => {
      expect(show('[1 -2; -3 4] * 1')).toBe('[1 -2; -3 4]')
      expect(show('[1 -2]')).toBe('[1 -2]')
    })
    it('5. decimals are cut to 6 figures so a row fits', () => {
      expect(show('inv([[1,2,3],[4,5,6],[7,8,10]])')).toBe('[-0.666667 -1.33333 1; -0.666667 3.66667 -2; 1 -2 1]')
    })
    it('6. simple fractions sit beside the decimals', () => {
      expect(evaluateLine('inv([[1,2,3],[4,5,6],[7,8,10]])').exact).toBe('[-2/3 -4/3 1; -2/3 11/3 -2; 1 -2 1]')
    })
    it('7. no fraction form once a denominator passes 1000', () => {
      expect(evaluateLine('[[1/1009, 1],[0, 1]]').exact).toBeUndefined()
      expect(evaluateLine('[[1/999, 1],[0, 1]]').exact).toBe('[1/999 1; 0 1]')
    })
    it('8. fraction mode makes the fractions the answer', () => {
      expect(show('inv([[4,7],[2,6]])', { fractionMode: true })).toBe('[3/5 -7/10; -1/5 2/5]')
    })
    it('9. float noise never shows: A times its inverse is exactly I', () => {
      expect(show('[[1,2,3],[0,1,4],[5,6,0]] * inv([[1,2,3],[0,1,4],[5,6,0]])')).toBe('[1 0 0; 0 1 0; 0 0 1]')
      expect(show('[[0.1,0.2],[0.3,0.4]] * 3')).toBe('[0.3 0.6; 0.9 1.2]')
    })
    it('10. a 1×1 answer is a number', () => {
      expect(show('[1 2 3] * [1; 2; 3]')).toBe('14')
      expect(evaluateLine('[[5]]').value?.n).toBe(5)
    })
  })

  describe('arithmetic', () => {
    it('11. adds entry by entry', () => {
      expect(show('[1 2; 3 4] + [10 20; 30 40]')).toBe('[11 22; 33 44]')
    })
    it('12. subtracting a matrix from itself is the zero matrix', () => {
      expect(show('[1 2; 3 4] - [1 2; 3 4]')).toBe('[0 0; 0 0]')
    })
    it('13. sizes that differ say both sizes', () => {
      expect(show('[1 2; 3 4] + [1 2 3; 4 5 6]')).toBe("can't add 2×2 and 2×3: sizes must match")
      expect(show('[1 2] - [1; 2]')).toBe("can't subtract 1×2 and 2×1: sizes must match")
    })
    it('14. a number plus a matrix is refused, not broadcast', () => {
      expect(show('[1 2; 3 4] + 1')).toBe("can't add a number and a matrix")
      expect(show('5 - [1 2; 3 4]')).toBe("can't subtract a number and a matrix")
    })
    it('15. a number scales on either side, written or implied', () => {
      expect(show('2 * [1 2; 3 4]')).toBe('[2 4; 6 8]')
      expect(show('[1 2; 3 4] * 2')).toBe('[2 4; 6 8]')
      expect(show('3[1 2; 3 4]')).toBe('[3 6; 9 12]')
      expect(show('-[1 2; 3 4]')).toBe('[-1 -2; -3 -4]')
      expect(show('[1 2; 3 4] * 1.5')).toBe('[1.5 3; 4.5 6]')
    })
    it('16. dividing by a number works; by 0 or a matrix says why not', () => {
      expect(show('[2 4; 6 8] / 2')).toBe('[1 2; 3 4]')
      expect(show('[1 2; 3 4] / 0')).toBe("can't divide by 0")
      expect(show('[1 2; 3 4] / [1 2; 3 4]')).toBe("can't divide by a matrix; multiply by its inverse instead")
    })
    it('17. multiplies rows by columns, and the shape follows', () => {
      expect(show('[1 2 3; 4 5 6] * [7 8; 9 10; 11 12]')).toBe('[58 64; 139 154]')
      expect(show('[1; 2] * [3 4]')).toBe('[3 4; 6 8]')
    })
    it('18. mismatched inner sizes say which rule broke', () => {
      expect(show('[1 2; 3 4] * [1 2 3]')).toBe("can't multiply 2×2 by 1×3: columns of the first must equal rows of the second")
      expect(show('[1 2 3] * [4 5 6]')).toBe(
        "can't multiply 1×3 by 1×3: columns of the first must equal rows of the second; for entry by entry, use dot for vectors",
      )
    })
    it('19. order matters: AB is not BA', () => {
      expect(sheet(['A = [1 2; 3 4]', 'B = [0 1; 1 0]', 'A*B', 'B*A']).slice(2)).toEqual(['[2 1; 4 3]', '[3 4; 1 2]'])
    })
    it('20. whole powers, including 0 and negatives', () => {
      expect(show('[1 2; 3 4]^2')).toBe('[7 10; 15 22]')
      expect(show('[1 2; 3 4]^0')).toBe('[1 0; 0 1]')
      expect(evaluateLine('[1 2; 3 4]^-2').exact).toBe('[11/2 -5/2; -15/4 7/4]')
    })
    it('21. a fractional or matrix power is refused', () => {
      expect(show('[1 2; 3 4]^(1/2)')).toBe('a matrix power must be a whole number')
      expect(show('2^[1 2; 3 4]')).toBe("can't raise to a matrix power")
    })
    it('22. a power needs a square matrix', () => {
      expect(show('[1 2 3]^2')).toBe('a power needs a square matrix (this is 1×3)')
    })
  })

  describe('transpose, determinant, trace, inverse', () => {
    it('23. every way of writing a transpose', () => {
      const want = '[1 4; 2 5; 3 6]'
      for (const t of ['transpose([1 2 3; 4 5 6])', '[1 2 3; 4 5 6]^T', '[1 2 3; 4 5 6]ᵀ', "[1 2 3; 4 5 6]'", 'transpose [1 2 3; 4 5 6]', 'transpose of [1 2 3; 4 5 6]', '[1 2 3; 4 5 6] transpose']) {
        expect(show(t), t).toBe(want)
      }
    })
    it('24. transposing twice gives the matrix back', () => {
      expect(show('([1 2 3; 4 5 6]^T)^T')).toBe('[1 2 3; 4 5 6]')
    })
    it('25. determinants of 1×1 to 3×3, a singular one, and a bare number', () => {
      expect(show('det([7])')).toBe('7')
      expect(show('det([1 2; 3 4])')).toBe('-2')
      expect(show('det([2 0 1; 1 3 2; 1 1 2])')).toBe('6')
      expect(show('det([1 2 3; 4 5 6; 7 8 9])')).toBe('0')
      expect(show('det(5)')).toBe('5')
    })
    it('26. a determinant needs a square matrix', () => {
      expect(show('det([1 2 3; 4 5 6])')).toBe('determinant needs a square matrix (this is 2×3)')
    })
    it('27. det(AB) = det(A) det(B), and a determinant is a number to keep using', () => {
      expect(show('det([1 2; 3 4] * [5 6; 7 8])')).toBe('4')
      expect(show('det([1 2; 3 4]) * det([5 6; 7 8])')).toBe('4')
      expect(show('det([1 2; 3 4]) + 1')).toBe('-1')
    })
    it('28. trace adds the diagonal; tr is the same', () => {
      expect(show('trace([1 2 3; 4 5 6; 7 8 9])')).toBe('15')
      expect(show('tr([1 2; 3 4])')).toBe('5')
      expect(show('trace([1 2])')).toBe('trace needs a square matrix (this is 1×2)')
    })
    it('29. inverts a 3×3 with whole entries', () => {
      expect(show('inv([1 2 3; 0 1 4; 5 6 0])')).toBe('[-24 18 5; 20 -15 -4; -5 4 1]')
    })
    it('30. every way of writing an inverse', () => {
      const want = '[-2 1; 1.5 -0.5]'
      for (const t of ['inv([1 2; 3 4])', 'inverse([1 2; 3 4])', '[1 2; 3 4]^-1', '[1 2; 3 4]⁻¹', 'inverse of [1 2; 3 4]', 'the inverse of [1 2; 3 4]', 'inv [1 2; 3 4]', '[1 2; 3 4] inverse']) {
        expect(show(t), t).toBe(want)
      }
    })
    it('31. no inverse when the determinant is 0', () => {
      expect(show('inv([1 2; 2 4])')).toBe('no inverse (determinant is 0)')
      expect(show('[1 2 3; 4 5 6; 7 8 9]^-1')).toBe('no inverse (determinant is 0)')
      expect(show('inv(0)')).toBe('no inverse (it is 0)')
      expect(show('inv(4)')).toBe('0.25')
    })
    it('32. a nearly singular matrix is refused rather than shown with junk digits', () => {
      expect(show('inv([1 1; 1 1.0000000001])')).toBe('no inverse (nearly singular: too close to determinant 0 to trust)')
    })
    it('33. tiny or lopsided entries are not "nearly singular"', () => {
      expect(show('inv([1e-20 0; 0 1e-20])')).toBe('[1e+20 0; 0 1e+20]')
      expect(show('inv([100000 0; 0 0.00001])')).toBe('[0.00001 0; 0 100000]')
    })
    it('34. an inverse needs a square matrix', () => {
      expect(show('inv([1 2 3])')).toBe('inverse needs a square matrix (this is 1×3)')
    })
  })

  describe('identity, rank, rref, vectors', () => {
    it('35. identity(n), eye(n), and sizes out of range', () => {
      expect(show('identity(3)')).toBe('[1 0 0; 0 1 0; 0 0 1]')
      expect(show('eye(2) * 5')).toBe('[5 0; 0 5]')
      expect(show('identity(0)')).toBe('identity needs a whole size from 1 to 20')
      expect(show('identity(2.5)')).toBe('identity needs a whole size from 1 to 20')
    })
    it('36. rank counts independent rows, square or not', () => {
      expect(show('rank([1 2; 3 4])')).toBe('2')
      expect(show('rank([1 2; 2 4])')).toBe('1')
      expect(show('rank([1 2 3; 4 5 6])')).toBe('2')
      expect(show('rank([0 0; 0 0])')).toBe('0')
    })
    it('37. rref reduces rows, with fractions where they come out', () => {
      expect(show('rref([1 2 3; 4 5 6])')).toBe('[1 0 -1; 0 1 2]')
      expect(show('rref([1 2; 2 4])')).toBe('[1 2; 0 0]')
      expect(evaluateLine('rref([2 1 1; 1 3 2])').exact).toBe('[1 0 1/5; 0 1 3/5]')
    })
    it('38. dot product of vectors, either way up', () => {
      expect(show('dot([1 2 3], [4 5 6])')).toBe('32')
      expect(show('dot([1; 2; 3], [4 5 6])')).toBe('32')
      expect(show('dot([1 2], [1 2 3])')).toBe('dot needs vectors of the same length (2 and 3)')
      expect(show('dot([1 2; 3 4], [1 2; 3 4])')).toBe('dot needs two vectors')
    })
    it('39. cross product keeps the first vector’s shape', () => {
      expect(show('cross([1 0 0], [0 1 0])')).toBe('[0 0 1]')
      expect(show('cross([1; 0; 0], [0; 1; 0])')).toBe('[0; 0; 1]')
      expect(show('cross([1 2], [3 4])')).toBe('cross needs two vectors of length 3')
    })
    it('40. norm is the length of a vector (and Frobenius for a matrix)', () => {
      expect(show('norm([3 4])')).toBe('5')
      expect(show('norm([1 2; 2 4])')).toBe('5')
    })
    it('41. word forms read as calls', () => {
      expect(show('det of [1 2; 3 4]')).toBe('-2')
      expect(show('the determinant of [1 2; 3 4]')).toBe('-2')
      expect(show('trace of [1 2; 3 4]')).toBe('5')
      expect(show('rank of [1 2; 2 4]')).toBe('1')
    })
  })

  describe('entries and names', () => {
    it('42. an entry can be any calculator math, in the angle mode set', () => {
      expect(show('[sqrt(2) 0; 0 1]')).toBe('[1.41421 0; 0 1]')
      expect(show('[[sin(30), 1],[0, 1]]')).toBe('[0.5 1; 0 1]')
      expect(show('[[sin(pi/6), 1],[0, 1]]', { angleMode: 'rad' })).toBe('[0.5 1; 0 1]')
      expect(show('[[2^10, 5!],[0, 1]]')).toBe('[1024 120; 0 1]')
    })
    it('43. stored numbers work inside and beside a matrix', () => {
      expect(sheet(['k = 3', '[k 0; 0 k]', 'k [1 2; 3 4]']).slice(1)).toEqual(['[3 0; 0 3]', '[3 6; 9 12]'])
    })
    it('44. a letter nobody defined leaves the answer blank, not "undefined"', () => {
      expect(show('[1 2; 3 x]')).toBe('')
      expect(show('[1 2; 3 4] * y')).toBe('')
    })
    it('45. ragged rows say so; a bracket still being typed stays quiet', () => {
      expect(show('[1, 2; 3]')).toBe('rows differ in length')
      expect(show('[[1,2],[3]]')).toBe('rows differ in length')
      expect(show('[[1,2],[3')).toBe('')
    })
    it('46. B = A^-1 stores a matrix, though B reads as a byte and A as an ampere', () => {
      const rows = evaluateSheet(['A = [1 2; 3 4]', 'B = A^-1', 'A*B', 'det A', "A'"])
      expect(rows[1]!.kind).toBe('assignment')
      expect(rows.map((r) => r.display).slice(1)).toEqual(['[-2 1; 1.5 -0.5]', '[1 0; 0 1]', '-2', '[1 3; 2 4]'])
    })
    it('47. ans is the whole matrix, and a number again after det', () => {
      expect(sheet(['[1 2; 3 4]', 'ans*2', 'inv(ans)', 'ans + 1'])).toEqual(['[1 2; 3 4]', '[2 4; 6 8]', '[-1 0.5; 0.75 -0.25]', "can't add a number and a matrix"])
      expect(sheet(['[1 2; 3 4]', 'det(ans)', 'ans*2'])).toEqual(['[1 2; 3 4]', '-2', '-4'])
    })
    it('48. a function the user named det wins over the matrix one', () => {
      expect(sheet(['det(x) = x + 1', 'det(4)'])).toEqual(['det(x) = x + 1', '5'])
    })
    it('49. plain lists and arithmetic are untouched', () => {
      expect(show('mean([1, 2, 3])')).toBe('2')
      expect(show('2 + 3')).toBe('5')
      expect(show('sin(30)')).toBe('0.5')
    })
  })

  describe('history', () => {
    it('50. a matrix row is ans, clears the old number, and keeps a long literal whole', () => {
      const row = (expr: string, display: string, extra: Partial<HistoryRow> = {}): HistoryRow => ({ id: expr, expr, display, ...extra })
      const matrix = evaluateLine('inv([1 2; 3 4])')
      const rows = [row('2+3', '5', { n: 5 }), row('inv([1 2; 3 4])', matrix.display, { quantity: matrix.quantity })]
      expect(isMatrixQuantity(matrix.quantity)).toBe(true)
      expect(historyQuantities(rows).ans).toBe('[[-2, 1], [3/2, -1/2]]')
      expect(lastHistoryNumber(rows)).toBeUndefined()
      expect(evaluateLine('ans * 2', { quantities: historyQuantities(rows) }).display).toBe('[-4 2; 3 -1]')
      const big = evaluateLine('identity(12) * 7').quantity!
      expect(big.length).toBeGreaterThan(240)
      expect(slimHistoryRow(row('identity(12) * 7', 'x', { quantity: big }))!.quantity).toBe(big)
    })
  })
})
