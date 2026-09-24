import { describe, expect, it } from 'vitest'
import { expectBlankOr, expectNum, expectUndefined, shown } from './audit.helpers'

const rad = { angleMode: 'rad' as const }
const deg = { angleMode: 'deg' as const }

describe('audit: trig in degrees (the default)', () => {
  it.each([
    ['sin(30)', 0.5],
    ['cos(60)', 0.5],
    ['tan(45)', 1],
    ['sin(90)', 1],
    ['cos(180)', -1],
    ['tan(135)', -1],
    ['sin(-30)', -0.5],
    ['sin(390)', 0.5],
    ['sin(-90)', -1],
    ['cos(-180)', -1],
    ['cos(360)', 1],
    ['tan(-45)', -1],
    ['tan(225)', 1],
    ['sin(45)', Math.SQRT1_2],
    ['cos(30)', Math.sqrt(3) / 2],
    ['tan(60)', Math.sqrt(3)],
    ['sin 30', 0.5],
    ['sin 30 + 1', 1.5],
    ['sin(30)^2 + cos(30)^2', 1],
    ['2sin(30)', 1],
    ['sec(60)', 2],
    ['csc(30)', 2],
    ['cot(45)', 1],
    ['sec(0)', 1],
    ['csc(90)', 1],
  ])('%s = %d', (text, want) => {
    expectNum(text, want, deg)
  })

  it.each(['cos(90)', 'sin(180)', 'sin(720)', 'cos(270)', 'tan(180)', 'sin(-180)'])('%s shows exactly 0', (text) => {
    expect(shown(text, deg)).toBe('0')
  })

  it.each(['tan(90)', 'tan(270)', 'tan(-90)', 'sec(90)', 'csc(0)', 'cot(0)', 'csc(180)'])('%s is undefined', (text) => {
    expectUndefined(text, deg)
  })

  // cot divides cos by sin without the chop the other functions get, so cot(90) = 6.12e-17
  it('cot(90) shows exactly 0', () => {
    expect(shown('cot(90)', deg)).toBe('0')
  })
  it('cot(270) shows exactly 0', () => {
    expect(shown('cot(270)', deg)).toBe('0')
  })

  // a degree sign typed straight into a trig call; blank would be acceptable, a different number is not
  it('sin(30°) is 0.5 or blank', () => {
    expectBlankOr('sin(30°)', ['0.5'], deg)
    expectBlankOr('cos(60°)', ['0.5'], deg)
  })

  it('sin(1e-11) degrees is 1.745e-13, not 0', () => {
    expectNum('sin(1e-11)', (1e-11 * Math.PI) / 180, deg, 1e-9)
    expect(shown('sin(1e-11)', deg)).not.toBe('0')
  })

  it('sin^2(30) is 0.25 or blank', () => {
    expectBlankOr('sin^2(30)', ['0.25'], deg)
  })
})

describe('audit: trig in radians', () => {
  it.each([
    ['sin(pi/6)', 0.5],
    ['cos(pi)', -1],
    ['tan(pi/4)', 1],
    ['sin(1)', 0.8414709848078965],
    ['cos(1)', 0.5403023058681398],
    ['tan(1)', 1.5574077246549023],
    ['sin(π/2)', 1],
    ['cos 2pi', 1],
    ['sin(-pi/2)', -1],
    ['sec(pi/3)', 2],
    ['csc(pi/6)', 2],
    ['cot(pi/4)', 1],
  ])('%s = %d', (text, want) => {
    expectNum(text, want, rad)
  })

  it.each(['sin(pi)', 'cos(pi/2)', 'sin(2pi)', 'tan(pi)'])('%s shows exactly 0', (text) => {
    expect(shown(text, rad)).toBe('0')
  })

  it.each(['tan(pi/2)', 'tan(3pi/2)', 'csc(pi)', 'cot(0)'])('%s is undefined', (text) => {
    expectUndefined(text, rad)
  })

  it('cot(pi/2) shows exactly 0', () => {
    expect(shown('cot(pi/2)', rad)).toBe('0')
  })

  // sin and tan chop any |result| < 1e-12 to 0, which is meant for sin(pi) but also wipes out
  // tiny angles: sin(1e-13) is 1e-13, not 0
  it('sin(1e-13) = 1e-13 in radians', () => {
    expectNum('sin(1e-13)', 1e-13, rad, 1e-9)
    expect(shown('sin(1e-13)', rad)).not.toBe('0')
  })
  it('sin(1e-13)/1e-13 = 1 in radians', () => {
    expectNum('sin(1e-13)/1e-13', 1, rad)
  })
  it('tan(1e-13)/1e-13 = 1 in radians', () => {
    expectNum('tan(1e-13)/1e-13', 1, rad)
  })

  // "30 deg" inside a call in radian mode; 'undefined' would be a wrong answer
  it('sin(30 deg) in radian mode is 0.5 or blank, not "undefined"', () => {
    expectBlankOr('sin(30 deg)', ['0.5'], rad)
  })
})

describe('audit: inverse trig', () => {
  it.each([
    ['asin(0.5)', 30],
    ['acos(0.5)', 60],
    ['atan(1)', 45],
    ['acos(0)', 90],
    ['atan(-1)', -45],
    ['atan(sqrt(3))', 60],
    ['acos(1)', 0],
    ['asin(-1)', -90],
    ['arcsin(1)', 90],
    ['arccos(-1)', 180],
    ['arctan(1)', 45],
    ['sin^-1(0.5)', 30],
    ['sin^(-1)(0.5)', 30],
    ['sin⁻¹(0.5)', 30],
    ['cos^-1(0.5)', 60],
    ['tan^-1(1)', 45],
    ['atan2(1, 1)', 45],
    ['atan2(1, -1)', 135],
    ['atan2(-1, -1)', -135],
    ['atan2(0, -1)', 180],
    ['arccot(0)', 90],
    ['arccot(1)', 45],
    ['arcsec(2)', 60],
    ['arccsc(2)', 30],
    ['asin(sin(40))', 40],
  ])('%s = %d degrees', (text, want) => {
    expectNum(text, want, deg)
  })

  it.each([
    ['asin(0.5)', Math.PI / 6],
    ['atan(1)', Math.PI / 4],
    ['acos(0)', Math.PI / 2],
    ['acos(-1)', Math.PI],
    ['asin(1)', Math.PI / 2],
    ['atan2(1, 0)', Math.PI / 2],
  ])('%s = %d radians', (text, want) => {
    expectNum(text, want, rad)
  })

  it('the short names acot, asec, acsc give the right angle or blank', () => {
    expectBlankOr('acot(1)', ['45'], deg)
    expectBlankOr('asec(2)', ['60'], deg)
    expectBlankOr('acsc(2)', ['30'], deg)
  })

  it.each(['asin(1.01)', 'acos(-2)', 'arcsec(0.5)', 'arccsc(0.5)'])('%s is undefined', (text) => {
    expectUndefined(text, deg)
  })
})

describe('audit: hyperbolic (never in degrees)', () => {
  it.each([
    ['sinh(0)', 0],
    ['cosh(0)', 1],
    ['tanh(1)', Math.tanh(1)],
    ['sinh(1)', (Math.E - 1 / Math.E) / 2],
    ['cosh(1)', (Math.E + 1 / Math.E) / 2],
    ['asinh(1)', Math.log(1 + Math.SQRT2)],
    ['acosh(1)', 0],
    ['acosh(2)', Math.log(2 + Math.sqrt(3))],
    ['atanh(0.5)', 0.5 * Math.log(3)],
    ['sech(0)', 1],
  ])('%s = %d', (text, want) => {
    expectNum(text, want, deg)
    expectNum(text, want, rad)
  })

  it.each(['acosh(0.5)', 'atanh(1)', 'coth(0)', 'csch(0)'])('%s is undefined', (text) => {
    expectUndefined(text)
  })
})

describe('audit: logs and exponentials', () => {
  it.each([
    ['log(100)', 2],
    ['log(1000)', 3],
    ['log(1e-5)', -5],
    ['log(10^2)', 2],
    ['log(10 + 90)', 2],
    ['log((100))', 2],
    ['log(100) + log(10)', 3],
    ['ln(e)', 1],
    ['ln(1)', 0],
    ['ln(e^3)', 3],
    ['ln(e^2)', 2],
    ['log2(8)', 3],
    ['log2(1024)', 10],
    ['log10(0.001)', -3],
    ['log(8, 2)', 3],
    ['log_2(8)', 3],
    ['log_2(32)', 5],
    ['log_3(81)', 4],
    ['log_10(1000)', 3],
    ['log(10, 10)', 1],
    ['10^log(3)', 3],
    ['exp(0)', 1],
    ['exp(1)', Math.E],
    ['exp(ln(5))', 5],
    ['e^2', Math.E ** 2],
    ['e^(ln 2)', 2],
    ['e', Math.E],
    ['2e', 2 * Math.E],
  ])('%s = %d', (text, want) => {
    expectNum(text, want)
  })

  // the bare-call wrap runs after the log( -> log10( rewrite, so 'log 1000' becomes mathjs's
  // natural log: ln(1000) = 6.908
  it('log 1000 = 3', () => {
    expectNum('log 1000', 3)
  })
  it('log 100 = 2', () => {
    expectNum('log 100', 2)
  })

  it.each(['log(0)', 'ln(0)', 'ln(-1)', 'log(-10)', 'log2(0)'])('%s is undefined', (text) => {
    expectUndefined(text)
  })

  // the log( -> log10( rewrite stops at the first comma or ')', so a call inside log()
  // leaves mathjs's natural log: log(max(10,100)) = ln(100) = 4.605
  it('log(max(10, 100)) = 2', () => {
    expectNum('log(max(10, 100))', 2)
  })
  it('log(mean(10, 1000)) = log10(505)', () => {
    expectNum('log(mean(10, 1000))', Math.log10(505))
  })
  it('log(gcd(100, 1000)) = 2', () => {
    expectNum('log(gcd(100, 1000))', 2)
  })
})

describe('audit: roots', () => {
  it.each([
    ['sqrt(16)', 4],
    ['sqrt(2)', Math.SQRT2],
    ['√16', 4],
    ['√(16)', 4],
    ['√16+9', 13],
    ['√(16+9)', 5],
    ['cbrt(27)', 3],
    ['cbrt(-27)', -3],
    ['∛27', 3],
    ['∛(-8)', -2],
    ['nthroot(16, 4)', 2],
    ['nthRoot(-32, 5)', -2],
    ['16^(1/4)', 2],
    ['√2*√2', 2],
    ['sqrt(2)^2', 2],
    ['sqrt(0)', 0],
    ['sqrt(0.25)', 0.5],
    ['sqrt(1e10)', 1e5],
    ['hypot(3, 4)', 5],
  ])('%s = %d', (text, want) => {
    expectNum(text, want)
  })

  it('an even root of a negative is undefined', () => {
    expectUndefined('sqrt(-1)')
    expectUndefined('√(-9)')
  })
})

describe('audit: statistics', () => {
  const data = '2,4,4,4,5,5,7,9'
  it.each([
    ['mean(1,2,3,4)', 2.5],
    ['mean(2, 4, 9)', 5],
    ['mean([1,2,3])', 2],
    ['mean([1...5])', 3],
    ['median(3,1,2)', 2],
    ['median(4,1,3,2)', 2.5],
    ['median(1,2,3,4,5,6)', 3.5],
    ['median(5)', 5],
    ['median(-5, 10, 0)', 0],
    [`stdev(${data})`, Math.sqrt(32 / 7)],
    [`std(${data})`, Math.sqrt(32 / 7)],
    [`stdevp(${data})`, 2],
    [`var(${data})`, 32 / 7],
    [`varp(${data})`, 4],
    ['var(1,2,3,4)', 5 / 3],
    ['varp(1,2,3,4)', 1.25],
    ['stdev(1,1,1)', 0],
    ['sum(1,2,3,4)', 10],
    ['sum([1...100])', 5050],
    ['total(1,2,3)', 6],
    ['min(3,-1,2)', -1],
    ['max(3,-1,2)', 3],
    ['count(4,5,6)', 3],
    ['n(4,5,6)', 3],
    ['corr(1,2,3, 2,4,6)', 1],
    ['corr(1,2,3, 6,4,2)', -1],
    ['quantile(1,2,3,4,5, 0.5)', 3],
    ['mean(1.5, 2.5)', 2],
  ])('%s = %d', (text, want) => {
    expectNum(text, want)
  })

  it('the sample stdev of one value is undefined', () => {
    expectUndefined('stdev(5)')
  })
})

describe('audit: gcd, gcf, hcf, lcm', () => {
  it.each([
    ['gcd(12, 18)', 6],
    ['gcf(12, 18)', 6],
    ['GCF(12, 18)', 6],
    ['hcf(8, 12)', 4],
    ['lcm(4, 6)', 12],
    ['LCM(4, 6)', 12],
    ['gcd(0, 5)', 5],
    ['lcm(0, 5)', 0],
    ['gcd(-12, 18)', 6],
    ['lcm(-4, 6)', 12],
    ['gcd(12, 18, 24)', 6],
    ['lcm(2, 3, 4)', 12],
    ['gcd(17, 5)', 1],
    ['lcm(21, 6)', 42],
    ['gcd(1/2, 1/3)', 1 / 6],
    ['lcm(1/2, 1/3)', 1],
    ['gcd(0.5, 0.75)', 0.25],
    ['gcd(2.5, 5)', 2.5],
    ['lcm(2/3, 3/4)', 6],
    ['gcd(2/3, 3/4)', 1 / 12],
    ['lcm(1e10, 3e10)', 3e10],
    ['gcd(1071, 462)', 21],
  ])('%s = %d', (text, want) => {
    expectNum(text, want)
  })

  it('upper-case GCD is 6 or blank', () => {
    expectBlankOr('GCD(12, 18)', ['6'])
  })

  it('gcd of an irrational is undefined', () => {
    expectUndefined('gcd(pi, 2)')
    expectUndefined('lcm(sqrt(2), 2)')
  })

  // gcd(0, 0) is 0 by convention (every integer divides 0); intGcd's "1 when both are 0" leaks out
  it('gcd(0, 0) is 0, not 1', () => {
    expectBlankOr('gcd(0, 0)', ['0', 'undefined'])
  })
})

describe('audit: combinations and permutations', () => {
  it.each([
    ['5 nCr 2', 10],
    ['nCr(5, 2)', 10],
    ['nPr(5, 2)', 20],
    ['5 nPr 2', 20],
    ['nCr(5, 6)', 0],
    ['nCr(0, 0)', 1],
    ['nCr(52, 5)', 2598960],
    ['nPr(10, 0)', 1],
    ['nPr(5, 5)', 120],
    ['combinations(10, 3)', 120],
    ['permutations(10, 3)', 720],
    ['nCr(10, 10)', 1],
    ['nCr(60, 30)', 118264581564861424],
    ['(4+1) nCr 2', 10],
  ])('%s = %d', (text, want) => {
    expectNum(text, want)
  })

  it.each(['nCr(5, -1)', 'nCr(5.5, 2)', 'nPr(-3, 2)', 'nCr(5, 2.5)'])('%s is undefined', (text) => {
    expectUndefined(text)
  })
})

describe('audit: rounding and friends', () => {
  it.each([
    ['round(2.5)', 3],
    ['round(2.4)', 2],
    ['round(3.14159, 2)', 3.14],
    ['round(123.456, 1)', 123.5],
    ['round(1234.5678, -2)', 1200],
    ['floor(-2.5)', -3],
    ['ceil(-2.5)', -2],
    ['floor(2.7)', 2],
    ['ceil(2.1)', 3],
    ['floor(5)', 5],
    ['sign(-3)', -1],
    ['sign(0)', 0],
    ['sign(4)', 1],
    ['abs(-3)', 3],
    ['clamp(15, 0, 10)', 10],
    ['clamp(-5, 0, 10)', 0],
    ['clamp(5, 0, 10)', 5],
  ])('%s = %d', (text, want) => {
    expectNum(text, want)
  })

  // the product is a hair off in binary before floor/ceil sees it: 0.29*100 = 28.999999999999996
  it('floor(0.29*100) = 29', () => {
    expectNum('floor(0.29*100)', 29)
  })
  it('ceil(1.1*100) = 110', () => {
    expectNum('ceil(1.1*100)', 110)
  })

  // Math.round rounds half up, so -2.5 goes to -2 while 2.5 goes to 3: round(-x) != -round(x)
  it('round(-2.5) = -3, the mirror of round(2.5) = 3', () => {
    expectNum('round(-2.5)', -3)
  })

  // x * 10^d is inexact: 1.005 * 100 = 100.49999999999999, so the typed 1.005 rounds down
  it('round(1.005, 2) = 1.01', () => {
    expectNum('round(1.005, 2)', 1.01)
  })
  it('round(9.995, 2) = 10', () => {
    expectNum('round(9.995, 2)', 10)
  })
  it('round(1.255, 2) = 1.26', () => {
    expectNum('round(1.255, 2)', 1.26)
  })
})
