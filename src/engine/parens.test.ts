import { describe, expect, it } from 'vitest'
import { evaluateLine, evaluateSheet } from './evaluate'
import { autofillParens, fillParens } from './parens'

type Case = { name: string; input: string; filled: string }

function suite(title: string, cases: Case[]) {
  describe(title, () => {
    it.each(cases)('$name', ({ input, filled }) => {
      expect(fillParens(input), input).toBe(filled)
    })
  })
}

suite('Auto-Prepend Opening Parentheses (Leading Missing ()', [
  { name: 'TC-01 Mid-expression closing parenthesis prepends ( to start', input: '5 + 3) * 2', filled: '(5 + 3) * 2' },
  { name: 'TC-02 Trailing closing parenthesis prepends ( to start', input: '100 / 2 + 5)', filled: '(100 / 2 + 5)' },
  { name: 'TC-03 Exponentiation before excess closing parenthesis', input: '4^2) - 1', filled: '(4^2) - 1' },
  { name: 'TC-04 Closing parenthesis placed at character index 0', input: ')5 + 3(', filled: '()5 + 3()' },
  { name: 'TC-05 Implicit multiplication immediately following excess )', input: '300 * 9)2', filled: '(300 * 9)2' },
  { name: 'TC-06 Prepend ( before first operand when operator follows )', input: '10 + 20) / 5', filled: '(10 + 20) / 5' },
  { name: 'TC-07 Prepend ( across multi-term expressions', input: '1 + 2) * 3 + 4', filled: '(1 + 2) * 3 + 4' },
  { name: 'TC-08 Decimal numbers inside prepended group', input: '0.5 + 2.5) * 4', filled: '(0.5 + 2.5) * 4' },
  { name: 'TC-09 Unary negative operand inside prepended group', input: '2 * -3)', filled: '(2 * -3)' },
  { name: 'TC-10 Postfix percentage symbol immediately after excess )', input: '100)%', filled: '(100)%' },
])

suite('Auto-Append Closing Parentheses (Trailing Missing ))', [
  { name: 'TC-11 Single unclosed ( at start appends ) at end', input: '(300 * 9', filled: '(300 * 9)' },
  { name: 'TC-12 Unclosed ( mid-expression appends ) at end', input: '2 * (5 + 3', filled: '2 * (5 + 3)' },
  { name: 'TC-13 Unclosed ( spanning multiple arithmetic operations', input: '(10 + 20 / 5', filled: '(10 + 20 / 5)' },
  { name: 'TC-14 First group balanced, second group appends trailing )', input: '(4 + 5) * (2 + 3', filled: '(4 + 5) * (2 + 3)' },
  { name: 'TC-15 Unclosed denominator group appends ) at end', input: '10 / (2 + 3', filled: '10 / (2 + 3)' },
  { name: 'TC-16 Unclosed group containing floating-point numbers', input: '(0.5 + 0.25', filled: '(0.5 + 0.25)' },
  { name: 'TC-17 Unclosed group starting with unary minus', input: '(-5 + 3', filled: '(-5 + 3)' },
  { name: 'TC-18 Unclosed group containing exponents', input: '(2^3 + 1', filled: '(2^3 + 1)' },
  { name: 'TC-19 Term-level unclosed opening parenthesis', input: '100 + (200', filled: '100 + (200)' },
  { name: 'TC-20 Unclosed ( at terminal position appends )', input: '(3 + 4) * (', filled: '(3 + 4) * ()' },
])

suite('Bidirectional Auto-Fill (Simultaneous Prepend & Append)', [
  { name: 'TC-21 Prepends leading ( for early ) AND appends trailing ) for late (', input: '3 + 4) * 2 + (5', filled: '(3 + 4) * 2 + (5)' },
  { name: 'TC-22 Single operand prepended, separate term appended', input: '5) * 2 + (3', filled: '(5) * 2 + (3)' },
  { name: 'TC-23 Division block prepended, multiplication block appended', input: '50 / 2) + 10 * (3', filled: '(50 / 2) + 10 * (3)' },
  { name: 'TC-24 Adjacent grouped factors balanced at both outer edges', input: '1 + 2) * (3 + 4', filled: '(1 + 2) * (3 + 4)' },
  { name: 'TC-25 Prepend ( at index 0, append ) at terminal end', input: '10) + 20 * (5 + 2', filled: '(10) + 20 * (5 + 2)' },
  { name: 'TC-26 Middle pair balanced; leading prepended and trailing appended', input: '2) * (3) + (4', filled: '(2) * (3) + (4)' },
  { name: 'TC-27 Prepend around numerator, append around denominator', input: '100) / (2 + 3', filled: '(100) / (2 + 3)' },
  { name: 'TC-28 Asymmetric operators with bidirectional missing bounds', input: '4) + 5 * (6', filled: '(4) + 5 * (6)' },
  { name: 'TC-29 Decimal expressions with bidirectional bounds', input: '0.5) * (1.5', filled: '(0.5) * (1.5)' },
  { name: 'TC-30 Leading term requires (, trailing term requires )', input: '8) + (2 * 3', filled: '(8) + (2 * 3)' },
])

suite('Deep Nesting & Multiple Missing Parentheses (Depth |Δ| > 1)', [
  { name: 'TC-31 Depth -2: Prepends (( at start', input: '3 + 4))', filled: '((3 + 4))' },
  { name: 'TC-32 Depth -3: Prepends ((( at start', input: '1 + 2)))', filled: '(((1 + 2)))' },
  { name: 'TC-33 Depth +2: Appends )) at end', input: '((10 + 20', filled: '((10 + 20))' },
  { name: 'TC-34 Depth +3: Appends ))) at end', input: '(((3 + 4', filled: '(((3 + 4)))' },
  { name: 'TC-35 Cumulative depth reaching -3 across multiple terms', input: '1 + 2)) * 3 + 4)', filled: '(((1 + 2)) * 3 + 4)' },
  { name: 'TC-36 Nested opening parentheses needing 3 closing parens at end', input: '((1 + (2 + 3', filled: '((1 + (2 + 3)))' },
  { name: 'TC-37 Depth -2 early, depth +1 late', input: '100)) + 2 * (5', filled: '((100)) + 2 * (5)' },
  { name: 'TC-38 Inner pair balanced, outer net depth prepends ( and appends )', input: '(1 + 2)) * (3 + 4', filled: '((1 + 2)) * (3 + 4)' },
  { name: 'TC-39 Deeply nested group missing 2 opening parentheses at start', input: '((3 + 4) * 2)))', filled: '((((3 + 4) * 2)))' },
  { name: 'TC-40 Partially closed nest (depth +1 remaining) appends single )', input: '(((1 + 2))', filled: '(((1 + 2)))' },
])

suite('Functions & Implicit Multiplication', [
  { name: 'TC-41 Function parens balanced internally, excess outer ) prepends (', input: 'sin(30)) + 1', filled: '(sin(30)) + 1' },
  { name: 'TC-42 Unclosed function argument auto-appends )', input: 'sin(30 + 10', filled: 'sin(30 + 10)' },
  { name: 'TC-43 Implicit multiplication before unclosed paren appends )', input: '2(3 + 4', filled: '2(3 + 4)' },
  { name: 'TC-44 Prepend ( to entire expression containing standard function call', input: 'sqrt(16) / 2)', filled: '(sqrt(16) / 2)' },
  { name: 'TC-45 Closed function followed by unclosed term appends )', input: 'log(100) + (5 * 2', filled: 'log(100) + (5 * 2)' },
  { name: 'TC-51 Unclosed log argument auto-appends )', input: 'log(2', filled: 'log(2)' },
  { name: 'TC-52 Unclosed sqrt argument auto-appends )', input: 'sqrt(2', filled: 'sqrt(2)' },
  { name: 'TC-53 Unclosed ln argument auto-appends )', input: 'ln(2', filled: 'ln(2)' },
])

suite('Boundary Conditions, Special Symbols & Control Cases', [
  { name: 'TC-46 Standalone closing parenthesis prepends (', input: ')', filled: '()' },
  { name: 'TC-47 Standalone opening parenthesis appends )', input: '(', filled: '()' },
  { name: 'TC-48 Isolated unmatched parens auto-fill into two empty groups', input: ') + (', filled: '() + ()' },
  { name: 'TC-49 Control: Valid expression without parens remains unchanged', input: '300 * 9', filled: '300 * 9' },
  { name: 'TC-50 Control: Fully balanced expression remains unchanged', input: '(300 * 9)', filled: '(300 * 9)' },
])

describe('Inferred parens evaluate', () => {
  it('evaluates 300*9)2 as (300*9)2', () => {
    expect(evaluateLine('300*9)2').value?.n).toBe(5400)
    expect(evaluateLine('300 * 9)2').value?.n).toBe(5400)
  })

  it('evaluates prepended and appended groups', () => {
    expect(evaluateLine('5 + 3) * 2').value?.n).toBe(16)
    expect(evaluateLine('100 / 2 + 5)').value?.n).toBe(55)
    expect(evaluateLine('(300 * 9').value?.n).toBe(2700)
    expect(evaluateLine('2 * (5 + 3').value?.n).toBe(16)
    expect(evaluateLine('1 + 2) * (3 + 4').value?.n).toBe(21)
    expect(evaluateLine('sin(30)) + 1').value?.n).toBe(1.5)
    expect(evaluateLine('log(2').value?.n).toBeCloseTo(Math.log10(2), 8)
    expect(evaluateLine('sqrt(2').value?.n).toBeCloseTo(Math.sqrt(2), 8)
    expect(evaluateLine('ln(2').value?.n).toBeCloseTo(Math.log(2), 8)
  })

  it('leaves already-valid expressions unchanged', () => {
    expect(evaluateLine('300 * 9').value?.n).toBe(2700)
    expect(evaluateLine('(300 * 9)').value?.n).toBe(2700)
    expect(evaluateLine('(5 + 3) * 4').value?.n).toBe(32)
  })

  it('divides a compound unit quantity by a trailing )/2 using inferred opening parens', () => {
    const inferred = evaluateLine('19600 kg m / s)/2')
    const grouped = evaluateLine('(19600 kg m / s)/2')
    expect(inferred.value?.n).toBeCloseTo(9800, 8)
    expect(grouped.value?.n).toBeCloseTo(9800, 8)
    expect(inferred.display).toMatch(/kg m \/ s$/)
    expect(grouped.display).toMatch(/kg m \/ s$/)
    expect(evaluateLine('19600 kg m / s').value?.n).toBeCloseTo(19600, 8)
  })

  it('divides force as (19600 kg m / s^2)/2', () => {
    const r = evaluateLine('19600 kg m / s^2)/2')
    expect(r.value?.n).toBeCloseTo(9800, 8)
    expect(r.display).toMatch(/N$/)
    expect(evaluateLine('19600 N)/2').value?.n).toBeCloseTo(9800, 8)
  })

  it('applies inferred parens before unit arithmetic', () => {
    expect(evaluateLine('10 m)/2').value?.n).toBeCloseTo(5, 8)
    expect(evaluateLine('10 m)/2').display).toMatch(/m$/)
    expect(evaluateLine('100 kg)/2').value?.n).toBeCloseTo(50, 8)
    expect(evaluateLine('2 in)/2').value?.n).toBeCloseTo(evaluateLine('(2 in)/2').value?.n ?? NaN, 8)
  })
})

const ATOMS = [
  '1 + 2',
  '3 * 4',
  '10 - 6',
  '8 / 2',
  '9 + 0.5',
  '7 * -2',
  '100 / 4',
  '2^3',
  '0.25 + 0.75',
  '11 - 3 + 1',
  '5 * 5',
  '12 / 3',
  '4 + 4 * 2',
  '6 - 1',
  '9 * 1',
  '15 / 5',
  '2 + 2 + 2',
  '3^2',
  '8 - 8',
  '14 + 7',
]

suite(
  'Auto-Prepend Opening Parentheses (Leading Missing ()',
  ATOMS.flatMap((expr, i) => [
    { name: `pre ${i}a`, input: `${expr})`, filled: `(${expr})` },
    { name: `pre ${i}b`, input: `${expr}) * 2`, filled: `(${expr}) * 2` },
    { name: `pre ${i}c`, input: `${expr}) / 2`, filled: `(${expr}) / 2` },
    { name: `pre ${i}d`, input: `${expr}) + 1`, filled: `(${expr}) + 1` },
    { name: `pre ${i}e`, input: `${expr})2`, filled: `(${expr})2` },
  ]),
)

suite(
  'Auto-Append Closing Parentheses (Trailing Missing ))',
  ATOMS.flatMap((expr, i) => [
    { name: `app ${i}a`, input: `(${expr}`, filled: `(${expr})` },
    { name: `app ${i}b`, input: `2 * (${expr}`, filled: `2 * (${expr})` },
    { name: `app ${i}c`, input: `2 + (${expr}`, filled: `2 + (${expr})` },
    { name: `app ${i}d`, input: `(${expr} + 1`, filled: `(${expr} + 1)` },
    { name: `app ${i}e`, input: `10 / (${expr}`, filled: `10 / (${expr})` },
  ]),
)

suite(
  'Bidirectional Auto-Fill (Simultaneous Prepend & Append)',
  ATOMS.flatMap((expr, i) => {
    const other = ATOMS[(i + 3) % ATOMS.length]!
    return [
      { name: `bi ${i}a`, input: `${expr}) * (${other}`, filled: `(${expr}) * (${other})` },
      { name: `bi ${i}b`, input: `${expr}) + (${other}`, filled: `(${expr}) + (${other})` },
      { name: `bi ${i}c`, input: `${expr}) / (${other}`, filled: `(${expr}) / (${other})` },
      { name: `bi ${i}d`, input: `${expr}) - (${other}`, filled: `(${expr}) - (${other})` },
      { name: `bi ${i}e`, input: `${expr}) + 1 * (${other}`, filled: `(${expr}) + 1 * (${other})` },
    ]
  }),
)

suite(
  'Deep Nesting & Multiple Missing Parentheses (Depth |Δ| > 1)',
  ATOMS.flatMap((expr, i) => [
    { name: `deep ${i}a`, input: `${expr}))`, filled: `((${expr}))` },
    { name: `deep ${i}b`, input: `((${expr}`, filled: `((${expr}))` },
    { name: `deep ${i}c`, input: `${expr})))`, filled: `(((${expr})))` },
    { name: `deep ${i}d`, input: `(((${expr}`, filled: `(((${expr})))` },
    { name: `deep ${i}e`, input: `((${expr})))`, filled: `(((${expr})))` },
  ]),
)

suite(
  'Functions & Implicit Multiplication',
  ATOMS.flatMap((expr, i) => [
    { name: `fn ${i}a`, input: `sin(${expr}`, filled: `sin(${expr})` },
    { name: `fn ${i}b`, input: `cos(${expr}`, filled: `cos(${expr})` },
    { name: `fn ${i}c`, input: `sqrt(${expr}`, filled: `sqrt(${expr})` },
    { name: `fn ${i}d`, input: `log(${expr}`, filled: `log(${expr})` },
    { name: `fn ${i}e`, input: `ln(${expr}`, filled: `ln(${expr})` },
    { name: `fn ${i}f`, input: `2(${expr}`, filled: `2(${expr})` },
  ]),
)

suite(
  'Boundary Conditions, Special Symbols & Control Cases',
  [
    ...ATOMS.map((expr, i) => ({ name: `ctrl balanced ${i}`, input: `(${expr})`, filled: `(${expr})` })),
    ...ATOMS.map((expr, i) => ({ name: `ctrl bare ${i}`, input: expr, filled: expr })),
    ...ATOMS.map((expr, i) => ({ name: `ctrl empty close ${i}`, input: `) + ${expr}`, filled: `() + ${expr}` })),
    ...ATOMS.map((expr, i) => ({ name: `ctrl empty open ${i}`, input: `${expr} + (`, filled: `${expr} + ()` })),
    { name: 'empty pair', input: '()', filled: '()' },
    { name: 'double empty', input: '()()', filled: '()()' },
    { name: 'nested empty', input: '(())', filled: '(())' },
    { name: 'triple empty open', input: '(((', filled: '((()))' },
    { name: 'triple empty close', input: ')))', filled: '((()))' },
    { name: 'mixed empty', input: ')()', filled: '()()' },
    { name: 'plus empties', input: ') + )', filled: '(() + )' },
    { name: 'times empties', input: ') * (', filled: '() * ()' },
    { name: 'minus empties', input: ') - (', filled: '() - ()' },
    { name: 'div empties', input: ') / (', filled: '() / ()' },
    { name: 'nested trailing', input: '(()', filled: '(())' },
    { name: 'nested leading', input: '())', filled: '(())' },
    { name: 'four open', input: '((((', filled: '(((())))' },
    { name: 'four close', input: '))))', filled: '(((())))' },
    { name: 'two close one open', input: '))(', filled: '(())()' },
    { name: 'open close open', input: '()(', filled: '()()' },
  ],
)

describe('Right-arrow autofill at caret end', () => {
  it('commits trailing inferred parens', () => {
    expect(autofillParens('sin(90', 6)).toBe('sin(90)')
    expect(autofillParens('((1 + 2', 7)).toBe('((1 + 2))')
  })

  it('commits leading inferred parens', () => {
    expect(autofillParens('5 + 3)', 6)).toBe('(5 + 3)')
  })

  it('commits leading and trailing together', () => {
    expect(autofillParens('3 + 4) * (5', 11)).toBe('(3 + 4) * (5)')
  })

  it('does nothing when balanced, mid-field, or selecting', () => {
    expect(autofillParens('sin(90)', 7)).toBeNull()
    expect(autofillParens('sin(90', 3)).toBeNull()
    expect(autofillParens('sin(90', 0, 6)).toBeNull()
    expect(autofillParens('', 0)).toBeNull()
  })
})

describe('Inferred parens evaluate', () => {
  it.each(
    ATOMS.flatMap((expr, i) => [
      { name: `eval pre ${i}`, input: `${expr})` },
      { name: `eval app ${i}`, input: `(${expr}` },
      { name: `eval both ${i}`, input: `${expr}) * (1` },
      { name: `eval fn ${i}`, input: `abs(${expr}` },
      { name: `eval add ${i}`, input: `${expr}) + 1` },
      { name: `eval mul ${i}`, input: `2*(${expr}` },
    ]),
  )('$name', ({ input }) => {
    expect(() => evaluateLine(input)).not.toThrow()
    const filled = fillParens(input)
    const a = evaluateLine(input).value?.n
    const b = evaluateLine(filled).value?.n
    if (a != null && b != null) expect(a).toBeCloseTo(b, 8)
  })
})

function line(text: string) {
  return evaluateLine(text, { angleMode: 'deg' })
}

describe('parentheses and glued input', () => {
  it.each([
    ['sin(30', '0.5'],
    ['(2+3', '5'],
    ['5+3)*2', '16'],
    ['2(3+4)', '14'],
    ['(2+3)(4+1)', '25'],
    ['2sin(90)', '2'],
    ['2sqrt(9)', '6'],
    ['sqrt(2', line('sqrt(2)').display],
    ['((1+2)*3', '9'],
    ['(3+1)!', '24'],
    ['5(2+1)', '15'],
    ['2(3)(4)', '24'],
    ['log 100', '2'],
    ['sin 90', '1'],
  ])('%s → %s', (text, display) => {
    expect(line(text).display).toBe(display)
  })

  it('a stored letter glues to a coefficient and to a function', () => {
    const rows = evaluateSheet(['x = 4', '3x', '2sinx'], { angleMode: 'deg' })
    expect(rows[1]!.display).toBe('12')
    expect(rows[2]!.value!.n).toBeCloseTo(2 * Math.sin((4 * Math.PI) / 180), 10)
  })

  it('fill only adds the missing ends, and autofill waits for the caret at the end', () => {
    expect(fillParens('sin(30')).toBe('sin(30)')
    expect(fillParens('5+3)*2')).toBe('(5+3)*2')
    expect(fillParens(')(')).toBe('()()')
    expect(autofillParens('sin(30', 6)).toBe('sin(30)')
    expect(autofillParens('sin(30', 3)).toBeNull()
    expect(autofillParens('(2+3)', 5)).toBeNull()
  })

  it('a trailing operator is still unfinished', () => {
    expect(line('sin(30+').display).toBe('')
    expect(line('2+').display).toBe('')
  })

  it('words that only look glued stay words', () => {
    expect(line('cost')).toBeTruthy()
    const rows = evaluateSheet(['cost = 5', 'cost'], { angleMode: 'deg' })
    expect(rows[1]!.display).toBe('5')
  })
})
