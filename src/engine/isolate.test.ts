import { describe, expect, it } from 'vitest'
import { evaluateLine } from './evaluate'
import { isolatePrevious, isolateVariable, parseIsolate } from './isolate'

const iso = (s: string) => isolateVariable(s)?.display

describe('parseIsolate', () => {
  it('reads the leading and trailing forms', () => {
    expect(parseIsolate('isolate x in a*x + b = c')).toEqual({ variable: 'x', lhs: 'a*x + b', rhs: 'c' })
    expect(parseIsolate('isolate h: v = sqrt(2gh)')).toEqual({ variable: 'h', lhs: 'v', rhs: 'sqrt(2gh)' })
    expect(parseIsolate('PV = nRT isolate T')).toEqual({ variable: 'T', lhs: 'PV', rhs: 'nRT' })
    expect(parseIsolate('E = mc^2, isolate c')).toEqual({ variable: 'c', lhs: 'E', rhs: 'mc^2' })
  })
  it('sets a lone expression to 0', () => {
    expect(parseIsolate('isolate x in 2x + 3y')).toEqual({ variable: 'x', lhs: '2x + 3y', rhs: '0' })
  })
  it('rejects inequalities and extra =', () => {
    expect(parseIsolate('isolate x in x < y')).toBeNull()
    expect(parseIsolate('isolate x in x = y = z')).toBeNull()
    expect(parseIsolate('isolate x')).toBeNull()
  })
})

describe('isolateVariable', () => {
  it('undoes operations around a single x', () => {
    expect(iso('isolate x in a*x + b = c')).toBe('x = (c - b)/a')
    expect(iso('y = mx + b isolate x')).toBe('x = (y - b)/m')
    expect(iso('isolate m in F = ma')).toBe('m = F/a')
    expect(iso('PV = nRT isolate T')).toBe('T = P*V/(R*n)')
    expect(iso('isolate h: v = sqrt(2gh)')).toBe('h = v^2/(2g)')
    expect(iso('isolate x in y = e^(kx)')).toBe('x = ln(y)/k')
    expect(iso('isolate x in y = 2^x')).toBe('x = ln(y)/ln(2)')
    expect(iso('isolate x in sin(x) = y')).toBe('x = asin(y)')
  })
  it('gives both roots of an even power', () => {
    expect(iso('E = mc^2, isolate c')).toBe('c = ±sqrt(E/m)')
    expect(iso('isolate r in A = pi r^2')).toBe('r = ±sqrt(A/pi)')
  })
  it('collects x when it appears more than once', () => {
    expect(iso('isolate R in 1/R = 1/R1 + 1/R2')).toBe('R = R1*R2/(R2 + R1)')
    expect(iso('isolate x in xy + x = z')).toBe('x = z/(y + 1)')
    expect(iso('isolate x in a x^2 + b x + c = 0')).toBe('x = (-b ± sqrt(b^2 - 4a*c))/(2a)')
    expect(iso('isolate t in d = v t + 1/2 a t^2')).toBe('t = (-v ± sqrt(v^2 + 2a*d))/a')
  })
  it('works for any of the letters', () => {
    expect(iso('isolate y in 3x + 2y = 6')).toBe('y = (6 - 3x)/2')
    expect(iso('isolate x in 3x + 2y = 6')).toBe('x = (6 - 2y)/3')
  })
  it('is null when x is missing or stuck', () => {
    expect(iso('isolate z in a + b = c')).toBeUndefined()
    expect(iso('isolate x in x + sin(x) = y')).toBeUndefined()
    expect(iso('isolate x in x^5 + x = y')).toBeUndefined()
  })
})

describe('isolatePrevious', () => {
  it('reads the equation on the line before', () => {
    expect(isolatePrevious('isolate x', 'y = mx + b')).toBe('isolate x in y = mx + b')
    expect(isolatePrevious('isolate b', 'isolate x in y = mx + b')).toBe('isolate b in y = mx + b')
  })
  it('needs an equation before, and a lone command now', () => {
    expect(isolatePrevious('isolate x', '2 + 3')).toBeNull()
    expect(isolatePrevious('isolate x', undefined)).toBeNull()
    expect(isolatePrevious('isolate x in y = x', 'a = b')).toBeNull()
  })
})

describe('subscripted names', () => {
  it('reads R_eq, R_(eq) and R_{eq} as one name', () => {
    for (const t of ['isolate R_eq in 1/R_eq = 1/R_1 + 1/R_2', 'isolate R_(eq) in 1/R_(eq) = 1/R_(1) + 1/R_(2)', 'isolate R_{eq} in 1/R_{eq} = 1/R_{1} + 1/R_{2}']) {
      expect(iso(t), t).toBe('R_eq = R_1*R_2/(R_2 + R_1)')
    }
  })
  it('takes a line copied as Typst, quoted words and all', () => {
    expect(evaluateLine('"isolate" R_("eq") "in" 1 / R_("eq") = 1 / R_(1) + 1 / R_(2)').display).toBe('R_eq = R_1*R_2/(R_2 + R_1)')
    expect(evaluateLine('"solve" R_("eq") "in" 1 / R_("eq") = 1 / R_(1) + 1 / R_(2)').display).toBe('R_eq = R_1*R_2/(R_2 + R_1)')
  })
  it('solves for a subscripted name with the others stored', () => {
    expect(evaluateLine('solve v_0 in v = v_0 + a t').display).toBe('v_0 = v - a*t')
  })
})

describe('in the sheet', () => {
  it('answers with the rearranged formula', () => {
    const r = evaluateLine('isolate x in a*x + b = c')
    expect(r.display).toBe('x = (c - b)/a')
  })
  it('keeps stored letters symbolic', () => {
    expect(evaluateLine('isolate x in a*x = c', { variables: { a: 2 } }).display).toBe('x = c/a')
  })
  it('is blank when it can not isolate', () => {
    expect(evaluateLine('isolate x in x + sin(x) = y').display).toBe('')
  })
})
