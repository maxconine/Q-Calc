// Generated arithmetic and number-formatting corpus. Nothing is asserted: the recorder keeps what the engine says.
import { it } from 'vitest'
import { evaluateSheet } from '../../../src/engine/evaluate'
import { formatNumber, formatScientific, prettyAnswer } from '../../../src/engine/format'
import { formatAsFraction } from '../../../src/engine/scientific'
import { arithmetic, double, int, pick, rng } from './gen'

const SETTINGS = [{}, { sigFigs: 6 }, { sigFigs: 16 }, { fractionMode: true }, { sigFigs: 3, fractionMode: false }]

// the hand-picked edges every port trips on
const EDGES = [
  '0.1+0.2', '0.1*3', '1/3', '2/3', '1/7', '10/4', '1-0.9', '0.3-0.1', '1.1*1.1', '3*1.1', '0.29*100', '4.35*100',
  '1e308*10', '-1e308*10', '1/0', '-1/0', '0/0', '0^0', '0^-1', '2^1024', '2^-1075', '2^53+1', '2^53', '2^63', '9007199254740993',
  '170!', '171!', '0!', '20!', '21!', '3!!', '(3!)!', '-3!', '(-3)!', '2.5!', '-0', '0*-1', '-0+0', '.5', '5.', '5.0', '05',
  '1e5', '1E-7', '1e-7', '2e+3', '1e21', '1e-21', '123456789012', '1234567890123', '0.000001', '0.0000001', '999999.5',
  '1,234,567', '1,234.5', '1,23', '12,345,678.9', '0xff', '0b1010', '0o17', '0xFF+1', '2³', '10⁻³', '2²+3²', '6÷3',
  '3×4', '3·4', '5−2', '2**3', '2**3**2', '2^3^2', '-2^2', '(-2)^2', '2^-2^2', '(-8)^(1/3)', '(-8)^(2/3)', '(-4)^0.5',
  '50%', '200 + 15%', '200 - 15%', '15% of 200', '17 % 5', '2:30', '3 4', '3 4 5', '2 1/2', '1/2 3', '2(3)', '(2)(3)',
  '2(3)(4)', '(1+2)3', '1/2(4)', '8/2(2+2)', '6/2*3', '((2)', '(2))', '2+', '*3', '', ' ', '2+3=', '2+3 =', '=5', '1/(1-1)',
  '10^15+1', '10^16+1', '1e16+1', '0.1^10', '1.0000000000000002', '0.30000000000000004', '100000000000000000000',
  '3.14159265358979323846', '2.718281828459045', '1/9801', '22/7', '355/113', '1/3+1/3+1/3', '7/3*3', '1-1e-16',
  '12345678.9', '-12345678.9', '0.5^0.5', '2^0.5', '4^0.5', '27^(1/3)', '1/1024', '-(-(-1))', '+5', '++5', '--5', '5--3',
]

it('arithmetic edges under every setting', () => {
  for (const s of SETTINGS) for (const e of EDGES) evaluateSheet([e], s)
})

it('generated arithmetic', () => {
  const r = rng(20261023)
  for (let i = 0; i < 3000; i++) {
    const expr = arithmetic(r)
    evaluateSheet([expr], pick(r, SETTINGS))
    if (i % 4 === 0) evaluateSheet([expr], {})
  }
})

it('formatNumber across magnitudes and figures', () => {
  const r = rng(7)
  for (let i = 0; i < 4000; i++) formatNumber(double(r), int(r, 1, 17))
  for (const n of [0, -0, NaN, Infinity, -Infinity, 1e-7, 1e-6, 9.99999e-7, 1e12, 999999999999.5, 5e-324, 1.7976931348623157e308]) {
    for (const f of [2, 6, 12, 16]) formatNumber(n, f)
  }
})

it('formatScientific and prettyAnswer', () => {
  const r = rng(11)
  for (let i = 0; i < 800; i++) {
    const n = double(r)
    formatScientific(n, int(r, 1, 16))
    prettyAnswer(formatNumber(n, int(r, 2, 16)))
  }
  for (const s of ['-2sqrt(3)', 'π - 1', '-5 m', '$1234.5', '12-05-2024', '-∞', '1234567 kg', '-1e-7']) prettyAnswer(s)
})

it('formatAsFraction', () => {
  const r = rng(13)
  for (let i = 0; i < 1500; i++) {
    const k = r()
    const n = k < 0.6 ? int(r, -500, 500) / int(r, 1, 400) : k < 0.8 ? double(r) : int(r, 1, 99) + int(r, 1, 7) / 8
    formatAsFraction(n)
  }
})
