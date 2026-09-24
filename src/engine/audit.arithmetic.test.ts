import { describe, expect, it } from 'vitest'
import { expectBlankOr, expectNum, expectUndefined, shown } from './audit.helpers'

// every expected value here is worked out by hand, not read off the engine

describe('audit: precedence and associativity', () => {
  it.each([
    ['2+3*4', 14],
    ['(2+3)*4', 20],
    ['2*3+4*5', 26],
    ['10-4-3', 3],
    ['100/10/2', 5],
    ['10/2*5', 25],
    ['2*3/4', 1.5],
    ['2^3^2', 512],
    ['(2^3)^2', 64],
    ['2^-1', 0.5],
    ['2^-2^2', 0.0625],
    ['-2^2', -4],
    ['(-2)^2', 4],
    ['-2^3', -8],
    ['(-2)^3', -8],
    ['-3^2 + 10', 1],
    ['2*-3', -6],
    ['-2*-3', 6],
    ['5 - -3', 8],
    ['3 - (-2)', 5],
    ['-(3+4)', -7],
    ['1 - 2 - 3', -4],
    ['2**3', 8],
    ['2**3**2', 512],
    ['6÷3', 2],
    ['3×4', 12],
    ['3·4', 12],
    ['5−2', 3],
    ['(((2)))', 2],
    ['1 + 2 * 3 ^ 2', 19],
    ['(1 + 2) * 3 ^ 2', 27],
    ['8 / 4 / 2', 1],
    ['2 ^ 0.5 ^ 2', 2 ** 0.25],
  ])('%s = %d', (text, want) => {
    expectNum(text, want)
  })
})

describe('audit: implicit multiplication', () => {
  it.each([
    ['2(3+4)', 14],
    ['(1+2)(3+4)', 21],
    ['3(2)(4)', 24],
    ['2sqrt(9)', 6],
    ['2pi', 2 * Math.PI],
    ['2π', 2 * Math.PI],
    ['3pi/2', (3 * Math.PI) / 2],
    ['2pi*2', 4 * Math.PI],
    ['4(2)^2', 16],
    ['2√3', 2 * Math.sqrt(3)],
    ['(2)(3)', 6],
    ['2(3)(4)+1', 25],
    ['-2(3)', -6],
    ['10 - 2(3)', 4],
  ])('%s = %d', (text, want) => {
    expectNum(text, want)
  })
})

describe('audit: autofilled parentheses', () => {
  it.each([
    ['2+(3', 5],
    ['(2+3', 5],
    ['2+3)', 5],
    ['sqrt(16', 4],
    ['sin(30', 0.5],
    ['sqrt(sqrt(16', 2],
    ['((2+3)*4', 20],
    ['2*(3+(4', 14],
  ])('%s = %d', (text, want) => {
    expectNum(text, want)
  })
})

describe('audit: decimal display', () => {
  it.each([
    ['0.1+0.2', '0.3'],
    ['0.1*3', '0.3'],
    ['1.1*1.1', '1.21'],
    ['1/3', '0.333333333333'],
    ['2/3', '0.666666666667'],
    ['1/7', '0.142857142857'],
    ['10/4', '2.5'],
    ['7/7', '1'],
    ['1e12', '1e+12'],
    ['999999999999', '999999999999'],
    ['1e-7', '1e-7'],
    ['-1e-7', '-1e-7'],
    ['0.000001', '0.000001'],
    ['123456789 * 1000', '123456789000'],
    ['10^100', '1e+100'],
    ['10^-10', '1e-10'],
    ['1/1024', '0.0009765625'],
    ['6.022e23 * 2', '1.2044e+24'],
    ['2^53', '9.00719925474e+15'],
    ['0.3 - 0.1', '0.2'],
    ['1 - 0.9', '0.1'],
    ['100 * 1.1', '110'],
    ['0.07 * 100', '7'],
    ['3 * 1.1', '3.3'],
    ['-0.5 * 2', '-1'],
    ['0 * -1', '0'],
  ])('%s shows %s', (text, want) => {
    expect(shown(text)).toBe(want)
  })

  // decimal input that cancels exactly: binary float leaves -2.78e-17, which is shown as the answer
  it('1 - 0.9 - 0.1 shows 0', () => {
    expect(shown('1 - 0.9 - 0.1')).toBe('0')
  })
  it('0.1 + 0.2 - 0.3 shows 0', () => {
    expect(shown('0.1 + 0.2 - 0.3')).toBe('0')
  })

  it.each([
    ['1e3', 1000],
    ['1.5e3 + 1', 1501],
    ['2e-3 * 1000', 2],
    ['1E3', 1000],
    ['1e+3', 1000],
    ['-1e3', -1000],
    ['1e308 / 1e300', 1e8],
    ['1e-300 * 1e300', 1],
  ])('%s = %d', (text, want) => {
    expectNum(text, want)
  })
})

describe('audit: thousands separators and absolute value', () => {
  it.each([
    ['1,000,000 + 1', 1000001],
    ['1,234.5 * 2', 2469],
    ['3,000 / 3', 1000],
    ['max(1,200)', 200],
    ['min(1,200)', 1],
    ['|-5|', 5],
    ['|3 - 10|', 7],
    ['abs(-2.5)', 2.5],
  ])('%s = %d', (text, want) => {
    expectNum(text, want)
  })

  it('2|-3| is 6 or blank, never a bitwise or', () => {
    expectBlankOr('2|-3|', ['6'])
  })

  it('a comma decimal (1,5) is never read as some other number', () => {
    expectBlankOr('1,5', ['1.5'])
    expectBlankOr('3,14', ['3.14'])
  })
})

describe('audit: factorial', () => {
  it.each([
    ['5!', 120],
    ['0!', 1],
    ['1!', 1],
    ['10!', 3628800],
    ['3!^2', 36],
    ['2^3!', 64],
    ['-3!', -6],
    ['(2+1)!', 6],
    ['4!/2!', 12],
    ['3! + 4!', 30],
    ['5 + 3!', 11],
    ['5! - 1', 119],
    ['2*3!', 12],
    ['sqrt(4)!', 2],
    ['0.5!', Math.sqrt(Math.PI) / 2],
    ['(-2.5)!', (4 * Math.sqrt(Math.PI)) / 3],
    ['1.5!', (3 * Math.sqrt(Math.PI)) / 4],
    ['20!', 2432902008176640000],
    ['170!', 7.257415615307994e306],
  ])('%s = %d', (text, want) => {
    expectNum(text, want, {}, 1e-9)
  })

  // the operand scan in rewriteFactorial walks back over [0-9.eE+] (the + is meant for 1e+5),
  // so '2+3!' becomes factorial(2+3) = 120, and '3!+4!' becomes 3! * factorial(+4) = 144
  it('2+3! = 8', () => {
    expectNum('2+3!', 8)
  })
  it('3!+4! = 30', () => {
    expectNum('3!+4!', 30)
  })
  it('1+5! = 121', () => {
    expectNum('1+5!', 121)
  })

  it('171! overflows to infinity, not a finite number', () => {
    expect(shown('171!')).toBe('∞')
  })

  it('negative integers have no factorial', () => {
    expectUndefined('(-1)!')
    expectUndefined('(-3)!')
  })
})

describe('audit: mod', () => {
  it.each([
    ['7 mod 3', 1],
    ['-7 mod 3', 2],
    ['10 mod 2.5', 0],
    ['5.5 mod 2', 1.5],
    ['mod(10, 3)', 1],
    ['2^10 mod 7', 2],
    ['3^100 mod 7', 4],
    ['2*3^4 mod 7', 1],
    ['10 + 7 mod 3', 11],
    ['100 mod 7 mod 3', 2],
    ['2^64 mod 10', 6],
    ['7^222 mod 10', 9],
    ['2^60 mod 7', 1],
    ['12 mod 12', 0],
    ['0 mod 5', 0],
  ])('%s = %d', (text, want) => {
    expectNum(text, want)
  })

  // 1 % 0.1 in binary is 0.09999999999999995, one float step short of the period
  it('1 mod 0.1 = 0', () => {
    expect(shown('1 mod 0.1')).toBe('0')
  })
  it('0.3 mod 0.1 = 0', () => {
    expect(shown('0.3 mod 0.1')).toBe('0')
  })

  it('x mod 0 is undefined', () => {
    expectUndefined('7 mod 0')
  })

  it('a mod past 2^53 is not a guessed number', () => {
    expectBlankOr('(2^64) mod 10', ['6'])
  })

  // programmers write % for remainder; 17 % 5 read as 17% × 5 = 0.85 is a wrong answer
  it('17 % 5 is the remainder 2 or blank, not 0.85', () => {
    expectBlankOr('17 % 5', ['2'])
  })
})

describe('audit: percent', () => {
  it.each([
    ['200 + 15%', 230],
    ['200 - 15%', 170],
    ['200 + 15 %', 230],
    ['15% of 200', 30],
    ['12.5% of 80', 10],
    ['50%', 0.5],
    ['200 * 15%', 30],
    ['200 / 50%', 400],
    ['100 + 10% + 10%', 121],
    ['10% + 10%', 0.2],
    ['2 * 3 + 10%', 6.6],
    ['50 + 50 + 20%', 120],
    ['(50 + 50) + 20%', 120],
    ['1000 - 5% - 5%', 902.5],
    ['what is 40% of 90?', 36],
    ['50% of 50% of 80', 20],
    ['-10%', -0.1],
    ['5% * 20', 1],
    ['(200 + 15%) * 2', 460],
    ['max(200 + 10%, 5)', 220],
    ['1e2 + 10%', 110],
    ['3 + 5% of 20', 4],
    ['100 - 100%', 0],
  ])('%s = %d', (text, want) => {
    expectNum(text, want)
  })
})

describe('audit: negatives', () => {
  it.each([
    ['-5 + 3', -2],
    ['-(-5)', 5],
    ['3 * -2', -6],
    ['2^-3', 0.125],
    ['-2.5 * 4', -10],
    ['-10 / -2', 5],
    ['-10 / 4', -2.5],
    ['cbrt(-27)', -3],
    ['-8^(1/3)', -2],
  ])('%s = %d', (text, want) => {
    expectNum(text, want)
  })

  it('-0 shows as 0', () => {
    expect(shown('-0')).toBe('0')
    expect(shown('0 * -5')).toBe('0')
  })

  it('a real cube root of a negative is -2 or blank, never some other number', () => {
    expectBlankOr('(-8)^(1/3)', ['-2', 'undefined'])
  })
})

describe('audit: undefined and overflow', () => {
  it.each(['1/0', '0/0', '5/(2-2)', 'sqrt(-4)', 'ln(0)', 'log(-10)', 'asin(2)', 'tan(90)'])('%s is undefined', (text) => {
    expectUndefined(text)
  })

  it('a finite overflow is not shown as a number', () => {
    const d = shown('10^400')
    expect(d === '' || d === 'undefined' || d === '∞').toBe(true)
  })
})
