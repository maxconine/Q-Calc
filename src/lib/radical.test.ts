import { describe, expect, it } from 'vitest'
import { evaluateLine, evaluateSheet } from '../engine/evaluate'
import { radicalAnswer, radicalSpans } from './radical'

// the text under each bar
function barred(text: string): string[] {
  return radicalSpans(text).map((s) => text.slice(s.sign + 1, s.end))
}

describe('radicalSpans', () => {
  it.each([
    ['√91', ['91']],
    ['√(2+3)', ['(2+3)']],
    ['√2x', ['2']],
    ['2√3+1', ['3']],
    ['√(1+√2)', ['(1+√2)', '2']],
    ['∛27', ['27']],
    ['∛8x', ['8']],
    ['√(2+', ['(2+']],
    ['√(2+(3', ['(2+(3']],
    ['√ 9', [' 9']],
    ['√2π', ['2π']],
    ['√2*π', ['2*π']],
    ['√2 π', ['2']],
    ['√2πx', ['2']],
    ['√2π√4', ['2', '4']],
    ['√πx', ['π']],
    ['√π', ['π']],
    ['√2^3', ['2^3']],
    ['√2^-1', ['2^-1']],
    ['√2^(1/2)', ['2^(1/2)']],
    ['√2^2^2', ['2^2^2']],
    ['√4!', ['4']],
    ['√4%', ['4%']],
    ['√2.5', ['2.5']],
    ['√1e4', ['1e4']],
    ['√2e', ['2']],
    ['√2(3)', ['2']],
    ['√16√16', ['16', '16']],
    ['√3/2', ['3']],
    ['√(4)^2', ['(4)']],
    ['√x', ['x']],
    ['√e', ['e']],
  ])('%s bars %j', (text, expected) => {
    expect(barred(text)).toEqual(expected)
  })

  it.each(['√', '√-4', '√.5', '√pi', 'sqrt(4)'])('%s gets no bar: the engine reads no radicand there', (text) => {
    expect(barred(text)).toEqual([])
  })
})

// the bar must never mislead: wrapping what it covers in parens can't change the answer
describe('the bar matches what the engine reads', () => {
  const heads = ['√', '∛', '2√', '1+√']
  const radicands = ['91', '2', '2.5', '1e4', 'π', '2π', '2*π', '2pi', '2^3', '2^-1', '2^(1/2)', '4%', '16', '2^2^2', '9', '2^π', '2τ', '2^2π', '3^(1+1)', '2 ^ 3']
  const tails = ['', 'x', '!', ' π', 'π', '+1', '/2', '(3)', ' 3', 'e', '^2', '*2', '√4', '%', 'τ', '°', '^π', '^(2)', '2', '.5', 'π^2', '∞', 'θ']
  const cases: string[] = []
  for (const h of heads) for (const r of radicands) for (const t of tails) cases.push(h + r + t)
  const last = (text: string) => evaluateSheet(['x = 4', text])[1]!.display

  it.each(cases)('%s', (text) => {
    const spans = radicalSpans(text)
    const first = spans[0]!
    expect(first).toBeDefined()
    const inner = text.slice(first.sign + 1, first.end)
    const wrapped = `${text.slice(0, first.sign + 1)}(${inner})${text.slice(first.end)}`
    const a = last(text)
    if (!a) return
    expect(last(wrapped)).toBe(a)
  })
})

describe('a letter under the root', () => {
  const last = (text: string) => evaluateSheet(['x = 9', 'y = 4', text]).at(-1)!.display

  it.each(['√xy', '√x y', '√xy+1', '√x^3', '√x!', '√xπ', '∛xy', '√x/2', '2√x', '√x%'])('%s: the bar covers x alone, as the engine reads it', (text) => {
    const first = radicalSpans(text)[0]!
    expect(text.slice(first.sign + 1, first.end)).toBe('x')
    const wrapped = `${text.slice(0, first.sign + 1)}(x)${text.slice(first.end)}`
    expect(last(text)).not.toBe('')
    expect(last(wrapped)).toBe(last(text))
  })

  it('draws no bar under a function name', () => {
    expect(radicalSpans('√sin(x)')).toEqual([])
    expect(radicalSpans('√x2')).toEqual([])
  })
})

describe('radicalAnswer', () => {
  it.each([
    ['sqrt(3)', '√3'],
    ['2sqrt(3)/3', '2√3/3'],
    ['-sqrt(2)/2', '-√2/2'],
    ['−sqrt(2)/2', '−√2/2'],
    ['sqrt(10)/2', '√10/2'],
    ['5sqrt(41)/41', '5√41/41'],
    ['sqrt(2 + sqrt(3))', '√(2 + √3)'],
    ['sqrt(2)π', '√(2)π'],
    ['sqrt(2)pi', 'sqrt(2)pi'],
    ['sqrt(2)^3', '√(2)^3'],
    ['cbrt(2)', '∛2'],
    ['sqrt(3) + 1', '√3 + 1'],
    ['9.53939201417', '9.53939201417'],
    ['sqrt(x)', 'sqrt(x)'],
    ['5 cm', '5 cm'],
  ])('%s reads %s', (s, expected) => {
    expect(radicalAnswer(s)).toBe(expected)
  })

  it.each(['sqrt(3)', '2sqrt(3)/3', '-sqrt(2)/2', 'sqrt(10)/2', '5sqrt(41)/41', 'sqrt(2 + sqrt(3))', 'sqrt(2)π', 'sqrt(2)^3', 'cbrt(2)', '3sqrt(2) + 1'])(
    '%s keeps its value when shown with √',
    (s) => {
      expect(evaluateLine(radicalAnswer(s)).display).toBe(evaluateLine(s).display)
    },
  )
})
