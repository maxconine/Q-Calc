import { describe, expect, it } from 'vitest'
import { evaluateLine } from './evaluate'
import { exactForm, wantsExactForm } from './simplify'

describe('exactForm rationalize', () => {
  it('rationalizes a radical denominator by default', () => {
    expect(exactForm(5 / Math.sqrt(41))).toBe('5sqrt(41)/41')
    expect(exactForm(1 / Math.sqrt(2))).toBe('sqrt(2)/2')
    expect(exactForm(2 / Math.sqrt(3))).toBe('2sqrt(3)/3')
  })

  it('keeps a radical in the denominator when rationalize is off', () => {
    expect(exactForm(5 / Math.sqrt(41), { rationalize: false })).toBe('5/sqrt(41)')
    expect(exactForm(1 / Math.sqrt(2), { rationalize: false })).toBe('1/sqrt(2)')
    expect(exactForm(2 / Math.sqrt(3), { rationalize: false })).toBe('2/sqrt(3)')
    expect(exactForm(-5 / Math.sqrt(41), { rationalize: false })).toBe('-5/sqrt(41)')
  })

  it('still simplifies square factors when not rationalizing', () => {
    expect(exactForm(Math.sqrt(12), { rationalize: false })).toBe('2sqrt(3)')
    expect(exactForm(5 / Math.sqrt(8), { rationalize: false })).toBe('5/(2sqrt(2))')
  })
})

describe('evaluateLine rationalize', () => {
  it('shows 5/sqrt(41) unrationalized when that setting is off', () => {
    expect(evaluateLine('5/sqrt(41)').exact).toBe('5sqrt(41)/41')
    expect(evaluateLine('5/sqrt(41)', { rationalize: false }).exact).toBe('5/sqrt(41)')
  })

  it('leaves tan(30) as 1/sqrt(3) when not rationalizing', () => {
    expect(evaluateLine('tan(30)').exact).toBe('sqrt(3)/3')
    expect(evaluateLine('tan(30)', { rationalize: false }).exact).toBe('1/sqrt(3)')
  })
})

describe('wantsExactForm', () => {
  it.each(['sin(30)', 'cos(pi/6)', 'tan(45)', 'csc(90)', 'sec(60)', 'cot(30)', 'arcsin(0.5)', 'arccos(0.5)', 'arctan(1)', 'asin(1)', 'sin^-1(0.5)', 'sin90', '\\sin(90)', 'sqrt(12)', '2sqrt(3)', '√2', '\\sqrt{8}', 'sqrt12'])(
    'is true for %s',
    (expr) => {
      expect(wantsExactForm(expr)).toBe(true)
    },
  )

  it.each(['1/2', '2/3', 'pi', 'pi/2', '2*pi', '2+2', '90 deg', '2032mm to ft', 'log(10)', 'ln(e)', '2^8', 'sinh(0)', '5!', 'x = 5'])(
    'is false for %s',
    (expr) => {
      expect(wantsExactForm(expr)).toBe(false)
    },
  )
})

describe('evaluateLine exact only for trig and square roots', () => {
  it('omits a closed form for fractions, π, and unit conversions', () => {
    expect(evaluateLine('1/2').exact).toBeUndefined()
    expect(evaluateLine('pi/2').exact).toBeUndefined()
    expect(evaluateLine('2*pi').exact).toBeUndefined()
    expect(evaluateLine('90 deg').exact).toBeUndefined()
    expect(evaluateLine('2032mm to ft').exact).toBeUndefined()
  })

  it('still attaches a closed form to trig and square roots', () => {
    expect(evaluateLine('sin(30)').exact).toBe('1/2')
    expect(evaluateLine('sqrt(12)').exact).toBe('2sqrt(3)')
    expect(evaluateLine('√8').exact).toBe('2sqrt(2)')
  })
})

describe('exactForm never claims a nearby value', () => {
  it('only snaps float noise', () => {
    expect(exactForm(Math.tan((89.9999999 * Math.PI) / 180))).toBeNull()
    expect(exactForm(Math.sin((30.00000001 * Math.PI) / 180))).toBeNull()
    expect(exactForm(Math.sqrt(2.000000001))).toBeNull()
    expect(exactForm(Math.sqrt(2000000))).toBe('1000sqrt(2)')
  })
  it('shows no exact form for these', () => {
    expect(evaluateLine('tan(89.9999999)').exact).toBeUndefined()
    expect(evaluateLine('sin(30.00000001)').exact).toBeUndefined()
  })
})

function line(text: string, opts: { fractionMode?: boolean; rationalize?: boolean; angleMode?: 'deg' | 'rad' } = {}) {
  return evaluateLine(text, { angleMode: 'deg', ...opts })
}

describe('exact forms', () => {
  it('shows a closed form only for trig and square roots', () => {
    expect(wantsExactForm('sin(30)')).toBe(true)
    expect(wantsExactForm('sqrt(12)')).toBe(true)
    expect(wantsExactForm('2+2')).toBe(false)
    expect(wantsExactForm('log(100)')).toBe(false)
    expect(line('2+2').exact).toBeUndefined()
  })

  it.each([
    ['sin(30)', '1/2'],
    ['sin(45)', 'sqrt(2)/2'],
    ['sin(60)', 'sqrt(3)/2'],
    ['sin(90)', '1'],
    ['sin(180)', '0'],
    ['sin(-30)', '-1/2'],
    ['cos(60)', '1/2'],
    ['cos(180)', '-1'],
    ['tan(45)', '1'],
    ['sqrt(12)', '2sqrt(3)'],
    ['sqrt(50)', '5sqrt(2)'],
    ['sqrt(8/9)', '2sqrt(2)/3'],
    ['1/sqrt(2)', 'sqrt(2)/2'],
    ['5/sqrt(41)', '5sqrt(41)/41'],
  ])('%s exact is %s', (text, exact) => {
    expect(line(text).exact, line(text).display).toBe(exact)
  })

  it('leaves the root in the denominator when rationalize is off', () => {
    expect(line('1/sqrt(2)', { rationalize: false }).exact).toBe('1/sqrt(2)')
    expect(line('5/sqrt(41)', { rationalize: false }).exact).toBe('5/sqrt(41)')
    expect(line('sin(45)', { rationalize: false }).exact).toBe('1/sqrt(2)')
    expect(line('-5/sqrt(41)', { rationalize: false }).exact).toBe('-5/sqrt(41)')
  })

  it('a plain decimal has no closed form', () => {
    expect(exactForm(1.23456789)).toBeNull()
    expect(line('1.23456789').exact).toBeUndefined()
  })

  it('pi multiples and the trig half-angles that have a nested form', () => {
    expect(exactForm(Math.PI / 6)).toBe('pi/6')
    expect(exactForm(Math.PI)).toBe('pi')
    expect(exactForm((Math.sqrt(6) - Math.sqrt(2)) / 4)).toBe('(sqrt(6)-sqrt(2))/4')
    expect(line('sin(15)').exact).toBe('(sqrt(6)-sqrt(2))/4')
    expect(line('tan(15)').exact).toBe('2-sqrt(3)')
  })
})

describe('fraction mode', () => {
  it.each([
    ['1/2 + 1/3', '5/6'],
    ['1/2 - 1/3', '1/6'],
    ['2/3 * 3/4', '1/2'],
    ['(1/2) / (1/4)', '2'],
    ['0.5', '1/2'],
    ['0.25', '1/4'],
    ['-0.75', '-3/4'],
    ['1/6 + 1/3', '1/2'],
    ['7/8', '7/8'],
  ])('%s displays %s', (text, display) => {
    expect(line(text, { fractionMode: true }).display).toBe(display)
  })

  it('keeps pi as a decimal and treats 0.1 as one tenth', () => {
    expect(line('pi', { fractionMode: true }).display).not.toMatch(/^\d+\/\d+$/)
    expect(line('1/3 + 0.1', { fractionMode: true }).display).toBe('13/30')
  })

  it('fraction mode still keeps the trig exact form', () => {
    const r = line('sin(45)', { fractionMode: true })
    expect(r.exact).toBe('sqrt(2)/2')
  })
})
