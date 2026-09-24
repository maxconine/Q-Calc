import { describe, expect, it } from 'vitest'
import { line } from './audit.helpers'

describe('audit2: exact-integer snapping runs before the 1e4 size gate', () => {
  // exactForm tries snapInteger first and only checks `Math.abs(n) >= 1e4` afterwards, so a
  // perfect square well past 10000 still gets its exact integer label
  it('sqrt(1000000) is exactly 1000', () => {
    expect(line('sqrt(1000000)').exact).toBe('1000')
  })
  it('sqrt(1e8) is exactly 10000', () => {
    expect(line('sqrt(1e8)').exact).toBe('10000')
  })
  it('sqrt(20000000) is 2000sqrt(5) (20000000 = 2000^2 * 5)', () => {
    expect(line('sqrt(20000000)').exact).toBe('2000sqrt(5)')
  })
  it('sqrt(123456) is 8sqrt(1929) (123456 = 8^2 * 1929, 1929 = 3 * 643 is squarefree)', () => {
    expect(line('sqrt(123456)').exact).toBe('8sqrt(1929)')
  })
})

describe('audit2: only a curated set of denominators counts as a "nice" fraction', () => {
  // asNiceFraction only accepts denominators {2,3,4,5,6,8,12} (NICE_DEN in simplify.ts). Sevenths
  // are exact and clean (1/7, 2/7, 3/7) but 7 isn't in that set, so no exact form is offered even
  // though sqrt(1/49), sqrt(4/49) and sqrt(9/49) are exactly 1/7, 2/7 and 3/7. This is a
  // deliberate curation choice (these denominators don't come from common angle identities),
  // not a bug, so it is documented here rather than failed.
  it.each(['sqrt(1/49)', 'sqrt(4/49)', 'sqrt(9/49)'])('%s has no exact form even though it is a clean seventh', (text) => {
    expect(line(text).exact).toBeUndefined()
  })

  it('a twelfth is nice: sqrt(1/144) is 1/12', () => {
    expect(line('sqrt(1/144)').exact).toBe('1/12')
  })
})

describe('audit2: pi-multiple detection on radian answers beyond the basics', () => {
  it('acos(-0.5) in radians is 2*pi/3', () => {
    expect(line('acos(-0.5)', { angleMode: 'rad' }).exact).toBe('2*pi/3')
  })
  it('atan(1/3) in radians is not mistaken for any nearby pi fraction', () => {
    expect(line('atan(1/3)', { angleMode: 'rad' }).exact).toBeUndefined()
  })
})
