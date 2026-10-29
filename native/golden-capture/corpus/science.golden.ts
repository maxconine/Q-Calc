// Generated corpus beyond plain arithmetic: functions, units, solve, calculus, matrices, chemistry, sums,
// uncertainty, sheets, phrases, graphs and the answer helpers. Nothing is asserted; the recorder keeps the answers.
import { it } from 'vitest'
import { evaluateSheet } from '../../../src/engine/evaluate'
import { buildGraph } from '../../../src/engine/graph'
import { answerPhrase } from '../../../src/engine/phrases'
import { UNIT_SETTING_GROUPS } from '../../../src/engine/units'
import { answerForms } from '../../../src/lib/answerForms'
import { blankReason } from '../../../src/lib/blankReason'
import { completionFor, COMPLETION_FUNCTIONS, COMPLETION_UNITS } from '../../../src/lib/completion'
import { ELEMENTS } from '../../../src/lib/elements'
import { toTypstMath, typstDocument } from '../../../src/lib/typstMath'
import type { TaggedFn } from '../recorder'
import { double, int, pick, rng, typedNumber } from './gen'

const FNS1 = [
  'sin', 'cos', 'tan', 'csc', 'sec', 'cot', 'asin', 'acos', 'atan', 'acsc', 'asec', 'acot', 'arcsin', 'arctan',
  'sinh', 'cosh', 'tanh', 'asinh', 'acosh', 'atanh', 'sqrt', 'cbrt', 'ln', 'log', 'log2', 'log10', 'exp', 'abs',
  'sign', 'floor', 'ceil', 'round', 'factorial', 'zeta',
]
const ARGS = ['0', '1', '-1', '0.5', '2', '30', '45', '60', '90', '180', '270', '360', '1e-9', '100', 'pi', 'pi/6', 'pi/4', '2pi', 'e', '-0.5', '1.5', '10', '1000']
const ANGLES = [{ angleMode: 'deg' as const }, { angleMode: 'rad' as const }]

it('scientific functions in both angle modes', () => {
  for (const mode of ANGLES) {
    for (const f of FNS1) for (const a of ARGS) evaluateSheet([`${f}(${a})`], mode)
    for (const f of ['sin', 'cos', 'tan', 'sqrt', 'ln', 'log']) for (const a of ['30', 'x', '2x', 'pi', '45 deg', '1 rad']) evaluateSheet([`${f} ${a}`], mode)
  }
  const multi = [
    'max(1,5,3)', 'min(4,-2)', 'mean(1,2,3,4)', 'median(3,1,2)', 'stdev(1,2,3,4)', 'var(2,4,4,4,5,5,7,9)', 'gcd(12,18)',
    'lcm(4,6)', 'gcd(1/2, 1/3)', 'mod(7,3)', '7 mod 3', '-7 mod 3', 'hypot(3,4)', 'nCr(10,3)', '10 nCr 3', 'nPr(5,2)',
    'combinations(1000,500)', 'round(2.675, 2)', 'round(-2.5)', 'round(1.005,2)', 'log(8, 2)', 'log_2(8)', 'nthRoot(27,3)',
    'atan2(1,1)', 'clamp(5,0,3)', 'quartile(1,2,3,4,5,1)', 'quantile(1,2,3,4,0.5)', 'corr([1,2,3],[2,4,7])',
    'sin(30)^2+cos(30)^2', 'sin^2(30)', 'sin^-1(0.5)', 'tan⁻¹(1)', 'SIN(30)', 'Ln(e)', 'e^(i*pi)', 'sqrt(-4)', '(1+2i)*(3-i)',
    'i^2', 'abs(3+4i)', 'conj(2+3i)', 're(2+3i)', '3^100 mod 7', '[1...5]', 'sum([1...100])', 'phi', 'tau', 'catalan',
  ]
  for (const mode of ANGLES) for (const m of multi) evaluateSheet([m], mode)
})

const UNIT_WORDS = ['m', 'km', 'cm', 'mm', 'ft', 'in', 'mi', 'kg', 'g', 'lb', 'oz', 's', 'min', 'h', 'L', 'mL', 'gal', 'N', 'J', 'W', 'Pa', 'atm', 'psi', 'V', 'A', 'Ω', 'Hz', '°C', '°F', 'K', 'deg', 'rad', 'mph', 'km/h', 'm/s', 'm/s^2', 'kWh', 'cal', 'eV', 'MB', 'GB']

it('units: quantities, conversions and arithmetic', () => {
  const r = rng(17)
  for (const u of [...UNIT_WORDS, ...COMPLETION_UNITS]) {
    evaluateSheet([`5 ${u}`])
    evaluateSheet([`2.5 ${u} * 3`])
  }
  for (const group of UNIT_SETTING_GROUPS) {
    for (const item of group.items) {
      for (const a of item.units) for (const b of item.units) if (a !== b) evaluateSheet([`${typedNumber(r)} ${a.id} to ${b.id}`])
    }
  }
  const mixed = [
    '5 m + 3 ft', '10 km / 2 h', '3 kg * 9.8 m/s^2', '100 °C to °F', '-40 °F to °C', '0 K to °C', '1 mi in km', '1 lb to kg',
    '60 mph to m/s', '5 ft 11 in to cm', '1 acre to m^2', '3 m * 4 m', 'sqrt(9 m^2)', '(3 m)^2', '1 kWh to J', '1 atm to psi',
    '2 h + 30 min', '1 day to s', '5 m to kg', '10 N * cos(30)', '1 GB to MB', '1 hp to W', '12 V * 2 A', '1/(2 s)', '9.8 m/s^2 * 2 s',
  ]
  for (const m of mixed) evaluateSheet([m])
  const defaults = [{ length: 'ft' }, { mass: 'lb' }, { temperature: 'F' }]
  for (const d of defaults) for (const m of mixed.slice(0, 12)) evaluateSheet([m], { defaultUnits: d as never })
})

it('solve and isolate', () => {
  const r = rng(19)
  const eqs = ['x^2 = 4', 'x^2 - 5x + 6 = 0', '2x + 3 = 7', 'x^3 = 2', 'sin(x) = 0.5', 'e^x = 10', 'x = x + 1', 'x = x', 'x^2 = -1',
    'solve x^2 = 9 for x', 'isolate x in y = m x + b', 'solve(x + y = 3, x - y = 1)', 'solve({x + y = 3, x - y = 1})', 'x/2 = 3 for x',
    'F = m a for a', 'v = d/t for t', '0.5 = e^(-t/0.384 ms)', 'ln(x) = 2', 'x^x = 27', '2^x = 1024', 'abs(x) = 3', 'sqrt(x) = 4']
  for (const mode of ANGLES) for (const e of eqs) evaluateSheet([e], mode)
  for (let i = 0; i < 150; i++) {
    const a = int(r, 1, 9)
    const b = int(r, -20, 20)
    const c = int(r, -30, 30)
    evaluateSheet([pick(r, [`${a}x + ${b} = ${c}`, `x^2 + ${b}x + ${c} = 0`, `${a}x^2 = ${Math.abs(c)}`, `${a}/x = ${b || 1}`])])
  }
})

it('calculus', () => {
  const exprs = ['d/dx x^3', 'd/dx sin(x)', 'd/dx e^(2x)', 'd/dx ln(x)', 'derivative of x^2 at 3', 'd/dx x^2 at x = 3',
    'integral of x^2 from 0 to 1', '∫0..1 x^2', '∫0..pi sin(x)', '∫ x dx', '∫1..e 1/x', 'limit of sin(x)/x as x -> 0',
    'lim x->0 (1-cos(x))/x^2', 'd/dt (x sin θ)', '∫0..1 1/x', '∫0..∞ e^(-x)', 'integral of e^(-x^2) from -inf to inf']
  for (const mode of ANGLES) for (const e of exprs) evaluateSheet([e], mode)
})

it('matrices, chemistry, sums and uncertainty', () => {
  const lines = ['[[1,2],[3,4]]', 'det([[1,2],[3,4]])', 'inv([[1,2],[3,4]])', '[[1,2],[3,4]] * [[5],[6]]', '[1,2,3] * 2',
    'transpose([[1,2],[3,4]])', '[[1,2],[3,4]]^-1', 'Σ n, n = 1..10', 'sum from n = 1 to 100 of n^2', 'Σ 1/n^2, n = 1..∞',
    '∏ n, n = 1..5', 'sum(1/2^n, n, 0, inf)', '5 ± 0.1', '(5 ± 0.1) * 2', '(2.0 ± 0.1) + (3.0 ± 0.2)', '10 ± 1 m / 2 s',
    '3.00 * 2.0', '0.0045 * 2', 'H2O', 'molar mass of H2O', 'NaCl', 'C6H12O6', 'H2 + O2 -> H2O', 'CH4 + O2 = CO2 + H2O', 'R']
  for (const l of lines) {
    evaluateSheet([l])
    evaluateSheet([l], { sigFigMode: true })
  }
  for (const el of ELEMENTS.slice(0, 118)) evaluateSheet([`molar mass of ${el.symbol}2`])
})

it('sheets with variables, functions and ans', () => {
  const r = rng(23)
  const sheets = [
    ['x = 5', 'x * 2', 'x^2 + 1'], ['a = 3', 'b = 4', 'sqrt(a^2 + b^2)'], ['f(x) = x^2 + 1', 'f(3)', 'f(f(2))'],
    ['2 + 3', 'ans * 4', 'ans / 2'], ['d = 5 cm', 'd * 2', 'd to in'], ['V = 12 V', 'V / 3 ohm'], ['m = 2 kg', 'a = 9.8 m/s^2', 'm a'],
    ['A = [[1,2],[3,4]]', 'A * 2', 'det(A)'], ['g(x, y) = x y', 'g(2, 3)'], ['n = 4', 'n(2+3)', 'n!'], ['x = 5 ± 0.1', 'x * 2'],
    ['r = 3', 'pi r^2', 'ans to sig figs'],
  ]
  for (const s of sheets) {
    evaluateSheet(s)
    evaluateSheet(s, { angleMode: 'rad', sigFigs: 6 })
  }
  for (let i = 0; i < 100; i++) {
    const v = typedNumber(r)
    evaluateSheet([`k = ${v}`, `k * ${typedNumber(r)}`, `ans + k`], { ans: double(r) })
  }
  for (const [ans, expr] of [[2, 'ans^2'], [1 / 3, 'ans*3'], [-3, 'ans^2'], [5, 'ans!'], [0.1, 'ans+0.2']] as const) {
    evaluateSheet([expr], { ans })
  }
})

// thursday 24 september 2026, 3:12:40 pm local time, as in phrases.test.ts
const NOW = new Date(2026, 8, 24, 15, 12, 40)
const RATES = { base: 'EUR', rates: { USD: 1.25, GBP: 0.8, JPY: 160, CAD: 1.5 } }

it('phrases with a fixed clock and fixed rates', () => {
  const phrases = ['20% off 80', '15% of 80', 'what % of 80 is 20', '$60 + 18% tip', '$100 split 4 ways', '$90 / 3 people',
    '10 usd in eur', '£50 to $', '100 yen to dollars', 'today + 3 days', 'tomorrow', '3 weeks from today', 'days until christmas',
    'time in tokyo', '9am + 2 hours', '2:30pm + 45 min', 'monday + 1 week', '5 nights at $120', '1 as a % of 3',
    '% change from 80 to 100', '3 hours ago', 'days between 1 jan and 1 mar', '20 percent of 80', '$10 for lunch + 15% tip']
  for (const p of phrases) {
    answerPhrase(p, { now: NOW, rates: RATES })
    answerPhrase(p, { now: NOW, rates: null, sigFigs: 6 })
  }
})

it('graphs', () => {
  const inputs = ['graph x^2', 'graph sin(x)', 'graph 1/x', 'graph y = 2x + 1', 'graph f(x) = x^3 - x', 'graph tan(x)', 'graph e^x',
    'graph ln(x)', 'graph sqrt(x)', 'graph abs(x)', 'graph x^2 + y^2 = 4', 'graph sin(x)/x', 'graph floor(x)']
  for (const g of inputs) {
    buildGraph(g)
    buildGraph(g, { angleMode: 'rad' })
  }
})

const evalNonBlank = (options: object): TaggedFn => {
  const ok = ((text: string) => Boolean(evaluateSheet([text], options)[0]?.display)) as TaggedFn
  ok.__golden = { kind: 'evalNonBlank', options }
  return ok
}

it('answer forms, blank reasons, completion and typst', () => {
  const r = rng(29)
  const inputs = ['1/3', '2/3', 'sqrt(2)', 'sin(60)', 'pi/4', '0.125', '1234567', '1e-5', '5 kg', '26.5 lb', '100 °C', '3 m * 4 m',
    'sqrt(8)', '1/sqrt(2)', 'cos(45)', 'tan(30)', '22/7', '0.1+0.2', '2^0.5', 'ln(2)']
  for (const sigFigs of [6, 12]) {
    for (const text of inputs) {
      const res = evaluateSheet([text], { sigFigs })[0]!
      if (res.value) answerForms({ value: res.value, display: res.display, exact: res.exact, meas: res.meas, sigFigs, rationalize: true })
    }
  }
  const ok = evalNonBlank({})
  for (const text of ['2+', 'sin(', '5 kg + 3 m', 'foo + 1', '2 3 +', 'sqrt(-1', 'x + 1', '1/0', 'hello world', 'log(-1)', '5 m to kg']) {
    blankReason(text, ok)
  }
  for (const word of [...COMPLETION_FUNCTIONS, ...COMPLETION_UNITS]) {
    for (let k = 1; k < Math.min(word.length, 5); k++) completionFor(`2 ${word.slice(0, k)}`)
  }
  for (let i = 0; i < 200; i++) completionFor(pick(r, ['', '3 ', 'x + ', 'sin(']) + pick(r, ['s', 'co', 'ki', 'me', 'gr', 'x', 'an']))
  for (const text of [...inputs, 'x^2 + 1', 'sqrt(x+1)/2', 'sum from n = 1 to 10 of n', '∫0..1 x^2', 'd/dx sin(x)', '[[1,2],[3,4]]']) {
    toTypstMath(text)
    typstDocument(text, '1')
  }
})
