// adversarial coverage for chain-from-last-answer, on top of chain.test.ts
import { describe, expect, it } from 'vitest'
import { evaluateLine } from '../engine/evaluate'
import { chainedExpr, chainedHistoryExpr, chainsFromAnswer } from './chain'

const num = { plain: '26', unit: false }
const lb = { plain: '26.5 lb', unit: true }

describe('chainsFromAnswer: edge punctuation', () => {
  it('a leading × or ÷ glyph chains', () => {
    expect(chainsFromAnswer('×5', num)).toBe(true)
    expect(chainsFromAnswer('÷5', num)).toBe(true)
  })
  it('a bare factorial with nothing else chains', () => {
    expect(chainsFromAnswer('!', num)).toBe(true)
  })
  it('a caret power chains', () => {
    expect(chainsFromAnswer('^3', num)).toBe(true)
  })
  it('two leading minus signs is not a chained subtraction', () => {
    expect(chainsFromAnswer('--3', num)).toBe(false)
  })
  it('a minus with spaces before the number is still a negative number', () => {
    expect(chainsFromAnswer('-   7', num)).toBe(false)
  })
  it('a lone minus sign with no operand does not chain (kept as a fresh negative)', () => {
    expect(chainsFromAnswer('-', num)).toBe(false)
  })
  it('leading whitespace before the operator is tolerated', () => {
    expect(chainsFromAnswer('   *2', num)).toBe(true)
  })
})

describe('chainsFromAnswer: unit conversion gate', () => {
  it('a plain number answer never chains a conversion word', () => {
    expect(chainsFromAnswer('to kg', num)).toBe(false)
  })
  it('an uppercase IN or TO still triggers the conversion (case-insensitive)', () => {
    expect(chainsFromAnswer('IN kg', lb)).toBe(true)
    expect(chainsFromAnswer('TO kg', lb)).toBe(true)
  })
  it('"in" with nothing after it does not chain (still typing)', () => {
    expect(chainsFromAnswer('in', lb)).toBe(false)
    expect(chainsFromAnswer('in ', lb)).toBe(false)
  })
  it('"into" is a different word and must not falsely chain', () => {
    expect(chainsFromAnswer('into kg', lb)).toBe(false)
  })
})

describe('chainedExpr: full precision through ans, more operators', () => {
  it('chains a percent-like caret and division', () => {
    expect(evaluateLine(chainedExpr('/4'), { ans: 10 }).display).toBe('2.5')
  })
  it('chains a factorial of a stored integer answer', () => {
    expect(evaluateLine(chainedExpr('!'), { ans: 5 }).display).toBe('120')
  })
  it('keeps unit division intact', () => {
    const quantities = { ans: '10 m' }
    expect(evaluateLine(chainedExpr('/2'), { quantities }).display).toBe('5 m')
  })
})

describe('chainedHistoryExpr: grouping correctness', () => {
  it('wraps a negative answer before squaring so it does not read as subtraction', () => {
    expect(chainedHistoryExpr('^3', '-2')).toBe('(-2)^3')
  })
  it('does not wrap a plain positive integer answer', () => {
    expect(chainedHistoryExpr('*5', '7')).toBe('7*5')
  })
  it('wraps a unit quantity before dividing', () => {
    expect(chainedHistoryExpr('/4', '10 m')).toBe('(10 m)/4')
  })
  it('a chained addition after a bare number needs no parens', () => {
    expect(chainedHistoryExpr('+3', '7')).toBe('7+3')
  })
  it('a leading + on the query is treated as "loose" and skips wrapping even for an expression', () => {
    expect(chainedHistoryExpr('+1', 'sqrt(3)/2')).toBe('sqrt(3)/2+1')
  })
  it('a to-conversion is joined with a space, not concatenated', () => {
    expect(chainedHistoryExpr(' to kg', '26.5 lb')).toBe('26.5 lb to kg')
  })
  it('produces an expression that gives the same answer as writing it out plainly', () => {
    expect(evaluateLine(chainedHistoryExpr('/4', '10 m')).display).toBe(evaluateLine('10 m / 4').display)
  })
})
