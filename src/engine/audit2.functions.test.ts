import { describe, expect, it } from 'vitest'
import { expectNum, expectUndefined, shown } from './audit.helpers'

describe('audit2: trig identities at a non-special angle (37 degrees)', () => {
  it.each([
    ['sin(37)', 0.6018150231520483],
    ['cos(37)', 0.7986355100472928],
    ['tan(37)', 0.7535540501027942],
    ['sec(37)', 1.2521356581562257],
    ['csc(37)', 1.6616401411224833],
    ['cot(37)', 1.32704482162041],
  ])('%s = %d', (text, want) => {
    expectNum(text, want, {}, 1e-9)
  })

  it('tan is sin over cos at 37 degrees', () => {
    expectNum('sin(37)/cos(37)', 0.7535540501027942, {}, 1e-9)
  })
  it('sec, csc and cot are the reciprocals of cos, sin and tan at 37 degrees', () => {
    expectNum('1/cos(37)', 1.2521356581562257, {}, 1e-9)
    expectNum('1/sin(37)', 1.6616401411224833, {}, 1e-9)
    expectNum('1/tan(37)', 1.32704482162041, {}, 1e-9)
  })
  it('sin(37)^2 + cos(37)^2 is 1', () => {
    expectNum('sin(37)^2 + cos(37)^2', 1)
  })
  it('a co-function pair sums to 90: sin(37) equals cos(53)', () => {
    expectNum('sin(37) - cos(53)', 0, {}, 1e-9)
  })
})

describe('audit2: hyperbolic identities at x = 2', () => {
  it.each([
    ['cosh(2)', 3.7621956910836314],
    ['sinh(2)', 3.626860407847019],
    ['tanh(2)', 0.9640275800758169],
    ['coth(2)', 1.0373147207275482],
    ['sech(2)', 0.2658022288340797],
    ['csch(2)', 0.2757205647717832],
  ])('%s = %d', (text, want) => {
    expectNum(text, want, {}, 1e-9)
  })

  it('cosh(2)^2 - sinh(2)^2 is 1', () => {
    expectNum('cosh(2)^2 - sinh(2)^2', 1)
  })
  it('tanh is sinh over cosh at x = 2', () => {
    expectNum('sinh(2)/cosh(2)', 0.9640275800758169, {}, 1e-9)
  })
})

describe('audit2: log with an explicit base, edge cases beyond the happy path', () => {
  it.each([
    ['log(1, 5)', 0],
    ['log(100, 2)', Math.log(100) / Math.log(2)],
  ])('%s = %d', (text, want) => {
    expectNum(text, want, {}, 1e-9)
  })

  it('a base of 1 is undefined (log base 1 has no meaning)', () => {
    expectUndefined('log(5, 1)')
  })
  it('a negative base with a negative argument stays undefined (no real log)', () => {
    expectUndefined('log(-8, -2)')
  })
  it('a negative argument with a valid base is undefined', () => {
    expectUndefined('log(-8, 2)')
  })
})

describe('audit2: only factorial-family overflow is shown as ∞; everything else overflowing is undefined', () => {
  // the policy is stated directly in scientific.ts: "Only factorials and counts may overflow to
  // ∞; any other infinity (1/0) is undefined." cosh/sinh of a large argument overflow the same
  // double-precision exponential that 171! does, but since the expression text has no
  // "!"/"factorial"/"combinations"/"permutations" in it, the ∞ is swapped for "undefined".
  it('cosh(1000) and sinh(1000) say overflow, not ∞', () => {
    expect(shown('cosh(1000)')).toBe('overflow')
    expect(shown('sinh(1000)')).toBe('overflow')
  })
  it('171! says overflow too', () => {
    expect(shown('171!')).toBe('overflow')
  })
  it('cosh(710) is still just inside double range and stays a finite number', () => {
    expectNum('cosh(710)', 1.1169973830808557e+308, {}, 1e-9)
  })
})

describe('audit2: atan2 at axis-aligned and degenerate points', () => {
  it.each([
    ['atan2(0, 1)', 0],
    ['atan2(0, -1)', 180],
    ['atan2(1, 0)', 90],
    ['atan2(-1, 0)', -90],
    // JS Math.atan2(0, 0) is conventionally 0; the engine follows that convention rather than
    // treating the origin as undefined
    ['atan2(0, 0)', 0],
  ])('%s = %d degrees', (text, want) => {
    expectNum(text, want)
  })
})

describe('audit2: combinatorics at extremes', () => {
  it('nCr(1000, 500) is about 2.70288240945e+299, still finite', () => {
    expectNum('nCr(1000, 500)', 2.7028824094543666e+299, {}, 1e-9)
  })
  it('nPr(170, 170) equals 170! exactly', () => {
    expectNum('nPr(170, 170)', 7.257415615307994e+306, {}, 1e-9)
  })
  it('nCr(2, 5) is 0 (choosing more than there are)', () => {
    expectNum('nCr(2, 5)', 0)
  })
  it('nPr(0, 0) and nCr(0, 5) are the boundary cases', () => {
    expectNum('nPr(0, 0)', 1)
    expectNum('nCr(0, 5)', 0)
  })
})

describe('audit2: variance and standard deviation on negative data', () => {
  const negs = '-2,-4,-4,-4,-5,-5,-7,-9'
  it.each([
    [`var(${negs})`, 32 / 7],
    [`varp(${negs})`, 4],
    [`stdev(${negs})`, Math.sqrt(32 / 7)],
  ])('%s = %d', (text, want) => {
    expectNum(text, want, {}, 1e-9)
  })

  it('corr of perfectly anti-correlated negative and positive series is -1', () => {
    // x decreases (-1,-2,-3) while y increases (1,2,3): perfectly opposite trends
    expectNum('corr(-1,-2,-3, 1,2,3)', -1)
  })
  it('corr of a constant series with anything is undefined (zero variance)', () => {
    expectUndefined('corr(5,5,5, 1,2,3)')
  })
})

describe('audit2: overflow and underflow follow the same undefined-unless-factorial policy', () => {
  it('1e300 * 1e300 overflows past double range and says so, not ∞', () => {
    expect(shown('1e300 * 1e300')).toBe('overflow')
  })
  it('1e-300 * 1e-300 underflows to plain 0, not undefined', () => {
    expectNum('1e-300 * 1e-300', 0)
  })
})

describe('audit2: unsupported statistical functions are blank, not a guess', () => {
  // there is no mode() in the function table at all
  it('mode(1,2,2,3) is blank', () => {
    expect(shown('mode(1,2,2,3)')).toBe('')
  })
})
