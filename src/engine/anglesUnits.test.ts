import { describe, expect, it } from 'vitest'
import { angleExponent } from './angles'
import { evaluateSheet } from './index'

const shown = (lines: string[], angleMode: 'deg' | 'rad' = 'deg', quantities?: Record<string, string>) =>
  evaluateSheet(lines, { angleMode, quantities }).map((r) => r.display)
const last = (lines: string[], angleMode: 'deg' | 'rad' = 'deg', quantities?: Record<string, string>) => shown(lines, angleMode, quantities).at(-1)

describe('an inverse trig answer says deg or rad', () => {
  it.each([
    ['asin(0.5)', '30 deg', '0.523598775598 rad'],
    ['acos(0.5)', '60 deg', '1.0471975512 rad'],
    ['arctan(1)', '45 deg', '0.785398163397 rad'],
    ['atan2(1, 1)', '45 deg', '0.785398163397 rad'],
    ['2*asin(0.5)', '60 deg', '1.0471975512 rad'],
    ['asin(0.5) + 10', '40 deg', '10.5235987756 rad'],
  ])('%s', (text, deg, rad) => {
    expect(last([text], 'deg')).toBe(deg)
    expect(last([text], 'rad')).toBe(rad)
  })

  it('stays a plain number when the angle cancels or goes back into trig', () => {
    expect(last(['asin(0.5) / acos(0.5)'])).toBe('0.5')
    expect(last(['sin(asin(0.5))'])).toBe('0.5')
    expect(last(['sin(30)'])).toBe('0.5')
  })

  it('carries its unit as ans', () => {
    expect(shown(['asin(0.5)', 'ans * 2'])).toEqual(['30 deg', '60 deg'])
    expect(shown(['asin(0.5)', 'sin(ans)'])).toEqual(['30 deg', '0.5'])
    expect(shown(['asin(0.5)', 'ans to rad'])).toEqual(['30 deg', '0.523598775598 rad'])
    expect(shown(['asin(0.5)', 'sin(ans)'], 'rad')).toEqual(['0.523598775598 rad', '0.5'])
  })

  it('its exact form says rad too', () => {
    expect(evaluateSheet(['asin(0.5)'], { angleMode: 'rad' })[0]!.exact).toBe('pi/6 rad')
  })
})

describe('angleExponent', () => {
  it.each([
    ['asin(x)', 1],
    ['asin(0.5)^2', 2],
    ['3 * acos(0.1) - 4', 1],
    ['asin(0.5) / acos(0.5)', 0],
    ['cos(asin(0.5))', 0],
    ['2 + 3', 0],
    ['asin(asin(0.5))', null],
  ])('%s → %s', (text, want) => {
    expect(angleExponent(text)).toBe(want)
  })
})

describe('something per angle reads in the mode’s angle, in N m rather than kg m²/s²', () => {
  it('549.16 Nm / (0.293 deg)', () => {
    expect(last(['549.16 Nm / (0.293 deg)'], 'deg')).toBe('1874.2662116 N m / deg')
    expect(last(['549.16 Nm / (0.293 deg)'], 'rad')).toBe('107387.543609 N m / rad')
  })

  it('a torque over radians in deg mode is converted, not just relabelled', () => {
    expect(last(['10 N m / (2 rad)'], 'rad')).toBe('5 N m / rad')
    expect(last(['10 N m / (2 rad)'], 'deg')).toBe('0.0872664625997 N m / deg')
  })

  it('force per angle is N / deg', () => {
    expect(last(['5 kg m / s^2 / rad'], 'rad')).toBe('5 N / rad')
  })

  it('chains as ans in the same angle', () => {
    expect(shown(['549.16 Nm / (0.293 deg)', 'ans * 2'], 'deg')).toEqual(['1874.2662116 N m / deg', '3748.53242321 N m / deg'])
  })

  it('a turning rate keeps its own units', () => {
    expect(last(['360 deg / 1 s'], 'deg')).not.toMatch(/N/)
  })
})

describe('a conversion inside arithmetic, or on ans', () => {
  it('(1.1009 in to mm) / 10', () => {
    expect(last(['(1.1009 in to mm) / 10'])).toBe('2.796286 mm')
  })

  it('arithmetic after the target unit works on the converted answer', () => {
    expect(last(['1.1009 in to mm / 10'])).toBe('2.796286 mm')
    expect(last(['2 ft to m * 3'])).toBe('1.8288 m')
    expect(last(['3 ft^2 to m^2 / 2'])).toBe('0.13935456 m²')
  })

  it('a unit still divides inside the target', () => {
    expect(last(['60 mph to m/s'])).toBe('26.8224 m/s')
  })

  it('ans in to mm, and other ways of converting ans', () => {
    const ans = { ans: '1.1009 in' }
    for (const text of ['ans in to mm', 'ans to mm', 'ans in mm', 'ans into mm']) expect(last([text], 'deg', ans)).toBe('27.96286 mm')
    expect(last(['ans to mm / 10'], 'deg', ans)).toBe('2.796286 mm')
    expect(last(['(ans to mm) / 10'], 'deg', ans)).toBe('2.796286 mm')
  })

  it('5 ft in to mm still reads in as into, and 1 in to mm as inches', () => {
    expect(last(['5 ft in to mm'])).toBe('1524 mm')
    expect(last(['1.1009 in to mm'])).toBe('27.96286 mm')
  })

  it('operations on a converted answer keep its unit', () => {
    expect(shown(['1.1009 in to mm', 'ans / 10'])).toEqual(['27.96286 mm', '2.796286 mm'])
    expect(shown(['1.1009 in to mm', 'ans + 1 in'])).toEqual(['27.96286 mm', '53.36286 mm'])
  })
})
