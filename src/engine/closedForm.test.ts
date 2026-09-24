import Decimal from 'decimal.js'
import { describe, expect, it } from 'vitest'
import { identifyJob, pslq, type ClosedFormJob } from './closedForm'
import { evaluateLine } from './evaluate'
import type { EvaluateOptions } from './types'

const rad: EvaluateOptions = { angleMode: 'rad' }
const deg: EvaluateOptions = { angleMode: 'deg' }

function jobFor(text: string, options: EvaluateOptions): { job: ClosedFormJob; n: number } {
  const r = evaluateLine(text, options)
  if (!r.closedForm || r.value?.kind !== 'number') throw new Error(`no job for ${text}`)
  return { job: r.closedForm, n: r.value.n }
}

describe('closed forms, found and verified', () => {
  it.each([
    ['∫0..1 x^2', '1/3'],
    ['∫0..1 x^10', '1/11'],
    ['∫0..1 x^2 ln(x)', '-1/9'],
    ['∫0..1 ln(x)ln(1-x)', '2 - π²/6'],
    ['∫0..1 ln(x)/(1-x)', '-π²/6'],
    ['∫0..1 ln(1+x)/x', 'π²/12'],
    ['∫0..1 atan(x)/x', 'catalan'],
    ['∫0..1 ln(1+x)/(1+x^2)', 'π ln(2)/8'],
    ['∫-∞..∞ e^(-x^2)', '√π'],
    ['∫0..∞ sqrt(x) e^(-x)', '√π/2'],
    ['∫0..1 1/sqrt(1-x^2)', 'π/2'],
    ['∫0..1 1/(1+x^2)', 'π/4'],
    ['∫0..∞ 1/(1+x^2)', 'π/2'],
    ['∫0..1 1/(x^2+x+1)', 'π√3/9'],
    ['∫0..1 1/sqrt(1+x^2)', 'ln(1 + √2)'],
    ['∫0..∞ x^3/(e^x-1)', 'π⁴/15'],
    ['∫0..∞ x^2/(e^x-1)', '2ζ(3)'],
    ['∫0..1 e^x', 'e - 1'],
    ['∫0..1 x e^(-x)', '1 - 2/e'],
    ['∫0..1 sin(x)', '1 - cos(1)'],
    ['∫1..10 1/x', 'ln(10)'],
    ['∫1..2 1/x', 'ln(2)'],
    ['lim x->∞ (1+1/x)^x', 'e'],
    ['lim x->0 (1-cos(x))/x^2', '1/2'],
    ['lim x->0 (2^x-1)/x', 'ln(2)'],
    ['lim x->∞ sqrt(x^2+x)-x', '1/2'],
  ])('%s = %s', (text, want) => {
    const { job, n } = jobFor(text, rad)
    const exact = identifyJob(job)
    expect(exact).toBe(want)
    // the exact form is typed text too: it reads back as the same number
    const back = evaluateLine(exact!, rad)
    expect(back.value?.kind).toBe('number')
    expect(Math.abs(back.value!.n - n)).toBeLessThanOrEqual(job.err + 1e-12 * Math.max(1, Math.abs(n)))
  }, 20_000)

  it('degree mode answers read back in degree mode', () => {
    for (const [text, want] of [
      ['∫0..90 sin(x)', '180/π'],
      ['lim x->0 sin(x)/x', 'π/180'],
      ['d/dx sin(x) at 60', 'π/360'],
      ['d/dx atan(x) at 1', '90/π'],
    ] as const) {
      const { job, n } = jobFor(text, deg)
      const exact = identifyJob(job)
      expect(exact, text).toBe(want)
      expect(Math.abs(evaluateLine(exact!, deg).value!.n - n)).toBeLessThan(1e-9 * Math.abs(n))
    }
  }, 20_000)

  it('sin(1) and friends only in radians, where they read back right', () => {
    const { job } = jobFor('∫0..1 cos(x) + x', deg)
    expect(identifyJob(job) ?? '').not.toMatch(/sin|cos/)
  }, 20_000)
})

describe('never a closed form that does not hold', () => {
  it.each(['∫0..1 e^(-x^2)', '∫0..1 x^x', '∫0..1 sin(x^2)', '∫1..2 e^x/x', '∫0..pi/2 sqrt(sin(x))', '∫0..∞ ln(x) e^(-x)', '∫0..1 x^pi', '∫0..3 sqrt(1+x^3)'])(
    '%s has none in the basis',
    (text) => {
      expect(identifyJob(jobFor(text, rad).job)).toBeNull()
    },
    20_000,
  )

  it('a job whose double answer disagrees is refused', () => {
    const { job } = jobFor('∫0..1 x^2', rad)
    expect(identifyJob({ ...job, approx: job.approx + 1e-6 })).toBeNull()
  })

  it('gives up when out of time', () => {
    const { job } = jobFor('∫0..1 atan(x)/x + x/7', rad)
    expect(identifyJob(job, 0)).toBeNull()
  })

  it('pslq finds true relations and refuses junk', () => {
    const D = Decimal.clone({ precision: 40 })
    const pi = D.acos(-1)
    const clock = { deadline: Date.now() + 10_000 }
    const x = new D(2).minus(pi.pow(2).div(6))
    expect(pslq([x, new D(1), pi.pow(2)], D, 30, 1e4, clock)?.map(Math.abs)).toEqual([6, 12, 1])
    // a number with no small relation to 1 and π: nothing under the bound
    const junk = new D('0.3183098861837906715377675267450287240689').plus(new D(2).sqrt().div(1000))
    expect(pslq([junk, new D(1), pi], D, 30, 1e4, clock)).toBeNull()
    // a relation needing coefficients past the bound is refused, not reported
    const big = new D(123457).div(98765)
    expect(pslq([big, new D(1)], D, 30, 1e4, clock)).toBeNull()
    expect(pslq([big, new D(1)], D, 30, 1e6, clock)?.map(Math.abs)).toEqual([98765, 123457])
  })
})

describe('timing', () => {
  it('typical integrals finish well inside the budget', () => {
    const slow: string[] = []
    for (const text of ['∫0..1 x^2', '∫0..1 ln(x)ln(1-x)', '∫-∞..∞ e^(-x^2)', '∫0..1 1/(1+x^2)']) {
      const { job } = jobFor(text, rad)
      const t0 = performance.now()
      identifyJob(job)
      if (performance.now() - t0 > 3000) slow.push(text)
    }
    expect(slow).toEqual([])
  }, 30_000)
})
