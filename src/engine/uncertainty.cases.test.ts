import { describe, expect, it } from 'vitest'
import { evaluateLine } from './evaluate'

// Q Calc adds absolute uncertainties (worst case) and rounds the shown ±.
// A "rad" note is the angle mode: a rad unit inside the call blanks the line.
type Case = {
  id: string
  expr: string
  display: string
  value?: number
  desc: string
  angleMode?: 'rad'
}

function check(c: Case) {
  const r = evaluateLine(c.expr, c.angleMode ? { angleMode: c.angleMode } : {})
  expect(r.display, `${c.id} ${c.desc}`).toBe(c.display)
  if (c.value == null) {
    expect(r.value, c.id).toBeUndefined()
    return
  }
  const n = r.value?.n
  expect(n, c.id).toBeTypeOf('number')
  const tol = 1e-6 * Math.max(1, Math.abs(c.value))
  expect(Math.abs(n! - c.value), c.id).toBeLessThan(tol)
}

function suite(title: string, cases: Case[]) {
  describe(title, () => {
    it.each(cases)('$id $desc', check)
  })
}

suite('addition and subtraction', [
  { id: 'TC-001', expr: '(1.0 ± 0.1) + (2.0 ± 0.2)', display: '3.0 ± 0.3', value: 3, desc: 'Basic positive addition' },
  { id: 'TC-002', expr: '(10.5 ± 0.05) + (4.3 ± 0.02)', display: '14.80 ± 0.07', value: 14.8, desc: 'Decimals addition' },
  { id: 'TC-003', expr: '(100.0 ± 2.0) - (45.0 ± 1.5)', display: '55 ± 4', value: 55, desc: 'Basic subtraction' },
  { id: 'TC-004', expr: '(0.005 ± 0.001) + (0.002 ± 0.001)', display: '0.007 ± 0.002', value: 0.007, desc: 'Small decimal addition' },
  { id: 'TC-005', expr: '(-5.0 ± 0.2) + (3.0 ± 0.1)', display: '-2.0 ± 0.3', value: -2, desc: 'Negative operand addition' },
  { id: 'TC-006', expr: '(-10.0 ± 0.4) - (-4.0 ± 0.3)', display: '-6.0 ± 0.7', value: -6, desc: 'Subtract negative operand' },
  { id: 'TC-007', expr: '(12.0 ± 0.0) + (8.0 ± 0.5)', display: '20.0 ± 0.5', value: 20, desc: 'Zero uncertainty operand' },
  { id: 'TC-008', expr: '(0.0 ± 0.1) + (5.5 ± 0.2)', display: '5.5 ± 0.3', value: 5.5, desc: 'Zero value operand' },
  { id: 'TC-009', expr: '(50.0 ± 0.1) - (50.0 ± 0.1)', display: '0.0 ± 0.2', value: 0, desc: 'Identical values subtraction' },
  { id: 'TC-010', expr: '(1.234 ± 0.005) + (5.678 ± 0.009)', display: '6.912 ± 0.014', value: 6.912, desc: '3 decimal places' },
  { id: 'TC-011', expr: '(250 ± 10) + (750 ± 20)', display: '1000 ± 30', value: 1000, desc: 'Integer representation' },
  { id: 'TC-012', expr: '(15.0 ± 0.3) - (20.0 ± 0.4)', display: '-5.0 ± 0.7', value: -5, desc: 'Result crosses into negative' },
  { id: 'TC-013', expr: '(1.0 ± 0.5) + (2.0 ± 0.5)', display: '3.0 ± 1.0', value: 3, desc: 'Equal uncertainties' },
  { id: 'TC-014', expr: '(100.0 ± 10.0) - (0.1 ± 0.01)', display: '100 ± 10', value: 99.9, desc: 'Dominant uncertainty term' },
  { id: 'TC-015', expr: '(3.14159 ± 0.00001) + (2.71828 ± 0.00001)', display: '5.85987 ± 0.00002', value: 5.85987, desc: 'High precision constants' },
  { id: 'TC-016', expr: '(-8.5 ± 0.2) - (1.5 ± 0.1)', display: '-10.0 ± 0.3', value: -10, desc: 'Negative minus positive' },
  { id: 'TC-017', expr: '(0.0 ± 0.0) + (0.0 ± 0.0)', display: '0', value: 0, desc: 'Absolute zeros' },
  { id: 'TC-018', expr: '(1000 ± 50) - (999 ± 50)', display: '0 ± 100', value: 1, desc: 'Subtractive cancellation' },
  { id: 'TC-019', expr: '(4.5 ± 0.25) + (1.5 ± 0.75)', display: '6.0 ± 1.0', value: 6, desc: 'Quarters decimals' },
  { id: 'TC-020', expr: '(12.345 ± 0.678) - (2.345 ± 0.123)', display: '10.0 ± 0.8', value: 10, desc: 'Asymmetric input values' },
])

suite('multiplication and division', [
  { id: 'TC-021', expr: '(4.0 ± 0.2) * (3.0 ± 0.1)', display: '12.0 ± 1.0', value: 12, desc: 'Basic multiplication' },
  { id: 'TC-022', expr: '(10.0 ± 0.5) / (2.0 ± 0.1)', display: '5.0 ± 0.5', value: 5, desc: 'Basic division' },
  { id: 'TC-023', expr: '(0.5 ± 0.05) * (0.2 ± 0.02)', display: '0.10 ± 0.02', value: 0.1, desc: 'Decimals product' },
  { id: 'TC-024', expr: '(1.0 ± 0.02) / (4.0 ± 0.08)', display: '0.250 ± 0.010', value: 0.25, desc: 'Fractional division' },
  { id: 'TC-025', expr: '(-5.0 ± 0.2) * (2.0 ± 0.1)', display: '-10.0 ± 0.9', value: -10, desc: 'Negative operand multiplication' },
  { id: 'TC-026', expr: '(-12.0 ± 0.6) / (-3.0 ± 0.3)', display: '4.0 ± 0.6', value: 4, desc: 'Negative divided by negative' },
  { id: 'TC-027', expr: '(100.0 ± 0.0) * (5.0 ± 0.25)', display: '500 ± 30', value: 500, desc: 'One term exact (zero uncertainty)' },
  { id: 'TC-028', expr: '(50.0 ± 2.5) / (10.0 ± 0.0)', display: '5.0 ± 0.3', value: 5, desc: 'Denominator exact' },
  { id: 'TC-029', expr: '(8.0 ± 0.8) * (0.1 ± 0.01)', display: '0.80 ± 0.16', value: 0.8, desc: 'Product with relative equal uncs' },
  { id: 'TC-030', expr: '(100.0 ± 10.0) / (0.5 ± 0.05)', display: '200 ± 40', value: 200, desc: 'Division scaling up' },
  { id: 'TC-031', expr: '(2.5 ± 0.1) * (4.0 ± 0.2)', display: '10.0 ± 0.9', value: 10, desc: 'Product results in integer' },
  { id: 'TC-032', expr: '(9.0 ± 0.3) / (3.0 ± 0.1)', display: '3.0 ± 0.2', value: 3, desc: 'Identical relative uncertainties' },
  { id: 'TC-033', expr: '(15.0 ± 1.5) * (1.0 ± 0.1)', display: '15 ± 3', value: 15, desc: 'Multiply by 1 with uncertainty' },
  { id: 'TC-034', expr: '(15.0 ± 1.5) / (1.0 ± 0.1)', display: '15 ± 3', value: 15, desc: 'Divide by 1 with uncertainty' },
  { id: 'TC-035', expr: '(0.002 ± 0.0001) * (500.0 ± 10.0)', display: '1.00 ± 0.07', value: 1, desc: 'Extreme scale multiplication' },
  { id: 'TC-036', expr: '(0.008 ± 0.0004) / (0.002 ± 0.0001)', display: '4.0 ± 0.4', value: 4, desc: 'Small decimal division' },
  { id: 'TC-037', expr: '(6.0 ± 0.6) * (-2.0 ± 0.1)', display: '-12.0 ± 1.8', value: -12, desc: 'Positive times negative' },
  { id: 'TC-038', expr: '(-20.0 ± 1.0) / (4.0 ± 0.2)', display: '-5.0 ± 0.5', value: -5, desc: 'Negative divided by positive' },
  { id: 'TC-039', expr: '(1000.0 ± 50.0) * (0.001 ± 0.0001)', display: '1.00 ± 0.15', value: 1, desc: 'Large and small magnitude product' },
  { id: 'TC-040', expr: '(1.0 ± 0.05) / (100.0 ± 2.0)', display: '0.0100 ± 0.0007', value: 0.01, desc: 'Division resulting in small decimal' },
])

suite('exponents, powers and roots', [
  { id: 'TC-041', expr: '(3.0 ± 0.1)^2', display: '9.0 ± 0.6', value: 9, desc: 'Square power' },
  { id: 'TC-042', expr: '(2.0 ± 0.1)^3', display: '8.0 ± 1.2', value: 8, desc: 'Cube power' },
  { id: 'TC-043', expr: '(16.0 ± 0.8)^0.5', display: '4.00 ± 0.10', value: 4, desc: 'Square root (power 0.5)' },
  { id: 'TC-044', expr: '(27.0 ± 2.7)^(1/3)', display: '3.00 ± 0.10', value: 3, desc: 'Cube root' },
  { id: 'TC-045', expr: '(2.0 ± 0.1)^(-1)', display: '0.50 ± 0.03', value: 0.5, desc: 'Inverse power (-1)' },
  { id: 'TC-046', expr: '(5.0 ± 0.2)^(-2)', display: '0.040 ± 0.003', value: 0.04, desc: 'Negative integer power' },
  { id: 'TC-047', expr: '(10.0 ± 0.5)^0', display: '1', value: 1, desc: 'Zero exponent' },
  { id: 'TC-048', expr: '(1.0 ± 0.05)^5', display: '1.0 ± 0.3', value: 1, desc: 'Power of 1' },
  { id: 'TC-049', expr: '(10.0 ± 0.1)^1', display: '10.00 ± 0.10', value: 10, desc: 'Power of 1 exponent' },
  { id: 'TC-050', expr: '(0.5 ± 0.02)^2', display: '0.25 ± 0.02', value: 0.25, desc: 'Fractional base square' },
  { id: 'TC-051', expr: '(81.0 ± 1.62)^0.25', display: '3.000 ± 0.015', value: 3, desc: '4th root' },
  { id: 'TC-052', expr: '(4.0 ± 0.1)^1.5', display: '8.0 ± 0.3', value: 8, desc: 'Decimal exponent (1.5)' },
  { id: 'TC-053', expr: '(100.0 ± 5.0)^(-0.5)', display: '0.100 ± 0.003', value: 0.1, desc: 'Inverse square root' },
  { id: 'TC-054', expr: '(2.5 ± 0.2)^4', display: '39 ± 13', value: 39.0625, desc: '4th power decimal base' },
  { id: 'TC-055', expr: '(0.01 ± 0.001)^0.5', display: '0.100 ± 0.005', value: 0.1, desc: 'Square root of small decimal' },
])

suite('mixed and multi-step expressions', [
  { id: 'TC-056', expr: '(2.0 ± 0.1) * (3.0 ± 0.1) + (4.0 ± 0.2)', display: '10.0 ± 0.7', value: 10, desc: 'Multiply then Add' },
  { id: 'TC-057', expr: '((5.0 ± 0.2) + (3.0 ± 0.1)) / (2.0 ± 0.1)', display: '4.0 ± 0.4', value: 4, desc: 'Add in parens then Divide' },
  { id: 'TC-058', expr: '(10.0 ± 0.5) - (2.0 ± 0.1)^2', display: '6.0 ± 0.9', value: 6, desc: 'Subtract squared term' },
  { id: 'TC-059', expr: '((4.0 ± 0.2) * (5.0 ± 0.3)) / ((2.0 ± 0.1) + (3.0 ± 0.1))', display: '4.0 ± 0.6', value: 4, desc: 'Fraction with products & sums' },
  { id: 'TC-060', expr: '3 * (2.0 ± 0.1) + 4 * (1.0 ± 0.05)', display: '10.0 ± 0.5', value: 10, desc: 'Linear combination of uncertain variables' },
  { id: 'TC-061', expr: '(1.0 ± 0.1) + (2.0 ± 0.1) + (3.0 ± 0.1) + (4.0 ± 0.1)', display: '10.0 ± 0.4', value: 10, desc: 'Four-term sum' },
  { id: 'TC-062', expr: '((2.0 ± 0.1)^2 + (3.0 ± 0.1)^2)^0.5', display: '3.61 ± 0.14', value: 3.605551275463989, desc: 'Pythagorean norm of uncertainties' },
  { id: 'TC-063', expr: '(100.0 ± 2.0) / ((5.0 ± 0.1) - (3.0 ± 0.1))', display: '50 ± 6', value: 50, desc: 'Divide by difference' },
  { id: 'TC-064', expr: '(2.0 ± 0.1) * (3.0 ± 0.1) * (4.0 ± 0.1)', display: '24 ± 3', value: 24, desc: 'Three-term product' },
  { id: 'TC-065', expr: '((10.0 ± 0.5) / (2.0 ± 0.1))^2', display: '25 ± 5', value: 25, desc: 'Square of quotient' },
  { id: 'TC-066', expr: '(1.0 ± 0.05) - (0.5 ± 0.02) * (2.0 ± 0.1)', display: '0.00 ± 0.14', value: 0, desc: 'Subtraction after product' },
  { id: 'TC-067', expr: '((8.0 ± 0.4)^0.5) * (2.0 ± 0.1)', display: '5.7 ± 0.4', value: 5.656854249492381, desc: 'Root multiplied by factor' },
  { id: 'TC-068', expr: '10 - (2.0 ± 0.1) * (3.0 ± 0.1)', display: '4.0 ± 0.5', value: 4, desc: 'Exact scalar minus product' },
  { id: 'TC-069', expr: '((6.0 ± 0.3) + (4.0 ± 0.2)) * ((5.0 ± 0.1) - (2.0 ± 0.1))', display: '30 ± 4', value: 30, desc: 'Product of sum and difference' },
  { id: 'TC-070', expr: '((1.0 ± 0.1) / (2.0 ± 0.1)) + ((3.0 ± 0.1) / (4.0 ± 0.1))', display: '1.25 ± 0.12', value: 1.25, desc: 'Sum of two fractions' },
])

suite('special and transcendental functions', [
  { id: 'TC-071', expr: 'exp(1.0 ± 0.05)', display: '2.72 ± 0.14', value: Math.exp(1), desc: 'Exponential function e^x' },
  { id: 'TC-072', expr: 'exp(0.0 ± 0.1)', display: '1.00 ± 0.11', value: 1, desc: 'e^0 with uncertainty' },
  { id: 'TC-073', expr: 'ln(2.71828 ± 0.1)', display: '1.00 ± 0.04', value: Math.log(2.71828), desc: 'Natural logarithm near e' },
  { id: 'TC-074', expr: 'ln(10.0 ± 0.5)', display: '2.30 ± 0.05', value: Math.log(10), desc: 'Natural log of 10' },
  { id: 'TC-075', expr: 'sin(0.0 ± 0.1)', display: '0.00 ± 0.10', value: 0, desc: 'Sine at 0 radians', angleMode: 'rad' },
  { id: 'TC-076', expr: 'cos(0.0 ± 0.1)', display: '1.000 ± 0.005', value: 1, desc: 'Cosine at 0 radians', angleMode: 'rad' },
  { id: 'TC-077', expr: 'sin(π/2 ± 0.05)', display: '1.0000 ± 0.0008', value: 1, desc: 'Sine at π/2 radians', angleMode: 'rad' },
  { id: 'TC-078', expr: 'cos(π/2 ± 0.05)', display: '0.00 ± 0.04', value: 0, desc: 'Cosine at π/2 radians', angleMode: 'rad' },
  { id: 'TC-079', expr: 'log10(100.0 ± 2.0)', display: '2.000 ± 0.009', value: 2, desc: 'Base-10 Logarithm' },
  { id: 'TC-080', expr: 'tan(0.5 ± 0.02)', display: '0.55 ± 0.03', value: Math.tan(0.5), desc: 'Tangent function', angleMode: 'rad' },
])

suite('scientific notation and edge cases', [
  { id: 'TC-081', expr: '(1.0e6 ± 1.0e4) + (2.0e6 ± 2.0e4)', display: '3000000 ± 30000', value: 3e6, desc: 'Large numbers in scientific notation' },
  { id: 'TC-082', expr: '(1.0e-6 ± 1.0e-7) + (2.0e-6 ± 2.0e-7)', display: '0.0000030 ± 0.0000003', value: 3e-6, desc: 'Tiny numbers in scientific notation' },
  { id: 'TC-083', expr: '(6.022e23 ± 1e21) * (1.602e-19 ± 1e-21)', display: '96500 ± 800', value: 6.022e23 * 1.602e-19, desc: 'Physical constants order scale product' },
  { id: 'TC-084', expr: '(1.0e10 ± 100) - (1.0e10 ± 100)', display: '0 ± 200', value: 0, desc: 'Large magnitude difference giving zero' },
  { id: 'TC-085', expr: '(1.0 ± 0.01) / (1.0e-5 ± 1.0e-7)', display: '100000 ± 2000', value: 1e5, desc: 'Divide by tiny number' },
  { id: 'TC-086', expr: '(0.0 ± 0.0) * (5.0 ± 0.2)', display: '0', value: 0, desc: 'Absolute zero multiplied by uncertain value' },
  { id: 'TC-087', expr: '(1.0e-3 ± 1.0e-4)^2', display: '0.0000010 ± 0.0000002', value: 1e-6, desc: 'Square of tiny number' },
  { id: 'TC-088', expr: '(100.0 ± 0.0001) + (0.0001 ± 0.00001)', display: '100.00010 ± 0.00011', value: 100.0001, desc: 'Asymmetric magnitude addition' },
  { id: 'TC-089', expr: '(10.0 ± 0.1)^(-3)', display: '0.00100 ± 0.00003', value: 0.001, desc: 'Negative cubic power' },
  { id: 'TC-090', expr: '(999999 ± 1) + (1 ± 1)', display: '1000000 ± 2', value: 1e6, desc: 'Integer boundary precision test' },
])

suite('invalid inputs and syntax', [
  { id: 'TC-091', expr: '(5.0 ± -0.1) + (2.0 ± 0.1)', display: '', desc: 'Negative uncertainty provided' },
  { id: 'TC-092', expr: '(5.0 ± 0.1) / (0.0 ± 0.0)', display: '', desc: 'Division by absolute zero (0 ± 0)' },
  { id: 'TC-093', expr: '(-4.0 ± 0.2)^0.5', display: '', desc: 'Square root of negative base' },
  { id: 'TC-094', expr: 'ln(-2.0 ± 0.1)', display: '', desc: 'Logarithm of negative nominal value' },
  { id: 'TC-095', expr: 'ln(0.0 ± 0.1)', display: '', desc: 'Logarithm of zero' },
  { id: 'TC-096', expr: '5.0 ± 0.1 +', display: '', desc: 'Dangling binary operator (Syntax Error)' },
  { id: 'TC-097', expr: '(3.0 ± 0.1) / (0.0 ± 0.5)', display: '', desc: 'Division by value zero with non-zero uncertainty' },
  { id: 'TC-098', expr: '((2.0 ± 0.1) + (3.0 ± 0.1)', display: '5.0 ± 0.2', value: 5, desc: 'Unmatched opening parenthesis is closed' },
  { id: 'TC-099', expr: '3.0 ± ± 0.2', display: '', desc: 'Double uncertainty symbol (±±)' },
  { id: 'TC-100', expr: 'tan(1.57079632679 ± 0.01)', display: '200000000000 ± 200000000000', value: Math.tan(1.57079632679), desc: 'Tangent near π/2 stays finite', angleMode: 'rad' },
])
