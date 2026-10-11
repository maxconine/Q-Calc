import { describe, expect, it } from 'vitest'
import { evaluateSheet } from './index'

const shown = (lines: string[]) => evaluateSheet(lines, { angleMode: 'deg' }).map((r) => r.display)
const last = (lines: string[]) => shown(lines).at(-1)

describe('phi is the golden ratio', () => {
  it.each([
    ['phi', '1.61803398875'],
    ['φ', '1.61803398875'],
    ['2 phi', '3.2360679775'],
    ['phi^2 - phi - 1', '0'],
    ['1/phi', '0.61803398875'],
    ['5 m * phi', '8.09016994375 m'],
  ])('%s → %s', (text, want) => {
    expect(last([text])).toBe(want)
  })

  it('unless it is a variable', () => {
    expect(shown(['phi = 30', 'sin(phi)', '2 phi'])).toEqual(['30', '0.5', '60'])
    expect(shown(['phi = 3', '5 m * phi'])).toEqual(['3', '15 m'])
  })

  it('and it is still a letter to d/dt', () => {
    expect(last(['d/dt (r cos(phi))'])).toBe('ṙ cos(φ) - rφ̇ sin(φ)')
  })

  it('stores like any number', () => {
    expect(shown(['x = phi', 'x^2'])).toEqual(['1.61803398875', '2.61803398875'])
  })
})

describe('complex answers, once an i is typed', () => {
  it.each([
    ['i', 'i'],
    ['2i', '2i'],
    ['3 + 4i', '3 + 4i'],
    ['(1+2i)*(3-i)', '5 + 5i'],
    ['(1+i)/(1-i)', 'i'],
    ['(2+3i)^2', '-5 + 12i'],
    ['i^3', '-i'],
    ['1/i', '-i'],
    ['conj(3+4i)', '3 - 4i'],
    ['2.5i * 2', '5i'],
    ['sqrt(-4) + i', '3i'],
    ['sqrt(4) + i', '2 + i'],
  ])('%s → %s', (text, want) => {
    expect(last([text])).toBe(want)
  })

  it('a real answer from complex parts is a plain number', () => {
    expect(last(['i^2'])).toBe('-1')
    expect(last(['e^(i pi)'])).toBe('-1')
    expect(last(['abs(3+4i)'])).toBe('5')
    expect(last(['re(3+4i)'])).toBe('3')
    expect(last(['im(3+4i)'])).toBe('4')
    expect(last(['sqrt(-9) * i'])).toBe('-3')
    expect(last(['1 - i - 1 + i'])).toBe('0')
  })

  it('chains as ans and stores in a variable', () => {
    expect(shown(['(1+2i)*(3-i)', 'ans + 1', 'ans * i', 'abs(ans)'])).toEqual(['5 + 5i', '6 + 5i', '-5 + 6i', '7.81024967591'])
    expect(shown(['z = 3 + 4i', 'z * 2', 'abs(z)', 'z^2'])).toEqual(['3 + 4i', '6 + 8i', '5', '-7 + 24i'])
  })

  it('without an i typed, real maths stays real', () => {
    expect(last(['sqrt(-1)'])).toBe('undefined')
    expect(last(['sqrt(-4)'])).toBe('undefined')
    expect(last(['asin(2)'])).toBe('undefined')
    expect(last(['x^2 = -4'])).toBe('no real solution')
  })

  it('i is still free to be a variable, and the i in other names is no imaginary unit', () => {
    expect(shown(['i = 3', 'i + 1'])).toEqual(['3', '4'])
    expect(last(['pi'])).toBe('3.14159265359')
    expect(last(['sin(30)'])).toBe('0.5')
  })
})

describe('a typed space between numbers', () => {
  const on = (text: string) => evaluateSheet([text], { spaceThousands: true })[0]!.display

  it.each([
    ['3 4', '12'],
    ['2 3 4', '24'],
    ['1.5 2', '3'],
    ['3 .5', '1.5'],
    ['2 10^3', '2000'],
    ['10 20 / 5', '40'],
    ['2 3 m', '6 m'],
    ['x = 3 4', '12'],
    ['1 000', '0'],
    ['12 345.67', '4148.04'],
  ])('multiplies by default: %s → %s', (text, want) => {
    expect(last([text])).toBe(want)
  })

  it.each([
    ['1 000 000 / 4', '250000'],
    ['1 000', '1000'],
    ['12 345.67', '12345.67'],
    ['1 000 000 + 2 500', '1002500'],
    ['3 000 m to km', '3 km'],
    ['2 500 mL to L', '2.5 L'],
    ['mean(1 000, 2 000)', '1500'],
    ['12 345 678.9 / 2', '6172839.45'],
    ['1 000 in hex', '0x3E8'],
  ])('groups thousands with the setting on: %s → %s', (text, want) => {
    expect(on(text)).toBe(want)
  })

  it('with the setting on, anything but a group of three still multiplies', () => {
    expect(on('3 4')).toBe('12')
    expect(on('1 00')).toBe('0')
    expect(on('2 3 m')).toBe('6 m')
  })

  it('a pasted thin or no-break space groups thousands either way: nobody types those', () => {
    for (const text of ['1\u202f000\u202f000', '1\u00a0000\u00a0000', '1\u2009000\u2009000']) {
      expect(last([text])).toBe('1000000')
      expect(on(text)).toBe('1000000')
    }
  })

  it('never between a function and its argument, and a mixed number stays unread', () => {
    expect(last(['sin^2 30'])).toBe('0.25')
    expect(last(['tan^-1 1'])).toBe('45 deg')
    expect(last(['2 1/2'])).toBe('')
    expect(last(['10 20 / 5'])).toBe('40')
  })

  it('leaves spaces that mean something else alone', () => {
    expect(last(['5 ft 3 in'])).toBe('5.25 ft')
    expect(last(['[1 2; 3 4]'])).toBe('[1 2; 3 4]')
    expect(last(['3 -4'])).toBe('-1')
    expect(last(['2 (3)'])).toBe('6')
    expect(last(['1,000,000 / 4'])).toBe('250000')
  })
})

describe('hex, binary and octal', () => {
  it.each([
    ['0xff', '255'],
    ['0xFF + 1', '256'],
    ['0b1010', '10'],
    ['0o17', '15'],
    ['0x1F * 2', '62'],
  ])('reads %s as %s', (text, want) => {
    expect(last([text])).toBe(want)
  })

  it.each([
    ['255 in hex', '0xFF'],
    ['255 to hex', '0xFF'],
    ['hex(255)', '0xFF'],
    ['10 in binary', '0b1010'],
    ['bin(10)', '0b1010'],
    ['oct(8)', '0o10'],
    ['10 as octal', '0o12'],
    ['0xff in binary', '0b11111111'],
    ['0b1010 to hex', '0xA'],
    ['0xff + 1 in hex', '0x100'],
    ['2^10 in hex', '0x400'],
    ['-255 in hex', '-0xFF'],
    ['0xff in decimal', '255'],
  ])('writes %s as %s', (text, want) => {
    expect(last([text])).toBe(want)
  })

  it('the answer underneath is still the number', () => {
    expect(shown(['255 in hex', 'ans + 1'])).toEqual(['0xFF', '256'])
    expect(shown(['x = 0xff', 'x in bin'])).toEqual(['255', '0b11111111'])
  })

  it('only a whole number without a unit has one', () => {
    for (const text of ['255.5 in hex', '5 m in hex', '0xZZ', 'hex(2)+(3)']) expect(last([text])).toBe('')
  })

  it('leaves ordinary conversions alone', () => {
    expect(last(['2 in to mm'])).toBe('50.8 mm')
    expect(last(['5 ft in m'])).toBe('1.524 m')
  })
})
