import { describe, expect, it } from 'vitest'
import { hasDualAnswer, insertableAnswer } from '../lib/answer'
import { evaluateLine, evaluateSheet } from './evaluate'
import { formatNumber } from './format'
import {
  extraArithmetic,
  extraCombinatorics,
  extraConstants,
  extraEdges,
  extraExactCopy,
  extraFractions,
  extraHyperbolic,
  extraInverseTrig,
  extraLatex,
  extraLogs,
  extraPowers,
  extraRadicals,
  extraRounding,
  extraSciNotation,
  extraStats,
  extraTrigDeg,
  extraTrigRad,
  extraTrigSimp,
} from './scientific.extras'
import { dualLabel } from './simplify'

type Angle = 'deg' | 'rad'

type Case = {
  name: string
  input: string | string[]
  /** Line index for multi-line sheets (default: last line). */
  at?: number
  expected?: number
  eps?: number
  display?: string | RegExp
  undefined?: true
  infinity?: true
  fractionMode?: boolean
  rationalize?: boolean
  angleMode?: Angle
  gte?: number
  lt?: number
  lte?: number
  integer?: boolean
  listLength?: number
  matches?: string
  exact?: string
}

function resultOf(c: Case) {
  const opts = { angleMode: c.angleMode ?? ('deg' as const), fractionMode: c.fractionMode, rationalize: c.rationalize }
  if (Array.isArray(c.input)) {
    return evaluateSheet(c.input, opts)[c.at ?? c.input.length - 1]!
  }
  return evaluateLine(c.input, opts)
}

function run(c: Case): void {
  const r = resultOf(c)
  const label = `${Array.isArray(c.input) ? c.input.join(' | ') : c.input} → ${r.display}`

  if (c.undefined) {
    expect(r.display, label).toBe('undefined')
    return
  }
  if (c.infinity) {
    expect(r.display, label).toMatch(/∞|Infinity/)
    return
  }
  if (c.exact !== undefined) {
    expect(r.exact, `${label} exact`).toBe(c.exact)
  }
  if (c.display !== undefined) {
    if (typeof c.display === 'string') expect(r.display, label).toBe(c.display)
    else expect(r.display, label).toMatch(c.display)
    if (c.expected === undefined && !c.matches && c.listLength === undefined) return
  }
  if (c.listLength !== undefined) {
    expect(r.display.startsWith('['), label).toBe(true)
    expect(r.display.split(',').length, label).toBe(c.listLength)
    return
  }
  if (c.matches) {
    const other = evaluateLine(c.matches, { angleMode: c.angleMode ?? 'deg' })
    expect(r.value?.n, label).toEqual(expect.any(Number))
    expect(other.value?.n).toEqual(expect.any(Number))
    expect(Math.abs(r.value!.n - other.value!.n), label).toBeLessThan(c.eps ?? 1e-8)
    return
  }

  const actual = r.value?.n
  expect(actual, label).toEqual(expect.any(Number))
  if (c.integer) expect(Number.isInteger(actual!), label).toBe(true)
  if (c.gte !== undefined) expect(actual!, label).toBeGreaterThanOrEqual(c.gte)
  if (c.lt !== undefined) expect(actual!, label).toBeLessThan(c.lt)
  if (c.lte !== undefined) expect(actual!, label).toBeLessThanOrEqual(c.lte)
  if (c.expected !== undefined) {
    const eps = c.eps ?? 1e-8
    expect(Math.abs(actual! - c.expected), `${label} (expected ${c.expected})`).toBeLessThan(eps)
  }
}

function suite(title: string, cases: Case[]) {
  describe(title, () => {
    it.each(cases)('$name', (c) => run(c))
  })
}

suite('Basic Arithmetic & Order of Operations', [
  { name: 'Simple Addition', input: '125 + 379', expected: 504 },
  { name: 'Subtraction with Negative Result', input: '42 - 189', expected: -147 },
  { name: 'Multiplication', input: '24 * 15', expected: 360 },
  { name: 'Typeset middle-dot multiplication', input: '24 · 15', expected: 360 },
  { name: 'Typeset dot-operator multiplication', input: '6⋅7', expected: 42 },
  { name: 'Typeset bullet-operator multiplication', input: '2∙8', expected: 16 },
  { name: 'Word-dot multiplication', input: '3 dot 4', expected: 12 },
  { name: 'Glued word-dot multiplication', input: '4dot1', expected: 4 },
  { name: 'Exact Division', input: '144 / 12', expected: 12 },
  { name: 'Chained Addition & Subtraction', input: '100 - 45 + 12 - 3', expected: 64 },
  { name: 'Order of Operations (No Parens)', input: '5 + 3 * 4', expected: 17 },
  { name: 'Order of Operations (With Parens)', input: '(5 + 3) * 4', expected: 32 },
  { name: 'Nested Parentheses', input: '((12 + 8) / 4) * (15 - (3 + 2))', expected: 50 },
  { name: 'Unary Negation', input: '-(-15)', expected: 15 },
  { name: 'Chained Multiplication', input: '2 * 3 * 4 * 5 * 6', expected: 720 },
  { name: 'Negative Addition', input: '-25 + (-30)', expected: -55 },
  { name: 'Negative Subtraction', input: '-40 - (-12)', expected: -28 },
  { name: 'Zero Multiplication', input: '99999 * 0', expected: 0 },
  { name: 'Multi-level Nesting', input: '2 * (3 + (4 * (5 - 2)))', expected: 30 },
  { name: 'Decimal Addition', input: '0.1 + 0.2', expected: 0.3 },
  { name: 'Mixed Arithmetic', input: '12 + 34 - 56 * 78 / 9', expected: 12 + 34 - (56 * 78) / 9 },
  { name: 'Subtracting Negative', input: '5 - (-10)', expected: 15 },
  { name: 'Implicit Multiplication (Paren)', input: '3(4 + 5)', expected: 27 },
  { name: 'Juxtaposition Multiplication', input: '(2+3)(4+5)', expected: 45 },
  { name: 'Left-to-Right Division Precedence', input: '100 / 5 / 2', expected: 10 },
  ...extraArithmetic(),
])

suite('Fractions & Decimals', [
  { name: 'Fraction Input', input: '3 / 4', expected: 0.75 },
  { name: 'Fraction to Decimal Toggle', input: '3/4', display: '0.75' },
  { name: 'Decimal to Fraction Toggle', input: '0.625', fractionMode: true, display: '5/8' },
  { name: 'Fraction Addition', input: '1/2 + 1/3', expected: 5 / 6 },
  { name: 'Fraction Addition (display)', input: '1/2 + 1/3', fractionMode: true, display: '5/6' },
  { name: 'Fraction Subtraction', input: '5/6 - 1/4', expected: 7 / 12 },
  { name: 'Fraction Multiplication', input: '(2/3) * (9/10)', expected: 3 / 5 },
  { name: 'Fraction Division', input: '(3/4) / (5/8)', expected: 6 / 5 },
  { name: 'Complex Fraction', input: '(1/2 + 3/4) / (2/3 - 1/6)', expected: 5 / 2 },
  { name: 'Mixed Number Entry', input: '2 + 1/3', expected: 7 / 3 },
  { name: 'Repeating Decimal Conversion', input: '0.333333333', fractionMode: true, display: '1/3' },
  { name: 'Negative Fraction', input: '-7/8', expected: -0.875 },
  { name: 'Improper Fraction Conversion', input: '11 / 4', expected: 2.75 },
  { name: 'Precision Fraction Conversion', input: '0.142857', fractionMode: true, display: '1/7' },
  { name: 'Unit conversion to fraction', input: '2032mm to ft', fractionMode: true, display: '20/3 ft' },
  { name: 'Unit conversion exact fraction', input: '2032mm to ft', exact: '20/3 ft' },
  { name: 'Unit arithmetic to fraction', input: '3 ft / 2', fractionMode: true, display: '3/2 ft' },
  { name: 'Unit sum to fraction', input: '(1/2) m + (1/3) m', fractionMode: true, display: '5/6 m' },
  { name: 'Chained Fractions', input: '1/2 + 1/4 + 1/8 + 1/16', expected: 15 / 16 },
  { name: 'Zero Numerator', input: '0 / 5', expected: 0 },
  { name: 'Fraction Exponents', input: '(2/3)^3', expected: 8 / 27 },
  { name: 'Square Root of Fraction', input: 'sqrt(9/16)', expected: 3 / 4 },
  { name: 'Percent Key Function', input: '20%', expected: 0.2 },
  { name: 'Percent Of Calculation', input: '15% * 200', expected: 30 },
  { name: 'Percent Of Expression', input: '25 % of 80', display: '20' },
  { name: 'What is percent of', input: 'what is 40% of 90', expected: 36 },
  { name: 'What is percent of (capitalized)', input: 'What is 40% of 90?', expected: 36 },
  ...extraFractions(),
])

suite('Exponents, Powers & Roots', [
  { name: 'Integer Power', input: '12^2', expected: 144 },
  { name: 'Cube Power', input: '5^3', expected: 125 },
  { name: 'Large Power', input: '2^10', expected: 1024 },
  { name: 'Fractional Power (Square Root)', input: '16^(1/2)', expected: 4 },
  { name: 'Rational Power', input: '8^(2/3)', expected: 4 },
  { name: 'Negative Exponent', input: '2^(-3)', expected: 0.125 },
  { name: 'Zero Exponent', input: '5^0', expected: 1 },
  { name: 'Square Root Button', input: 'sqrt(144)', expected: 12 },
  { name: 'Irrational Square Root', input: 'sqrt(2)', expected: Math.SQRT2 },
  { name: 'Cube Root Function', input: 'nthroot(27, 3)', expected: 3 },
  { name: 'Cube Root Function (power)', input: '27^(1/3)', expected: 3 },
  { name: '4th Root Function', input: 'nthroot(256, 4)', expected: 4 },
  { name: 'Nested Square Roots', input: 'sqrt(sqrt(81))', expected: 3 },
  { name: 'Pythagorean Sum Root', input: 'sqrt(3^2 + 4^2)', expected: 5 },
  { name: 'Power of Power', input: '(2^3)^4', expected: 4096 },
  { name: 'Stacked Exponent', input: '2^(3^2)', expected: 512 },
  { name: 'Negative Base (Even Power)', input: '(-3)^4', expected: 81 },
  { name: 'Negative Base (Odd Power)', input: '(-3)^3', expected: -27 },
  { name: 'Ten Power Button (10^x)', input: '10^4', expected: 10_000 },
  { name: 'Base e Exponent (e^x)', input: 'e^2', expected: Math.E ** 2 },
  { name: 'Negative Rational Power', input: '32^(-3/5)', expected: 0.125 },
  ...extraPowers(),
])

suite('Exponents & Logarithms', [
  { name: 'Natural Log of e', input: 'ln(e)', expected: 1 },
  { name: 'Natural Log of 1', input: 'ln(1)', expected: 0 },
  { name: 'Natural Log of Decimal', input: 'ln(0.5)', expected: Math.log(0.5) },
  { name: 'Natural Log Inverse Power', input: 'ln(e^5)', expected: 5 },
  { name: 'Common Log of 100', input: 'log(100)', expected: 2 },
  { name: 'Common Log of 100000', input: 'log(100000)', expected: 5 },
  { name: 'Common Log of 1', input: 'log(1)', expected: 0 },
  { name: 'Common Log of Decimal', input: 'log(0.001)', expected: -3 },
  { name: 'Custom Base Logarithm (Integer)', input: 'log_2(8)', expected: 3 },
  { name: 'Custom Base Logarithm', input: 'log_3(81)', expected: 4 },
  { name: 'Custom Base Logarithm (Fractional)', input: 'log_5(0.2)', expected: -1 },
  { name: 'Base e constant', input: 'e^1', expected: Math.E },
  { name: 'Log Product Rule Verification', input: 'ln(5 * 8)', matches: 'ln(5) + ln(8)' },
  { name: 'Log Quotient Rule Verification', input: 'ln(20 / 4)', matches: 'ln(20) - ln(4)' },
  { name: 'Log Power Rule Verification', input: 'log(10^3)', expected: 3 },
  ...extraLogs(),
])

suite('Trigonometry in Radians (Toggle RAD active)', [
  { name: 'Sine in Radians', input: 'sin(pi/6)', angleMode: 'rad', expected: 0.5 },
  { name: 'Sine of Pi', input: 'sin(pi)', angleMode: 'rad', expected: 0 },
  { name: 'Cosine in Radians', input: 'cos(pi/3)', angleMode: 'rad', expected: 0.5 },
  { name: 'Cosine of Pi', input: 'cos(pi)', angleMode: 'rad', expected: -1 },
  { name: 'Tangent in Radians', input: 'tan(pi/4)', angleMode: 'rad', expected: 1 },
  { name: 'Tangent of Zero', input: 'tan(0)', angleMode: 'rad', expected: 0 },
  { name: 'Cosecant in Radians', input: 'csc(pi/2)', angleMode: 'rad', expected: 1 },
  { name: 'Secant in Radians', input: 'sec(0)', angleMode: 'rad', expected: 1 },
  { name: 'Cotangent in Radians', input: 'cot(pi/4)', angleMode: 'rad', expected: 1 },
  { name: 'Pythagorean Identity', input: 'sin(1.2)^2 + cos(1.2)^2', angleMode: 'rad', expected: 1 },
  { name: 'Sine Negative Angle', input: 'sin(-pi/4)', angleMode: 'rad', expected: -Math.SQRT2 / 2 },
  { name: 'Cosine Negative Angle', input: 'cos(-pi/3)', angleMode: 'rad', expected: 0.5 },
  { name: 'Double Angle Verification', input: '2 * sin(0.5) * cos(0.5)', angleMode: 'rad', matches: 'sin(1)' },
  { name: 'Tangent 2nd Quadrant', input: 'tan(3*pi/4)', angleMode: 'rad', expected: -1 },
  { name: 'Cosecant in Radians (pi/6)', input: 'csc(pi/6)', angleMode: 'rad', expected: 2 },
  { name: 'Secant in Radians (pi/3)', input: 'sec(pi/3)', angleMode: 'rad', expected: 2 },
  { name: 'Cotangent in Radians (pi/6)', input: 'cot(pi/6)', angleMode: 'rad', expected: Math.sqrt(3) },
  { name: 'Sine Full Circle', input: 'sin(2*pi)', angleMode: 'rad', expected: 0 },
  { name: 'Cosine Full Circle', input: 'cos(2*pi)', angleMode: 'rad', expected: 1 },
  { name: 'Large Angle Trigonometry', input: 'sin(100*pi)', angleMode: 'rad', expected: 0 },
  ...extraTrigRad(),
])

suite('Trigonometry in Degrees (Toggle DEG active)', [
  { name: 'Sine in Degrees', input: 'sin(30)', expected: 0.5 },
  { name: 'Sine 90 Degrees', input: 'sin(90)', expected: 1 },
  { name: 'Cosine in Degrees', input: 'cos(60)', expected: 0.5 },
  { name: 'Cosine 180 Degrees', input: 'cos(180)', expected: -1 },
  { name: 'Tangent in Degrees', input: 'tan(45)', expected: 1 },
  { name: 'Tangent 0 Degrees', input: 'tan(0)', expected: 0 },
  { name: 'Cosecant in Degrees', input: 'csc(30)', expected: 2 },
  { name: 'Secant in Degrees', input: 'sec(60)', expected: 2 },
  { name: 'Cotangent in Degrees', input: 'cot(45)', expected: 1 },
  { name: 'Sine 45 Degrees', input: 'sin(45)', expected: Math.SQRT2 / 2 },
  { name: 'Cosine 45 Degrees', input: 'cos(45)', expected: Math.SQRT2 / 2 },
  { name: 'Tangent 135 Degrees', input: 'tan(135)', expected: -1 },
  { name: 'Sine 270 Degrees', input: 'sin(270)', expected: -1 },
  { name: 'Cosine 360 Degrees', input: 'cos(360)', expected: 1 },
  { name: 'Sine Negative Degrees', input: 'sin(-90)', expected: -1 },
  { name: 'Cosecant 90 Degrees', input: 'csc(90)', expected: 1 },
  { name: 'Secant 180 Degrees', input: 'sec(180)', expected: -1 },
  { name: 'Cotangent 30 Degrees', input: 'cot(30)', expected: Math.sqrt(3) },
  { name: 'Sine 15 Degrees', input: 'sin(15)', expected: Math.sin((15 * Math.PI) / 180) },
  { name: 'Degree Mode Identity Check', input: 'sin(37)^2 + cos(37)^2', expected: 1 },
  ...extraTrigDeg(),
])

suite('Inverse Trigonometry', [
  { name: 'Arcsine (RAD Mode)', input: 'arcsin(0.5)', angleMode: 'rad', expected: Math.PI / 6 },
  { name: 'Arcsine (DEG Mode)', input: 'arcsin(0.5)', expected: 30 },
  { name: 'Arccosine (DEG Mode)', input: 'arccos(0.5)', expected: 60 },
  { name: 'Arctangent (DEG Mode)', input: 'arctan(1)', expected: 45 },
  { name: 'Arcsine Boundary 1 (DEG)', input: 'arcsin(1)', expected: 90 },
  { name: 'Arccosine Boundary 0 (DEG)', input: 'arccos(0)', expected: 90 },
  { name: 'Arctangent 0 (DEG)', input: 'arctan(0)', expected: 0 },
  { name: 'Arctangent Negative (DEG)', input: 'arctan(-1)', expected: -45 },
  { name: 'Arccosecant (DEG Mode)', input: 'arccsc(2)', expected: 30 },
  { name: 'Arcsecant (DEG Mode)', input: 'arcsec(2)', expected: 60 },
  { name: 'Arccotangent (DEG Mode)', input: 'arccot(1)', expected: 45 },
  { name: 'asin alias (DEG)', input: 'asin(0.5)', expected: 30 },
  { name: 'acos alias (DEG)', input: 'acos(0.5)', expected: 60 },
  { name: 'atan alias (DEG)', input: 'atan(1)', expected: 45 },
  { name: 'sin^-1 (DEG)', input: 'sin^-1(0.5)', expected: 30 },
  { name: 'cos^-1 (DEG)', input: 'cos^-1(0.5)', expected: 60 },
  { name: 'tan^-1 (DEG)', input: 'tan^-1(1)', expected: 45 },
  { name: 'sin^(-1) (DEG)', input: 'sin^(-1)(0.5)', expected: 30 },
  { name: 'sin⁻¹ (DEG)', input: 'sin⁻¹(0.5)', expected: 30 },
  { name: 'csc^-1 (DEG)', input: 'csc^-1(2)', expected: 30 },
  { name: 'sec^-1 (DEG)', input: 'sec^-1(2)', expected: 60 },
  { name: 'cot^-1 (DEG)', input: 'cot^-1(1)', expected: 45 },
  { name: 'sin^-1 without parens', input: 'sin^-1 0.5', expected: 30 },
  { name: 'atan2 first quadrant (DEG)', input: 'atan2(1,1)', expected: 45 },
  { name: 'atan2 positive y-axis (DEG)', input: 'atan2(1,0)', expected: 90 },
  { name: 'atan2 negative x-axis (DEG)', input: 'atan2(0,-1)', expected: 180 },
  { name: 'atan2 first quadrant (RAD)', input: 'atan2(1,1)', angleMode: 'rad', expected: Math.PI / 4 },
  { name: 'Trig Cancellation', input: 'sin(arcsin(0.8))', expected: 0.8 },
  { name: 'Inverse Trig Cancellation (DEG)', input: 'arcsin(sin(30))', expected: 30 },
  { name: 'Arccosine Boundary -1 (DEG)', input: 'arccos(-1)', expected: 180 },
  { name: 'Arctangent High Value (DEG)', input: 'arctan(9999999)', expected: 90, eps: 1e-3 },
  ...extraInverseTrig(),
])

suite('Hyperbolic & Inverse Hyperbolic Functions', [
  { name: 'Hyperbolic Sine Zero', input: 'sinh(0)', expected: 0 },
  { name: 'Hyperbolic Sine 1', input: 'sinh(1)', expected: Math.sinh(1) },
  { name: 'Hyperbolic Cosine Zero', input: 'cosh(0)', expected: 1 },
  { name: 'Hyperbolic Cosine 1', input: 'cosh(1)', expected: Math.cosh(1) },
  { name: 'Hyperbolic Tangent Zero', input: 'tanh(0)', expected: 0 },
  { name: 'Hyperbolic Tangent 1', input: 'tanh(1)', expected: Math.tanh(1) },
  { name: 'Hyperbolic Identity Check', input: 'cosh(1.5)^2 - sinh(1.5)^2', expected: 1 },
  { name: 'Hyperbolic Cosecant', input: 'csch(1)', expected: 1 / Math.sinh(1) },
  { name: 'Hyperbolic Secant', input: 'sech(0)', expected: 1 },
  { name: 'Hyperbolic Cotangent', input: 'coth(1)', expected: 1 / Math.tanh(1) },
  { name: 'Inverse Hyperbolic Sine', input: 'arcsinh(0)', expected: 0 },
  { name: 'Inverse Hyperbolic Cosine', input: 'arccosh(1)', expected: 0 },
  { name: 'Inverse Hyperbolic Tangent', input: 'arctanh(0.5)', expected: Math.atanh(0.5) },
  { name: 'Inverse Hyperbolic Cosecant', input: 'arccsch(1)', expected: Math.asinh(1) },
  { name: 'Inverse Hyperbolic Secant', input: 'arcsech(0.5)', expected: Math.acosh(2) },
  ...extraHyperbolic(),
])

suite('Scientific Notation & Scale', [
  { name: 'Scientific Notation Entry', input: '1.23 * 10^6', expected: 1_230_000 },
  { name: 'Small Scientific Notation', input: '4.5 * 10^(-8)', expected: 4.5e-8 },
  { name: 'Scientific Notation Multiplication', input: '(3 * 10^4) * (2 * 10^5)', expected: 6e9 },
  { name: 'Scientific Notation Division', input: '(8 * 10^8) / (2 * 10^3)', expected: 400_000 },
  { name: 'Googol Scale Power', input: '10^100', expected: 1e100 },
  { name: 'Tiny Scale Power', input: '10^(-100)', expected: 1e-100 },
  { name: 'Large/Small Magnitude Cancellation', input: '(10^50) * (10^(-50))', expected: 1 },
  { name: 'Root of Scientific Notation', input: 'sqrt(4 * 10^12)', expected: 2_000_000 },
  { name: 'Auto Scientific Output Conversion', input: '0.0000000000123 * 2', display: /2\.46e-11/i },
  { name: 'Precision Edge Test', input: '1 + 10^(-15)', expected: 1 + 1e-15 },
  ...extraSciNotation(),
])

suite('Constants & Variables', [
  { name: 'Pi Constant Key', input: 'pi', expected: Math.PI },
  { name: 'Euler Constant Key', input: 'e', expected: Math.E },
  { name: 'Circle Circumference Formula', input: '2 * pi * 5', expected: 2 * Math.PI * 5 },
  { name: 'Circle Area Formula', input: 'pi * 3^2', expected: Math.PI * 9 },
  { name: 'Variable Assignment 1', input: ['a = 5'], expected: 5 },
  { name: 'Variable Assignment 2', input: ['a = 5', 'b = 10'], expected: 10 },
  { name: 'Variable Sum', input: ['a = 5', 'b = 10', 'a + b'], expected: 15 },
  { name: 'Variable Polynomial', input: ['a = 5', 'b = 10', 'a^2 + 2*a*b + b^2'], expected: 225 },
  { name: 'Variable Dependency', input: ['x = 4', 'y = x + 3'], expected: 7 },
  { name: 'Multi-Letter Variable Name', input: ['radius = 7'], expected: 7 },
  { name: 'Multi-Letter Variable Use', input: ['radius = 7', 'pi * radius^2'], expected: Math.PI * 49 },
  { name: 'Answer Key (ans)', input: ['5 + 5', 'ans * 2'], expected: 20 },
  { name: 'Chained ans Addition', input: ['10', 'ans + 5', 'ans * 3'], expected: 45 },
  { name: 'Variable in Trigonometry', input: ['theta = pi/4', 'sin(theta)'], angleMode: 'rad', expected: Math.SQRT2 / 2 },
  { name: 'Variable in List', input: ['k = 3', 'mean([1, 2, k, 4])'], expected: 2.5 },
  ...extraConstants(),
])

suite('Statistical Functions & Lists', [
  { name: 'Mean of Numbers', input: 'mean(1, 2, 3, 4, 5)', expected: 3 },
  { name: 'Mean of Bracketed List', input: 'mean([10, 20, 30])', expected: 20 },
  { name: 'Median Odd Count', input: 'median([10, 20, 30, 40, 50])', expected: 30 },
  { name: 'Median Even Count', input: 'median([10, 20, 30, 40])', expected: 25 },
  { name: 'Minimum Value', input: 'min([4, 2, 8, 1, 9])', expected: 1 },
  { name: 'Maximum Value', input: 'max([4, 2, 8, 1, 9])', expected: 9 },
  { name: 'List Total / Sum', input: 'total([1, 2, 3, 4, 5])', expected: 15 },
  { name: 'List Element Count', input: 'length([10, 20, 30, 40, 50])', expected: 5 },
  { name: 'Sample Standard Deviation', input: 'stdev([2, 4, 4, 4, 5, 5, 7, 9])', expected: Math.sqrt(32 / 7) },
  { name: 'Population Standard Deviation', input: 'stdevp([2, 4, 4, 4, 5, 5, 7, 9])', expected: 2 },
  { name: 'Sample Variance', input: 'var([2, 4, 4, 4, 5, 5, 7, 9])', expected: 32 / 7 },
  { name: 'Mean Absolute Deviation', input: 'mad([2, 4, 4, 4, 5, 5, 7, 9])', expected: 1.5 },
  { name: 'First Quartile', input: 'quartile([1, 2, 3, 4, 5, 6, 7], 1)', expected: 2 },
  { name: 'Third Quartile', input: 'quartile([1, 2, 3, 4, 5, 6, 7], 3)', expected: 6 },
  { name: 'Quantile Evaluation', input: 'quantile([1, 2, 3, 4, 5], 0.5)', expected: 3 },
  { name: 'Quantile Evaluation (90th percentile)', input: 'quantile([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 0.9)', expected: 9.1 },
  { name: 'Linear Correlation Coefficient', input: 'corr([1, 2, 3, 4], [2, 4, 6, 8])', expected: 1 },
  { name: 'Single Element Mean', input: 'mean([5])', expected: 5 },
  { name: 'Total with Negative Values', input: 'total([-5, -10, 15])', expected: 0 },
  { name: 'Sequence Range Generator', input: '[1...10]', display: '[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]' },
  ...extraStats(),
])

suite('Combinatorics, Probability & Randomization', [
  { name: 'Zero Factorial', input: '0!', expected: 1 },
  { name: 'Integer Factorial', input: '5!', expected: 120 },
  { name: 'Larger Factorial', input: '10!', expected: 3_628_800 },
  { name: 'Combinations Function (nCr)', input: 'nCr(5, 2)', expected: 10 },
  { name: 'Alternative Combinations Syntax', input: '5 nCr 2', expected: 10 },
  { name: 'Combinations Choose Zero', input: 'nCr(10, 0)', expected: 1 },
  { name: 'Combinations Choose All', input: 'nCr(10, 10)', expected: 1 },
  { name: 'Permutations Function (nPr)', input: 'nPr(5, 2)', expected: 20 },
  { name: 'Alternative Permutations Syntax', input: '5 nPr 2', expected: 20 },
  { name: 'Permutations Select Zero', input: 'nPr(10, 0)', expected: 1 },
  { name: 'Permutations Select All', input: 'nPr(5, 5)', expected: 120 },
  { name: 'Random Float (0 to 1)', input: 'random()', gte: 0, lt: 1 },
  { name: 'Random List Generator', input: 'random(5)', listLength: 5 },
  { name: 'Random Integer Range', input: 'randint(1, 6)', integer: true, gte: 1, lte: 6 },
  { name: 'Random Integer List', input: 'randint(1, 100, 5)', listLength: 5 },
  { name: 'Factorial Division', input: '10! / 8!', expected: 90 },
  { name: 'Poker Hand Combination', input: 'nCr(52, 5)', expected: 2_598_960 },
  ...extraCombinatorics(),
])

suite('Rounding & Number Theory Functions', [
  { name: 'Absolute Value (Positive)', input: 'abs(5)', expected: 5 },
  { name: 'Absolute Value (Negative)', input: 'abs(-12.5)', expected: 12.5 },
  { name: 'Round Up Nearest', input: 'round(3.6)', expected: 4 },
  { name: 'Round Down Nearest', input: 'round(3.4)', expected: 3 },
  { name: 'Round to Decimal Places', input: 'round(3.14159, 2)', expected: 3.14 },
  { name: 'Round to Decimal Places (4)', input: 'round(3.14159, 4)', expected: 3.1416 },
  { name: 'Floor Function (Positive)', input: 'floor(4.9)', expected: 4 },
  { name: 'Floor Function (Negative)', input: 'floor(-4.1)', expected: -5 },
  { name: 'Ceiling Function (Positive)', input: 'ceil(4.1)', expected: 5 },
  { name: 'Ceiling Function (Negative)', input: 'ceil(-4.9)', expected: -4 },
  { name: 'Sign Function (Positive)', input: 'sign(15)', expected: 1 },
  { name: 'Sign Function (Negative)', input: 'sign(-8)', expected: -1 },
  { name: 'Sign Function (Zero)', input: 'sign(0)', expected: 0 },
  { name: 'Greatest Common Divisor', input: 'gcd(12, 18)', expected: 6 },
  { name: 'Multi-Argument GCD', input: 'gcd(24, 36, 48)', expected: 12 },
  { name: 'Least Common Multiple', input: 'lcm(4, 6)', expected: 12 },
  { name: 'Multi-Argument LCM', input: 'lcm(3, 5, 7)', expected: 105 },
  { name: 'Modulo Function', input: 'mod(10, 3)', expected: 1 },
  { name: 'Modulo with Decimals', input: 'mod(5.5, 2)', expected: 1.5 },
  { name: 'Modulo Negative Dividend', input: 'mod(-10, 3)', expected: 2 },
  ...extraRounding(),
])

suite('Edge Cases, Domain Errors & Undefined Behavior', [
  { name: 'Division by Zero', input: '1 / 0', undefined: true },
  { name: 'Zero Divided by Zero', input: '0 / 0', undefined: true },
  { name: 'Square Root of Negative Number', input: 'sqrt(-4)', undefined: true },
  { name: 'Natural Log of Zero', input: 'ln(0)', undefined: true },
  { name: 'Natural Log of Negative Number', input: 'ln(-5)', undefined: true },
  { name: 'Base 10 Log of Zero', input: 'log(0)', undefined: true },
  { name: 'Arcsine Out of Domain (> 1)', input: 'arcsin(1.5)', undefined: true },
  { name: 'Arccosine Out of Domain (< -1)', input: 'arccos(-2)', undefined: true },
  { name: 'Tangent Asymptote (DEG Mode)', input: 'tan(90)', undefined: true },
  { name: 'Tangent Asymptote (RAD Mode)', input: 'tan(pi/2)', angleMode: 'rad', undefined: true },
  { name: 'Cosecant Zero Angle', input: 'csc(0)', undefined: true },
  { name: 'Secant Asymptote (DEG Mode)', input: 'sec(90)', undefined: true },
  { name: 'Cotangent Zero Angle', input: 'cot(0)', undefined: true },
  { name: 'Factorial of Negative Integer', input: '(-3)!', undefined: true },
  { name: 'Factorial Overflow Test', input: '200!', infinity: true },
  { name: 'Modulo by Zero', input: 'mod(5, 0)', undefined: true },
  { name: 'Inverse Hyperbolic Cosine Out of Domain', input: 'arccosh(0.5)', undefined: true },
  { name: 'Inverse Hyperbolic Tangent Boundary', input: 'arctanh(1)', undefined: true },
  ...extraEdges(),
])

suite('Radical & Square Root Simplifications', [
  { name: 'sqrt(12)', input: 'sqrt(12)', exact: '2sqrt(3)', expected: 3.464101615 },
  { name: 'sqrt(12) unclosed paren', input: 'sqrt(12', exact: '2sqrt(3)', expected: 3.464101615 },
  { name: 'sqrt(18)', input: 'sqrt(18)', exact: '3sqrt(2)', expected: 4.242640687 },
  { name: 'sqrt(20)', input: 'sqrt(20)', exact: '2sqrt(5)', expected: 4.472135955 },
  { name: 'sqrt(24)', input: 'sqrt(24)', exact: '2sqrt(6)', expected: 4.898979486 },
  { name: 'sqrt(27)', input: 'sqrt(27)', exact: '3sqrt(3)', expected: 5.196152423 },
  { name: 'sqrt(32)', input: 'sqrt(32)', exact: '4sqrt(2)', expected: 5.656854249 },
  { name: 'sqrt(45)', input: 'sqrt(45)', exact: '3sqrt(5)', expected: 6.708203932 },
  { name: 'sqrt(48)', input: 'sqrt(48)', exact: '4sqrt(3)', expected: 6.928203230 },
  { name: 'sqrt(50)', input: 'sqrt(50)', exact: '5sqrt(2)', expected: 7.071067812 },
  { name: 'sqrt(72)', input: 'sqrt(72)', exact: '6sqrt(2)', expected: 8.485281374 },
  { name: 'sqrt(75)', input: 'sqrt(75)', exact: '5sqrt(3)', expected: 8.660254038 },
  { name: 'sqrt(80)', input: 'sqrt(80)', exact: '4sqrt(5)', expected: 8.944271910 },
  { name: 'sqrt(96)', input: 'sqrt(96)', exact: '4sqrt(6)', expected: 9.797958971 },
  { name: 'sqrt(98)', input: 'sqrt(98)', exact: '7sqrt(2)', expected: 9.899494937 },
  { name: 'sqrt(108)', input: 'sqrt(108)', exact: '6sqrt(3)', expected: 10.392304845 },
  { name: 'sqrt(125)', input: 'sqrt(125)', exact: '5sqrt(5)', expected: 11.180339887 },
  { name: 'sqrt(128)', input: 'sqrt(128)', exact: '8sqrt(2)', expected: 11.313708499 },
  { name: 'sqrt(147)', input: 'sqrt(147)', exact: '7sqrt(3)', expected: 12.124355653 },
  { name: 'sqrt(162)', input: 'sqrt(162)', exact: '9sqrt(2)', expected: 12.727922061 },
  { name: 'sqrt(180)', input: 'sqrt(180)', exact: '6sqrt(5)', expected: 13.416407865 },
  { name: 'sqrt(200)', input: 'sqrt(200)', exact: '10sqrt(2)', expected: 14.142135624 },
  { name: 'sqrt(242)', input: 'sqrt(242)', exact: '11sqrt(2)', expected: 15.556349186 },
  { name: 'sqrt(288)', input: 'sqrt(288)', exact: '12sqrt(2)', expected: 16.970562748 },
  { name: 'sqrt(300)', input: 'sqrt(300)', exact: '10sqrt(3)', expected: 17.320508076 },
  { name: 'sqrt(320)', input: 'sqrt(320)', exact: '8sqrt(5)', expected: 17.888543820 },
  { name: 'sqrt(338)', input: 'sqrt(338)', exact: '13sqrt(2)', expected: 18.384776311 },
  { name: 'sqrt(363)', input: 'sqrt(363)', exact: '11sqrt(3)', expected: 19.052558883 },
  { name: 'sqrt(392)', input: 'sqrt(392)', exact: '14sqrt(2)', expected: 19.798989873 },
  { name: 'sqrt(432)', input: 'sqrt(432)', exact: '12sqrt(3)', expected: 20.784609691 },
  { name: 'sqrt(450)', input: 'sqrt(450)', exact: '15sqrt(2)', expected: 21.213203436 },
  { name: 'sqrt(500)', input: 'sqrt(500)', exact: '10sqrt(5)', expected: 22.360679775 },
  { name: 'sqrt(512)', input: 'sqrt(512)', exact: '16sqrt(2)', expected: 22.627416998 },
  { name: 'sqrt(588)', input: 'sqrt(588)', exact: '14sqrt(3)', expected: 24.248711306 },
  { name: 'sqrt(600)', input: 'sqrt(600)', exact: '10sqrt(6)', expected: 24.494897428 },
  { name: 'sqrt(675)', input: 'sqrt(675)', exact: '15sqrt(3)', expected: 25.980762114 },
  { name: 'sqrt(720)', input: 'sqrt(720)', exact: '12sqrt(5)', expected: 26.832815730 },
  { name: 'sqrt(800)', input: 'sqrt(800)', exact: '20sqrt(2)', expected: 28.284271247 },
  { name: 'sqrt(847)', input: 'sqrt(847)', exact: '11sqrt(7)', expected: 29.103264422 },
  { name: 'sqrt(968)', input: 'sqrt(968)', exact: '22sqrt(2)', expected: 31.112698372 },
  { name: 'sqrt(1008)', input: 'sqrt(1008)', exact: '12sqrt(7)', expected: 31.749015733 },
  { name: 'sqrt(1089)', input: 'sqrt(1089)', exact: '33', expected: 33 },
  { name: 'sqrt(1250)', input: 'sqrt(1250)', exact: '25sqrt(2)', expected: 35.355339059 },
  { name: 'sqrt(1331)', input: 'sqrt(1331)', exact: '11sqrt(11)', expected: 36.482872694 },
  { name: 'sqrt(1452)', input: 'sqrt(1452)', exact: '22sqrt(3)', expected: 38.105117767 },
  { name: 'sqrt(1568)', input: 'sqrt(1568)', exact: '28sqrt(2)', expected: 39.597979746 },
  { name: 'sqrt(1800)', input: 'sqrt(1800)', exact: '30sqrt(2)', expected: 42.426406871 },
  { name: 'sqrt(2000)', input: 'sqrt(2000)', exact: '20sqrt(5)', expected: 44.721359550 },
  { name: 'sqrt(2420)', input: 'sqrt(2420)', exact: '22sqrt(5)', expected: 49.193495505 },
  { name: 'sqrt(2500)', input: 'sqrt(2500)', exact: '50', expected: 50 },
  { name: 'sqrt(3200)', input: 'sqrt(3200)', exact: '40sqrt(2)', expected: 56.568542495 },
  { name: 'Inserted exact radical evaluates', input: '2sqrt(3)', exact: '2sqrt(3)', expected: 2 * Math.sqrt(3) },
  ...extraRadicals(),
])

suite('Trigonometric & Inverse Trigonometric Simplifications', [
  { name: 'sin(pi/6)', input: 'sin(pi/6)', angleMode: 'rad', exact: '1/2', expected: 0.5 },
  { name: 'cos(pi/6)', input: 'cos(pi/6)', angleMode: 'rad', exact: 'sqrt(3)/2', expected: 0.866025404 },
  { name: 'tan(pi/6)', input: 'tan(pi/6)', angleMode: 'rad', exact: 'sqrt(3)/3', expected: 0.577350269 },
  { name: 'sin(pi/4)', input: 'sin(pi/4)', angleMode: 'rad', exact: 'sqrt(2)/2', expected: 0.707106781 },
  { name: 'cos(pi/4)', input: 'cos(pi/4)', angleMode: 'rad', exact: 'sqrt(2)/2', expected: 0.707106781 },
  { name: 'tan(pi/4)', input: 'tan(pi/4)', angleMode: 'rad', exact: '1', expected: 1 },
  { name: 'sin(pi/3)', input: 'sin(pi/3)', angleMode: 'rad', exact: 'sqrt(3)/2', expected: 0.866025404 },
  { name: 'cos(pi/3)', input: 'cos(pi/3)', angleMode: 'rad', exact: '1/2', expected: 0.5 },
  { name: 'tan(pi/3)', input: 'tan(pi/3)', angleMode: 'rad', exact: 'sqrt(3)', expected: 1.732050808 },
  { name: 'sin(pi/2)', input: 'sin(pi/2)', angleMode: 'rad', exact: '1', expected: 1 },
  { name: 'cos(pi/2)', input: 'cos(pi/2)', angleMode: 'rad', exact: '0', expected: 0 },
  { name: 'sin(0)', input: 'sin(0)', angleMode: 'rad', exact: '0', expected: 0 },
  { name: 'cos(0)', input: 'cos(0)', angleMode: 'rad', exact: '1', expected: 1 },
  { name: 'tan(0)', input: 'tan(0)', angleMode: 'rad', exact: '0', expected: 0 },
  { name: 'sin(2*pi/3)', input: 'sin(2*pi/3)', angleMode: 'rad', exact: 'sqrt(3)/2', expected: 0.866025404 },
  { name: 'cos(2*pi/3)', input: 'cos(2*pi/3)', angleMode: 'rad', exact: '-1/2', expected: -0.5 },
  { name: 'tan(2*pi/3)', input: 'tan(2*pi/3)', angleMode: 'rad', exact: '-sqrt(3)', expected: -1.732050808 },
  { name: 'sin(3*pi/4)', input: 'sin(3*pi/4)', angleMode: 'rad', exact: 'sqrt(2)/2', expected: 0.707106781 },
  { name: 'cos(3*pi/4)', input: 'cos(3*pi/4)', angleMode: 'rad', exact: '-sqrt(2)/2', expected: -0.707106781 },
  { name: 'tan(3*pi/4)', input: 'tan(3*pi/4)', angleMode: 'rad', exact: '-1', expected: -1 },
  { name: 'sin(5*pi/6)', input: 'sin(5*pi/6)', angleMode: 'rad', exact: '1/2', expected: 0.5 },
  { name: 'cos(5*pi/6)', input: 'cos(5*pi/6)', angleMode: 'rad', exact: '-sqrt(3)/2', expected: -0.866025404 },
  { name: 'tan(5*pi/6)', input: 'tan(5*pi/6)', angleMode: 'rad', exact: '-sqrt(3)/3', expected: -0.577350269 },
  { name: 'sin(pi)', input: 'sin(pi)', angleMode: 'rad', exact: '0', expected: 0 },
  { name: 'cos(pi)', input: 'cos(pi)', angleMode: 'rad', exact: '-1', expected: -1 },
  { name: 'arcsin(0)', input: 'arcsin(0)', angleMode: 'rad', exact: '0', expected: 0 },
  { name: 'arcsin(1/2)', input: 'arcsin(1/2)', angleMode: 'rad', exact: 'pi/6', expected: 0.523598776 },
  { name: 'arcsin(sqrt(2)/2)', input: 'arcsin(sqrt(2)/2)', angleMode: 'rad', exact: 'pi/4', expected: 0.785398163 },
  { name: 'arcsin(sqrt(3)/2)', input: 'arcsin(sqrt(3)/2)', angleMode: 'rad', exact: 'pi/3', expected: 1.047197551 },
  { name: 'arcsin(1)', input: 'arcsin(1)', angleMode: 'rad', exact: 'pi/2', expected: 1.570796327 },
  { name: 'arccos(0)', input: 'arccos(0)', angleMode: 'rad', exact: 'pi/2', expected: 1.570796327 },
  { name: 'arccos(1/2)', input: 'arccos(1/2)', angleMode: 'rad', exact: 'pi/3', expected: 1.047197551 },
  { name: 'arccos(sqrt(2)/2)', input: 'arccos(sqrt(2)/2)', angleMode: 'rad', exact: 'pi/4', expected: 0.785398163 },
  { name: 'arccos(sqrt(3)/2)', input: 'arccos(sqrt(3)/2)', angleMode: 'rad', exact: 'pi/6', expected: 0.523598776 },
  { name: 'arccos(1)', input: 'arccos(1)', angleMode: 'rad', exact: '0', expected: 0 },
  { name: 'arctan(0)', input: 'arctan(0)', angleMode: 'rad', exact: '0', expected: 0 },
  { name: 'arctan(1/sqrt(3))', input: 'arctan(1/sqrt(3))', angleMode: 'rad', exact: 'pi/6', expected: 0.523598776 },
  { name: 'arctan(1)', input: 'arctan(1)', angleMode: 'rad', exact: 'pi/4', expected: 0.785398163 },
  { name: 'arctan(sqrt(3))', input: 'arctan(sqrt(3))', angleMode: 'rad', exact: 'pi/3', expected: 1.047197551 },
  { name: 'arccos(-1/2)', input: 'arccos(-1/2)', angleMode: 'rad', exact: '2*pi/3', expected: 2.094395102 },
  { name: 'arcsin(-1/2)', input: 'arcsin(-1/2)', angleMode: 'rad', exact: '-pi/6', expected: -0.523598776 },
  { name: 'arctan(-1)', input: 'arctan(-1)', angleMode: 'rad', exact: '-pi/4', expected: -0.785398163 },
  { name: 'arccos(-1)', input: 'arccos(-1)', angleMode: 'rad', exact: 'pi', expected: 3.141592654 },
  { name: 'arcsin(-1)', input: 'arcsin(-1)', angleMode: 'rad', exact: '-pi/2', expected: -1.570796327 },
  { name: 'arctan(-sqrt(3))', input: 'arctan(-sqrt(3))', angleMode: 'rad', exact: '-pi/3', expected: -1.047197551 },
  { name: 'sin(7*pi/6)', input: 'sin(7*pi/6)', angleMode: 'rad', exact: '-1/2', expected: -0.5 },
  { name: 'cos(7*pi/6)', input: 'cos(7*pi/6)', angleMode: 'rad', exact: '-sqrt(3)/2', expected: -0.866025404 },
  { name: 'tan(7*pi/6)', input: 'tan(7*pi/6)', angleMode: 'rad', exact: 'sqrt(3)/3', expected: 0.577350269 },
  { name: 'sin(pi/12)', input: 'sin(pi/12)', angleMode: 'rad', exact: '(sqrt(6)-sqrt(2))/4', expected: 0.258819045 },
  { name: 'cos(pi/12)', input: 'cos(pi/12)', angleMode: 'rad', exact: '(sqrt(6)+sqrt(2))/4', expected: 0.965925826 },
  ...extraTrigSimp(),
])

function n(text: string, angleMode: Angle = 'deg'): number {
  const r = evaluateLine(text, { angleMode })
  if (r.value == null || r.value.kind === 'text' || !Number.isFinite(r.value.n)) {
    throw new Error(`No numeric result for ${JSON.stringify(text)} → ${r.display}`)
  }
  return r.value.n
}

describe('Additional latex and constant aliases', () => {
  it('accepts spaced, dotted, and latex pi/e', () => {
    expect(Math.abs(n('p i') - Math.PI)).toBeLessThan(1e-8)
    expect(Math.abs(n('p*i') - Math.PI)).toBeLessThan(1e-8)
    expect(Math.abs(n('p · i') - Math.PI)).toBeLessThan(1e-8)
    expect(Math.abs(n('p dot i') - Math.PI)).toBeLessThan(1e-8)
    expect(Math.abs(n('\\pi') - Math.PI)).toBeLessThan(1e-8)
    expect(Math.abs(n('p\\cdot i') - Math.PI)).toBeLessThan(1e-8)
    expect(Math.abs(n('p\\imaginaryI') - Math.PI)).toBeLessThan(1e-8)
    expect(Math.abs(n('2pi') - 2 * Math.PI)).toBeLessThan(1e-8)
    expect(Math.abs(n('\\exponentialE') - Math.E)).toBeLessThan(1e-8)
  })

  it('evaluates typeset function latex', () => {
    expect(Math.abs(n('\\cos\\left(2\\right)') - Math.cos((2 * Math.PI) / 180))).toBeLessThan(1e-8)
    expect(Math.abs(n('\\cos2') - Math.cos((2 * Math.PI) / 180))).toBeLessThan(1e-8)
    expect(Math.abs(n('\\sin\\left(\\pi/6\\right)', 'rad') - 0.5)).toBeLessThan(1e-8)
    expect(Math.abs(n('\\ln\\left(\\exponentialE\\right)') - 1)).toBeLessThan(1e-8)
    expect(n('\\log\\left(100\\right)')).toBe(2)
    expect(n('\\log_{2}\\left(8\\right)')).toBe(3)
    expect(n('\\operatorname{nCr}\\left(5,2\\right)')).toBe(10)
    expect(n('\\operatorname{mean}\\left(1,2,3,4,5\\right)')).toBe(3)
    expect(evaluateLine('[1\\ldots10]').display).toBe('[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]')
    expect(Math.abs(n('\\sqrt{9/16}') - 0.75)).toBeLessThan(1e-8)
  })

  it('uses the active angle mode for a bare cosine argument', () => {
    expect(Math.abs(n('cos(2)', 'rad') - Math.cos(2))).toBeLessThan(1e-8)
    expect(Math.abs(n('cos(2)', 'deg') - Math.cos((2 * Math.PI) / 180))).toBeLessThan(1e-8)
  })

  it.each(extraLatex)('$name', (c) => {
    expect(Math.abs(n(c.input, c.angleMode ?? 'deg') - c.expected)).toBeLessThan(1e-7)
  })
})

describe('Exact form vs decimal copy', () => {
  it('keeps a decimal display so copy stays numeric', () => {
    const r = evaluateLine('sqrt(12)')
    expect(r.exact).toBe('2sqrt(3)')
    expect(r.display).toMatch(/^3\.464/)
    expect(r.display).not.toMatch(/sqrt|≈/)
    expect(dualLabel(r.exact, r.display)).toBe(`2sqrt(3) ≈ ${r.display}`)
  })

  it('gives sin(π/3) a clickable exact and decimal pair', () => {
    const r = evaluateLine('sin(pi/3)', { angleMode: 'rad' })
    expect(r.exact).toBe('sqrt(3)/2')
    expect(r.display).not.toMatch(/sqrt|≈/)
    expect(Number(r.display)).toBeCloseTo(Math.sqrt(3) / 2, 8)
    expect(hasDualAnswer(r)).toBe(true)
    expect(dualLabel(r.exact, r.display)).toBe(`sqrt(3)/2 ≈ ${r.display}`)
  })

  it('inserts the exact radical when that form is chosen', () => {
    expect(insertableAnswer('2sqrt(3)')).toBe('2sqrt(3)')
    expect(insertableAnswer('(sqrt(6)-sqrt(2))/4')).toBe('(sqrt(6)-sqrt(2))/4')
  })

  it('inserts the decimal when given the numeric result', () => {
    const r = evaluateLine('sqrt(12)')
    const inserted = insertableAnswer(r.display, r.value?.n)
    expect(inserted).not.toMatch(/sqrt/)
    expect(inserted).toBe(r.display)
    expect(Number(inserted)).toBeCloseTo(Math.sqrt(12), 8)
    expect(insertableAnswer(r.display, r.value?.n, 4)).toBe(formatNumber(Math.sqrt(12), 4))
  })

  it.each(extraExactCopy)('$name', (c) => {
    const r = evaluateLine(c.input)
    expect(r.exact, c.input).toBe(c.exact)
    expect(r.display, c.input).not.toMatch(/≈/)
    if (r.exact && r.exact !== r.display) {
      expect(dualLabel(r.exact, r.display)).toBe(`${r.exact} ≈ ${r.display}`)
    }
    expect(insertableAnswer(r.exact ?? r.display)).toBe(r.exact ?? r.display)
  })
})
