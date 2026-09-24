import { describe, expect, it } from 'vitest'
import { line } from './audit.helpers'

const both = { fractionMode: true, sigFigMode: true }

describe('audit2: fraction mode and sig-fig mode together, precedence worked out from the source', () => {
  // evaluate.ts only calls show() (which honours fractionMode) when there is no sig-fig display
  // to fall back on. A written decimal literal ("2.50") always carries a finite sig-fig count,
  // so formatMeasured succeeds and wins. A bare integer literal ("1/3") is stored as *exact*
  // (Infinity sig figs), and formatSig refuses anything past 16 sig figs, so formatMeasured
  // returns null and control falls through to show(), where fraction mode finally gets to run.
  it('a decimal literal makes sig-fig mode win over fraction mode', () => {
    expect(line('2.50 cm * 2', both).display).toBe('5.00 cm')
    expect(line('2.50 m * 2.0 m', both).display).toBe('5.0 m²')
    expect(line('0.75 kg to lb', both).display).toBe('1.7 lbs')
  })

  it('an all-integer literal expression lets fraction mode show through even with sig-fig mode on', () => {
    expect(line('1/3 + 1/6', both).display).toBe('1/2')
    expect(line('2/4', both).display).toBe('1/2')
  })

  it('sig-fig mode alone (fraction mode off) leaves 1/3 as a plain decimal', () => {
    expect(line('1/3', { sigFigMode: true }).display).toBe('0.333333333333')
  })
})

describe('audit2: exact-form labels do not depend on fraction or sig-fig mode', () => {
  it('sqrt(8) still shows the 2sqrt(2) exact label with fraction mode on', () => {
    expect(line('sqrt(8)', { fractionMode: true }).exact).toBe('2sqrt(2)')
  })
  it('sin(60) still shows sqrt(3)/2 with fraction mode on', () => {
    expect(line('sin(60)', { fractionMode: true }).exact).toBe('sqrt(3)/2')
  })
  it('degree vs radian mode changes the exact label, independent of fraction/sig-fig settings', () => {
    expect(line('acos(-0.5)', { angleMode: 'deg', fractionMode: true }).exact).toBe('120')
    expect(line('acos(-0.5)', { angleMode: 'rad', fractionMode: true }).exact).toBe('2*pi/3')
  })
})

describe('audit2: sig-fig mode carries a unit answer\'s precision through a conversion', () => {
  it('2.5 mi to km keeps 2 sig figs: 4.0 km', () => {
    expect(line('2.5 mi to km', { sigFigMode: true }).display).toBe('4.0 km')
  })
  it('the un-decorated integer form of the same conversion has no sig-fig metadata to show', () => {
    expect(line('1 lb to kg', { fractionMode: true }).display).toBe('0.45359237 kg')
  })
})
