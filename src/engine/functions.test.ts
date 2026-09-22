import { describe, expect, it } from 'vitest'
import { evaluateLine, evaluateSheet, parseFunctionDef } from './evaluate'
import { MAX_LIST_ALLOC } from './scientific'

describe('parseFunctionDef', () => {
  it('parses single- and multi-arg definitions', () => {
    expect(parseFunctionDef('f(x) = x^2')).toEqual({ name: 'f', params: ['x'], body: 'x^2' })
    expect(parseFunctionDef('g(a, b) = a + b')).toEqual({
      name: 'g',
      params: ['a', 'b'],
      body: 'a + b',
    })
    expect(parseFunctionDef('h() = 42')).toEqual({ name: 'h', params: [], body: '42' })
  })

  it('rejects reserved names, bad params, and empty bodies', () => {
    expect(parseFunctionDef('sin(x) = x')).toBeNull()
    expect(parseFunctionDef('f(x, x) = x')).toBeNull()
    expect(parseFunctionDef('f(2) = x')).toBeNull()
    expect(parseFunctionDef('f(pi) = 1')).toBeNull()
    expect(parseFunctionDef('f(x) =')).toBeNull()
    expect(parseFunctionDef('x = 5')).toBeNull()
  })
})

describe('user-defined functions', () => {
  it('evaluates f(x)=x^2 then f(3)', () => {
    const rows = evaluateSheet(['f(x) = x^2', 'f(3)'])
    expect(rows[0]).toMatchObject({
      kind: 'function',
      fnName: 'f',
      fnParams: ['x'],
      fnBody: 'x^2',
      display: 'f(x) = x^2',
    })
    expect(rows[1]?.value?.n).toBe(9)
  })

  it('supports multi-arg calls', () => {
    const rows = evaluateSheet(['add(a, b) = a + b', 'add(2, 3)'])
    expect(rows[1]?.value?.n).toBe(5)
  })

  it('redefines a function', () => {
    const rows = evaluateSheet(['f(x) = x', 'f(x) = x^2', 'f(4)'])
    expect(rows[2]?.value?.n).toBe(16)
  })

  it('seeds functions from options without prior lines', () => {
    const r = evaluateLine('f(5)', {
      functions: { f: { params: ['x'], body: 'x*x' } },
    })
    expect(r.value?.n).toBe(25)
  })

  it('lets functions use outer variables', () => {
    const rows = evaluateSheet(['k = 10', 'f(x) = x + k', 'f(2)'])
    expect(rows[2]?.value?.n).toBe(12)
  })

  it('does not treat reserved-name defs as functions', () => {
    const r = evaluateLine('log(x) = x')
    expect(r.kind).not.toBe('function')
    expect(r.display).toBe('')
  })
})

describe('unit vs variable precedence', () => {
  it('prefers variable n over newton when n is defined', () => {
    const rows = evaluateSheet(['n = 5', 'n*2'])
    expect(rows[1]?.value?.n).toBe(10)
    expect(rows[1]?.value?.unit).toBeUndefined()
  })

  it('still converts bare unit expressions without a matching variable', () => {
    const r = evaluateLine('5 n')
    expect(r.value?.unit).toBeTruthy()
  })
})

describe('clamp and list allocation caps', () => {
  it('implements clamp', () => {
    expect(evaluateLine('clamp(5, 0, 3)').value?.n).toBe(3)
    expect(evaluateLine('clamp(-1, 0, 3)').value?.n).toBe(0)
    expect(evaluateLine('clamp(1.5, 0, 3)').value?.n).toBe(1.5)
    expect(evaluateLine('clamp(5, 3, 0)').value?.n).toBe(3)
  })

  it('caps inclusiveRange and random list size', () => {
    expect(evaluateLine(`[1...${MAX_LIST_ALLOC + 1}]`).display).toBe('')
    expect(evaluateLine(`random(${MAX_LIST_ALLOC + 1})`).display).toBe('')
    expect(evaluateLine(`randint(1, 10, ${MAX_LIST_ALLOC + 1})`).display).toBe('')
    expect(evaluateLine(`[1...${MAX_LIST_ALLOC}]`).display.startsWith('[')).toBe(true)
  })
})
