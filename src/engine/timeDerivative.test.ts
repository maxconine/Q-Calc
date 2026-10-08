import { describe, expect, it } from 'vitest'
import { evaluateSheet } from './evaluate'
import { latexToAscii } from './plainMath'

const d = (line: string, timeVarying?: Record<string, boolean>) => evaluateSheet([line], { timeVarying })[0]!

describe('time derivatives', () => {
  // the problem set: x, θ move with time; L, h, k are constants
  it.each([
    ['d/dt (x sin(theta))', 'ẋ sin(θ) + xθ̇ cos(θ)'],
    ['d/dt (L cos(theta)^2)', '-2Lθ̇ sin(θ) cos(θ)'],
    ['d/dt (theta^2 thetadot)', '2θ θ̇² + θ²θ̈'],
    ['d/dt sqrt(x^2 + h^2)', 'xẋ/√(x² + h²)'],
    ['d/dt (e^(k theta) thetadot)', 'e^(kθ) (k θ̇² + θ̈)'],
  ])('%s is %s', (line, shown) => {
    expect(d(line).display).toBe(shown)
  })

  it.each([
    ['d/dt $x\\sin\\theta$', 'ẋ sin(θ) + xθ̇ cos(θ)'],
    ['d/dt $L\\cos^2\\theta$', '-2Lθ̇ sin(θ) cos(θ)'],
    ['d/dt $\\theta^2\\dot\\theta$', '2θ θ̇² + θ²θ̈'],
    ['d/dt $\\sqrt{x^2+h^2}$', 'xẋ/√(x² + h²)'],
    ['d/dt $e^{k\\theta}\\,\\dot\\theta$', 'e^(kθ) (k θ̇² + θ̈)'],
  ])('reads LaTeX: %s', (line, shown) => {
    expect(d(line).display).toBe(shown)
  })

  it('reads every dotted spelling', () => {
    for (const line of ['d/dt (θ²θ̇)', 'd/dt (theta^2 thetadot)', 'd/dt (theta^2 dot(theta))', 'd/dt $\\theta^2\\dot{\\theta}$']) {
      expect(d(line).display).toBe('2θ θ̇² + θ²θ̈')
    }
    expect(d('d/dt (e^(kθ) θ̇)').display).toBe('e^(kθ) (k θ̇² + θ̈)')
  })

  it('takes the second derivative', () => {
    expect(d('d²/dt² (L sin(theta))').display).toBe('L·(θ̈ cos(θ) - θ̇² sin(θ))')
    expect(d('d^2/dt^2 x^3').display).toBe('6x ẋ² + 3x²ẍ')
  })

  it('says which letters moved, and lets a letter be switched', () => {
    expect(d('d/dt (L cos(theta)^2)').time).toEqual({ varying: ['θ'], constant: ['L'], locked: [] })
    const moved = d('d/dt (L cos(theta)^2)', { L: true })
    expect(moved.display).toBe('L̇ cos(θ)² - 2Lθ̇ sin(θ) cos(θ)')
    expect(moved.time).toEqual({ varying: ['L', 'θ'], constant: [], locked: [] })
    expect(d('d/dt sqrt(x^2 + h^2)', { h: true, x: false }).display).toBe('hḣ/√(x² + h²)')
  })

  it('keeps a dotted letter moving, whatever the switch says', () => {
    const r = d('d/dt (1/2 m xdot^2)', { x: false })
    expect(r.display).toBe('mẋẍ')
    expect(r.time).toEqual({ varying: ['x'], constant: ['m'], locked: ['x'] })
  })

  it('mixes with an explicit t', () => {
    expect(d('d/dt (x t^2)').display).toBe('2tx + t²ẋ')
  })

  it('leaves ordinary and partial derivatives as they were', () => {
    expect(d('d/dt (t^2 sin(t))').time).toBeUndefined()
    expect(evaluateSheet(['∂/∂t A sin(w t)'], { angleMode: 'rad' })[0]!.display).toBe('Aw cos(tw)')
    expect(d('d/dx (x sin(theta))').display).toBe('sin(θ)')
  })
})

describe('LaTeX trig without brackets', () => {
  it.each([
    ['x\\sin\\theta', 'x sin(theta)'],
    ['L\\cos^2\\theta', 'L cos(theta)^(2)'],
    ['\\sin^2 x', 'sin(x)^(2)'],
    ['\\cos^2(\\theta)', 'cos(theta)^(2)'],
    ['2\\sin\\theta\\cos\\theta', '2 sin(theta) cos(theta)'],
    ['\\sin 30', 'sin(30)'],
    ['\\sin^{-1}(0.5)', 'asin(0.5)'],
  ])('%s is %s', (latex, ascii) => {
    expect(latexToAscii(latex)).toBe(ascii)
  })

  it('evaluates', () => {
    expect(evaluateSheet(['$\\sin^2 30$'])[0]!.display).toBe('0.25')
    expect(evaluateSheet(['$\\cos^2(60) + \\sin^2(60)$'])[0]!.display).toBe('1')
  })
})
