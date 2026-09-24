// cross-feature adversarial coverage: solve + chain, sums inside solve, chem vs solve/=,
// theta across solve and sums, fraction/sig-fig modes touching each new feature.
import { describe, expect, it } from 'vitest'
import { chainedExpr, chainsFromAnswer } from '../lib/chain'
import { evaluateLine, evaluateSheet } from './evaluate'
import type { EvaluateOptions } from './types'

function line(text: string, opts: EvaluateOptions = {}) {
  return evaluateLine(text, { angleMode: 'deg', ...opts })
}

describe('cross: solve then chain from the last answer', () => {
  it('a single-root solve chains like any other number', () => {
    const first = line('3x = 12')
    expect(first.display).toBe('4')
    expect(chainsFromAnswer('*2', { plain: '4', unit: false })).toBe(true)
    const next = evaluateSheet(['3x = 12', chainedExpr('*2')], { angleMode: 'deg' })
    expect(next[1]!.display).toBe('8')
  })

  it('a multi-root solve is a text message answer, so ans is not a chainable number', () => {
    // 'x^2-5x+6=0' has two roots; the row's value is a text answer ("2, 3"), and ans (the
    // engine's numeric last answer) still only carries the first shown root, not the list
    const rows = evaluateSheet(['x^2-5x+6=0', 'ans'], { angleMode: 'deg' })
    expect(rows[0]!.display).toBe('2, 3')
    expect(rows[0]!.value?.kind).toBe('text')
  })

  it('a message outcome (no real solution) does not leave a usable ans', () => {
    const rows = evaluateSheet(['x^2 = -1', 'ans + 1'], { angleMode: 'deg' })
    expect(rows[0]!.display).toBe('no real solution')
    // ans is whatever it was before (undefined here), so ans+1 is unreadable
    expect(rows[1]!.display).toBe('')
  })
})

describe('cross: Σ feeding solve, and solve feeding Σ bounds', () => {
  it('a sum computed on one line sets the bound for a solve on the next', () => {
    const rows = evaluateSheet(['Σ n, n=1..4', '2x = ans'], { angleMode: 'deg' })
    expect(rows[0]!.display).toBe('10')
    expect(rows[1]!.display).toBe('5')
  })

  it('a solved single root sets the upper bound of a sum on the next line', () => {
    // 5x = 25 -> x = 5, then Σ n, n=1..ans sums 1..5 = 15
    const rows = evaluateSheet(['5x = 25', 'Σ n, n=1..ans'], { angleMode: 'deg' })
    expect(rows[0]!.display).toBe('5')
    expect(rows[1]!.display).toBe('15')
  })
})

describe('cross: chemistry vs solve/= disambiguation', () => {
  it('a genuine reaction with = balances instead of solving', () => {
    // C + O2 = CO2 balances trivially 1:1:1
    const r = line('C + O2 = CO2')
    expect(r.display).toBe('C + O₂ → CO₂')
    expect(r.kind).not.toBe('solve')
  })

  it('an = line with a single species and a bare number is neither chemistry nor solvable, so it is a plain assignment', () => {
    // "12" alone can never parse as a chemical species, so chemAnswer declines entirely;
    // solve also declines since assigning Na doesn't mention Na again on the right
    const r = line('Na = 12')
    expect(r.kind).toBe('assignment')
    expect(r.display).toBe('12')
  })

  it('a stored variable named like an element keeps a solve-shaped line as a solve', () => {
    // H is stored as 3; "2H + x = 10" must solve for x = 4, not attempt to balance "2H"
    const r = line('2H + x = 10', { variables: { H: 3 } })
    expect(r.kind).toBe('solve')
    expect(r.display).toBe('4')
  })
})

describe('cross: θ used identically in solve and in a degree-mode sum', () => {
  it('θ solves an equation the same way x would', () => {
    const withTheta = line('sin(θ) = 0.5')
    const withX = line('sin(x) = 0.5')
    expect(withTheta.display).toBe(withX.display)
    expect(withTheta.solve?.variable).toBe('θ')
  })

  it('θ names a sum index, as it names the unknown in solve', () => {
    expect(evaluateLine('Σ θ^2, θ=1..3').display).toBe('14')
  })
})

describe('cross: fraction mode and sig-fig mode across solve, sums and chem together', () => {
  it('fraction mode changes a linear solve display', () => {
    expect(line('3x = 1', { fractionMode: true }).display).toBe('1/3')
  })
  it('fraction mode changes a finite sum display', () => {
    expect(evaluateLine('Σ 1/n, n=1..3', { fractionMode: true }).display).toBe('11/6')
  })
  it('sig fig mode does not disturb a solve answer (roots are exact, not measured)', () => {
    const r = line('4x = 6', { sigFigMode: true })
    expect(r.display).toBe('1.5')
    expect(r.meas?.unc).toBeUndefined()
  })
  it('sig fig mode does not disturb a balanced chemical equation', () => {
    const r = evaluateLine('Na + Cl2 -> NaCl', { sigFigMode: true })
    expect(r.display).toBe('2 Na + Cl₂ → 2 NaCl')
  })
})
