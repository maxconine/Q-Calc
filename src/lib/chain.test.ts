import { describe, expect, it } from 'vitest'
import { evaluateLine } from '../engine/evaluate'
import { chainedExpr, chainedHistoryExpr, chainsFromAnswer } from './chain'

const num = { plain: '26', unit: false }
const lb = { plain: '26.5 lb', unit: true }

describe('chainsFromAnswer', () => {
  it('chains a leading operator', () => {
    for (const q of ['*2', '× 2', '/3', '÷3', '^2', '+5', '!', '- 5']) expect(chainsFromAnswer(q, num)).toBe(true)
  })

  it('keeps a leading minus as a negative number', () => {
    for (const q of ['-3', '-', '-(2+3)', '−3', '-x']) expect(chainsFromAnswer(q, num)).toBe(false)
  })

  it('needs a last answer', () => {
    expect(chainsFromAnswer('*2', undefined)).toBe(false)
    expect(chainsFromAnswer('*2', { plain: '', unit: false })).toBe(false)
  })

  it('leaves ordinary input alone', () => {
    for (const q of ['2*3', 'sqrt(2)', '', ' ', 'in', 'inch', '%5']) expect(chainsFromAnswer(q, lb)).toBe(false)
  })

  it('converts a unit answer with a leading in or to', () => {
    expect(chainsFromAnswer('in kg', lb)).toBe(true)
    expect(chainsFromAnswer('to kg', lb)).toBe(true)
    expect(chainsFromAnswer('in kg', num)).toBe(false)
  })
})

describe('chainedExpr', () => {
  it('evaluates through ans at full precision', () => {
    expect(evaluateLine(chainedExpr('*3'), { ans: 1 / 3 }).display).toBe('1')
    expect(evaluateLine(chainedExpr('^2'), { ans: -3 }).display).toBe('9')
    expect(evaluateLine(chainedExpr('- 5'), { ans: 2 }).display).toBe('-3')
    expect(evaluateLine(chainedExpr('!'), { ans: 4 }).display).toBe('24')
  })

  it('keeps the unit', () => {
    const quantities = { ans: '26.5 lb' }
    expect(evaluateLine(chainedExpr('*2'), { quantities }).display).toBe('53 lbs')
    expect(evaluateLine(chainedExpr('in kg'), { quantities }).value?.unit).toBe('kg')
  })
})

describe('chainedHistoryExpr', () => {
  it('writes the answer out, grouped where precedence needs it', () => {
    expect(chainedHistoryExpr('*2', '26')).toBe('26*2')
    expect(chainedHistoryExpr('^2', '-3')).toBe('(-3)^2')
    expect(chainedHistoryExpr('^2', 'sqrt(3)/2')).toBe('(sqrt(3)/2)^2')
    expect(chainedHistoryExpr('/2', '26.5 lb')).toBe('(26.5 lb)/2')
    expect(chainedHistoryExpr('+ 1', 'sqrt(3)/2')).toBe('sqrt(3)/2+ 1')
    expect(chainedHistoryExpr(' in kg', '26.5 lb')).toBe('26.5 lb in kg')
  })

  it('saves an expression that gives the same answer', () => {
    expect(evaluateLine(chainedHistoryExpr('^2', '-3')).display).toBe('9')
    expect(evaluateLine(chainedHistoryExpr('*2', '26.5 lb')).display).toBe(evaluateLine('26.5 lb * 2').display)
  })
})
