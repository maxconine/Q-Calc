/** Extra scientific testbench cases. Target is max(5× current, 100) per category. */

export type ExtraCase = {
  name: string
  input: string | string[]
  at?: number
  expected?: number
  eps?: number
  display?: string | RegExp
  undefined?: true
  infinity?: true
  fractionMode?: boolean
  angleMode?: 'deg' | 'rad'
  gte?: number
  lt?: number
  lte?: number
  integer?: boolean
  listLength?: number
  matches?: string
  exact?: string
}

function num(name: string, input: string | string[], expected: number, extra: Partial<ExtraCase> = {}): ExtraCase {
  return { name, input, expected, ...extra }
}

export function extraArithmetic(): ExtraCase[] {
  const out: ExtraCase[] = []
  for (let i = 1; i <= 25; i++) {
    out.push(num(`Sum ${i}+${i + 11}`, `${i} + ${i + 11}`, 2 * i + 11))
    out.push(num(`Difference ${80 - i}-${i}`, `${80 - i} - ${i}`, 80 - 2 * i))
    out.push(num(`Product ${i}×${i + 3}`, `${i} * ${i + 3}`, i * (i + 3)))
  }
  for (let i = 1; i <= 12; i++) {
    out.push(num(`Quotient ${i * 24}/${i}`, `${i * 24} / ${i}`, 24))
    out.push(num(`Precedence ${i}+${i}*${i}`, `${i} + ${i} * ${i}`, i + i * i))
    out.push(num(`Parens (${i}+${i})*${i}`, `(${i} + ${i}) * ${i}`, (i + i) * i))
    out.push(num(`Implicit ${i}(${i}+1)`, `${i}(${i} + 1)`, i * (i + 1)))
  }
  out.push(
    num('Decimal chain 0.25+0.5+0.25', '0.25 + 0.5 + 0.25', 1),
    num('Unary minus chain -(-(-8))', '-(-(-8))', -8),
    num('Left-to-right 81/9/3', '81 / 9 / 3', 3),
    num('Nested 1+(2+(3+(4+5)))', '1 + (2 + (3 + (4 + 5)))', 15),
    num('Juxtapose (7-2)(6-1)', '(7 - 2)(6 - 1)', 25),
    num('Mixed 8+2*3-4/2', '8 + 2 * 3 - 4 / 2', 12),
    num('Zero plus 0+17', '0 + 17', 17),
    num('Zero minus 17-0', '17 - 0', 17),
    num('Neg times neg (-8)*(-7)', '(-8) * (-7)', 56),
    num('Distribute 4*(10-3+1)', '4 * (10 - 3 + 1)', 32),
  )
  return out
}

export function extraFractions(): ExtraCase[] {
  const out: ExtraCase[] = []
  const pairs: Array<[number, number]> = [
    [1, 2],
    [1, 3],
    [1, 4],
    [1, 5],
    [1, 6],
    [1, 8],
    [2, 3],
    [2, 5],
    [3, 4],
    [3, 5],
    [3, 8],
    [4, 5],
    [5, 6],
    [5, 8],
    [7, 8],
    [5, 12],
    [7, 12],
    [11, 12],
    [3, 16],
    [9, 16],
    [13, 16],
    [15, 16],
    [2, 7],
    [3, 7],
    [4, 7],
    [5, 7],
    [6, 7],
    [1, 9],
    [2, 9],
    [4, 9],
    [5, 9],
    [8, 9],
    [1, 10],
    [3, 10],
    [7, 10],
    [9, 10],
  ]
  for (const [a, b] of pairs) {
    out.push(num(`${a}/${b}`, `${a}/${b}`, a / b))
    out.push({
      name: `${a}/${b} fraction mode`,
      input: `${a}/${b}`,
      fractionMode: true,
      display: `${a}/${b}`,
    })
  }
  out.push(
    num('1/2+1/5', '1/2 + 1/5', 0.7),
    num('2/3-1/6', '2/3 - 1/6', 0.5),
    num('(5/8)*(4/5)', '(5/8) * (4/5)', 0.5),
    num('(7/8)/(7/16)', '(7/8) / (7/16)', 2),
    num('3+2/5', '3 + 2/5', 3.4),
    num('(-3)/4', '(-3)/4', -0.75),
    num('(3/5)^2', '(3/5)^2', 0.36),
    num('sqrt(4/9)', 'sqrt(4/9)', 2 / 3),
    num('50%', '50%', 0.5),
    num('12.5%', '12.5%', 0.125),
    num('10% * 50', '10% * 50', 5),
    { name: '30% of 90', input: '30 % of 90', display: '27' },
    num('what is 10% of 250', 'what is 10% of 250', 25),
    num('What is 5% of 40?', 'What is 5% of 40?', 2),
    { name: '0.5 as 1/2', input: '0.5', fractionMode: true, display: '1/2' },
    { name: '0.25 as 1/4', input: '0.25', fractionMode: true, display: '1/4' },
    { name: '0.2 as 1/5', input: '0.2', fractionMode: true, display: '1/5' },
    { name: '0.125 as 1/8', input: '0.125', fractionMode: true, display: '1/8' },
    { name: '0.375 as 3/8', input: '0.375', fractionMode: true, display: '3/8' },
    { name: '0.875 as 7/8', input: '0.875', fractionMode: true, display: '7/8' },
    { name: '2032mm to ft fraction', input: '2032mm to ft', fractionMode: true, display: '20/3 ft' },
    { name: '2032 mm to feet fraction', input: '2032 mm to feet', fractionMode: true, display: '20/3 ft' },
    { name: '80 in to ft fraction', input: '80 in to ft', fractionMode: true, display: '20/3 ft' },
    { name: '3 ft / 2 fraction', input: '3 ft / 2', fractionMode: true, display: '3/2 ft' },
    { name: '(1/2) m + (1/3) m fraction', input: '(1/2) m + (1/3) m', fractionMode: true, display: '5/6 m' },
    { name: '2 in * 1/2 fraction', input: '2 in * 1/2', fractionMode: true, display: '1 in' },
    { name: '90 deg exact with unit', input: '90 deg', exact: 'pi/2 rad' },
  )
  return out
}

export function extraPowers(): ExtraCase[] {
  const out: ExtraCase[] = []
  for (let i = 0; i <= 12; i++) out.push(num(`2^${i}`, `2^${i}`, 2 ** i))
  for (let i = 2; i <= 10; i++) out.push(num(`${i}^3`, `${i}^3`, i ** 3))
  for (const n of [4, 9, 25, 36, 49, 64, 81, 100, 121, 169, 196, 225]) {
    out.push(num(`sqrt(${n})`, `sqrt(${n})`, Math.sqrt(n)))
  }
  out.push(
    num('27^(1/3)', '27^(1/3)', 3),
    num('64^(1/3)', '64^(1/3)', 4),
    num('125^(1/3)', '125^(1/3)', 5),
    num('81^(1/4)', '81^(1/4)', 3),
    num('32^(1/5)', '32^(1/5)', 2),
    num('nthroot(32, 5)', 'nthroot(32, 5)', 2),
    num('nthroot(64, 6)', 'nthroot(64, 6)', 2),
    num('nthroot(1000000, 6)', 'nthroot(1000000, 6)', 10),
    num('sqrt(sqrt(16))', 'sqrt(sqrt(16))', 2),
    num('sqrt(sqrt(256))', 'sqrt(sqrt(256))', 4),
    num('sqrt(5^2+12^2)', 'sqrt(5^2 + 12^2)', 13),
    num('sqrt(8^2+15^2)', 'sqrt(8^2 + 15^2)', 17),
    num('(3^2)^3', '(3^2)^3', 729),
    num('3^(2^3)', '3^(2^3)', 6561),
    num('(-2)^4', '(-2)^4', 16),
    num('(-2)^5', '(-2)^5', -32),
    num('(-4)^3', '(-4)^3', -64),
    num('7^0', '7^0', 1),
    num('(-9)^0', '(-9)^0', 1),
    num('4^(-1)', '4^(-1)', 0.25),
    num('4^(-2)', '4^(-2)', 0.0625),
    num('8^(-2/3)', '8^(-2/3)', 0.25),
    num('16^(-3/4)', '16^(-3/4)', 0.125),
    num('10^6', '10^6', 1e6),
    num('10^5', '10^5', 1e5),
    num('e^0', 'e^0', 1),
    num('e^1', 'e^1', Math.E),
    num('e^(-1)', 'e^(-1)', 1 / Math.E),
    num('cbrt(8)', 'cbrt(8)', 2),
    num('cbrt(64)', 'cbrt(64)', 4),
    num('cbrt(-8)', 'cbrt(-8)', -2),
    num('cbrt(-27)', 'cbrt(-27)', -3),
    num('sqrt(0)', 'sqrt(0)', 0),
    num('0^5', '0^5', 0),
    num('1^100', '1^100', 1),
    num('9^2', '9^2', 81),
    num('11^2', '11^2', 121),
    num('13^2', '13^2', 169),
    num('15^2', '15^2', 225),
    num('20^2', '20^2', 400),
    num('6^4', '6^4', 1296),
    num('7^4', '7^4', 2401),
    num('9^(3/2)', '9^(3/2)', 27),
    num('4^(5/2)', '4^(5/2)', 32),
    num('81^(3/4)', '81^(3/4)', 27),
    num('nthroot(243, 5)', 'nthroot(243, 5)', 3),
    num('nthroot(1024, 10)', 'nthroot(1024, 10)', 2),
    num('sqrt(6^2+8^2)', 'sqrt(6^2 + 8^2)', 10),
    num('sqrt(9^2+12^2)', 'sqrt(9^2 + 12^2)', 15),
    num('sqrt(7^2+24^2)', 'sqrt(7^2 + 24^2)', 25),
    num('(-5)^2', '(-5)^2', 25),
    num('(-5)^3', '(-5)^3', -125),
    num('16^(-1/2)', '16^(-1/2)', 0.25),
    num('81^(-1/4)', '81^(-1/4)', 1 / 3),
    num('cbrt(1000)', 'cbrt(1000)', 10),
    num('cbrt(-64)', 'cbrt(-64)', -4),
    num('e^3', 'e^3', Math.E ** 3),
    num('10^0', '10^0', 1),
  )
  return out
}

export function extraLogs(): ExtraCase[] {
  const out: ExtraCase[] = []
  for (let i = 0; i <= 6; i++) {
    out.push(num(`log(10^${i})`, `log(${10 ** i})`, i))
    out.push(num(`ln(e^${i})`, `ln(e^${i})`, i))
  }
  for (const n of [2, 3, 4, 5, 6, 7, 8, 9, 10, 16, 32, 64, 81, 100, 243, 256, 512, 1024]) {
    out.push(num(`ln(${n})`, `ln(${n})`, Math.log(n)))
    out.push(num(`log(${n})`, `log(${n})`, Math.log10(n)))
  }
  out.push(
    num('log_2(16)', 'log_2(16)', 4),
    num('log_2(32)', 'log_2(32)', 5),
    num('log_2(64)', 'log_2(64)', 6),
    num('log_4(64)', 'log_4(64)', 3),
    num('log_9(81)', 'log_9(81)', 2),
    num('log_10(0.01)', 'log(0.01)', -2),
    num('log_10(0.1)', 'log(0.1)', -1),
    num('ln(e^pi)', 'ln(e^pi)', Math.PI, { angleMode: 'rad' }),
    { name: 'ln product 6*7', input: 'ln(6 * 7)', matches: 'ln(6) + ln(7)' },
    { name: 'ln product 2*9', input: 'ln(2 * 9)', matches: 'ln(2) + ln(9)' },
    { name: 'ln quotient 50/2', input: 'ln(50 / 2)', matches: 'ln(50) - ln(2)' },
    { name: 'ln quotient 81/3', input: 'ln(81 / 3)', matches: 'ln(81) - ln(3)' },
    { name: 'log product 4*25', input: 'log(4 * 25)', matches: 'log(4) + log(25)' },
    num('e^ln(12)', 'e^ln(12)', 12),
    num('10^log(77)', '10^log(77)', 77),
    num('log2(8)', 'log2(8)', 3),
    num('log2(1)', 'log2(1)', 0),
    num('log10(1000)', 'log10(1000)', 3),
    num('exp(0)', 'exp(0)', 1),
    num('exp(1)', 'exp(1)', Math.E),
    num('exp(ln(5))', 'exp(ln(5))', 5),
    num('log_2(2)', 'log_2(2)', 1),
    num('log_2(4)', 'log_2(4)', 2),
    num('log_2(256)', 'log_2(256)', 8),
    num('log_4(16)', 'log_4(16)', 2),
    num('log_5(25)', 'log_5(25)', 2),
    num('log_5(125)', 'log_5(125)', 3),
    num('log_6(36)', 'log_6(36)', 2),
    num('log_7(49)', 'log_7(49)', 2),
    num('log_8(512)', 'log_8(512)', 3),
    num('ln(e^7)', 'ln(e^7)', 7),
    num('log(10^7)', 'log(1e7)', 7),
    num('log2(1024)', 'log2(1024)', 10),
    num('log10(1e6)', 'log10(1e6)', 6),
    num('e^ln(99)', 'e^ln(99)', 99),
    { name: 'ln product 3*11', input: 'ln(3 * 11)', matches: 'ln(3) + ln(11)' },
    { name: 'ln quotient 64/8', input: 'ln(64 / 8)', matches: 'ln(64) - ln(8)' },
    num('exp(2)', 'exp(2)', Math.exp(2)),
    num('exp(-1)', 'exp(-1)', Math.exp(-1)),
  )
  return out
}

const DEG = [0, 15, 30, 45, 60, 75, 90, 105, 120, 135, 150, 165, 180, 195, 210, 225, 240, 255, 270, 285, 300, 315, 330, 345, 360]

function toRad(deg: number): number {
  return (deg * Math.PI) / 180
}

export function extraTrigRad(): ExtraCase[] {
  const out: ExtraCase[] = []
  for (const d of DEG) {
    const r = toRad(d)
    const expr = d === 0 ? '0' : `${d}*pi/180`
    out.push(num(`sin(${d}°) rad`, `sin(${expr})`, Math.sin(r), { angleMode: 'rad', eps: 1e-7 }))
    out.push(num(`cos(${d}°) rad`, `cos(${expr})`, Math.cos(r), { angleMode: 'rad', eps: 1e-7 }))
    if (Math.abs(Math.cos(r)) > 1e-10) {
      out.push(num(`tan(${d}°) rad`, `tan(${expr})`, Math.tan(r), { angleMode: 'rad', eps: 1e-7 }))
    }
  }
  out.push(
    num('sin(pi/2)', 'sin(pi/2)', 1, { angleMode: 'rad' }),
    num('cos(3*pi/2)', 'cos(3*pi/2)', 0, { angleMode: 'rad', eps: 1e-7 }),
    num('sin(-pi/6)', 'sin(-pi/6)', -0.5, { angleMode: 'rad' }),
    num('cos(-pi/4)', 'cos(-pi/4)', Math.SQRT2 / 2, { angleMode: 'rad' }),
    { name: 'double-angle 0.8', input: '2 * sin(0.8) * cos(0.8)', matches: 'sin(1.6)', angleMode: 'rad' },
    { name: 'pythagoras 0.4', input: 'sin(0.4)^2 + cos(0.4)^2', expected: 1, angleMode: 'rad' },
    { name: 'pythagoras 2.2', input: 'sin(2.2)^2 + cos(2.2)^2', expected: 1, angleMode: 'rad' },
    num('csc(pi/3)', 'csc(pi/3)', 2 / Math.sqrt(3), { angleMode: 'rad' }),
    num('sec(pi/6)', 'sec(pi/6)', 2 / Math.sqrt(3), { angleMode: 'rad' }),
    num('cot(pi/3)', 'cot(pi/3)', 1 / Math.sqrt(3), { angleMode: 'rad' }),
  )
  return out
}

export function extraTrigDeg(): ExtraCase[] {
  const out: ExtraCase[] = []
  for (const d of DEG) {
    const r = toRad(d)
    out.push(num(`sin(${d})`, `sin(${d})`, Math.sin(r), { eps: 1e-7 }))
    out.push(num(`cos(${d})`, `cos(${d})`, Math.cos(r), { eps: 1e-7 }))
    if (Math.abs(Math.cos(r)) > 1e-10) {
      out.push(num(`tan(${d})`, `tan(${d})`, Math.tan(r), { eps: 1e-7 }))
    }
  }
  out.push(
    num('csc(45)', 'csc(45)', Math.SQRT2),
    num('sec(45)', 'sec(45)', Math.SQRT2),
    num('cot(60)', 'cot(60)', 1 / Math.sqrt(3)),
    num('sin(-30)', 'sin(-30)', -0.5),
    num('cos(-60)', 'cos(-60)', 0.5),
    num('tan(-45)', 'tan(-45)', -1),
    num('sin(390)', 'sin(390)', 0.5),
    num('cos(420)', 'cos(420)', 0.5),
    { name: 'identity 12°', input: 'sin(12)^2 + cos(12)^2', expected: 1 },
    { name: 'identity 73°', input: 'sin(73)^2 + cos(73)^2', expected: 1 },
    { name: 'identity 200°', input: 'sin(200)^2 + cos(200)^2', expected: 1 },
  )
  return out
}

export function extraInverseTrig(): ExtraCase[] {
  const out: ExtraCase[] = []
  const samples = [-1, -0.9, -0.75, -0.6, -0.5, -Math.SQRT2 / 2, -0.25, 0, 0.25, 0.5, Math.SQRT2 / 2, 0.6, 0.75, 0.9, 1]
  for (const x of samples) {
    const label = String(x)
    out.push(num(`arcsin(${label}) deg`, `arcsin(${x})`, (Math.asin(x) * 180) / Math.PI, { eps: 1e-7 }))
    out.push(num(`arccos(${label}) deg`, `arccos(${x})`, (Math.acos(x) * 180) / Math.PI, { eps: 1e-7 }))
    out.push(num(`arcsin(${label}) rad`, `arcsin(${x})`, Math.asin(x), { angleMode: 'rad', eps: 1e-7 }))
    out.push(num(`arccos(${label}) rad`, `arccos(${x})`, Math.acos(x), { angleMode: 'rad', eps: 1e-7 }))
  }
  for (const x of [-10, -2, -1, -0.5, 0, 0.5, 1, 2, 10, 100]) {
    out.push(num(`arctan(${x}) deg`, `arctan(${x})`, (Math.atan(x) * 180) / Math.PI, { eps: 1e-7 }))
    out.push(num(`arctan(${x}) rad`, `arctan(${x})`, Math.atan(x), { angleMode: 'rad', eps: 1e-7 }))
  }
  out.push(
    num('arccsc(1)', 'arccsc(1)', 90),
    num('arcsec(1)', 'arcsec(1)', 0),
    num('arccot(0)', 'arccot(0)', 90),
    num('sin(arcsin(0.3))', 'sin(arcsin(0.3))', 0.3),
    num('cos(arccos(0.4))', 'cos(arccos(0.4))', 0.4),
    num('tan(arctan(2))', 'tan(arctan(2))', 2),
    num('arcsin(sin(20))', 'arcsin(sin(20))', 20),
    num('arccos(cos(40))', 'arccos(cos(40))', 40),
    num('asin(0.5)', 'asin(0.5)', 30),
    num('sin^-1(0.5)', 'sin^-1(0.5)', 30),
    num('cos^-1(0.5)', 'cos^-1(0.5)', 60),
    num('tan^-1(1)', 'tan^-1(1)', 45),
    num('sin^(-1)(1)', 'sin^(-1)(1)', 90),
    num('sin⁻¹(0)', 'sin⁻¹(0)', 0),
    num('atan2(1,1)', 'atan2(1,1)', 45),
    num('arctan2(1,0)', 'arctan2(1,0)', 90),
  )
  return out
}

export function extraHyperbolic(): ExtraCase[] {
  const out: ExtraCase[] = []
  const xs = [-2, -1.5, -1, -0.5, 0, 0.25, 0.5, 0.75, 1, 1.25, 1.5, 2, 2.5, 3]
  for (const x of xs) {
    out.push(num(`sinh(${x})`, `sinh(${x})`, Math.sinh(x)))
    out.push(num(`cosh(${x})`, `cosh(${x})`, Math.cosh(x)))
    out.push(num(`tanh(${x})`, `tanh(${x})`, Math.tanh(x)))
    if (x !== 0) {
      out.push(num(`csch(${x})`, `csch(${x})`, 1 / Math.sinh(x)))
      out.push(num(`coth(${x})`, `coth(${x})`, 1 / Math.tanh(x)))
    }
    out.push(num(`sech(${x})`, `sech(${x})`, 1 / Math.cosh(x)))
    out.push(num(`arcsinh(${x})`, `arcsinh(${x})`, Math.asinh(x)))
  }
  for (const x of [1, 1.2, 1.5, 2, 3, 4, 10]) {
    out.push(num(`arccosh(${x})`, `arccosh(${x})`, Math.acosh(x)))
  }
  for (const x of [-0.9, -0.5, -0.2, 0, 0.2, 0.5, 0.9]) {
    out.push(num(`arctanh(${x})`, `arctanh(${x})`, Math.atanh(x)))
  }
  out.push(
    { name: 'identity 0.7', input: 'cosh(0.7)^2 - sinh(0.7)^2', expected: 1 },
    { name: 'identity 2.4', input: 'cosh(2.4)^2 - sinh(2.4)^2', expected: 1 },
    num('arccsch(2)', 'arccsch(2)', Math.asinh(0.5)),
    num('arcsech(0.8)', 'arcsech(0.8)', Math.acosh(1.25)),
  )
  return out
}

export function extraSciNotation(): ExtraCase[] {
  const out: ExtraCase[] = []
  for (let e = -12; e <= 12; e++) {
    if (e === 0) continue
    out.push(num(`10^${e}`, `10^${e}`, 10 ** e, { eps: Math.max(1e-8, 10 ** e * 1e-10) }))
    out.push(num(`2.5*10^${e}`, `2.5 * 10^${e}`, 2.5 * 10 ** e, { eps: Math.max(1e-8, Math.abs(2.5 * 10 ** e) * 1e-10) }))
  }
  out.push(
    num('1.2e4', '1.2e4', 12000),
    num('3.4E-5', '3.4E-5', 3.4e-5),
    num('6.02e23', '6.02e23', 6.02e23, { eps: 1e9 }),
    num('9.1e-31', '9.1e-31', 9.1e-31, { eps: 1e-40 }),
    num('(4e3)*(5e2)', '(4e3) * (5e2)', 2e6),
    num('(9e8)/(3e2)', '(9e8) / (3e2)', 3e6),
    num('sqrt(9e10)', 'sqrt(9e10)', 3e5),
    num('(1e20)*(1e-20)', '(1e20) * (1e-20)', 1),
    num('1.5*10^3 + 2.5*10^3', '1.5 * 10^3 + 2.5 * 10^3', 4000),
    num('7.7*10^(-4)', '7.7 * 10^(-4)', 7.7e-4),
    { name: 'tiny display', input: '1.1e-12 * 3', display: /3\.3e-12/i },
    { name: 'large display', input: '4e13 * 2', display: /8e\+?13/i },
    num('1.1e2', '1.1e2', 110),
    num('2.2e3', '2.2e3', 2200),
    num('3.3e-2', '3.3e-2', 0.033),
    num('4.4e-3', '4.4e-3', 0.0044),
    num('5.5e1', '5.5e1', 55),
    num('6.6E4', '6.6E4', 66000),
    num('7.7e-1', '7.7e-1', 0.77),
    num('8.8e5', '8.8e5', 880000),
    num('9.9e-6', '9.9e-6', 9.9e-6),
    num('1e0', '1e0', 1),
    num('(2e4)+(3e4)', '(2e4) + (3e4)', 50000),
    num('(8e5)-(3e5)', '(8e5) - (3e5)', 500000),
    num('(1.5e2)/(5e-1)', '(1.5e2) / (5e-1)', 300),
    num('sqrt(1.6e5)', 'sqrt(1.6e5)', 400),
    num('sqrt(2.5e-3)', 'sqrt(2.5e-3)', Math.sqrt(0.0025)),
    num('(10^8)/(10^3)', '(10^8) / (10^3)', 1e5),
    num('(10^(-2))*(10^(-3))', '(10^(-2)) * (10^(-3))', 1e-5),
    num('3.14*10^2', '3.14 * 10^2', 314),
    num('6.022*10^23', '6.022 * 10^23', 6.022e23, { eps: 1e9 }),
    num('1.602*10^(-19)', '1.602 * 10^(-19)', 1.602e-19, { eps: 1e-28 }),
    num('9*10^9', '9 * 10^9', 9e9),
    num('1*10^(-12)', '1 * 10^(-12)', 1e-12, { eps: 1e-20 }),
    num('(5*10^4)^2', '(5 * 10^4)^2', 2.5e9),
    num('2.0e+2', '2.0e+2', 200),
    num('4e+3 * 2e-3', '4e+3 * 2e-3', 8),
    num('1.23*10^0', '1.23 * 10^0', 1.23),
    num('8.15*10^1', '8.15 * 10^1', 81.5),
    num('4.5*10^4 / 1.5*10^2', '4.5 * 10^4 / (1.5 * 10^2)', 300),
    num('10^3 + 10^2', '10^3 + 10^2', 1100),
    num('10^6 - 10^5', '10^6 - 10^5', 900000),
  )
  return out
}

export function extraConstants(): ExtraCase[] {
  const out: ExtraCase[] = []
  for (let i = 1; i <= 30; i++) {
    out.push(num(`assign val=${i}`, [`val = ${i}`], i))
    out.push(num(`val=${i}; val*3`, [`val = ${i}`, 'val * 3'], i * 3))
    out.push(num(`ans chain ${i}`, [`${i}`, 'ans + 1', 'ans * 2'], (i + 1) * 2))
  }
  out.push(
    num('2*pi', '2 * pi', 2 * Math.PI),
    num('pi/2', 'pi/2', Math.PI / 2),
    num('tau/2', 'tau / 2', Math.PI),
    num('e*e', 'e * e', Math.E * Math.E),
    num('pi*e', 'pi * e', Math.PI * Math.E),
    num('sphere 4/3*pi*radius^3', ['radius = 3', '(4/3) * pi * radius^3'], (4 / 3) * Math.PI * 27),
    num('poly x=2', ['x = 2', 'x^3 - 2*x + 1'], 5),
    num('dep chain', ['foo = 3', 'bar = foo * 4', 'bar - foo'], 9),
    num('theta 30', ['theta = 30', 'sin(theta)'], 0.5),
    num('mean with var', ['qty = 5', 'mean([qty, qty+1, qty+2])'], 6),
    num('multi letter', ['width = 4', 'height = 6', 'width * height'], 24),
    num('ans trig', ['90', 'sin(ans)'], 1),
    num('pi^2', 'pi^2', Math.PI ** 2),
    num('e^pi', 'e^pi', Math.E ** Math.PI),
  )
  return out
}

function meanOf(xs: number[]): number {
  return xs.reduce((s, x) => s + x, 0) / xs.length
}

function medianOf(xs: number[]): number {
  const s = [...xs].sort((a, b) => a - b)
  const m = (s.length - 1) / 2
  return (s[Math.floor(m)]! + s[Math.ceil(m)]!) / 2
}

export function extraStats(): ExtraCase[] {
  const out: ExtraCase[] = []
  for (let n = 2; n <= 12; n++) {
    const xs = Array.from({ length: n }, (_, i) => i + 1)
    const list = `[${xs.join(', ')}]`
    out.push(num(`mean 1..${n}`, `mean(${list})`, meanOf(xs)))
    out.push(num(`median 1..${n}`, `median(${list})`, medianOf(xs)))
    out.push(num(`min 1..${n}`, `min(${list})`, 1))
    out.push(num(`max 1..${n}`, `max(${list})`, n))
    out.push(num(`total 1..${n}`, `total(${list})`, (n * (n + 1)) / 2))
    out.push(num(`length 1..${n}`, `length(${list})`, n))
  }
  out.push(
    num('mean args', 'mean(2, 4, 6, 8)', 5),
    num('mean negatives', 'mean([-2, 0, 2])', 0),
    num('stdevp [1,1,1,1]', 'stdevp([1, 1, 1, 1])', 0),
    num('varp [1,3]', 'varp([1, 3])', 1),
    num('corr identical', 'corr([1, 2, 3], [1, 2, 3])', 1),
    num('corr anti', 'corr([1, 2, 3], [3, 2, 1])', -1),
    { name: 'range 5..8', input: '[5...8]', display: '[5, 6, 7, 8]' },
    { name: 'range 0..4', input: '[0...4]', display: '[0, 1, 2, 3, 4]' },
    num('quantile 0', 'quantile([1, 2, 3, 4, 5], 0)', 1),
    num('quantile 1', 'quantile([1, 2, 3, 4, 5], 1)', 5),
    num('quartile 2', 'quartile([1, 2, 3, 4, 5, 6, 7], 2)', 4),
    num('mean 10,20,30,40', 'mean([10, 20, 30, 40])', 25),
    num('median 1,100', 'median([1, 100])', 50.5),
    num('min negatives', 'min([-8, -2, -15])', -15),
    num('max mixed', 'max([-3, 0, 4, 2])', 4),
    num('total 10 nums', 'total([1, 1, 1, 1, 1, 1, 1, 1, 1, 1])', 10),
    num('length args', 'length(9, 8, 7, 6)', 4),
    { name: 'range 2..2', input: '[2...2]', display: '[2]' },
    { name: 'range 9..12', input: '[9...12]', display: '[9, 10, 11, 12]' },
  )
  return out
}

function fact(n: number): number {
  let p = 1
  for (let i = 2; i <= n; i++) p *= i
  return p
}

function nCr(n: number, k: number): number {
  if (k < 0 || k > n) return 0
  return fact(n) / (fact(k) * fact(n - k))
}

function nPr(n: number, k: number): number {
  if (k < 0 || k > n) return 0
  return fact(n) / fact(n - k)
}

export function extraCombinatorics(): ExtraCase[] {
  const out: ExtraCase[] = []
  for (let n = 0; n <= 12; n++) out.push(num(`${n}!`, `${n}!`, fact(n)))
  for (let n = 4; n <= 10; n++) {
    for (let k = 0; k <= n; k++) {
      out.push(num(`nCr(${n},${k})`, `nCr(${n}, ${k})`, nCr(n, k)))
      out.push(num(`nPr(${n},${k})`, `nPr(${n}, ${k})`, nPr(n, k)))
    }
  }
  out.push(
    num('6 nCr 3', '6 nCr 3', 20),
    num('7 nPr 2', '7 nPr 2', 42),
    num('nCr(15, 2)', 'nCr(15, 2)', 105),
    num('nPr(8, 3)', 'nPr(8, 3)', 336),
    { name: 'random 0-1 again', input: 'random()', gte: 0, lt: 1 },
    { name: 'random list 8', input: 'random(8)', listLength: 8 },
    { name: 'randint 10-20', input: 'randint(10, 20)', integer: true, gte: 10, lte: 20 },
    { name: 'randint list 4', input: 'randint(0, 9, 4)', listLength: 4 },
    num('12!/10!', '12! / 10!', 132),
    num('nCr(10, 3)', 'nCr(10, 3)', 120),
  )
  return out
}

function gcd(a: number, b: number): number {
  a = Math.abs(Math.trunc(a))
  b = Math.abs(Math.trunc(b))
  while (b) {
    const t = b
    b = a % b
    a = t
  }
  return a || 1
}

function lcm(a: number, b: number): number {
  return Math.abs(a * b) / gcd(a, b)
}

export function extraRounding(): ExtraCase[] {
  const out: ExtraCase[] = []
  const vals = [-9.9, -4.1, -3.5, -1.1, -0.4, 0, 0.4, 1.1, 2.5, 3.2, 7.8, 12.49, 12.5]
  for (const v of vals) {
    out.push(num(`abs(${v})`, `abs(${v})`, Math.abs(v)))
    out.push(num(`floor(${v})`, `floor(${v})`, Math.floor(v)))
    out.push(num(`ceil(${v})`, `ceil(${v})`, Math.ceil(v)))
    out.push(num(`sign(${v})`, `sign(${v})`, Math.sign(v)))
    out.push(num(`round(${v})`, `round(${v})`, Math.round(v)))
  }
  for (let a = 6; a <= 24; a += 3) {
    for (let b = 4; b <= 18; b += 4) {
      out.push(num(`gcd(${a},${b})`, `gcd(${a}, ${b})`, gcd(a, b)))
      out.push(num(`lcm(${a},${b})`, `lcm(${a}, ${b})`, lcm(a, b)))
      out.push(num(`mod(${a},${b})`, `mod(${a}, ${b})`, ((a % b) + b) % b))
    }
  }
  out.push(
    num('round(pi, 3)', 'round(3.14159, 3)', 3.142),
    num('round(2.71828, 2)', 'round(2.71828, 2)', 2.72),
    num('gcd(15, 25, 40)', 'gcd(15, 25, 40)', 5),
    num('lcm(2, 4, 5)', 'lcm(2, 4, 5)', 20),
    num('mod(-7, 4)', 'mod(-7, 4)', 1),
  )
  return out
}

export function extraEdges(): ExtraCase[] {
  const out: ExtraCase[] = []
  for (let n = 1; n <= 20; n++) {
    out.push({ name: `sqrt(-${n})`, input: `sqrt(-${n})`, undefined: true })
    out.push({ name: `ln(-${n})`, input: `ln(-${n})`, undefined: true })
    out.push({ name: `log(-${n})`, input: `log(-${n})`, undefined: true })
  }
  for (const x of [1.1, 1.2, 1.5, 2, 3, 10, -1.1, -2, -5]) {
    out.push({ name: `arcsin(${x})`, input: `arcsin(${x})`, undefined: true })
    out.push({ name: `arccos(${x})`, input: `arccos(${x})`, undefined: true })
  }
  for (let k = 0; k <= 6; k++) {
    out.push({ name: `tan(${90 + 180 * k})`, input: `tan(${90 + 180 * k})`, undefined: true })
    out.push({ name: `sec(${90 + 180 * k})`, input: `sec(${90 + 180 * k})`, undefined: true })
  }
  for (let k = 0; k <= 6; k++) {
    out.push({ name: `csc(${180 * k})`, input: `csc(${180 * k})`, undefined: true })
    out.push({ name: `cot(${180 * k})`, input: `cot(${180 * k})`, undefined: true })
  }
  for (let n = 1; n <= 12; n++) {
    out.push({ name: `(-${n})!`, input: `(-${n})!`, undefined: true })
    out.push({ name: `${n}/0`, input: `${n} / 0`, undefined: true })
    out.push({ name: `mod(${n},0)`, input: `mod(${n}, 0)`, undefined: true })
  }
  out.push(
    { name: 'ln(0)', input: 'ln(0)', undefined: true },
    { name: 'log(0)', input: 'log(0)', undefined: true },
    { name: '0/0', input: '0 / 0', undefined: true },
    { name: 'tan(pi/2) rad', input: 'tan(pi/2)', angleMode: 'rad', undefined: true },
    { name: 'tan(3*pi/2) rad', input: 'tan(3*pi/2)', angleMode: 'rad', undefined: true },
    { name: 'arccosh(0)', input: 'arccosh(0)', undefined: true },
    { name: 'arccosh(0.9)', input: 'arccosh(0.9)', undefined: true },
    { name: 'arctanh(1)', input: 'arctanh(1)', undefined: true },
    { name: 'arctanh(-1)', input: 'arctanh(-1)', undefined: true },
    { name: 'arctanh(2)', input: 'arctanh(2)', undefined: true },
    { name: 'csch(0)', input: 'csch(0)', undefined: true },
    { name: 'coth(0)', input: 'coth(0)', undefined: true },
    { name: 'arcsech(0)', input: 'arcsech(0)', undefined: true },
    { name: 'arcsech(1.2)', input: 'arcsech(1.2)', undefined: true },
    { name: 'arccsch(0)', input: 'arccsch(0)', undefined: true },
    { name: '171!', input: '171!', infinity: true },
    { name: '180!', input: '180!', infinity: true },
    { name: '250!', input: '250!', infinity: true },
  )
  return out
}

const EXISTING_RADICALS = new Set([
  12, 18, 20, 24, 27, 32, 45, 48, 50, 72, 75, 80, 96, 98, 108, 125, 128, 147, 162, 180, 200, 242, 288, 300, 320, 338,
  363, 392, 432, 450, 500, 512, 588, 600, 675, 720, 800, 847, 968, 1008, 1089, 1250, 1331, 1452, 1568, 1800, 2000, 2420,
  2500, 3200,
])

export function extraRadicals(): ExtraCase[] {
  const out: ExtraCase[] = []
  const squareFree = [2, 3, 5, 6, 7, 10, 11, 13, 14, 15]
  for (let k = 2; k <= 30; k++) {
    for (const m of squareFree) {
      const n = k * k * m
      if (EXISTING_RADICALS.has(n)) continue
      out.push({
        name: `sqrt(${n})`,
        input: `sqrt(${n})`,
        exact: `${k}sqrt(${m})`,
        expected: k * Math.sqrt(m),
      })
    }
  }
  for (const k of [6, 7, 8, 9, 10, 11, 12, 14, 15, 16, 18, 20, 21, 22, 24, 25, 26, 27, 28, 30]) {
    out.push({
      name: `sqrt(${k * k})`,
      input: `sqrt(${k * k})`,
      exact: String(k),
      expected: k,
    })
  }
  out.push(
    num('inserted 3sqrt(2)', '3sqrt(2)', 3 * Math.sqrt(2), { exact: '3sqrt(2)' }),
    num('inserted 5sqrt(3)', '5sqrt(3)', 5 * Math.sqrt(3), { exact: '5sqrt(3)' }),
    num('inserted 2sqrt(7)', '2sqrt(7)', 2 * Math.sqrt(7), { exact: '2sqrt(7)' }),
  )
  return out
}

const NICE_TRIG: ExtraCase[] = [
  { name: 'sin(3*pi/2)', input: 'sin(3*pi/2)', angleMode: 'rad', exact: '-1', expected: -1 },
  { name: 'cos(3*pi/2)', input: 'cos(3*pi/2)', angleMode: 'rad', exact: '0', expected: 0, eps: 1e-7 },
  { name: 'sin(5*pi/4)', input: 'sin(5*pi/4)', angleMode: 'rad', exact: '-sqrt(2)/2', expected: -Math.SQRT2 / 2 },
  { name: 'cos(5*pi/4)', input: 'cos(5*pi/4)', angleMode: 'rad', exact: '-sqrt(2)/2', expected: -Math.SQRT2 / 2 },
  { name: 'tan(5*pi/4)', input: 'tan(5*pi/4)', angleMode: 'rad', exact: '1', expected: 1 },
  { name: 'sin(7*pi/4)', input: 'sin(7*pi/4)', angleMode: 'rad', exact: '-sqrt(2)/2', expected: -Math.SQRT2 / 2 },
  { name: 'cos(7*pi/4)', input: 'cos(7*pi/4)', angleMode: 'rad', exact: 'sqrt(2)/2', expected: Math.SQRT2 / 2 },
  { name: 'tan(7*pi/4)', input: 'tan(7*pi/4)', angleMode: 'rad', exact: '-1', expected: -1 },
  { name: 'sin(11*pi/6)', input: 'sin(11*pi/6)', angleMode: 'rad', exact: '-1/2', expected: -0.5 },
  { name: 'cos(11*pi/6)', input: 'cos(11*pi/6)', angleMode: 'rad', exact: 'sqrt(3)/2', expected: Math.sqrt(3) / 2 },
  { name: 'tan(11*pi/6)', input: 'tan(11*pi/6)', angleMode: 'rad', exact: '-sqrt(3)/3', expected: -Math.sqrt(3) / 3 },
  { name: 'sin(4*pi/3)', input: 'sin(4*pi/3)', angleMode: 'rad', exact: '-sqrt(3)/2', expected: -Math.sqrt(3) / 2 },
  { name: 'cos(4*pi/3)', input: 'cos(4*pi/3)', angleMode: 'rad', exact: '-1/2', expected: -0.5 },
  { name: 'tan(4*pi/3)', input: 'tan(4*pi/3)', angleMode: 'rad', exact: 'sqrt(3)', expected: Math.sqrt(3) },
  { name: 'sin(5*pi/3)', input: 'sin(5*pi/3)', angleMode: 'rad', exact: '-sqrt(3)/2', expected: -Math.sqrt(3) / 2 },
  { name: 'cos(5*pi/3)', input: 'cos(5*pi/3)', angleMode: 'rad', exact: '1/2', expected: 0.5 },
  { name: 'tan(5*pi/3)', input: 'tan(5*pi/3)', angleMode: 'rad', exact: '-sqrt(3)', expected: -Math.sqrt(3) },
  { name: 'sin(-pi/3)', input: 'sin(-pi/3)', angleMode: 'rad', exact: '-sqrt(3)/2', expected: -Math.sqrt(3) / 2 },
  { name: 'cos(-pi/6)', input: 'cos(-pi/6)', angleMode: 'rad', exact: 'sqrt(3)/2', expected: Math.sqrt(3) / 2 },
  { name: 'tan(-pi/4)', input: 'tan(-pi/4)', angleMode: 'rad', exact: '-1', expected: -1 },
  { name: 'sin(30) exact', input: 'sin(30)', exact: '1/2', expected: 0.5 },
  { name: 'cos(30) exact', input: 'cos(30)', exact: 'sqrt(3)/2', expected: Math.sqrt(3) / 2 },
  { name: 'tan(30) exact', input: 'tan(30)', exact: 'sqrt(3)/3', expected: Math.sqrt(3) / 3 },
  { name: 'sin(45) exact', input: 'sin(45)', exact: 'sqrt(2)/2', expected: Math.SQRT2 / 2 },
  { name: 'cos(60) exact', input: 'cos(60)', exact: '1/2', expected: 0.5 },
  { name: 'tan(60) exact', input: 'tan(60)', exact: 'sqrt(3)', expected: Math.sqrt(3) },
  { name: 'sin(90) exact', input: 'sin(90)', exact: '1', expected: 1 },
  { name: 'cos(90) exact', input: 'cos(90)', exact: '0', expected: 0 },
  { name: 'sin(0) exact deg', input: 'sin(0)', exact: '0', expected: 0 },
  { name: 'cos(180) exact', input: 'cos(180)', exact: '-1', expected: -1 },
  { name: 'sin(150) exact', input: 'sin(150)', exact: '1/2', expected: 0.5 },
  { name: 'cos(150) exact', input: 'cos(150)', exact: '-sqrt(3)/2', expected: -Math.sqrt(3) / 2 },
  { name: 'tan(150) exact', input: 'tan(150)', exact: '-sqrt(3)/3', expected: -Math.sqrt(3) / 3 },
  { name: 'sin(210) exact', input: 'sin(210)', exact: '-1/2', expected: -0.5 },
  { name: 'cos(210) exact', input: 'cos(210)', exact: '-sqrt(3)/2', expected: -Math.sqrt(3) / 2 },
  { name: 'sin(330) exact', input: 'sin(330)', exact: '-1/2', expected: -0.5 },
  { name: 'cos(330) exact', input: 'cos(330)', exact: 'sqrt(3)/2', expected: Math.sqrt(3) / 2 },
  { name: 'csc(pi/6) exact', input: 'csc(pi/6)', angleMode: 'rad', exact: '2', expected: 2 },
  { name: 'sec(pi/3) exact', input: 'sec(pi/3)', angleMode: 'rad', exact: '2', expected: 2 },
  { name: 'cot(pi/4) exact', input: 'cot(pi/4)', angleMode: 'rad', exact: '1', expected: 1 },
  { name: 'csc(pi/4) exact', input: 'csc(pi/4)', angleMode: 'rad', exact: 'sqrt(2)', expected: Math.SQRT2 },
  { name: 'sec(pi/4) exact', input: 'sec(pi/4)', angleMode: 'rad', exact: 'sqrt(2)', expected: Math.SQRT2 },
  { name: 'cot(pi/6) exact', input: 'cot(pi/6)', angleMode: 'rad', exact: 'sqrt(3)', expected: Math.sqrt(3) },
  { name: 'arcsin(-1) rad exact', input: 'arcsin(-1)', angleMode: 'rad', exact: '-pi/2', expected: -Math.PI / 2 },
  { name: 'arccos(-1/2) already', input: 'arccos(-1/2)', angleMode: 'rad', exact: '2*pi/3', expected: (2 * Math.PI) / 3 },
  { name: 'arctan(-1/sqrt(3))', input: 'arctan(-1/sqrt(3))', angleMode: 'rad', exact: '-pi/6', expected: -Math.PI / 6 },
  { name: 'sin(pi/12) already covered extra tan', input: 'tan(pi/12)', angleMode: 'rad', exact: '2-sqrt(3)', expected: 2 - Math.sqrt(3) },
  { name: 'tan(5*pi/12)', input: 'tan(5*pi/12)', angleMode: 'rad', exact: '2+sqrt(3)', expected: 2 + Math.sqrt(3) },
  { name: 'sin(5*pi/12)', input: 'sin(5*pi/12)', angleMode: 'rad', exact: '(sqrt(6)+sqrt(2))/4', expected: (Math.sqrt(6) + Math.sqrt(2)) / 4 },
  { name: 'cos(5*pi/12)', input: 'cos(5*pi/12)', angleMode: 'rad', exact: '(sqrt(6)-sqrt(2))/4', expected: (Math.sqrt(6) - Math.sqrt(2)) / 4 },
]

export function extraTrigSimp(): ExtraCase[] {
  const out = [...NICE_TRIG]
  for (const d of DEG) {
    const r = toRad(d)
    out.push(num(`simp sin ${d}°`, `sin(${d})`, Math.sin(r), { eps: 1e-7 }))
    out.push(num(`simp cos ${d}°`, `cos(${d})`, Math.cos(r), { eps: 1e-7 }))
    if (Math.abs(Math.cos(r)) > 1e-10) {
      out.push(num(`simp tan ${d}°`, `tan(${d})`, Math.tan(r), { eps: 1e-7 }))
    }
    const expr = d === 0 ? '0' : `${d}*pi/180`
    out.push(num(`simp sin ${d} rad`, `sin(${expr})`, Math.sin(r), { angleMode: 'rad', eps: 1e-7 }))
    out.push(num(`simp cos ${d} rad`, `cos(${expr})`, Math.cos(r), { angleMode: 'rad', eps: 1e-7 }))
  }
  return out
}

export const extraLatex: Array<{ name: string; input: string; expected: number; angleMode?: 'deg' | 'rad' }> = []

for (const n of [0, 30, 45, 60, 90, 120, 180, 270, 360]) {
  extraLatex.push({ name: `latex sin ${n}`, input: `\\sin\\left(${n}\\right)`, expected: Math.sin(toRad(n)) })
  extraLatex.push({ name: `latex cos ${n}`, input: `\\cos\\left(${n}\\right)`, expected: Math.cos(toRad(n)) })
}
for (const n of [2, 4, 9, 16, 25, 36, 49, 64, 81, 100]) {
  extraLatex.push({ name: `latex sqrt ${n}`, input: `\\sqrt{${n}}`, expected: Math.sqrt(n) })
}
for (let a = 1; a <= 6; a++) {
  for (let b = 2; b <= 6; b++) {
    extraLatex.push({ name: `latex frac ${a}/${b}`, input: `\\frac{${a}}{${b}}`, expected: a / b })
  }
}
extraLatex.push(
  { name: 'latex pi', input: '\\pi', expected: Math.PI },
  { name: 'latex 2pi', input: '2\\pi', expected: 2 * Math.PI },
  { name: 'latex e', input: '\\exponentialE', expected: Math.E },
  { name: 'latex ln e', input: '\\ln\\left(\\exponentialE\\right)', expected: 1 },
  { name: 'latex log 1000', input: '\\log\\left(1000\\right)', expected: 3 },
  { name: 'latex log2 16', input: '\\log_{2}\\left(16\\right)', expected: 4 },
  { name: 'latex nCr 8 3', input: '\\operatorname{nCr}\\left(8,3\\right)', expected: 56 },
  { name: 'latex mean', input: '\\operatorname{mean}\\left(2,4,6\\right)', expected: 4 },
  { name: 'latex cbrt 27', input: '\\sqrt[3]{27}', expected: 3 },
  { name: 'latex abs -8', input: '\\left|-8\\right|', expected: 8 },
  { name: 'latex exp 0', input: '\\exp\\left(0\\right)', expected: 1 },
  { name: 'latex sin pi/2 rad', input: '\\sin\\left(\\pi/2\\right)', expected: 1, angleMode: 'rad' },
  { name: 'latex tan pi/4 rad', input: '\\tan\\left(\\pi/4\\right)', expected: 1, angleMode: 'rad' },
  { name: 'spaced pi', input: 'p i', expected: Math.PI },
  { name: 'dot pi', input: 'p · i', expected: Math.PI },
  { name: 'middle-dot mul', input: '2 · 3', expected: 6 },
  { name: 'dot-operator mul', input: '4⋅5', expected: 20 },
  { name: 'typeset 3 · 10^2', input: '3 · 10^2', expected: 300 },
  { name: 'word-dot mul', input: '3 dot 4', expected: 12 },
  { name: 'cdot pi', input: 'p\\cdot i', expected: Math.PI },
  { name: '2pi concat', input: '2pi', expected: 2 * Math.PI },
  { name: 'latex binom 5 2', input: '\\binom{5}{2}', expected: 10 },
  { name: 'latex frac sum', input: '\\frac{1}{4}+\\frac{1}{4}', expected: 0.5 },
)
for (const n of [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]) {
  extraLatex.push({ name: `latex ${n}^2`, input: `${n}^{2}`, expected: n * n })
  extraLatex.push({ name: `latex sqrt frac ${n}`, input: `\\sqrt{\\frac{${n * n}}{4}}`, expected: n / 2 })
  extraLatex.push({ name: `latex log 10^${n}`, input: `\\log\\left(${10 ** n}\\right)`, expected: n })
}

export const extraExactCopy: Array<{ name: string; input: string; exact: string }> = [
  { name: 'sqrt(8)', input: 'sqrt(8)', exact: '2sqrt(2)' },
  { name: 'sqrt(28)', input: 'sqrt(28)', exact: '2sqrt(7)' },
  { name: 'sqrt(44)', input: 'sqrt(44)', exact: '2sqrt(11)' },
  { name: 'sqrt(52)', input: 'sqrt(52)', exact: '2sqrt(13)' },
  { name: 'sqrt(63)', input: 'sqrt(63)', exact: '3sqrt(7)' },
  { name: 'sqrt(90)', input: 'sqrt(90)', exact: '3sqrt(10)' },
  { name: 'sin(45) exact copy', input: 'sin(45)', exact: 'sqrt(2)/2' },
  { name: 'cos(30) exact copy', input: 'cos(30)', exact: 'sqrt(3)/2' },
  { name: 'tan(60) exact copy', input: 'tan(60)', exact: 'sqrt(3)' },
  { name: '1/2 exact copy', input: '1/2', exact: '1/2' },
  { name: '2/3 exact copy', input: '2/3', exact: '2/3' },
  { name: '3/4 exact copy', input: '3/4', exact: '3/4' },
  { name: '5/6 exact copy', input: '5/6', exact: '5/6' },
  { name: 'pi/2 exact copy', input: 'pi/2', exact: 'pi/2' },
  { name: 'pi/3 exact copy', input: 'pi/3', exact: 'pi/3' },
  { name: 'pi/4 exact copy', input: 'pi/4', exact: 'pi/4' },
  { name: '2*pi exact copy', input: '2*pi', exact: '2*pi' },
  { name: 'sqrt(2)/2 already', input: 'sqrt(2)/2', exact: 'sqrt(2)/2' },
]

for (let k = 2; k <= 40; k++) {
  extraExactCopy.push({ name: `copy sqrt(${k * k}*2)`, input: `sqrt(${k * k * 2})`, exact: `${k}sqrt(2)` })
}
for (let k = 2; k <= 30; k++) {
  extraExactCopy.push({ name: `copy sqrt(${k * k}*3)`, input: `sqrt(${k * k * 3})`, exact: `${k}sqrt(3)` })
}
for (let k = 2; k <= 20; k++) {
  extraExactCopy.push({ name: `copy sqrt(${k * k}*5)`, input: `sqrt(${k * k * 5})`, exact: `${k}sqrt(5)` })
}
