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

describe('every way of writing an inverse says deg or rad', () => {
  it.each([
    'tan ^(-1) (1781 / 1135)',
    'tan^-1(1781/1135)',
    'tan^(-1)(1781/1135)',
    'tan⁻¹(1781/1135)',
    'tan^{-1}(1781/1135)',
    '\\tan^{-1}(1781/1135)',
    '\\arctan(1781/1135)',
    'arctan(1781/1135)',
    'atan(1781/1135)',
    'atan2(1781, 1135)',
    'ATAN(1781/1135)',
    'Tan^-1(1781/1135)',
  ])('%s', (text) => {
    expect(last([text], 'deg')).toBe('57.4913540527 deg')
    expect(last([text], 'rad')).toBe('1.00341341965 rad')
  })

  it.each([
    ['sin^-1(0.5)', '30 deg'],
    ['cos^-1(0.5)', '60 deg'],
    ['sec^-1(2)', '60 deg'],
    ['csc^-1(2)', '30 deg'],
    ['cot^-1(1)', '45 deg'],
    ['asec(2)', '60 deg'],
    ['acsc(2)', '30 deg'],
    ['acot(1)', '45 deg'],
    ['tan^-1 1', '45 deg'],
    ['2tan^-1(1)', '90 deg'],
    ['2 tan^-1(1)', '90 deg'],
    ['atan(inf)', '90 deg'],
    ['atan(infinity)', '90 deg'],
  ])('%s → %s', (text, want) => {
    expect(last([text], 'deg')).toBe(want)
  })

  it('a hyperbolic inverse is not an angle', () => {
    expect(last(['sinh^-1(1)'])).toBe('0.88137358702')
    expect(last(['asinh(1)'])).toBe('0.88137358702')
  })

  it('converts with to and in', () => {
    expect(last(['tan^-1(1781/1135) to rad'], 'deg')).toBe('1.00341341965 rad')
    expect(last(['tan^-1(1781/1135) in rad'], 'deg')).toBe('1.00341341965 rad')
    expect(last(['tan^-1(1781/1135) to deg'], 'rad')).toBe('57.4913540527 deg')
    expect(last(['asin(0.5) to arcmin'], 'deg')).toBe('1800 arcmin')
    expect(last(['asin(0.5) to kg'], 'deg')).toBe('improper unit conversion')
  })
})

describe('function notation', () => {
  it('a power on the function name is a power of its answer', () => {
    expect(last(['sin^2(30)'])).toBe('0.25')
    expect(last(['sin²(45)'])).toBe('0.5')
    expect(last(['cos²(30) + sin²(30)'])).toBe('1')
    expect(last(['tan^2(45)'])).toBe('1')
    expect(last(['ln^2(e)'])).toBe('1')
    expect(last(['sin^2(30 + 15)'])).toBe('0.5')
  })

  it('built-in names are read in any case when called', () => {
    expect(last(['SIN(30)'])).toBe('0.5')
    expect(last(['Sin(30)'])).toBe('0.5')
    expect(last(['LN(e)'])).toBe('1')
    expect(last(['Sqrt(16)'])).toBe('4')
    expect(last(['MAX(3, 7, 2)'])).toBe('7')
  })

  it('a user function keeps its own capitals', () => {
    expect(evaluateSheet(['Foo(x) = 2x', 'Foo(3)']).at(-1)!.display).toBe('6')
    expect(evaluateSheet(['Max2(x) = 2x', 'Max2(3)']).at(-1)!.display).toBe('6')
  })

  it('an angle unit at the end of a trig argument is for the whole argument', () => {
    expect(last(['sin(pi/6 rad)'])).toBe('0.5')
    expect(last(['cos(pi/3 rad)'])).toBe('0.5')
    expect(last(['sin(30 deg)'])).toBe('0.5')
  })
})

describe('an angle answer carries on like a number', () => {
  it('ans plus a plain number stays in the angle', () => {
    expect(shown(['asin(0.5)', 'ans + 30', 'ans - 15', 'ans / 2'])).toEqual(['30 deg', '60 deg', '45 deg', '22.5 deg'])
    expect(last(['30 deg + 30'])).toBe('60 deg')
    expect(last(['90 deg - 45'])).toBe('45 deg')
  })

  it('other units still refuse a plain number', () => {
    expect(last(['5 m + 3'])).toBe('')
  })

  it('round keeps the angle', () => {
    expect(last(['round(atan(2), 1)'])).toBe('63.4 deg')
    expect(last(['round(atan(2))'])).toBe('63 deg')
  })

  it('a variable holds it', () => {
    expect(shown(['theta = atan(3/4)', '5 * cos(theta)', '5 * sin(theta)'])).toEqual(['36.8698976458 deg', '4', '3'])
  })
})

describe('functions among typed units', () => {
  it.each([
    ['5 m * sin(30)', '2.5 m'],
    ['10 N * cos(30)', '8.66025403784 N'],
    ['10 N cos(30)', '8.66025403784 N'],
    ['sin(30) * 5 m', '2.5 m'],
    ['5 m * sqrt(4)', '10 m'],
    ['9.81 m/s^2 * sin(30 deg)', '4.905 m/s²'],
    ['100 N * cos(30 deg) * 2 m', '173.205080757 N·m'],
    ['5 m / tan(45)', '5 m'],
    ['3 m * max(2, 5)', '15 m'],
    ['100 kPa * exp(-1)', '36.7879441171 kPa'],
    ['10 N * COS(60)', '5 N'],
    ['50 N * sin^2(30)', '12.5 N'],
    ['5 kg * 9.81 m/s^2 * sin(asin(0.5))', '24.525 N'],
    ['(5 ft to m) * cos(60)', '0.762 m'],
    ['10 N * cos(30) to lbf', '1.94690255722 lbf'],
  ])('%s → %s', (text, want) => {
    expect(last([text])).toBe(want)
  })

  it('a root of a quantity', () => {
    expect(last(['sqrt(16 m^2)'])).toBe('4 m')
    expect(last(['sqrt((3 m)^2 + (4 m)^2)'])).toBe('5 m')
    expect(last(['sqrt(2 * 9.81 m/s^2 * 10 m)'])).toBe('14.0071410359 m/s')
    expect(last(['sqrt(2 * 10 m / (9.81 m/s^2))'])).toBe('1.42784312293 s')
    expect(last(['cbrt(27 m^3)'])).toBe('3 m')
    expect(last(['√(16 m^2)'])).toBe('4 m')
    expect(last(['√(9) km'])).toBe('3 km')
    expect(last(['∛(8 m^3)'])).toBe('2 m')
  })

  it('but not one that leaves half a unit, and no other function takes a unit', () => {
    for (const text of ['sqrt(5 m)', 'sqrt(g)', 'sqrt(2 kg)', 'abs(-5 m)', 'max(3 m, 2)', 'ln(5 m)', 'sin(5 m)', 'exp(2 s)']) expect(last([text])).toBe('')
  })

  it('inverse trig among units goes in as an angle', () => {
    expect(last(['180 deg - asin(0.5)'])).toBe('150 deg')
    expect(last(['549.16 Nm / asin(0.5)'], 'deg')).toBe('18.3053333333 N m / deg')
    expect(last(['549.16 Nm / asin(0.5)'], 'rad')).toBe('1048.81834258 N m / rad')
    expect(last(['\\arcsin(0.5) + 10 deg'])).toBe('40 deg')
  })

  it('an unknown function is still blank, and plain maths is untouched', () => {
    expect(last(['5 m * foo(2)'])).toBe('')
    expect(last(['sin(30)'])).toBe('0.5')
    expect(last(['sqrt(16)'])).toBe('4')
  })
})

describe('more notation', () => {
  it('a power on a function with a bare argument', () => {
    expect(last(['sin^2 30'])).toBe('0.25')
    expect(last(['sin^2 30 + cos^2 30'])).toBe('1')
  })

  it('Pi in any case', () => {
    expect(last(['Pi * 2'])).toBe('6.28318530718')
    expect(last(['2 PI'])).toBe('6.28318530718')
  })
})
