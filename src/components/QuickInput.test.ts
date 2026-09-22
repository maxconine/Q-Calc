import { describe, expect, it } from 'vitest'
import { evaluateLine } from '../engine/evaluate'
import { flattenPastedText, prettyTokens, spliceText } from './QuickInput'

function n(text: string): number {
  const r = evaluateLine(text)
  if (r.value == null || !Number.isFinite(r.value.n)) {
    throw new Error(`No numeric result for ${JSON.stringify(text)} → ${r.display} ${r.error ?? ''}`)
  }
  return r.value.n
}

describe('flattenPastedText', () => {
  it('keeps a single line', () => {
    expect(flattenPastedText('sin(90)')).toBe('sin(90)')
  })

  it('turns line breaks into spaces', () => {
    expect(flattenPastedText('2+2\n3+3')).toBe('2+2 3+3')
    expect(flattenPastedText('a\r\nb\rc')).toBe('a b c')
  })
})

describe('prettyTokens', () => {
  it('replaces typed pi with π', () => {
    expect(prettyTokens('sin(pi/2)')).toBe('sin(π/2)')
    expect(prettyTokens('2 * pi')).toBe('2 * π')
  })

  it('leaves latex \\dot alone', () => {
    expect(prettyTokens('\\dot{x}')).toBe('\\dot{x}')
  })

  it('replaces typed ans with the previous answer as shown', () => {
    expect(prettyTokens('ans * 2', '3.142')).toBe('3.142 * 2')
    expect(prettyTokens('ANS', '3.14159')).toBe('3.14159')
  })
})

const DOT_CASES: Array<{ name: string; input: string; rewritten: string; expected: number }> = [
  { name: 'glued digits', input: '4dot1', rewritten: '4*1', expected: 4 },
  { name: 'glued 3dot4', input: '3dot4', rewritten: '3*4', expected: 12 },
  { name: 'uppercase glued', input: '4DOT1', rewritten: '4*1', expected: 4 },
  { name: 'mixed-case glued', input: '4Dot1', rewritten: '4*1', expected: 4 },
  { name: 'spaced word', input: '4 dot 1', rewritten: '4 * 1', expected: 4 },
  { name: 'space after', input: '4dot 1', rewritten: '4* 1', expected: 4 },
  { name: 'space before', input: '4 dot1', rewritten: '4 *1', expected: 4 },
  { name: 'parens glued', input: '(1+2)dot4', rewritten: '(1+2)*4', expected: 12 },
  { name: 'chained glued', input: '2dot3dot4', rewritten: '2*3*4', expected: 24 },
  { name: 'decimal left', input: '0.5dot8', rewritten: '0.5*8', expected: 4 },
  { name: 'add after mul', input: '2dot3+4', rewritten: '2*3+4', expected: 10 },
  { name: 'two-digit factors', input: '10dot10', rewritten: '10*10', expected: 100 },
  { name: 'negative paren', input: '(-2)dot5', rewritten: '(-2)*5', expected: -10 },
  { name: 'power then mul', input: '2^3dot4', rewritten: '2^3*4', expected: 32 },
  { name: 'mul then div', input: '6dot7/2', rewritten: '6*7/2', expected: 21 },
  { name: 'inside sin degrees', input: 'sin(2dot45)', rewritten: 'sin(2*45)', expected: 1 },
  { name: 'times power of ten', input: '3dot10^2', rewritten: '3*10^2', expected: 300 },
  { name: 'times zero', input: '12dot0', rewritten: '12*0', expected: 0 },
  { name: 'grouped then add', input: '(4dot1)+2', rewritten: '(4*1)+2', expected: 6 },
  { name: 'decimal right', input: '100dot0.5', rewritten: '100*0.5', expected: 50 },
]

describe('word-dot multiplication', () => {
  it.each(DOT_CASES)('$name: $input → $rewritten', ({ input, rewritten, expected }) => {
    expect(prettyTokens(input)).toBe(rewritten)
    expect(n(input)).toBeCloseTo(expected)
  })
})

describe('spliceText', () => {
  it('inserts a history answer at the caret', () => {
    expect(spliceText('cos(', '31', 4, 4)).toEqual({ next: 'cos(31', cursor: 6 })
  })

  it('inserts a history expression at the caret', () => {
    expect(spliceText('4*', '2+3', 2, 2)).toEqual({ next: '4*2+3', cursor: 5 })
    expect(spliceText('sin(', '2+3', 4, 4)).toEqual({ next: 'sin(2+3', cursor: 7 })
    expect(spliceText('cos()', 'pi/6', 4, 4)).toEqual({ next: 'cos(pi/6)', cursor: 8 })
  })

  it('replaces a selection', () => {
    expect(spliceText('cos(x)', '31', 4, 5)).toEqual({ next: 'cos(31)', cursor: 6 })
  })
})
